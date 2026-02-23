use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Container, Pod, Probe, ResourceRequirements, TCPSocketAction};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, Patch, PatchParams};
use kube::Client;
use std::collections::BTreeMap;
use tracing::{info, warn};

use crate::crd::{DefaultProbeConfig, DefaultResourceConfig, DevOpsPolicySpec, EnforcementMode};

/* ============================= TYPES ============================= */

/// Identifies a parent workload (Deployment, StatefulSet, or DaemonSet).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkloadRef {
    pub kind: String,
    pub name: String,
    pub namespace: String,
}

impl WorkloadRef {
    /// Canonical key used for deduplication: "kind/namespace/name".
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.kind.to_lowercase(), self.namespace, self.name)
    }
}

/// A single remediation action to apply to a container.
#[derive(Debug, Clone, PartialEq)]
pub enum RemediationAction {
    InjectLivenessProbe { container_index: usize },
    InjectReadinessProbe { container_index: usize },
    InjectResources { container_index: usize },
}

/// A plan describing all remediations for a single workload.
#[derive(Debug, Clone)]
pub struct RemediationPlan {
    pub workload: WorkloadRef,
    pub actions: Vec<RemediationAction>,
}

/// Result of applying a remediation plan.
#[derive(Debug, Clone)]
pub struct RemediationResult {
    pub workload: WorkloadRef,
    pub success: bool,
    pub message: String,
}

/* ============================= PROTECTED NAMESPACES ============================= */

const PROTECTED_NAMESPACES: &[&str] = &[
    "kube-system",
    "kube-public",
    "kube-node-lease",
    "kube-flannel",
    "cert-manager",
    "istio-system",
    "monitoring",
    "observability",
    "argocd",
];

/// Returns true if the namespace should never have enforcement applied.
pub fn is_protected_namespace(ns: &str) -> bool {
    PROTECTED_NAMESPACES.contains(&ns)
        || ns.starts_with("kube-")
        || ns.ends_with("-system")
}

/* ============================= ENFORCEMENT CHECKS ============================= */

/// Returns true if the policy has enforcement mode set to Enforce.
pub fn is_enforcement_enabled(policy: &DevOpsPolicySpec) -> bool {
    matches!(policy.enforcement_mode, Some(EnforcementMode::Enforce))
}

/* ============================= OWNER RESOLUTION ============================= */

/// Attempt to resolve the parent workload from a pod's owner_references.
///
/// Walks owner_references to find a Deployment, StatefulSet, or DaemonSet.
/// For pods owned by a ReplicaSet, strips the hash suffix to derive the
/// Deployment name (offline heuristic — see `resolve_owner_via_api` for
/// API-based resolution).
pub fn resolve_owner(pod: &Pod) -> Option<WorkloadRef> {
    let namespace = pod.metadata.namespace.clone().unwrap_or_default();
    let owners = pod.metadata.owner_references.as_ref()?;

    for owner in owners {
        match owner.kind.as_str() {
            "Deployment" | "StatefulSet" | "DaemonSet" => {
                return Some(WorkloadRef {
                    kind: owner.kind.clone(),
                    name: owner.name.clone(),
                    namespace,
                });
            }
            "ReplicaSet" => {
                let deployment_name = strip_replicaset_hash(&owner.name);
                return Some(WorkloadRef {
                    kind: "Deployment".to_string(),
                    name: deployment_name,
                    namespace,
                });
            }
            _ => continue,
        }
    }

    None
}

/// Strip the ReplicaSet pod-template-hash suffix to derive the Deployment name.
///
/// A ReplicaSet name like `"web-app-5d4f8b9c7f"` becomes `"web-app"`.
/// If there is no `-` in the name, returns the name unchanged.
pub fn strip_replicaset_hash(rs_name: &str) -> String {
    match rs_name.rfind('-') {
        Some(pos) if pos > 0 => rs_name[..pos].to_string(),
        _ => rs_name.to_string(),
    }
}

/* ============================= PROBE BUILDING ============================= */

