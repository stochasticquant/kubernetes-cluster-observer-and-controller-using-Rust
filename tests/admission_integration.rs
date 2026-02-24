mod common;

use kube_devops::admission::{
    build_admission_policy_for_validation, format_denial_message, validate_pod_admission,
};
use kube_devops::crd::DevOpsPolicySpec;

use k8s_openapi::api::core::v1::{Container, Pod, PodSpec, PodStatus, Probe};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/* ============================= HELPERS ============================= */

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

fn make_admission_pod(name: &str, namespace: &str, containers: Vec<Container>) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers,
            ..Default::default()
        }),
        status: None,
    }
}

fn container_with(
    name: &str,
    image: &str,
    has_liveness: bool,
    has_readiness: bool,
) -> Container {
    let probe = || -> Option<Probe> { Some(Probe::default()) };

    Container {
        name: name.to_string(),
        image: Some(image.to_string()),
        liveness_probe: if has_liveness { probe() } else { None },
        readiness_probe: if has_readiness { probe() } else { None },
        ..Default::default()
    }
}

/* ============================= FULL PIPELINE TESTS ============================= */

/// Simulate the full admission pipeline: build AdmissionReview JSON → validate → check response.
#[test]
fn test_full_admission_pipeline_allow() {
    let pod = make_admission_pod(
        "compliant-pod",
        "production",
        vec![container_with("nginx", "nginx:1.25", true, true)],
    );
    let policy = all_enabled_policy();

    let verdict = validate_pod_admission(&pod, &policy);

    assert!(verdict.allowed);
    assert!(verdict.message.is_none());
    assert!(verdict.violations.is_empty());
}

#[test]
fn test_full_admission_pipeline_deny_latest() {
    let pod = make_admission_pod(
        "bad-pod",
        "production",
        vec![container_with("nginx", "nginx:latest", true, true)],
    );
    let policy = all_enabled_policy();

    let verdict = validate_pod_admission(&pod, &policy);

    assert!(!verdict.allowed);
    assert_eq!(verdict.violations.len(), 1);
    assert!(verdict.violations[0].contains(":latest"));
    assert!(verdict.message.unwrap().starts_with("Denied by DevOpsPolicy:"));
}

#[test]
fn test_full_admission_pipeline_deny_missing_probes() {
    let pod = make_admission_pod(
        "no-probes",
        "production",
        vec![container_with("app", "myapp:v2", false, false)],
    );
    let policy = all_enabled_policy();

    let verdict = validate_pod_admission(&pod, &policy);

    assert!(!verdict.allowed);
    assert_eq!(verdict.violations.len(), 2);
    assert!(verdict.violations.iter().any(|v| v.contains("liveness")));
    assert!(verdict.violations.iter().any(|v| v.contains("readiness")));
}

#[test]
fn test_full_admission_pipeline_deny_multiple_violations() {
    let pod = make_admission_pod(
        "bad-pod",
        "production",
        vec![container_with("nginx", "nginx:latest", false, false)],
    );
    let policy = all_enabled_policy();

    let verdict = validate_pod_admission(&pod, &policy);

    assert!(!verdict.allowed);
    assert_eq!(verdict.violations.len(), 3);
    let msg = verdict.message.unwrap();
    assert!(msg.contains(":latest"));
    assert!(msg.contains("liveness"));
    assert!(msg.contains("readiness"));
}

#[test]
fn test_failopen_no_policy() {
    // Empty policy = all checks disabled → allow everything
    let pod = make_admission_pod(
        "anything",
        "production",
        vec![container_with("nginx", "nginx:latest", false, false)],
    );
    let verdict = validate_pod_admission(&pod, &empty_policy());

    assert!(verdict.allowed);
    assert!(verdict.violations.is_empty());
}

#[test]
fn test_allow_when_policy_disables_all_checks() {
    let policy = DevOpsPolicySpec {
        forbid_latest_tag: Some(false),
        require_liveness_probe: Some(false),
        require_readiness_probe: Some(false),
        ..Default::default()
    };
    let pod = make_admission_pod(
        "anything",
        "production",
        vec![container_with("nginx", "nginx:latest", false, false)],
    );
    let verdict = validate_pod_admission(&pod, &policy);

    assert!(verdict.allowed);
    assert!(verdict.violations.is_empty());
}

