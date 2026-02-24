use crate::crd::{
    DefaultProbeConfig, DefaultResourceConfig, DevOpsPolicySpec, EnforcementMode, Severity,
    SeverityOverrides,
};

/* ============================= TYPES ============================= */

/// A pre-defined policy template with a name and description.
#[derive(Debug, Clone)]
pub struct PolicyBundle {
    pub name: String,
    pub description: String,
    pub spec: DevOpsPolicySpec,
}

/* ============================= BUNDLES ============================= */

/// Return all built-in policy bundles.
pub fn all_bundles() -> Vec<PolicyBundle> {
    vec![baseline_bundle(), restricted_bundle(), permissive_bundle()]
}

/// Look up a bundle by name (case-insensitive).
pub fn get_bundle(name: &str) -> Option<PolicyBundle> {
    let lower = name.to_lowercase();
    all_bundles().into_iter().find(|b| b.name == lower)
}

fn baseline_bundle() -> PolicyBundle {
    PolicyBundle {
        name: "baseline".to_string(),
        description: "Forbid :latest tags and require readiness probes. Audit mode.".to_string(),
        spec: DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_readiness_probe: Some(true),
            enforcement_mode: Some(EnforcementMode::Audit),
            ..Default::default()
        },
    }
}

fn restricted_bundle() -> PolicyBundle {
    PolicyBundle {
        name: "restricted".to_string(),
        description: "All checks enabled with strict thresholds. Enforce mode.".to_string(),
        spec: DevOpsPolicySpec {
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
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Critical),
                missing_liveness: Some(Severity::High),
                missing_readiness: Some(Severity::High),
                high_restarts: Some(Severity::Critical),
                pending: Some(Severity::High),
            }),
        },
    }
}

fn permissive_bundle() -> PolicyBundle {
    PolicyBundle {
        name: "permissive".to_string(),
        description: "All checks enabled with lenient thresholds. Audit mode.".to_string(),
        spec: DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(10),
            forbid_pending_duration: Some(600),
            enforcement_mode: Some(EnforcementMode::Audit),
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Low),
                missing_liveness: Some(Severity::Low),
                missing_readiness: Some(Severity::Low),
                high_restarts: Some(Severity::Medium),
                pending: Some(Severity::Low),
            }),
            ..Default::default()
        },
    }
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_bundles_count() {
        assert_eq!(all_bundles().len(), 3);
    }

    #[test]
    fn test_get_bundle_baseline() {
        let bundle = get_bundle("baseline");
        assert!(bundle.is_some());
        let bundle = bundle.unwrap();
        assert_eq!(bundle.name, "baseline");
        assert_eq!(bundle.spec.forbid_latest_tag, Some(true));
        assert_eq!(bundle.spec.require_readiness_probe, Some(true));
        assert_eq!(bundle.spec.require_liveness_probe, None);
    }

    #[test]
    fn test_get_bundle_restricted() {
        let bundle = get_bundle("restricted").unwrap();
        assert_eq!(bundle.name, "restricted");
        assert_eq!(bundle.spec.enforcement_mode, Some(EnforcementMode::Enforce));
        assert_eq!(bundle.spec.max_restart_count, Some(3));
        assert!(bundle.spec.default_probe.is_some());
        assert!(bundle.spec.default_resources.is_some());
        assert!(bundle.spec.severity_overrides.is_some());
    }

    #[test]
    fn test_get_bundle_permissive() {
        let bundle = get_bundle("permissive").unwrap();
        assert_eq!(bundle.name, "permissive");
        assert_eq!(bundle.spec.enforcement_mode, Some(EnforcementMode::Audit));
        assert_eq!(bundle.spec.max_restart_count, Some(10));
        assert_eq!(bundle.spec.forbid_pending_duration, Some(600));
        let overrides = bundle.spec.severity_overrides.as_ref().unwrap();
        assert_eq!(overrides.latest_tag, Some(Severity::Low));
    }

    #[test]
    fn test_get_bundle_unknown_returns_none() {
        assert!(get_bundle("nonexistent").is_none());
    }

    #[test]
    fn test_get_bundle_case_insensitive() {
        assert!(get_bundle("Baseline").is_some());
        assert!(get_bundle("RESTRICTED").is_some());
    }

    #[test]
    fn test_baseline_bundle_valid_serialization() {
        let bundle = get_bundle("baseline").unwrap();
        let json = serde_json::to_string(&bundle.spec).expect("should serialize");
        let _: DevOpsPolicySpec = serde_json::from_str(&json).expect("should deserialize");
    }

    #[test]
    fn test_restricted_bundle_valid_serialization() {
        let bundle = get_bundle("restricted").unwrap();
        let json = serde_json::to_string(&bundle.spec).expect("should serialize");
        let _: DevOpsPolicySpec = serde_json::from_str(&json).expect("should deserialize");
    }

    #[test]
    fn test_permissive_bundle_valid_serialization() {
        let bundle = get_bundle("permissive").unwrap();
        let json = serde_json::to_string(&bundle.spec).expect("should serialize");
        let _: DevOpsPolicySpec = serde_json::from_str(&json).expect("should deserialize");
    }

    #[test]
    fn test_bundle_names_unique() {
        let bundles = all_bundles();
        let names: Vec<&str> = bundles.iter().map(|b| b.name.as_str()).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "bundle names should be unique");
    }
}