/// Build a default TCP socket probe for a container.
///
/// Port resolution order:
/// 1. Explicit `config.tcp_port`
/// 2. Container's first declared port
/// 3. Fallback to 8080
pub fn build_default_probe(container: &Container, config: &DefaultProbeConfig) -> Probe {
    let port = config
        .tcp_port
        .map(|p| p as i32)
        .or_else(|| {
            container
                .ports
                .as_ref()
                .and_then(|ports| ports.first())
                .map(|p| p.container_port)
        })
        .unwrap_or(8080);

    Probe {
        tcp_socket: Some(TCPSocketAction {
            port: IntOrString::Int(port),
            ..Default::default()
        }),
        initial_delay_seconds: Some(config.initial_delay_seconds.unwrap_or(5)),
        period_seconds: Some(config.period_seconds.unwrap_or(10)),
        ..Default::default()
    }
}

/* ============================= RESOURCE BUILDING ============================= */

/// Build default resource requirements from the policy configuration.
///
/// Falls back to sensible defaults if specific values aren't configured:
/// - CPU: 100m request, 500m limit
/// - Memory: 128Mi request, 256Mi limit
pub fn build_default_resources(config: &DefaultResourceConfig) -> ResourceRequirements {
    let mut requests = BTreeMap::new();
    let mut limits = BTreeMap::new();

    requests.insert(
        "cpu".to_string(),
        Quantity(config.cpu_request.clone().unwrap_or_else(|| "100m".to_string())),
    );
    requests.insert(
        "memory".to_string(),
        Quantity(config.memory_request.clone().unwrap_or_else(|| "128Mi".to_string())),
    );
    limits.insert(
        "cpu".to_string(),
        Quantity(config.cpu_limit.clone().unwrap_or_else(|| "500m".to_string())),
    );
    limits.insert(
        "memory".to_string(),
        Quantity(config.memory_limit.clone().unwrap_or_else(|| "256Mi".to_string())),
    );

    ResourceRequirements {
        requests: Some(requests),
        limits: Some(limits),
        ..Default::default()
    }
}

/* ============================= REMEDIATION PLANNING ============================= */

/// Determine what remediations are needed for a pod's violations.
///
/// Only patchable violations produce actions:
/// - Missing liveness/readiness probes → inject default TCP probe
/// - Missing resource limits → inject default requests+limits
///
/// Non-patchable violations (`:latest` tag, high restarts, pending) are skipped.
///
/// Returns `None` if no patchable remediation is needed or if the pod
/// has no resolvable parent workload.
pub fn plan_remediation(pod: &Pod, policy: &DevOpsPolicySpec) -> Option<RemediationPlan> {
    let namespace = pod.metadata.namespace.as_deref().unwrap_or_default();

    // Never enforce in protected namespaces
    if is_protected_namespace(namespace) {
        return None;
    }

    // Must have enforcement enabled
    if !is_enforcement_enabled(policy) {
        return None;
    }

    let workload = resolve_owner(pod)?;

    let containers = pod.spec.as_ref().map(|s| &s.containers).cloned().unwrap_or_default();

    let mut actions = Vec::new();

    for (i, container) in containers.iter().enumerate() {
        // Missing liveness probe (patchable)
        if policy.require_liveness_probe.unwrap_or(false) && container.liveness_probe.is_none() {
            actions.push(RemediationAction::InjectLivenessProbe { container_index: i });
        }

        // Missing readiness probe (patchable)
        if policy.require_readiness_probe.unwrap_or(false) && container.readiness_probe.is_none() {
            actions.push(RemediationAction::InjectReadinessProbe { container_index: i });
        }

        // Missing resource requests/limits (patchable)
        let has_resources = container
            .resources
            .as_ref()
            .is_some_and(|r| r.limits.is_some() || r.requests.is_some());
        if !has_resources && policy.default_resources.is_some() {
            actions.push(RemediationAction::InjectResources { container_index: i });
        }
    }

    if actions.is_empty() {
        return None;
    }

    Some(RemediationPlan { workload, actions })
}

/* ============================= PATCH GENERATION ============================= */

