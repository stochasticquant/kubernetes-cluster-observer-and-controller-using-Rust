mod common;

use common::make_test_pod;
use kube_devops::governance;

// ══════════════════════════════════════════════════════════════════
// End-to-end governance pipeline tests (no cluster required)
//
// Each test exercises: pod construction → evaluate → accumulate →
// score → classify, verifying the full pipeline in one shot.
// ══════════════════════════════════════════════════════════════════

#[test]
fn test_single_healthy_pod_pipeline() {
    let pod = make_test_pod("web", "production", "nginx:1.25", true, true, 0, "Running");

    let metrics = governance::evaluate_pod(&pod);
    let score = governance::calculate_health_score(&metrics);
    let status = governance::classify_health(score);

    assert_eq!(metrics.total_pods, 1);
    assert_eq!(score, 100);
    assert_eq!(status, "Healthy");
}

#[test]
fn test_single_noncompliant_pod_pipeline() {
    let pod = make_test_pod("bad", "staging", "nginx:latest", false, false, 10, "Pending");

    let metrics = governance::evaluate_pod(&pod);
    let violations = governance::detect_violations(&pod);
    let score = governance::calculate_health_score(&metrics);
    let status = governance::classify_health(score);

    assert!(metrics.latest_tag >= 1);
    assert!(metrics.missing_liveness >= 1);
    assert!(metrics.missing_readiness >= 1);
    assert!(metrics.high_restarts > 0);
    assert_eq!(metrics.pending, 1);
    assert!(!violations.is_empty());
    assert!(score < 80);
    assert_ne!(status, "Healthy");
}

#[test]
fn test_multi_pod_aggregation_pipeline() {
    let pods = vec![
        make_test_pod("a", "prod", "nginx:1.25", true, true, 0, "Running"),
        make_test_pod("b", "prod", "redis:7.0", true, true, 0, "Running"),
        make_test_pod("c", "prod", "app:latest", false, false, 0, "Running"),
    ];

    let mut aggregate = governance::PodMetrics::default();
    for pod in &pods {
        let m = governance::evaluate_pod(pod);
        governance::add_metrics(&mut aggregate, &m);
    }

    assert_eq!(aggregate.total_pods, 3);
    assert_eq!(aggregate.latest_tag, 1);
    assert_eq!(aggregate.missing_liveness, 1);
    assert_eq!(aggregate.missing_readiness, 1);

    let score = governance::calculate_health_score(&aggregate);
    let status = governance::classify_health(score);

    // 2 of 3 pods are clean → score should still be reasonable
    assert!(score > 0);
    assert!(status == "Healthy" || status == "Stable");
}

#[test]
fn test_namespace_independence() {
    let pod_a = make_test_pod("a", "staging", "nginx:latest", false, false, 0, "Running");
    let pod_b = make_test_pod("b", "production", "nginx:1.25", true, true, 0, "Running");

    let metrics_a = governance::evaluate_pod(&pod_a);
    let metrics_b = governance::evaluate_pod(&pod_b);

    let score_a = governance::calculate_health_score(&metrics_a);
    let score_b = governance::calculate_health_score(&metrics_b);

    // production pod is fully compliant, staging pod is not
    assert_eq!(score_b, 100);
    assert!(score_a < score_b);
}

#[test]
fn test_pod_lifecycle_add_remove() {
    let mut cluster = governance::PodMetrics::default();

    let pod = make_test_pod("web", "default", "nginx:latest", false, false, 0, "Running");
    let contribution = governance::evaluate_pod(&pod);

    governance::add_metrics(&mut cluster, &contribution);
    assert_eq!(cluster.total_pods, 1);
    assert_eq!(cluster.latest_tag, 1);

    let score_with = governance::calculate_health_score(&cluster);

    governance::subtract_metrics(&mut cluster, &contribution);
    assert_eq!(cluster.total_pods, 0);

    let score_without = governance::calculate_health_score(&cluster);

    // After removing the problematic pod, score should recover to 100
    assert!(score_without > score_with);
    assert_eq!(score_without, 100);
}

#[test]
fn test_system_namespace_filtering() {
    let pods = vec![
        make_test_pod("a", "kube-system", "nginx:latest", false, false, 0, "Running"),
        make_test_pod("b", "kube-flannel", "nginx:latest", false, false, 0, "Running"),
        make_test_pod("c", "cert-manager", "nginx:latest", false, false, 0, "Running"),
        make_test_pod("d", "production", "nginx:1.25", true, true, 0, "Running"),
    ];

    let mut aggregate = governance::PodMetrics::default();
    for pod in &pods {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }
        let m = governance::evaluate_pod(pod);
        governance::add_metrics(&mut aggregate, &m);
    }

    // Only the "production" pod should be counted
    assert_eq!(aggregate.total_pods, 1);
    assert_eq!(aggregate.latest_tag, 0);
    assert_eq!(governance::calculate_health_score(&aggregate), 100);
}