#[test]
fn test_runtime_checks_not_applied_at_admission() {
    // Even with restarts and pending checks enabled, they should be stripped
    let policy = DevOpsPolicySpec {
        max_restart_count: Some(3),
        forbid_pending_duration: Some(300),
        ..empty_policy()
    };
    let admission_policy = build_admission_policy_for_validation(&policy);
    assert!(admission_policy.max_restart_count.is_none());
    assert!(admission_policy.forbid_pending_duration.is_none());

    // Pod with high restarts and pending status should still be allowed
    let pod = Pod {
        metadata: ObjectMeta {
            name: Some("runtime-pod".to_string()),
            namespace: Some("default".to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![container_with("app", "app:v1", true, true)],
            ..Default::default()
        }),
        status: Some(PodStatus {
            phase: Some("Pending".to_string()),
            ..Default::default()
        }),
    };
    let verdict = validate_pod_admission(&pod, &policy);
    assert!(verdict.allowed);
}

#[test]
fn test_multi_container_mixed_compliance() {
    let pod = make_admission_pod(
        "multi",
        "production",
        vec![
            container_with("good", "nginx:1.25", true, true),
            container_with("bad", "redis:latest", false, true),
        ],
    );
    let verdict = validate_pod_admission(&pod, &all_enabled_policy());

    assert!(!verdict.allowed);
    // bad container has: :latest + missing liveness = 2 violations
    assert_eq!(verdict.violations.len(), 2);
    assert!(verdict.violations.iter().all(|v| v.contains("bad")));
}

#[test]
fn test_denial_message_format() {
    let violations = vec![
        "container 'nginx' uses :latest tag".to_string(),
        "container 'nginx' missing liveness probe".to_string(),
    ];
    let msg = format_denial_message(&violations);
    assert_eq!(
        msg,
        "Denied by DevOpsPolicy: container 'nginx' uses :latest tag, container 'nginx' missing liveness probe"
    );
}

#[test]
fn test_pod_with_no_spec_failopen() {
    let pod = Pod {
        metadata: ObjectMeta {
            name: Some("no-spec".to_string()),
            namespace: Some("default".to_string()),
            ..Default::default()
        },
        spec: None,
        status: None,
    };
    let verdict = validate_pod_admission(&pod, &all_enabled_policy());
    assert!(verdict.allowed);
    assert!(verdict.violations.is_empty());
}

#[test]
fn test_verdict_fields_consistency() {
    // When allowed, message should be None and violations empty
    let pod = make_admission_pod(
        "good",
        "default",
        vec![container_with("app", "app:v1", true, true)],
    );
    let verdict = validate_pod_admission(&pod, &all_enabled_policy());
    assert!(verdict.allowed);
    assert!(verdict.message.is_none());
    assert!(verdict.violations.is_empty());

    // When denied, message should be Some and violations non-empty
    let bad_pod = make_admission_pod(
        "bad",
        "default",
        vec![container_with("app", "app:latest", false, false)],
    );
    let bad_verdict = validate_pod_admission(&bad_pod, &all_enabled_policy());
    assert!(!bad_verdict.allowed);
    assert!(bad_verdict.message.is_some());
    assert!(!bad_verdict.violations.is_empty());
}

#[test]
fn test_system_namespace_pod_evaluated_normally() {
    // The admission module itself doesn't filter system namespaces —
    // that's done at the HTTP handler level. The pure validation should
    // still detect violations regardless of namespace.
    let pod = make_admission_pod(
        "sys-pod",
        "kube-system",
        vec![container_with("app", "app:latest", false, false)],
    );
    let verdict = validate_pod_admission(&pod, &all_enabled_policy());
    // Pure validation still finds violations — system ns bypass is in the handler
    assert!(!verdict.allowed);
    assert_eq!(verdict.violations.len(), 3);
}