/// Build a JSON strategic-merge patch for a workload's pod template containers.
///
/// The patch targets `spec.template.spec.containers[i]` for each action.
pub fn build_container_patches(
    actions: &[RemediationAction],
    containers: &[Container],
    policy: &DevOpsPolicySpec,
) -> serde_json::Value {
    let probe_config = policy.default_probe.clone().unwrap_or(DefaultProbeConfig {
        tcp_port: None,
        initial_delay_seconds: None,
        period_seconds: None,
    });

    let resource_config = policy.default_resources.clone().unwrap_or(DefaultResourceConfig {
        cpu_request: None,
        cpu_limit: None,
        memory_request: None,
        memory_limit: None,
    });

    let mut container_patches: Vec<serde_json::Value> = containers
        .iter()
        .map(|c| serde_json::json!({ "name": c.name }))
        .collect();

    for action in actions {
        match action {
            RemediationAction::InjectLivenessProbe { container_index } => {
                if let Some(container) = containers.get(*container_index) {
                    let probe = build_default_probe(container, &probe_config);
                    if let Some(patch) = container_patches.get_mut(*container_index) {
                        patch["livenessProbe"] = serde_json::to_value(&probe)
                            .unwrap_or_default();
                    }
                }
            }
            RemediationAction::InjectReadinessProbe { container_index } => {
                if let Some(container) = containers.get(*container_index) {
                    let probe = build_default_probe(container, &probe_config);
                    if let Some(patch) = container_patches.get_mut(*container_index) {
                        patch["readinessProbe"] = serde_json::to_value(&probe)
                            .unwrap_or_default();
                    }
                }
            }
            RemediationAction::InjectResources { container_index } => {
                let resources = build_default_resources(&resource_config);
                if let Some(patch) = container_patches.get_mut(*container_index) {
                    patch["resources"] = serde_json::to_value(&resources)
                        .unwrap_or_default();
                }
            }
        }
    }

    serde_json::json!({
        "spec": {
            "template": {
                "metadata": {
                    "annotations": {
                        "devops.stochastic.io/patched-by": "kube-devops-operator"
                    }
                },
                "spec": {
                    "containers": container_patches
                }
            }
        }
    })
}

/* ============================= ASYNC API ============================= */

/// Apply a remediation plan to the cluster by patching the parent workload.
///
/// Patches the workload's pod template with the remediation actions,
/// then returns a result indicating success or failure.
pub async fn apply_remediation(
    plan: &RemediationPlan,
    client: &Client,
    policy: &DevOpsPolicySpec,
) -> RemediationResult {
    let containers = match get_workload_containers(plan, client).await {
        Ok(c) => c,
        Err(e) => {
            warn!(
                workload = %plan.workload.key(),
                error = %e,
                "failed_to_get_workload_containers"
            );
            return RemediationResult {
                workload: plan.workload.clone(),
                success: false,
                message: format!("Failed to read workload: {e}"),
            };
        }
    };

    let patch_body = build_container_patches(&plan.actions, &containers, policy);

    let result = match plan.workload.kind.as_str() {
        "Deployment" => {
            let api: Api<Deployment> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            api.patch(
                &plan.workload.name,
                &PatchParams::apply("kube-devops-operator"),
                &Patch::Strategic(&patch_body),
            )
            .await
            .map(|_| ())
        }
        "StatefulSet" => {
            let api: Api<StatefulSet> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            api.patch(
                &plan.workload.name,
                &PatchParams::apply("kube-devops-operator"),
                &Patch::Strategic(&patch_body),
            )
            .await
            .map(|_| ())
        }
        "DaemonSet" => {
            let api: Api<DaemonSet> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            api.patch(
                &plan.workload.name,
                &PatchParams::apply("kube-devops-operator"),
                &Patch::Strategic(&patch_body),
            )
            .await
            .map(|_| ())
        }
        other => {
            return RemediationResult {
                workload: plan.workload.clone(),
                success: false,
                message: format!("Unsupported workload kind: {other}"),
            };
        }
    };

    match result {
        Ok(()) => {
            info!(
                workload = %plan.workload.key(),
                actions = plan.actions.len(),
                "remediation_applied"
            );
            RemediationResult {
                workload: plan.workload.clone(),
                success: true,
                message: format!(
                    "Applied {} remediation(s) to {}",
                    plan.actions.len(),
                    plan.workload.key()
                ),
            }
        }
        Err(e) => {
            warn!(
                workload = %plan.workload.key(),
                error = %e,
                "remediation_failed"
            );
            RemediationResult {
                workload: plan.workload.clone(),
                success: false,
                message: format!("Patch failed: {e}"),
            }
        }
    }
}

