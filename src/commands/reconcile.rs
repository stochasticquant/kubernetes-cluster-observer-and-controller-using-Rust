use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::{Action, Controller};
use kube::{Client, ResourceExt};
use k8s_openapi::api::core::v1::Pod;
use prometheus::{IntCounter, IntGaugeVec, Registry};
use tokio::signal;
use tracing::{info, warn};

use kube_devops::crd::{DevOpsPolicy, DevOpsPolicyStatus};
use kube_devops::governance;

/* ============================= CONFIG ============================= */

const FINALIZER: &str = "devops.stochastic.io/cleanup";
const REQUEUE_INTERVAL: Duration = Duration::from_secs(30);

/* ============================= PROMETHEUS ============================= */

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

static RECONCILE_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_reconcile_total",
        "Total DevOpsPolicy reconciliation cycles",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static RECONCILE_ERRORS: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_reconcile_errors_total",
        "Total DevOpsPolicy reconciliation errors",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static POLICY_VIOLATIONS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_violations_total",
            "Policy violations per namespace and policy",
        ),
        &["namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

static POLICY_HEALTH: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_health_score",
            "Health score per namespace and policy",
        ),
        &["namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

/* ============================= CONTEXT ============================= */

struct ReconcileContext {
    client: Client,
}

/* ============================= ENTRY ============================= */

pub async fn run() -> Result<()> {
    println!("Starting DevOpsPolicy operator...\n");

    let client = Client::try_default()
        .await
        .context("Failed to load kubeconfig")?;

    // Verify actual cluster connectivity before starting the controller
    print!("  Cluster connection .......... ");
    match client.apiserver_version().await {
        Ok(v) => println!("OK (v{}.{})", v.major, v.minor),
        Err(e) => {
            println!("FAIL");
            anyhow::bail!(
                "Cannot reach cluster: {}. Is the cluster running?",
                e
            );
        }
    }

    let policies: Api<DevOpsPolicy> = Api::all(client.clone());
    let pods: Api<Pod> = Api::all(client.clone());

    let ctx = Arc::new(ReconcileContext {
        client: client.clone(),
    });

    // Force-init Prometheus metrics so they appear on /metrics
    LazyLock::force(&RECONCILE_TOTAL);
    LazyLock::force(&RECONCILE_ERRORS);
    LazyLock::force(&POLICY_VIOLATIONS);
    LazyLock::force(&POLICY_HEALTH);

    println!("  CRD watch ................... DevOpsPolicy.devops.stochastic.io/v1");
    println!("  Requeue interval ............ {}s", REQUEUE_INTERVAL.as_secs());
    println!("\nOperator running. Press Ctrl+C to stop.\n");
    println!("{}", "=".repeat(70));

    info!("operator_controller_started");

    let controller = Controller::new(policies, Default::default())
        .owns(pods, Default::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|result| async move {
            match result {
                Ok((_obj, _action)) => {}
                Err(e) => {
                    warn!(error = %e, "reconcile_dispatch_error");
                    eprintln!("[ERROR] Reconcile dispatch: {e}");
                }
            }
        });

    tokio::select! {
        _ = controller => {
            info!("operator_controller_stream_ended");
            println!("\nController stream ended unexpectedly.");
        }
        _ = signal::ctrl_c() => {
            info!("shutdown_signal_received");
            println!("\n{}", "=".repeat(70));
            println!("Shutdown signal received. Stopping operator...");
            println!("{}", "=".repeat(70));
        }
    }

    info!("operator_stopped");
    println!("Operator stopped.");

    Ok(())
}

/* ============================= RECONCILE ============================= */

async fn reconcile(
    policy: Arc<DevOpsPolicy>,
    ctx: Arc<ReconcileContext>,
) -> std::result::Result<Action, kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let generation = policy.metadata.generation;

    // ── Skip if already reconciled this generation ──
    let already_reconciled = policy
        .status
        .as_ref()
        .and_then(|s| s.observed_generation)
        == generation;

    if already_reconciled {
        return Ok(Action::requeue(REQUEUE_INTERVAL));
    }

    RECONCILE_TOTAL.inc();

    info!(
        policy = %name,
        namespace = %namespace,
        "reconcile_start"
    );

    // ── Handle deletion with finalizer ──
    if policy.metadata.deletion_timestamp.is_some() {
        return handle_deletion(&policy, &ctx.client).await;
    }

    // ── Ensure finalizer is present ──
    if !has_finalizer(&policy) {
        add_finalizer(&policy, &ctx.client).await?;
    }

    // ── List pods in the policy's namespace ──
    let pods_api: Api<Pod> = Api::namespaced(ctx.client.clone(), &namespace);
    let pod_list = pods_api.list(&Default::default()).await?;

    // ── Evaluate pods against the policy spec ──
    let mut aggregate = governance::PodMetrics::default();
    let mut total_violations: u32 = 0;

    for pod in &pod_list.items {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }

        let contribution = governance::evaluate_pod_with_policy(pod, &policy.spec);
        governance::add_metrics(&mut aggregate, &contribution);

        let violations = governance::detect_violations_with_policy(pod, &policy.spec);
        total_violations += violations.len() as u32;
    }

    let health_score = governance::calculate_health_score(&aggregate);
    let classification = governance::classify_health(health_score);
    let healthy = health_score >= 80;

    let message = format!(
        "{} violations across {} pods — {} ({})",
        total_violations, aggregate.total_pods, classification, health_score
    );

    // ── Print human-readable summary ──
    let now = chrono::Utc::now();
    let timestamp = now.format("%H:%M:%S");

    println!(
        "[{timestamp}] {namespace}/{name}: {classification} — score {health_score}/100, \
         {total_violations} violations, {pods} pods",
        pods = aggregate.total_pods
    );

    info!(
        policy = %name,
        namespace = %namespace,
        health_score,
        violations = total_violations,
        pods = aggregate.total_pods,
        classification,
        "reconcile_evaluated"
    );

    // ── Update Prometheus metrics ──
    POLICY_VIOLATIONS
        .with_label_values(&[&namespace, &name])
        .set(total_violations as i64);
    POLICY_HEALTH
        .with_label_values(&[&namespace, &name])
        .set(health_score as i64);

    // ── Update status sub-resource ──
    let status = DevOpsPolicyStatus {
        observed_generation: generation,
        healthy: Some(healthy),
        health_score: Some(health_score),
        violations: Some(total_violations),
        last_evaluated: Some(now.to_rfc3339()),
        message: Some(message),
    };

    let status_patch = serde_json::json!({ "status": status });
    let policies_api: Api<DevOpsPolicy> = Api::namespaced(ctx.client.clone(), &namespace);

    policies_api
        .patch_status(
            &name,
            &PatchParams::apply("kube-devops-operator"),
            &Patch::Merge(&status_patch),
        )
        .await?;

    info!(
        policy = %name,
        namespace = %namespace,
        "status_updated"
    );

    Ok(Action::requeue(REQUEUE_INTERVAL))
}

