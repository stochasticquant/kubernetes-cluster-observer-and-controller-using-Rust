mod common;

use common::{make_test_pod, make_test_pod_with_owner};
use kube_devops::crd::{
    DefaultProbeConfig, DefaultResourceConfig, DevOpsPolicySpec, EnforcementMode,
};
use kube_devops::enforcement;

// ══════════════════════════════════════════════════════════════════
// Enforcement integration tests (no cluster required)
//
// Exercises the full enforcement pipeline: detection → plan →
// verify plan correctness, audit vs enforce mode, system namespace
// protection, and deduplication.
// ══════════════════════════════════════════════════════════════════

fn enforce_policy() -> DevOpsPolicySpec {
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
        ..Default::default()
    }
}

fn audit_policy() -> DevOpsPolicySpec {
    DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        require_readiness_probe: Some(true),
        max_restart_count: Some(3),
        forbid_pending_duration: Some(300),
        enforcement_mode: Some(EnforcementMode::Audit),
        ..Default::default()
    }
}

// ── Full enforcement pipeline ──

#[test]
fn test_enforcement_pipeline_detect_plan_verify() {
    // Pod missing both probes, owned by a Deployment via ReplicaSet
    let pod = make_test_pod_with_owner(
        "web-pod-abc",
        "production",
        "nginx:1.25",
        "ReplicaSet",
        "web-app-5d4f8b9c7f",
        false,
        false,
    );

    let policy = enforce_policy();

    // Step 1: detect violations
    let violations = kube_devops::governance::detect_violations_with_policy(&pod, &policy);
    assert!(violations.contains(&"missing_liveness"));
    assert!(violations.contains(&"missing_readiness"));

    // Step 2: plan remediation
    let plan = enforcement::plan_remediation(&pod, &policy);
    assert!(plan.is_some());
    let plan = plan.unwrap();

    // Step 3: verify plan correctness
    assert_eq!(plan.workload.kind, "Deployment");
    assert_eq!(plan.workload.name, "web-app");
    assert_eq!(plan.workload.namespace, "production");
    assert!(plan.actions.len() >= 2); // at least liveness + readiness
}

#[test]
fn test_audit_mode_never_produces_plans() {
    let pod = make_test_pod_with_owner(
        "web-pod",
        "production",
        "nginx:latest",
        "ReplicaSet",
        "web-app-abc123",
        false,
        false,
    );

    let policy = audit_policy();
    let plan = enforcement::plan_remediation(&pod, &policy);
    assert!(
        plan.is_none(),
        "Audit mode should never produce remediation plans"
    );
}

#[test]
fn test_system_namespace_protection() {
    let namespaces = vec![
        "kube-system",
        "kube-flannel",
        "cert-manager",
        "istio-system",
        "monitoring",
    ];

    let policy = enforce_policy();

    for ns in namespaces {
        let pod = make_test_pod_with_owner(
            "sys-pod",
            ns,
            "img:1.0",
            "DaemonSet",
            "daemon",
            false,
            false,
        );
        let plan = enforcement::plan_remediation(&pod, &policy);
        assert!(
            plan.is_none(),
            "Should not enforce in protected namespace: {ns}"
        );
    }
}

#[test]
fn test_enforcement_deduplication_by_workload() {
    let policy = enforce_policy();

    // Two pods from the same Deployment (same ReplicaSet)
    let pod_a = make_test_pod_with_owner(
        "web-pod-a",
        "production",
        "nginx:1.25",
        "ReplicaSet",
        "web-app-5d4f8b9c7f",
        false,
        false,
    );
    let pod_b = make_test_pod_with_owner(
        "web-pod-b",
        "production",
        "nginx:1.25",
        "ReplicaSet",
        "web-app-5d4f8b9c7f",
        false,
        false,
    );

    let plan_a = enforcement::plan_remediation(&pod_a, &policy).unwrap();
    let plan_b = enforcement::plan_remediation(&pod_b, &policy).unwrap();

    // Both plans target the same workload
    assert_eq!(plan_a.workload.key(), plan_b.workload.key());

    // The reconcile loop should deduplicate — simulate that logic
    let mut seen = std::collections::HashSet::new();
    let mut unique_plans = Vec::new();

    for plan in [plan_a, plan_b] {
        if seen.insert(plan.workload.key()) {
            unique_plans.push(plan);
        }
    }

    assert_eq!(unique_plans.len(), 1, "Should deduplicate to one workload");
}

#[test]
fn test_enforcement_skips_pods_without_owners() {
    let pod = make_test_pod(
        "orphan",
        "production",
        "nginx:1.25",
        false,
        false,
        0,
        "Running",
    );

    let policy = enforce_policy();
    let plan = enforcement::plan_remediation(&pod, &policy);
    assert!(
        plan.is_none(),
        "Pods without owners should not be remediated"
    );
}

#[test]
fn test_enforcement_patch_structure() {
    let pod = make_test_pod_with_owner(
        "web-pod",
        "production",
        "nginx:1.25",
        "Deployment",
        "web-app",
        false,
        false,
    );

    let policy = enforce_policy();
    let plan = enforcement::plan_remediation(&pod, &policy).unwrap();

    let containers = pod.spec.unwrap().containers;
    let patch = enforcement::build_container_patches(&plan.actions, &containers, &policy);

    // Verify patch structure
    assert!(
        patch["spec"]["template"]["metadata"]["annotations"]["devops.stochastic.io/patched-by"]
            .is_string()
    );
    assert!(patch["spec"]["template"]["spec"]["containers"].is_array());

    let container_patch = &patch["spec"]["template"]["spec"]["containers"][0];
    assert_eq!(container_patch["name"], "main");
    assert!(container_patch.get("livenessProbe").is_some());
    assert!(container_patch.get("readinessProbe").is_some());
}

#[test]
fn test_enforcement_none_mode_same_as_audit() {
    let pod = make_test_pod_with_owner(
        "web-pod",
        "production",
        "nginx:1.25",
        "Deployment",
        "web-app",
        false,
        false,
    );

    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        require_readiness_probe: Some(true),
        ..Default::default() // enforcement_mode: None = same as audit
    };

    let plan = enforcement::plan_remediation(&pod, &policy);
    assert!(
        plan.is_none(),
        "enforcement_mode: None should behave like audit"
    );
}

#[test]
fn test_enforcement_multiple_workload_types() {
    let policy = enforce_policy();

    // Deployment (via ReplicaSet)
    let dep_pod = make_test_pod_with_owner(
        "dep-pod",
        "prod",
        "nginx:1.25",
        "ReplicaSet",
        "web-app-abc123",
        false,
        false,
    );
    let dep_plan = enforcement::plan_remediation(&dep_pod, &policy).unwrap();
    assert_eq!(dep_plan.workload.kind, "Deployment");

    // StatefulSet
    let sts_pod = make_test_pod_with_owner(
        "sts-pod",
        "prod",
        "postgres:15",
        "StatefulSet",
        "db",
        false,
        false,
    );
    let sts_plan = enforcement::plan_remediation(&sts_pod, &policy).unwrap();
    assert_eq!(sts_plan.workload.kind, "StatefulSet");

    // DaemonSet
    let ds_pod = make_test_pod_with_owner(
        "ds-pod",
        "prod",
        "fluent-bit:2.0",
        "DaemonSet",
        "logger",
        false,
        false,
    );
    let ds_plan = enforcement::plan_remediation(&ds_pod, &policy).unwrap();
    assert_eq!(ds_plan.workload.kind, "DaemonSet");
}