/// Look up the containers in a workload's pod template spec.
async fn get_workload_containers(
    plan: &RemediationPlan,
    client: &Client,
) -> Result<Vec<Container>, kube::Error> {
    match plan.workload.kind.as_str() {
        "Deployment" => {
            let api: Api<Deployment> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            let dep = api.get(&plan.workload.name).await?;
            Ok(dep
                .spec
                .and_then(|s| s.template.spec)
                .map(|s| s.containers)
                .unwrap_or_default())
        }
        "StatefulSet" => {
            let api: Api<StatefulSet> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            let sts = api.get(&plan.workload.name).await?;
            Ok(sts
                .spec
                .and_then(|s| s.template.spec)
                .map(|s| s.containers)
                .unwrap_or_default())
        }
        "DaemonSet" => {
            let api: Api<DaemonSet> =
                Api::namespaced(client.clone(), &plan.workload.namespace);
            let ds = api.get(&plan.workload.name).await?;
            Ok(ds
                .spec
                .and_then(|s| s.template.spec)
                .map(|s| s.containers)
                .unwrap_or_default())
        }
        _ => Ok(vec![]),
    }
}

/// Resolve the owner of a pod via API lookup (more accurate than offline heuristic).
///
/// When a pod is owned by a ReplicaSet, this function looks up the ReplicaSet
/// to find its Deployment parent, avoiding the hash-stripping heuristic.
pub async fn resolve_owner_via_api(pod: &Pod, client: &Client) -> Option<WorkloadRef> {
    let namespace = pod.metadata.namespace.clone().unwrap_or_default();
    let owners = pod.metadata.owner_references.as_ref()?;

    for owner in owners {
        match owner.kind.as_str() {
            "Deployment" | "StatefulSet" | "DaemonSet" => {
                return Some(WorkloadRef {
                    kind: owner.kind.clone(),
                    name: owner.name.clone(),
                    namespace,
                });
            }
            "ReplicaSet" => {
                // Look up the ReplicaSet to find its Deployment parent
                let rs_api: Api<k8s_openapi::api::apps::v1::ReplicaSet> =
                    Api::namespaced(client.clone(), &namespace);
                if let Ok(rs) = rs_api.get(&owner.name).await
                    && let Some(rs_owners) = &rs.metadata.owner_references
                {
                    for rs_owner in rs_owners {
                        if rs_owner.kind == "Deployment" {
                            return Some(WorkloadRef {
                                kind: "Deployment".to_string(),
                                name: rs_owner.name.clone(),
                                namespace,
                            });
                        }
                    }
                }
                // Fallback to offline heuristic if API lookup fails
                return Some(WorkloadRef {
                    kind: "Deployment".to_string(),
                    name: strip_replicaset_hash(&owner.name),
                    namespace,
                });
            }
            _ => continue,
        }
    }

    None
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{
        Container, ContainerPort, ContainerStatus, Pod, PodSpec, PodStatus, Probe,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    fn make_enforce_policy() -> DevOpsPolicySpec {
        DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
            enforcement_mode: Some(EnforcementMode::Enforce),
            default_probe: Some(DefaultProbeConfig {
                tcp_port: None,
                initial_delay_seconds: Some(5),
                period_seconds: Some(10),
            }),
            default_resources: Some(DefaultResourceConfig {
                cpu_request: Some("100m".to_string()),
                cpu_limit: Some("500m".to_string()),
                memory_request: Some("128Mi".to_string()),
                memory_limit: Some("256Mi".to_string()),
            }),
        }
    }

    fn make_audit_policy() -> DevOpsPolicySpec {
        DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
            enforcement_mode: Some(EnforcementMode::Audit),
            default_probe: None,
            default_resources: None,
        }
    }

    fn make_pod_with_owner(
        name: &str,
        namespace: &str,
        image: &str,
        owner_kind: &str,
        owner_name: &str,
        has_liveness: bool,
        has_readiness: bool,
    ) -> Pod {
        let probes = |has: bool| -> Option<Probe> {
            if has { Some(Probe::default()) } else { None }
        };

        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                owner_references: Some(vec![OwnerReference {
                    kind: owner_kind.to_string(),
                    name: owner_name.to_string(),
                    api_version: "apps/v1".to_string(),
                    uid: "test-uid".to_string(),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "main".to_string(),
                    image: Some(image.to_string()),
                    liveness_probe: probes(has_liveness),
                    readiness_probe: probes(has_readiness),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "main".to_string(),
                    restart_count: 0,
                    ready: true,
                    image: image.to_string(),
                    image_id: String::new(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    // ── strip_replicaset_hash ──

    #[test]
    fn test_strip_hash_normal() {
        assert_eq!(strip_replicaset_hash("web-app-5d4f8b9c7f"), "web-app");
    }

    #[test]
    fn test_strip_hash_multi_dash() {
        assert_eq!(strip_replicaset_hash("my-cool-app-abc123"), "my-cool-app");
    }

    #[test]
    fn test_strip_hash_no_dash() {
        assert_eq!(strip_replicaset_hash("webapp"), "webapp");
    }

    #[test]
    fn test_strip_hash_single_segment() {
        assert_eq!(strip_replicaset_hash("app-hash"), "app");
    }

    // ── resolve_owner ──

    #[test]
    fn test_resolve_owner_deployment() {
        let pod = make_pod_with_owner("p", "default", "img:1.0", "Deployment", "web-app", true, true);
        let owner = resolve_owner(&pod);
        assert!(owner.is_some());
        let owner = owner.unwrap();
        assert_eq!(owner.kind, "Deployment");
        assert_eq!(owner.name, "web-app");
        assert_eq!(owner.namespace, "default");
    }

    #[test]
    fn test_resolve_owner_statefulset() {
        let pod = make_pod_with_owner("p", "db", "img:1.0", "StatefulSet", "mysql", true, true);
        let owner = resolve_owner(&pod).unwrap();
        assert_eq!(owner.kind, "StatefulSet");
        assert_eq!(owner.name, "mysql");
    }

    #[test]
    fn test_resolve_owner_daemonset() {
        let pod = make_pod_with_owner("p", "mon", "img:1.0", "DaemonSet", "fluent-bit", true, true);
        let owner = resolve_owner(&pod).unwrap();
        assert_eq!(owner.kind, "DaemonSet");
        assert_eq!(owner.name, "fluent-bit");
    }

    #[test]
    fn test_resolve_owner_replicaset_derives_deployment() {
        let pod = make_pod_with_owner("p", "default", "img:1.0", "ReplicaSet", "web-app-5d4f8b9c7f", true, true);
        let owner = resolve_owner(&pod).unwrap();
        assert_eq!(owner.kind, "Deployment");
        assert_eq!(owner.name, "web-app");
    }

    #[test]
    fn test_resolve_owner_no_owners() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("orphan".to_string()),
                namespace: Some("default".to_string()),
                owner_references: None,
                ..Default::default()
            },
            spec: None,
            status: None,
        };
        assert!(resolve_owner(&pod).is_none());
    }

    #[test]
    fn test_resolve_owner_unknown_kind() {
        let pod = make_pod_with_owner("p", "default", "img:1.0", "Job", "batch-job", true, true);
        assert!(resolve_owner(&pod).is_none());
    }

    // ── is_enforcement_enabled ──

    #[test]
    fn test_enforcement_enabled_when_enforce() {
        let policy = make_enforce_policy();
        assert!(is_enforcement_enabled(&policy));
    }

    #[test]
    fn test_enforcement_disabled_when_audit() {
        let policy = make_audit_policy();
        assert!(!is_enforcement_enabled(&policy));
    }

    #[test]
    fn test_enforcement_disabled_when_none() {
        let policy = DevOpsPolicySpec {
            enforcement_mode: None,
            forbid_latest_tag: None,
            require_liveness_probe: None,
            require_readiness_probe: None,
            max_restart_count: None,
            forbid_pending_duration: None,
            default_probe: None,
            default_resources: None,
        };
        assert!(!is_enforcement_enabled(&policy));
    }

    // ── is_protected_namespace ──

    #[test]
    fn test_protected_kube_system() {
        assert!(is_protected_namespace("kube-system"));
    }

    #[test]
    fn test_protected_cert_manager() {
        assert!(is_protected_namespace("cert-manager"));
    }

    #[test]
    fn test_protected_kube_prefix() {
        assert!(is_protected_namespace("kube-flannel"));
    }

    #[test]
    fn test_not_protected_default() {
        assert!(!is_protected_namespace("default"));
    }

    #[test]
    fn test_not_protected_production() {
        assert!(!is_protected_namespace("production"));
    }

    // ── build_default_probe ──

    #[test]
    fn test_probe_uses_config_port() {
        let container = Container {
            name: "main".to_string(),
            ..Default::default()
        };
        let config = DefaultProbeConfig {
            tcp_port: Some(3000),
            initial_delay_seconds: Some(10),
            period_seconds: Some(15),
        };
        let probe = build_default_probe(&container, &config);
        let tcp = probe.tcp_socket.unwrap();
        assert_eq!(tcp.port, IntOrString::Int(3000));
        assert_eq!(probe.initial_delay_seconds, Some(10));
        assert_eq!(probe.period_seconds, Some(15));
    }

    #[test]
    fn test_probe_uses_container_port() {
        let container = Container {
            name: "main".to_string(),
            ports: Some(vec![ContainerPort {
                container_port: 9090,
                ..Default::default()
            }]),
            ..Default::default()
        };
        let config = DefaultProbeConfig {
            tcp_port: None,
            initial_delay_seconds: None,
            period_seconds: None,
        };
        let probe = build_default_probe(&container, &config);
        let tcp = probe.tcp_socket.unwrap();
        assert_eq!(tcp.port, IntOrString::Int(9090));
    }

    #[test]
    fn test_probe_fallback_8080() {
        let container = Container {
            name: "main".to_string(),
            ..Default::default()
        };
        let config = DefaultProbeConfig {
            tcp_port: None,
            initial_delay_seconds: None,
            period_seconds: None,
        };
        let probe = build_default_probe(&container, &config);
        let tcp = probe.tcp_socket.unwrap();
        assert_eq!(tcp.port, IntOrString::Int(8080));
        assert_eq!(probe.initial_delay_seconds, Some(5));
        assert_eq!(probe.period_seconds, Some(10));
    }

    // ── build_default_resources ──

    #[test]
    fn test_resources_from_config() {
        let config = DefaultResourceConfig {
            cpu_request: Some("200m".to_string()),
            cpu_limit: Some("1".to_string()),
            memory_request: Some("256Mi".to_string()),
            memory_limit: Some("512Mi".to_string()),
        };
        let resources = build_default_resources(&config);
        let requests = resources.requests.unwrap();
        let limits = resources.limits.unwrap();
        assert_eq!(requests["cpu"].0, "200m");
        assert_eq!(limits["memory"].0, "512Mi");
    }

    #[test]
    fn test_resources_defaults() {
        let config = DefaultResourceConfig {
            cpu_request: None,
            cpu_limit: None,
            memory_request: None,
            memory_limit: None,
        };
        let resources = build_default_resources(&config);
        let requests = resources.requests.unwrap();
        let limits = resources.limits.unwrap();
        assert_eq!(requests["cpu"].0, "100m");
        assert_eq!(requests["memory"].0, "128Mi");
        assert_eq!(limits["cpu"].0, "500m");
        assert_eq!(limits["memory"].0, "256Mi");
    }

    // ── plan_remediation ──

    #[test]
    fn test_plan_missing_probes() {
        let pod = make_pod_with_owner("p", "prod", "img:1.0", "ReplicaSet", "web-abc123", false, false);
        let policy = make_enforce_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.workload.kind, "Deployment");
        assert_eq!(plan.workload.name, "web");
        assert!(plan.actions.iter().any(|a| matches!(a, RemediationAction::InjectLivenessProbe { .. })));
        assert!(plan.actions.iter().any(|a| matches!(a, RemediationAction::InjectReadinessProbe { .. })));
    }

    #[test]
    fn test_plan_missing_resources() {
        let pod = make_pod_with_owner("p", "prod", "img:1.0", "Deployment", "api", true, true);
        let policy = make_enforce_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(plan.actions.iter().any(|a| matches!(a, RemediationAction::InjectResources { .. })));
    }

    #[test]
    fn test_plan_compliant_pod_returns_none() {
        let mut pod = make_pod_with_owner("p", "prod", "img:1.0", "Deployment", "api", true, true);
        // Add resources so the pod is fully compliant
        if let Some(spec) = &mut pod.spec {
            spec.containers[0].resources = Some(ResourceRequirements {
                requests: Some(BTreeMap::from([
                    ("cpu".to_string(), Quantity("100m".to_string())),
                    ("memory".to_string(), Quantity("128Mi".to_string())),
                ])),
                limits: Some(BTreeMap::from([
                    ("cpu".to_string(), Quantity("500m".to_string())),
                    ("memory".to_string(), Quantity("256Mi".to_string())),
                ])),
                ..Default::default()
            });
        }
        let policy = make_enforce_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_none());
    }

    #[test]
    fn test_plan_audit_mode_returns_none() {
        let pod = make_pod_with_owner("p", "prod", "img:1.0", "Deployment", "api", false, false);
        let policy = make_audit_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_none());
    }

    #[test]
    fn test_plan_protected_namespace_returns_none() {
        let pod = make_pod_with_owner("p", "kube-system", "img:1.0", "DaemonSet", "kube-proxy", false, false);
        let policy = make_enforce_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_none());
    }

    #[test]
    fn test_plan_no_owner_returns_none() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("orphan".to_string()),
                namespace: Some("prod".to_string()),
                owner_references: None,
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "main".to_string(),
                    image: Some("img:1.0".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "main".to_string(),
                    restart_count: 0,
                    ready: true,
                    image: "img:1.0".to_string(),
                    image_id: String::new(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        };
        let policy = make_enforce_policy();
        let plan = plan_remediation(&pod, &policy);
        assert!(plan.is_none());
    }

    #[test]
    fn test_plan_latest_tag_not_patchable() {
        // Pod only has :latest tag violation (probes present, resources configured
        // via no default_resources in policy)
        let mut pod = make_pod_with_owner("p", "prod", "img:latest", "Deployment", "api", true, true);
        if let Some(spec) = &mut pod.spec {
            spec.containers[0].resources = Some(ResourceRequirements {
                requests: Some(BTreeMap::from([
                    ("cpu".to_string(), Quantity("100m".to_string())),
                ])),
                limits: Some(BTreeMap::from([
                    ("cpu".to_string(), Quantity("500m".to_string())),
                ])),
                ..Default::default()
            });
        }
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: None,
            forbid_pending_duration: None,
            enforcement_mode: Some(EnforcementMode::Enforce),
            default_probe: None,
            default_resources: None,
        };
        let plan = plan_remediation(&pod, &policy);
        // :latest is not patchable, and probes are present → no remediation plan
        assert!(plan.is_none());
    }

    // ── build_container_patches ──

    #[test]
    fn test_patch_includes_annotation() {
        let containers = vec![Container {
            name: "main".to_string(),
            ..Default::default()
        }];
        let actions = vec![RemediationAction::InjectLivenessProbe { container_index: 0 }];
        let policy = make_enforce_policy();
        let patch = build_container_patches(&actions, &containers, &policy);

        let annotation = &patch["spec"]["template"]["metadata"]["annotations"]["devops.stochastic.io/patched-by"];
        assert_eq!(annotation, "kube-devops-operator");
    }

    #[test]
    fn test_patch_includes_liveness_probe() {
        let containers = vec![Container {
            name: "main".to_string(),
            ..Default::default()
        }];
        let actions = vec![RemediationAction::InjectLivenessProbe { container_index: 0 }];
        let policy = make_enforce_policy();
        let patch = build_container_patches(&actions, &containers, &policy);

        let container_patch = &patch["spec"]["template"]["spec"]["containers"][0];
        assert!(container_patch.get("livenessProbe").is_some());
        assert_eq!(container_patch["name"], "main");
    }

    #[test]
    fn test_patch_includes_resources() {
        let containers = vec![Container {
            name: "app".to_string(),
            ..Default::default()
        }];
        let actions = vec![RemediationAction::InjectResources { container_index: 0 }];
        let policy = make_enforce_policy();
        let patch = build_container_patches(&actions, &containers, &policy);

        let container_patch = &patch["spec"]["template"]["spec"]["containers"][0];
        assert!(container_patch.get("resources").is_some());
    }

    #[test]
    fn test_patch_multiple_actions() {
        let containers = vec![Container {
            name: "main".to_string(),
            ..Default::default()
        }];
        let actions = vec![
            RemediationAction::InjectLivenessProbe { container_index: 0 },
            RemediationAction::InjectReadinessProbe { container_index: 0 },
            RemediationAction::InjectResources { container_index: 0 },
        ];
        let policy = make_enforce_policy();
        let patch = build_container_patches(&actions, &containers, &policy);

        let container_patch = &patch["spec"]["template"]["spec"]["containers"][0];
        assert!(container_patch.get("livenessProbe").is_some());
        assert!(container_patch.get("readinessProbe").is_some());
        assert!(container_patch.get("resources").is_some());
    }

    // ── WorkloadRef ──

    #[test]
    fn test_workload_ref_key() {
        let wr = WorkloadRef {
            kind: "Deployment".to_string(),
            name: "web-app".to_string(),
            namespace: "production".to_string(),
        };
        assert_eq!(wr.key(), "deployment/production/web-app");
    }

    #[test]
    fn test_workload_ref_equality() {
        let a = WorkloadRef {
            kind: "Deployment".to_string(),
            name: "app".to_string(),
            namespace: "default".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