/* ============================= ERROR POLICY ============================= */

fn error_policy(
    _policy: Arc<DevOpsPolicy>,
    error: &kube::Error,
    _ctx: Arc<ReconcileContext>,
) -> Action {
    RECONCILE_ERRORS.inc();
    warn!(error = %error, "reconcile_error");
    Action::requeue(Duration::from_secs(60))
}

/* ============================= FINALIZER ============================= */

fn has_finalizer(policy: &DevOpsPolicy) -> bool {
    policy
        .metadata
        .finalizers
        .as_ref()
        .is_some_and(|f| f.iter().any(|s| s == FINALIZER))
}

async fn add_finalizer(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<(), kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let api: Api<DevOpsPolicy> = Api::namespaced(client.clone(), &namespace);

    let patch = serde_json::json!({
        "metadata": {
            "finalizers": [FINALIZER]
        }
    });

    api.patch(
        &name,
        &PatchParams::apply("kube-devops-operator"),
        &Patch::Merge(&patch),
    )
    .await?;

    info!(policy = %name, "finalizer_added");
    Ok(())
}

async fn remove_finalizer(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<(), kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let api: Api<DevOpsPolicy> = Api::namespaced(client.clone(), &namespace);

    let patch = serde_json::json!({
        "metadata": {
            "finalizers": []
        }
    });

    api.patch(
        &name,
        &PatchParams::apply("kube-devops-operator"),
        &Patch::Merge(&patch),
    )
    .await?;

    info!(policy = %name, "finalizer_removed");
    Ok(())
}

