use k8s_openapi::api::core::v1::Pod;

use crate::crd::{DevOpsPolicySpec, Severity};
use crate::governance;

/* ============================= TYPES ============================= */

/// Result of evaluating a pod against admission policy checks.
#[derive(Debug, Clone)]
pub struct AdmissionVerdict {
    pub allowed: bool,
    pub message: Option<String>,
    pub violations: Vec<String>,
}

/* ============================= CORE LOGIC ============================= */

/// Build a copy of the policy with runtime-only checks disabled.
///
/// Admission happens before a pod runs, so `maxRestartCount` and
/// `forbidPendingDuration` are meaningless at admission time.
pub fn build_admission_policy_for_validation(policy: &DevOpsPolicySpec) -> DevOpsPolicySpec {
    DevOpsPolicySpec {
        max_restart_count: None,
        forbid_pending_duration: None,
        ..policy.clone()
    }
}

/// Validate a pod against admission-relevant policy checks.
///
/// Returns an `AdmissionVerdict` with `allowed = true` if the pod is compliant,
/// or `allowed = false` with a denial message listing all violations.
///
/// Only checks that the policy enables are evaluated. Runtime-only checks
/// (restart count, pending duration) are automatically skipped.
pub fn validate_pod_admission(pod: &Pod, policy: &DevOpsPolicySpec) -> AdmissionVerdict {
    let admission_policy = build_admission_policy_for_validation(policy);
    let mut violations = Vec::new();

    let Some(spec) = &pod.spec else {
        // No spec → nothing to validate → allow (fail-open)
        return AdmissionVerdict {
            allowed: true,
            message: None,
            violations,
        };
    };

    for c in &spec.containers {
        let container_name = &c.name;

        if admission_policy.forbid_latest_tag.unwrap_or(false)
            && c.image.as_deref().unwrap_or("").ends_with(":latest")
        {
            violations.push(format!("container '{}' uses :latest tag", container_name));
        }

        if admission_policy.require_liveness_probe.unwrap_or(false) && c.liveness_probe.is_none() {
            violations.push(format!(
                "container '{}' missing liveness probe",
                container_name
            ));
        }

        if admission_policy.require_readiness_probe.unwrap_or(false) && c.readiness_probe.is_none()
        {
            violations.push(format!(
                "container '{}' missing readiness probe",
                container_name
            ));
        }
    }

    if violations.is_empty() {
        AdmissionVerdict {
            allowed: true,
            message: None,
            violations,
        }
    } else {
        let message = format_denial_message(&violations);
        AdmissionVerdict {
            allowed: false,
            message: Some(message),
            violations,
        }
    }
}

/// Format a human-readable denial message from a list of violations.
pub fn format_denial_message(violations: &[String]) -> String {
    format!("Denied by DevOpsPolicy: {}", violations.join(", "))
}

/* ============================= SEVERITY-AWARE ADMISSION ============================= */

/// Numeric ordering for severity levels (higher = more severe).
fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Low => 1,
        Severity::Medium => 2,
        Severity::High => 3,
        Severity::Critical => 4,
    }
}

