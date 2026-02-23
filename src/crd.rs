use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/* ============================= SPEC ============================= */

/// DevOpsPolicy defines a governance policy for Kubernetes workloads.
///
/// Each field enables or configures a specific compliance check.
/// When a field is omitted (`None`), that check is skipped during evaluation.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "devops.stochastic.io",
    version = "v1",
    kind = "DevOpsPolicy",
    plural = "devopspolicies",
    status = "DevOpsPolicyStatus",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct DevOpsPolicySpec {
    /// Forbid container images tagged with `:latest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbid_latest_tag: Option<bool>,

    /// Require liveness probes on all containers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_liveness_probe: Option<bool>,

    /// Require readiness probes on all containers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_readiness_probe: Option<bool>,

    /// Maximum allowed restart count before flagging a violation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_restart_count: Option<i32>,

    /// Maximum duration (seconds) a pod may remain in Pending phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbid_pending_duration: Option<u64>,
}

/* ============================= STATUS ============================= */

/// DevOpsPolicyStatus reports the observed compliance state.
///
/// Updated by the reconciler on every evaluation cycle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct DevOpsPolicyStatus {
    /// The `.metadata.generation` that was last reconciled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,

    /// Whether the namespace meets the health threshold (score >= 80).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy: Option<bool>,

    /// Governance health score (0–100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_score: Option<u32>,

    /// Total number of policy violations detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violations: Option<u32>,

    /// ISO 8601 timestamp of the last evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_evaluated: Option<String>,

    /// Human-readable summary of the evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;
    use kube::CustomResourceExt;

    #[test]
    fn test_crd_generates_valid_yaml() {
        let crd = DevOpsPolicy::crd();
        let yaml = serde_yaml::to_string(&crd).expect("CRD should serialize to YAML");
        assert!(yaml.contains("devops.stochastic.io"));
        assert!(yaml.contains("DevOpsPolicy"));
        assert!(yaml.contains("devopspolicies"));
    }

    #[test]
    fn test_crd_api_group() {
        let crd = DevOpsPolicy::crd();
        assert_eq!(crd.spec.group, "devops.stochastic.io");
    }

    #[test]
    fn test_crd_version() {
        let crd = DevOpsPolicy::crd();
        assert!(!crd.spec.versions.is_empty());
        assert_eq!(crd.spec.versions[0].name, "v1");
    }

    #[test]
    fn test_crd_kind() {
        let crd = DevOpsPolicy::crd();
        let names = &crd.spec.names;
        assert_eq!(names.kind, "DevOpsPolicy");
        assert_eq!(names.plural, "devopspolicies");
    }

    #[test]
    fn test_crd_is_namespaced() {
        let crd = DevOpsPolicy::crd();
        assert_eq!(crd.spec.scope, "Namespaced");
    }

    #[test]
    fn test_spec_serialization_roundtrip() {
        let spec = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(false),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        let deserialized: DevOpsPolicySpec =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.forbid_latest_tag, Some(true));
        assert_eq!(deserialized.require_liveness_probe, Some(true));
        assert_eq!(deserialized.require_readiness_probe, Some(false));
        assert_eq!(deserialized.max_restart_count, Some(3));
        assert_eq!(deserialized.forbid_pending_duration, Some(300));
    }

    #[test]
    fn test_spec_omitted_fields_deserialize_as_none() {
        let json = r#"{}"#;
        let spec: DevOpsPolicySpec =
            serde_json::from_str(json).expect("empty object should deserialize");

        assert_eq!(spec.forbid_latest_tag, None);
        assert_eq!(spec.require_liveness_probe, None);
        assert_eq!(spec.require_readiness_probe, None);
        assert_eq!(spec.max_restart_count, None);
        assert_eq!(spec.forbid_pending_duration, None);
    }

    #[test]
    fn test_status_default() {
        let status = DevOpsPolicyStatus::default();
        assert_eq!(status.observed_generation, None);
        assert_eq!(status.healthy, None);
        assert_eq!(status.health_score, None);
        assert_eq!(status.violations, None);
        assert_eq!(status.last_evaluated, None);
        assert_eq!(status.message, None);
    }

    #[test]
    fn test_status_serialization_roundtrip() {
        let status = DevOpsPolicyStatus {
            observed_generation: Some(1),
            healthy: Some(true),
            health_score: Some(87),
            violations: Some(3),
            last_evaluated: Some("2026-02-22T10:00:00Z".to_string()),
            message: Some("3 violations across 42 pods".to_string()),
        };

        let json = serde_json::to_string(&status).expect("should serialize");
        let deserialized: DevOpsPolicyStatus =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.observed_generation, Some(1));
        assert_eq!(deserialized.healthy, Some(true));
        assert_eq!(deserialized.health_score, Some(87));
        assert_eq!(deserialized.violations, Some(3));
        assert_eq!(
            deserialized.last_evaluated.as_deref(),
            Some("2026-02-22T10:00:00Z")
        );
    }

    #[test]
    fn test_status_omits_none_fields_in_json() {
        let status = DevOpsPolicyStatus {
            health_score: Some(95),
            ..Default::default()
        };

        let json = serde_json::to_string(&status).expect("should serialize");
        assert!(json.contains("healthScore"));
        assert!(!json.contains("observedGeneration"));
        assert!(!json.contains("violations"));
    }
}
