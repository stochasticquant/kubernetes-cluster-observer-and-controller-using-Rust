mod common;

use common::make_test_pod;
use kube_devops::crd::{DevOpsPolicySpec, DevOpsPolicyStatus};
use kube_devops::governance;

// ══════════════════════════════════════════════════════════════════
// Operator integration tests (no cluster required)
//
// Exercises the full Step 5 pipeline: policy + pods → policy-aware
// evaluation → aggregate → score → status construction.
// ══════════════════════════════════════════════════════════════════

fn all_enabled_policy() -> DevOpsPolicySpec {
    DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        require_readiness_probe: Some(true),
        max_restart_count: Some(3),
        forbid_pending_duration: Some(300),
        ..Default::default()
    }
}

fn empty_policy() -> DevOpsPolicySpec {
    DevOpsPolicySpec::default()
}

/// Simulate a full reconcile cycle: evaluate pods, aggregate, score, build status.
fn simulate_reconcile(
    pods: &[k8s_openapi::api::core::v1::Pod],
    policy: &DevOpsPolicySpec,
) -> DevOpsPolicyStatus {
    let mut aggregate = governance::PodMetrics::default();
    let mut total_violations: u32 = 0;

    for pod in pods {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }
        let contribution = governance::evaluate_pod_with_policy(pod, policy);
        governance::add_metrics(&mut aggregate, &contribution);
        let v = governance::detect_violations_with_policy(pod, policy);
        total_violations += v.len() as u32;
    }

    let health_score = governance::calculate_health_score(&aggregate);
    let classification = governance::classify_health(health_score);
    let healthy = health_score >= 80;

    let message = format!(
        "{} violations across {} pods — {} ({})",
        total_violations, aggregate.total_pods, classification, health_score
    );

    DevOpsPolicyStatus {
        observed_generation: Some(1),
        healthy: Some(healthy),
        health_score: Some(health_score),
        violations: Some(total_violations),
        last_evaluated: Some("2026-02-22T00:00:00Z".to_string()),
        message: Some(message),
        remediations_applied: None,
        remediations_failed: None,
        remediated_workloads: None,
    }
}

// ── Full reconcile pipeline ──

#[test]
fn test_reconcile_all_compliant_pods() {
    let pods = vec![
        make_test_pod("a", "production", "nginx:1.25", true, true, 0, "Running"),
        make_test_pod("b", "production", "redis:7.0", true, true, 0, "Running"),
        make_test_pod("c", "production", "app:2.0", true, true, 0, "Running"),
    ];

    let status = simulate_reconcile(&pods, &all_enabled_policy());

    assert_eq!(status.health_score, Some(100));
    assert_eq!(status.healthy, Some(true));
    assert_eq!(status.violations, Some(0));
    assert!(status.message.as_ref().unwrap().contains("Healthy"));
}

#[test]
fn test_reconcile_mixed_compliance() {
    let pods = vec![
        make_test_pod("good", "staging", "nginx:1.25", true, true, 0, "Running"),
        make_test_pod("bad", "staging", "nginx:latest", false, false, 0, "Running"),
    ];

    let status = simulate_reconcile(&pods, &all_enabled_policy());

    assert!(status.violations.unwrap() > 0);
    assert!(status.health_score.unwrap() < 100);
}

#[test]
fn test_reconcile_all_noncompliant_pods() {
    let pods = vec![
        make_test_pod("a", "dev", "img:latest", false, false, 10, "Pending"),
        make_test_pod("b", "dev", "img:latest", false, false, 8, "Pending"),
    ];

    let status = simulate_reconcile(&pods, &all_enabled_policy());

    assert_eq!(status.healthy, Some(false));
    assert!(status.health_score.unwrap() < 80);
    assert!(status.violations.unwrap() >= 6); // each pod has at least 3 violations
}

#[test]
fn test_reconcile_empty_namespace() {
    let pods: Vec<k8s_openapi::api::core::v1::Pod> = vec![];

    let status = simulate_reconcile(&pods, &all_enabled_policy());

    assert_eq!(status.health_score, Some(100));
    assert_eq!(status.healthy, Some(true));
    assert_eq!(status.violations, Some(0));
}

