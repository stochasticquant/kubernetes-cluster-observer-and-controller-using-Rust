mod common;

use common::make_test_pod;
use kube_devops::bundles;
use kube_devops::crd::{
    AuditViolation, DevOpsPolicySpec, PolicyAuditResultSpec, Severity, SeverityOverrides,
};
use kube_devops::governance;
use kube_devops::multi_cluster::{ClusterEvaluation, aggregate_report};

// ══════════════════════════════════════════════════════════════════
// Multi-cluster and audit result integration tests (no cluster required)
//
// Tests the full pipeline: bundle → evaluate pods → audit result
// construction → multi-cluster report aggregation.
// ══════════════════════════════════════════════════════════════════

fn evaluate_pods_synthetic(
    context_name: &str,
    pods: &[k8s_openapi::api::core::v1::Pod],
    policy: &DevOpsPolicySpec,
) -> ClusterEvaluation {
    let mut aggregate = governance::PodMetrics::default();
    let mut all_violations = Vec::new();

    for pod in pods {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }
        let contribution = governance::evaluate_pod_with_policy(pod, policy);
        governance::add_metrics(&mut aggregate, &contribution);
        let details = governance::detect_violations_detailed(pod, policy);
        all_violations.extend(details);
    }

    let health_score = governance::calculate_health_score(&aggregate);
    let classification = governance::classify_health(health_score).to_string();

    ClusterEvaluation {
        context_name: context_name.to_string(),
        health_score,
        classification,
        total_pods: aggregate.total_pods,
        total_violations: all_violations.len() as u32,
        violations: all_violations,
    }
}

#[test]
fn test_bundle_evaluate_audit_pipeline() {
    let bundle = bundles::get_bundle("restricted").unwrap();

    let pods = vec![
        make_test_pod("good", "prod", "nginx:1.25", true, true, 0, "Running"),
        make_test_pod("bad", "prod", "nginx:latest", false, false, 0, "Running"),
    ];

    let eval = evaluate_pods_synthetic("prod-cluster", &pods, &bundle.spec);
    assert_eq!(eval.total_pods, 2);
    assert!(eval.total_violations > 0);
    assert!(eval.health_score < 100);

    // Construct audit result spec from evaluation
    let audit_spec = PolicyAuditResultSpec {
        policy_name: "restricted-policy".to_string(),
        cluster_name: Some(eval.context_name.clone()),
        timestamp: "2026-02-24T10:00:00Z".to_string(),
        health_score: eval.health_score,
        total_violations: eval.total_violations,
        total_pods: eval.total_pods,
        classification: eval.classification.clone(),
        violations: eval
            .violations
            .iter()
            .map(|v| AuditViolation {
                pod_name: v.pod_name.clone(),
                container_name: v.container_name.clone(),
                violation_type: v.violation_type.clone(),
                severity: v.severity.clone(),
                message: v.message.clone(),
            })
            .collect(),
    };

    assert_eq!(audit_spec.policy_name, "restricted-policy");
    assert_eq!(audit_spec.health_score, eval.health_score);
    assert!(!audit_spec.violations.is_empty());
}

#[test]
fn test_multi_cluster_aggregation_pipeline() {
    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        ..Default::default()
    };

    let prod_pods = vec![
        make_test_pod("a", "prod", "nginx:1.25", true, true, 0, "Running"),
        make_test_pod("b", "prod", "redis:7", true, true, 0, "Running"),
    ];

    let staging_pods = vec![
        make_test_pod("a", "staging", "nginx:latest", false, true, 0, "Running"),
        make_test_pod("b", "staging", "app:latest", false, false, 0, "Running"),
    ];

    let prod_eval = evaluate_pods_synthetic("prod-context", &prod_pods, &policy);
    let staging_eval = evaluate_pods_synthetic("staging-context", &staging_pods, &policy);

    assert_eq!(prod_eval.total_violations, 0);
    assert!(staging_eval.total_violations > 0);

    let report = aggregate_report(vec![prod_eval, staging_eval]);
    assert_eq!(report.clusters.len(), 2);
    assert!(report.aggregate_score > 0);
    assert!(report.aggregate_score < 100);
}

#[test]
fn test_multi_cluster_all_healthy() {
    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        ..Default::default()
    };

    let evals: Vec<_> = (0..3)
        .map(|i| {
            let pods = vec![make_test_pod(
                &format!("pod-{i}"),
                "ns",
                "nginx:1.25",
                true,
                true,
                0,
                "Running",
            )];
            evaluate_pods_synthetic(&format!("cluster-{i}"), &pods, &policy)
        })
        .collect();

    let report = aggregate_report(evals);
    assert_eq!(report.aggregate_score, 100);
    assert_eq!(report.aggregate_classification, "Healthy");
}

#[test]
fn test_multi_cluster_with_severity_overrides() {
    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        severity_overrides: Some(SeverityOverrides {
            latest_tag: Some(Severity::Critical),
            ..Default::default()
        }),
        ..Default::default()
    };

    let pods = vec![make_test_pod(
        "bad",
        "prod",
        "nginx:latest",
        true,
        true,
        0,
        "Running",
    )];

    let eval = evaluate_pods_synthetic("prod", &pods, &policy);
    assert!(eval.total_violations > 0);

    // Verify severity is in the violation details
    let latest_violation = eval
        .violations
        .iter()
        .find(|v| v.violation_type == "latest_tag");
    assert!(latest_violation.is_some());
    assert_eq!(latest_violation.unwrap().severity, Severity::Critical);
}

#[test]
fn test_audit_result_construction_from_evaluation() {
    let eval = ClusterEvaluation {
        context_name: "test-cluster".to_string(),
        health_score: 75,
        classification: "Stable".to_string(),
        total_pods: 10,
        total_violations: 5,
        violations: vec![governance::ViolationDetail {
            violation_type: "latest_tag".to_string(),
            severity: Severity::High,
            pod_name: "web-pod".to_string(),
            namespace: "prod".to_string(),
            container_name: "nginx".to_string(),
            message: "uses :latest".to_string(),
        }],
    };

    let audit_spec = PolicyAuditResultSpec {
        policy_name: "my-policy".to_string(),
        cluster_name: Some(eval.context_name.clone()),
        timestamp: "2026-02-24T12:00:00Z".to_string(),
        health_score: eval.health_score,
        total_violations: eval.total_violations,
        total_pods: eval.total_pods,
        classification: eval.classification.clone(),
        violations: eval
            .violations
            .iter()
            .map(|v| AuditViolation {
                pod_name: v.pod_name.clone(),
                container_name: v.container_name.clone(),
                violation_type: v.violation_type.clone(),
                severity: v.severity.clone(),
                message: v.message.clone(),
            })
            .collect(),
    };

    // Verify serialization works
    let json = serde_json::to_string(&audit_spec).expect("should serialize");
    let deserialized: PolicyAuditResultSpec =
        serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(deserialized.health_score, 75);
    assert_eq!(deserialized.violations.len(), 1);
}

#[test]
fn test_empty_cluster_evaluation() {
    let policy = DevOpsPolicySpec::default();
    let eval = evaluate_pods_synthetic("empty", &[], &policy);
    assert_eq!(eval.total_pods, 0);
    assert_eq!(eval.total_violations, 0);
    assert_eq!(eval.health_score, 100);
}