/// Validate a pod against admission-relevant policy checks with severity threshold.
///
/// Only violations at or above `min_deny_severity` cause denial.
/// Runtime-only checks (restart count, pending duration) are automatically skipped.
pub fn validate_pod_admission_with_severity(
    pod: &Pod,
    policy: &DevOpsPolicySpec,
    min_deny_severity: &Severity,
) -> AdmissionVerdict {
    let admission_policy = build_admission_policy_for_validation(policy);
    let details = governance::detect_violations_detailed(pod, &admission_policy);

    let threshold = severity_rank(min_deny_severity);
    let violations: Vec<String> = details
        .iter()
        .filter(|v| severity_rank(&v.severity) >= threshold)
        .map(|v| v.message.clone())
        .collect();

    if violations.is_empty() {
        AdmissionVerdict {
            allowed: true,
            message: None,
            violations,
        }
    } else {
        let message = format_denial_message(&violations);
        AdmissionVerdict {
            allowed: false,
            message: Some(message),
            violations,
        }
    }
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, PodSpec, Probe};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    use crate::crd::SeverityOverrides;

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

    fn make_admission_pod(name: &str, containers: Vec<Container>) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some("default".to_string()),
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

    // ── allow compliant pod ──

    #[test]
    fn test_allow_compliant_pod() {
        let pod = make_admission_pod(
            "good-pod",
            vec![container_with("nginx", "nginx:1.25", true, true)],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(verdict.allowed);
        assert!(verdict.message.is_none());
        assert!(verdict.violations.is_empty());
    }

    // ── deny :latest tag ──

    #[test]
    fn test_deny_latest_tag() {
        let pod = make_admission_pod(
            "bad-pod",
            vec![container_with("nginx", "nginx:latest", true, true)],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        assert!(verdict.violations.len() == 1);
        assert!(verdict.violations[0].contains(":latest"));
    }

    // ── deny missing liveness probe ──

    #[test]
    fn test_deny_missing_liveness_probe() {
        let pod = make_admission_pod(
            "bad-pod",
            vec![container_with("nginx", "nginx:1.25", false, true)],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        assert!(verdict.violations.len() == 1);
        assert!(verdict.violations[0].contains("liveness probe"));
    }

    // ── deny missing readiness probe ──

    #[test]
    fn test_deny_missing_readiness_probe() {
        let pod = make_admission_pod(
            "bad-pod",
            vec![container_with("nginx", "nginx:1.25", true, false)],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        assert!(verdict.violations.len() == 1);
        assert!(verdict.violations[0].contains("readiness probe"));
    }

    // ── deny multiple violations ──

    #[test]
    fn test_deny_multiple_violations() {
        let pod = make_admission_pod(
            "bad-pod",
            vec![container_with("nginx", "nginx:latest", false, false)],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        assert_eq!(verdict.violations.len(), 3);
        let msg = verdict.message.unwrap();
        assert!(msg.starts_with("Denied by DevOpsPolicy:"));
        assert!(msg.contains(":latest"));
        assert!(msg.contains("liveness"));
        assert!(msg.contains("readiness"));
    }

    // ── skip runtime-only checks ──

    #[test]
    fn test_skip_runtime_checks_max_restart_count() {
        let policy = DevOpsPolicySpec {
            max_restart_count: Some(3),
            ..empty_policy()
        };
        let admission = build_admission_policy_for_validation(&policy);
        assert!(admission.max_restart_count.is_none());
    }

    #[test]
    fn test_skip_runtime_checks_forbid_pending_duration() {
        let policy = DevOpsPolicySpec {
            forbid_pending_duration: Some(300),
            ..empty_policy()
        };
        let admission = build_admission_policy_for_validation(&policy);
        assert!(admission.forbid_pending_duration.is_none());
    }

    #[test]
    fn test_admission_preserves_non_runtime_checks() {
        let admission = build_admission_policy_for_validation(&all_enabled_policy());
        assert_eq!(admission.forbid_latest_tag, Some(true));
        assert_eq!(admission.require_liveness_probe, Some(true));
        assert_eq!(admission.require_readiness_probe, Some(true));
        assert!(admission.max_restart_count.is_none());
        assert!(admission.forbid_pending_duration.is_none());
    }

    // ── empty policy = allow all ──

    #[test]
    fn test_empty_policy_allows_all() {
        let pod = make_admission_pod(
            "bad-pod",
            vec![container_with("nginx", "nginx:latest", false, false)],
        );
        let verdict = validate_pod_admission(&pod, &empty_policy());
        assert!(verdict.allowed);
        assert!(verdict.violations.is_empty());
    }

    // ── multi-container pod ──

    #[test]
    fn test_multi_container_violation_in_one() {
        let pod = make_admission_pod(
            "multi-pod",
            vec![
                container_with("good", "nginx:1.25", true, true),
                container_with("bad", "redis:latest", true, true),
            ],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        assert_eq!(verdict.violations.len(), 1);
        assert!(verdict.violations[0].contains("bad"));
        assert!(verdict.violations[0].contains(":latest"));
    }

    #[test]
    fn test_multi_container_all_violations() {
        let pod = make_admission_pod(
            "multi-pod",
            vec![
                container_with("a", "img:latest", false, false),
                container_with("b", "img:latest", false, false),
            ],
        );
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(!verdict.allowed);
        // 3 violations per container × 2 containers = 6
        assert_eq!(verdict.violations.len(), 6);
    }

    // ── pod with no spec ──

    #[test]
    fn test_no_spec_allows_failopen() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("no-spec".to_string()),
                ..Default::default()
            },
            spec: None,
            status: None,
        };
        let verdict = validate_pod_admission(&pod, &all_enabled_policy());
        assert!(verdict.allowed);
        assert!(verdict.violations.is_empty());
    }

    // ── format_denial_message ──

    #[test]
    fn test_format_denial_message_single() {
        let violations = vec!["container 'nginx' uses :latest tag".to_string()];
        let msg = format_denial_message(&violations);
        assert_eq!(
            msg,
            "Denied by DevOpsPolicy: container 'nginx' uses :latest tag"
        );
    }

    #[test]
    fn test_format_denial_message_multiple() {
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

    // ── partial policy ──

    #[test]
    fn test_only_latest_tag_enabled() {
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            ..empty_policy()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", false, false)],
        );
        let verdict = validate_pod_admission(&pod, &policy);
        assert!(!verdict.allowed);
        assert_eq!(verdict.violations.len(), 1);
        assert!(verdict.violations[0].contains(":latest"));
    }

    #[test]
    fn test_disabled_checks_not_enforced() {
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(false),
            require_liveness_probe: Some(false),
            require_readiness_probe: Some(false),
            ..empty_policy()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", false, false)],
        );
        let verdict = validate_pod_admission(&pod, &policy);
        assert!(verdict.allowed);
        assert!(verdict.violations.is_empty());
    }

    // ── severity-aware admission tests ──

    #[test]
    fn test_severity_admission_critical_threshold_allows_high() {
        // Default latest_tag severity is High. With Critical threshold, should allow.
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            ..Default::default()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", true, true)],
        );
        let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::Critical);
        assert!(
            verdict.allowed,
            "High severity violation should be allowed with Critical threshold"
        );
    }

    #[test]
    fn test_severity_admission_high_threshold_denies_high() {
        // Default latest_tag severity is High. With High threshold, should deny.
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            ..Default::default()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", true, true)],
        );
        let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::High);
        assert!(
            !verdict.allowed,
            "High severity violation should be denied with High threshold"
        );
    }

    #[test]
    fn test_severity_admission_low_threshold_denies_all() {
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            ..Default::default()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", false, false)],
        );
        let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::Low);
        assert!(!verdict.allowed);
        assert_eq!(verdict.violations.len(), 3);
    }

    #[test]
    fn test_severity_admission_medium_threshold_skips_low() {
        // missing_readiness default severity is Low. With Medium threshold, should be skipped.
        let policy = DevOpsPolicySpec {
            require_readiness_probe: Some(true),
            ..Default::default()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:1.25", true, false)],
        );
        let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::Medium);
        assert!(
            verdict.allowed,
            "Low severity violation should be allowed with Medium threshold"
        );
    }

    #[test]
    fn test_severity_admission_overrides_respected() {
        // Override latest_tag to Low, then with High threshold, should allow.
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Low),
                ..Default::default()
            }),
            ..Default::default()
        };
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:latest", true, true)],
        );
        let verdict = validate_pod_admission_with_severity(&pod, &policy, &Severity::High);
        assert!(
            verdict.allowed,
            "Low severity (overridden) should be allowed with High threshold"
        );
    }

    #[test]
    fn test_severity_admission_compliant_pod_all_thresholds() {
        let policy = all_enabled_policy();
        let pod = make_admission_pod(
            "pod",
            vec![container_with("nginx", "nginx:1.25", true, true)],
        );
        for threshold in [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            let verdict = validate_pod_admission_with_severity(&pod, &policy, &threshold);
            assert!(
                verdict.allowed,
                "compliant pod should be allowed at any threshold"
            );
        }
    }

    #[test]
    fn test_severity_admission_no_spec_allows() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("no-spec".to_string()),
                ..Default::default()
            },
            spec: None,
            status: None,
        };
        let verdict =
            validate_pod_admission_with_severity(&pod, &all_enabled_policy(), &Severity::Low);
        assert!(verdict.allowed);
    }

    #[test]
    fn test_severity_admission_runtime_checks_skipped() {
        let admission_policy = build_admission_policy_for_validation(&all_enabled_policy());
        assert!(admission_policy.max_restart_count.is_none());
        assert!(admission_policy.forbid_pending_duration.is_none());
        // severity_overrides should carry through
        let policy_with_overrides = DevOpsPolicySpec {
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Critical),
                ..Default::default()
            }),
            ..all_enabled_policy()
        };
        let admission = build_admission_policy_for_validation(&policy_with_overrides);
        assert!(admission.severity_overrides.is_some());
    }
}