#[test]
fn test_reconcile_empty_policy_skips_all_checks() {
    let pods = vec![make_test_pod(
        "bad",
        "prod",
        "img:latest",
        false,
        false,
        10,
        "Pending",
    )];

    let status = simulate_reconcile(&pods, &empty_policy());

    // Empty policy means no checks → no violations → score 100
    assert_eq!(status.health_score, Some(100));
    assert_eq!(status.violations, Some(0));
    assert_eq!(status.healthy, Some(true));
}

#[test]
fn test_reconcile_system_namespace_excluded() {
    let pods = vec![
        make_test_pod(
            "sys",
            "kube-system",
            "img:latest",
            false,
            false,
            10,
            "Pending",
        ),
        make_test_pod("app", "production", "nginx:1.25", true, true, 0, "Running"),
    ];

    let status = simulate_reconcile(&pods, &all_enabled_policy());

    // kube-system pod should be skipped, only production counted
    assert_eq!(status.health_score, Some(100));
    assert_eq!(status.violations, Some(0));
}

// ── Policy update simulation ──

#[test]
fn test_policy_change_affects_score() {
    let pods = vec![make_test_pod(
        "a",
        "prod",
        "nginx:latest",
        false,
        true,
        0,
        "Running",
    )];

    // Strict policy: catches latest_tag + missing_liveness
    let strict = all_enabled_policy();
    let status_strict = simulate_reconcile(&pods, &strict);

    // Relaxed policy: only checks readiness (which the pod has)
    let relaxed = DevOpsPolicySpec {
        require_readiness_probe: Some(true),
        ..empty_policy()
    };

    let status_relaxed = simulate_reconcile(&pods, &relaxed);

    assert!(status_strict.violations.unwrap() > status_relaxed.violations.unwrap());
    assert!(status_relaxed.health_score.unwrap() > status_strict.health_score.unwrap());
}

#[test]
fn test_custom_restart_threshold() {
    let pods = vec![make_test_pod(
        "a",
        "prod",
        "nginx:1.25",
        true,
        true,
        5,
        "Running",
    )];

    // Threshold 3: restart_count=5 exceeds → violation
    let low_threshold = DevOpsPolicySpec {
        max_restart_count: Some(3),
        ..empty_policy()
    };
    let status_low = simulate_reconcile(&pods, &low_threshold);

    // Threshold 10: restart_count=5 is under → no violation
    let high_threshold = DevOpsPolicySpec {
        max_restart_count: Some(10),
        ..empty_policy()
    };
    let status_high = simulate_reconcile(&pods, &high_threshold);

    assert!(status_low.violations.unwrap() > 0);
    assert_eq!(status_high.violations, Some(0));
}

// ── Status message format ──

#[test]
fn test_status_message_contains_classification() {
    let pods = vec![make_test_pod(
        "a",
        "prod",
        "nginx:1.25",
        true,
        true,
        0,
        "Running",
    )];
    let status = simulate_reconcile(&pods, &all_enabled_policy());
    let msg = status.message.unwrap();

    assert!(
        msg.contains("Healthy")
            || msg.contains("Stable")
            || msg.contains("Degraded")
            || msg.contains("Critical")
    );
    assert!(msg.contains("violations"));
    assert!(msg.contains("pods"));
}

#[test]
fn test_status_message_contains_counts() {
    let pods = vec![
        make_test_pod("a", "prod", "nginx:latest", false, false, 0, "Running"),
        make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
    ];
    let status = simulate_reconcile(&pods, &all_enabled_policy());
    let msg = status.message.unwrap();

    // Should contain "X violations across 2 pods"
    assert!(msg.contains("2 pods"));
}

// ── Status fields ──

#[test]
fn test_status_observed_generation_set() {
    let pods = vec![];
    let status = simulate_reconcile(&pods, &all_enabled_policy());
    assert_eq!(status.observed_generation, Some(1));
}

#[test]
fn test_status_last_evaluated_set() {
    let pods = vec![];
    let status = simulate_reconcile(&pods, &all_enabled_policy());
    assert!(status.last_evaluated.is_some());
}

// ── CRD schema ──

#[test]
fn test_crd_schema_round_trip() {
    use kube::CustomResourceExt;
    use kube_devops::crd::DevOpsPolicy;

    let crd = DevOpsPolicy::crd();
    let json = serde_json::to_string(&crd).expect("CRD should serialize to JSON");
    assert!(json.contains("DevOpsPolicy"));
    assert!(json.contains("devops.stochastic.io"));
    assert!(json.contains("forbidLatestTag"));
    assert!(json.contains("healthScore"));
}
