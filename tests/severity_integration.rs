mod common;

use common::make_test_pod;
use kube_devops::crd::{DevOpsPolicySpec, Severity, SeverityOverrides};
use kube_devops::governance;

// ══════════════════════════════════════════════════════════════════
// Severity integration tests (no cluster required)
//
// Exercises the end-to-end severity pipeline: same pods with
// different severity overrides produce different scores. Also
// verifies bundles generate correct policies.
// ══════════════════════════════════════════════════════════════════

#[test]
fn test_same_pods_different_severity_overrides_different_scores() {
    let pods = vec![
        make_test_pod("a", "prod", "nginx:latest", false, false, 0, "Running"),
        make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
    ];

    let base_policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        require_readiness_probe: Some(true),
        ..Default::default()
    };

    // Policy with all-Critical severity overrides
    let critical_policy = DevOpsPolicySpec {
        severity_overrides: Some(SeverityOverrides {
            latest_tag: Some(Severity::Critical),
            missing_liveness: Some(Severity::Critical),
            missing_readiness: Some(Severity::Critical),
            ..Default::default()
        }),
        ..base_policy.clone()
    };

    // Policy with all-Low severity overrides
    let low_policy = DevOpsPolicySpec {
        severity_overrides: Some(SeverityOverrides {
            latest_tag: Some(Severity::Low),
            missing_liveness: Some(Severity::Low),
            missing_readiness: Some(Severity::Low),
            ..Default::default()
        }),
        ..base_policy.clone()
    };

    let mut aggregate = governance::PodMetrics::default();
    for pod in &pods {
        let m = governance::evaluate_pod_with_policy(pod, &base_policy);
        governance::add_metrics(&mut aggregate, &m);
    }

    let score_critical = governance::calculate_health_score_with_severity(
        &aggregate,
        critical_policy.severity_overrides.as_ref(),
    );
    let score_low = governance::calculate_health_score_with_severity(
        &aggregate,
        low_policy.severity_overrides.as_ref(),
    );

    assert!(
        score_low > score_critical,
        "Low severity score ({score_low}) should be higher than Critical ({score_critical})"
    );
}

#[test]
fn test_severity_pipeline_detailed_violations() {
    let pod = make_test_pod("web", "prod", "nginx:latest", false, true, 0, "Running");

    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        severity_overrides: Some(SeverityOverrides {
            latest_tag: Some(Severity::Critical),
            missing_liveness: Some(Severity::Low),
            ..Default::default()
        }),
        ..Default::default()
    };

    let violations = governance::detect_violations_detailed(&pod, &policy);
    assert_eq!(violations.len(), 2);

    let latest = violations.iter().find(|v| v.violation_type == "latest_tag").unwrap();
    assert_eq!(latest.severity, Severity::Critical);

    let liveness = violations.iter().find(|v| v.violation_type == "missing_liveness").unwrap();
    assert_eq!(liveness.severity, Severity::Low);
}

#[test]
fn test_bundles_generate_correct_policies() {
    use kube_devops::bundles;
    use kube_devops::crd::EnforcementMode;

    let baseline = bundles::get_bundle("baseline").unwrap();
    assert_eq!(baseline.spec.forbid_latest_tag, Some(true));
    assert_eq!(baseline.spec.require_readiness_probe, Some(true));
    assert_eq!(baseline.spec.enforcement_mode, Some(EnforcementMode::Audit));

    let restricted = bundles::get_bundle("restricted").unwrap();
    assert_eq!(restricted.spec.enforcement_mode, Some(EnforcementMode::Enforce));
    assert!(restricted.spec.severity_overrides.is_some());

    let permissive = bundles::get_bundle("permissive").unwrap();
    assert_eq!(permissive.spec.max_restart_count, Some(10));
}

#[test]
fn test_bundle_policy_evaluation() {
    use kube_devops::bundles;

    let restricted = bundles::get_bundle("restricted").unwrap();
    let pods = vec![
        make_test_pod("a", "prod", "nginx:latest", false, false, 10, "Pending"),
    ];

    let mut aggregate = governance::PodMetrics::default();
    let mut total_violations = 0u32;
    for pod in &pods {
        let m = governance::evaluate_pod_with_policy(pod, &restricted.spec);
        governance::add_metrics(&mut aggregate, &m);
        let v = governance::detect_violations_detailed(pod, &restricted.spec);
        total_violations += v.len() as u32;
    }

    assert!(total_violations >= 4, "restricted should catch many violations, got {total_violations}");
    let score = governance::calculate_health_score_with_severity(
        &aggregate,
        restricted.spec.severity_overrides.as_ref(),
    );
    assert!(score < 80, "score should be unhealthy with many Critical violations, got {score}");
}

#[test]
fn test_severity_backward_compat_no_overrides() {
    // Policy without severity overrides should still work the same
    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        require_liveness_probe: Some(true),
        ..Default::default()
    };

    let pod = make_test_pod("a", "prod", "nginx:latest", false, true, 0, "Running");
    let violations = governance::detect_violations_detailed(&pod, &policy);
    assert_eq!(violations.len(), 2);

    // Default severities should apply
    let latest = violations.iter().find(|v| v.violation_type == "latest_tag").unwrap();
    assert_eq!(latest.severity, Severity::High); // default for latest_tag

    let liveness = violations.iter().find(|v| v.violation_type == "missing_liveness").unwrap();
    assert_eq!(liveness.severity, Severity::Medium); // default for missing_liveness
}

#[test]
fn test_severity_admission_integration() {
    use kube_devops::admission::validate_pod_admission_with_severity;
    use k8s_openapi::api::core::v1::{Container, PodSpec, Probe};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use k8s_openapi::api::core::v1::Pod;

    let pod = Pod {
        metadata: ObjectMeta {
            name: Some("test".to_string()),
            namespace: Some("default".to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "app".to_string(),
                image: Some("nginx:latest".to_string()),
                liveness_probe: Some(Probe::default()),
                readiness_probe: Some(Probe::default()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: None,
    };

    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(true),
        severity_overrides: Some(SeverityOverrides {
            latest_tag: Some(Severity::Low),
            ..Default::default()
        }),
        ..Default::default()
    };

    // With Critical threshold, Low severity violation should be allowed
    let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::Critical);
    assert!(verdict.allowed);

    // With Low threshold, Low severity violation should be denied
    let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::Low);
    assert!(!verdict.allowed);
}