async fn handle_deletion(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<Action, kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();

    info!(policy = %name, namespace = %namespace, "handling_deletion");

    // Clear Prometheus metrics for this policy
    let _ = POLICY_VIOLATIONS.remove_label_values(&[&namespace, &name]);
    let _ = POLICY_HEALTH.remove_label_values(&[&namespace, &name]);

    if has_finalizer(policy) {
        remove_finalizer(policy, client).await?;
    }

    Ok(Action::await_change())
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;
    use kube_devops::crd::DevOpsPolicySpec;
    use k8s_openapi::api::core::v1::{Container, ContainerStatus, PodSpec, PodStatus, Probe};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn make_test_pod(
        name: &str,
        namespace: &str,
        image: &str,
        has_liveness: bool,
        has_readiness: bool,
        restart_count: i32,
        phase: &str,
    ) -> Pod {
        let probes = |has: bool| -> Option<Probe> {
            if has {
                Some(Probe::default())
            } else {
                None
            }
        };

        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
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
                phase: Some(phase.to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "main".to_string(),
                    restart_count,
                    ready: phase == "Running",
                    image: image.to_string(),
                    image_id: String::new(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn all_enabled_policy() -> DevOpsPolicySpec {
        DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
        }
    }

    // ── Reconcile status computation ──

    #[test]
    fn test_status_healthy_at_80() {
        let score: u32 = 80;
        let healthy = score >= 80;
        assert!(healthy);
    }

    #[test]
    fn test_status_unhealthy_at_79() {
        let score: u32 = 79;
        let healthy = score >= 80;
        assert!(!healthy);
    }

    #[test]
    fn test_aggregate_multiple_pods() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod("a", "prod", "nginx:latest", false, false, 0, "Running"),
            make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
            make_test_pod("c", "prod", "nginx:latest", true, false, 10, "Pending"),
        ];

        let mut aggregate = governance::PodMetrics::default();
        let mut total_violations: u32 = 0;

        for pod in &pods {
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
            let v = governance::detect_violations_with_policy(pod, &policy);
            total_violations += v.len() as u32;
        }

        assert_eq!(aggregate.total_pods, 3);
        assert_eq!(aggregate.latest_tag, 2);
        assert_eq!(aggregate.missing_liveness, 1);
        assert_eq!(aggregate.missing_readiness, 2);
        assert!(aggregate.high_restarts > 0);
        assert_eq!(aggregate.pending, 1);
        assert!(total_violations > 0);
    }

    #[test]
    fn test_aggregate_all_compliant_pods() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod("a", "prod", "nginx:1.25", true, true, 0, "Running"),
            make_test_pod("b", "prod", "redis:7.0", true, true, 0, "Running"),
        ];

        let mut aggregate = governance::PodMetrics::default();
        let mut total_violations: u32 = 0;

        for pod in &pods {
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
            let v = governance::detect_violations_with_policy(pod, &policy);
            total_violations += v.len() as u32;
        }

        assert_eq!(aggregate.total_pods, 2);
        assert_eq!(total_violations, 0);

        let score = governance::calculate_health_score(&aggregate);
        assert_eq!(score, 100);
    }

    #[test]
    fn test_system_namespace_pods_skipped() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod("a", "kube-system", "nginx:latest", false, false, 0, "Running"),
            make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
        ];

        let mut aggregate = governance::PodMetrics::default();

        for pod in &pods {
            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
            if governance::is_system_namespace(ns) {
                continue;
            }
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
        }

        // Only the "prod" pod should be counted
        assert_eq!(aggregate.total_pods, 1);
        assert_eq!(aggregate.latest_tag, 0);
    }

    #[test]
    fn test_status_message_format() {
        let total_violations: u32 = 5;
        let total_pods: u32 = 10;
        let health_score: u32 = 72;
        let classification = governance::classify_health(health_score);

        let message = format!(
            "{} violations across {} pods — {} ({})",
            total_violations, total_pods, classification, health_score
        );

        assert_eq!(message, "5 violations across 10 pods — Stable (72)");
    }

    #[test]
    fn test_status_fields_populated() {
        let status = DevOpsPolicyStatus {
            observed_generation: Some(3),
            healthy: Some(true),
            health_score: Some(95),
            violations: Some(2),
            last_evaluated: Some("2026-01-01T00:00:00Z".to_string()),
            message: Some("2 violations across 20 pods — Healthy (95)".to_string()),
        };

        assert_eq!(status.observed_generation, Some(3));
        assert!(status.healthy.unwrap());
        assert_eq!(status.health_score, Some(95));
        assert_eq!(status.violations, Some(2));
        assert!(status.last_evaluated.is_some());
        assert!(status.message.unwrap().contains("Healthy"));
    }

    // ── Finalizer detection ──

    #[test]
    fn test_has_finalizer_when_present() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec![FINALIZER.to_string()]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_when_absent() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: None,
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_with_other_finalizers() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec!["some-other/finalizer".to_string()]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_empty_list() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec![]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    // ── Deletion detection ──

    #[test]
    fn test_deletion_timestamp_present() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                deletion_timestamp: Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
                    chrono::Utc::now(),
                )),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(policy.metadata.deletion_timestamp.is_some());
    }

    #[test]
    fn test_deletion_timestamp_absent() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                deletion_timestamp: None,
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(policy.metadata.deletion_timestamp.is_none());
    }
}
