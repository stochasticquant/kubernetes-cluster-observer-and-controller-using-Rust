use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/* ============================= SEVERITY TYPES ============================= */

/// Severity level for policy violations.
///
/// Used by severity overrides and audit results to weight violations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Critical,
    High,
    #[default]
    Medium,
    Low,
}

/// Per-check severity overrides.
///
/// When set on a policy, these override the default severity for each check type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SeverityOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_tag: Option<Severity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_liveness: Option<Severity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_readiness: Option<Severity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_restarts: Option<Severity>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Severity>,
}

/// A single violation found during audit evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditViolation {
    pub pod_name: String,
    pub container_name: String,
    pub violation_type: String,
    pub severity: Severity,
    pub message: String,
}

/* ============================= ENFORCEMENT TYPES ============================= */

/// Enforcement mode for a DevOpsPolicy.
///
/// - `Audit` (default): detect and report violations, never mutate workloads.
/// - `Enforce`: automatically patch patchable violations on parent workloads.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EnforcementMode {
    Audit,
    Enforce,
}

/// Default probe configuration injected when a container is missing probes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultProbeConfig {
    /// TCP port to probe. Falls back to the container's first port, then 8080.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_port: Option<u16>,

    /// Seconds before the first probe after container start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_delay_seconds: Option<i32>,

    /// Seconds between consecutive probes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_seconds: Option<i32>,
}

/// Default resource requests and limits injected when a container has none.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultResourceConfig {
    /// CPU request (e.g. "100m").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_request: Option<String>,

    /// CPU limit (e.g. "500m").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<String>,

    /// Memory request (e.g. "128Mi").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_request: Option<String>,

    /// Memory limit (e.g. "256Mi").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
}

/* ============================= SPEC ============================= */

/// DevOpsPolicy defines a governance policy for Kubernetes workloads.
///
/// Each field enables or configures a specific compliance check.
/// When a field is omitted (`None`), that check is skipped during evaluation.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
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

    /// Enforcement mode: `audit` (default) or `enforce`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement_mode: Option<EnforcementMode>,

    /// Default probe configuration for enforcement remediation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_probe: Option<DefaultProbeConfig>,

    /// Default resource requests/limits for enforcement remediation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_resources: Option<DefaultResourceConfig>,

    /// Per-check severity overrides for violation weighting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity_overrides: Option<SeverityOverrides>,
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

    /// Number of successful remediations applied in the last cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediations_applied: Option<u32>,

    /// Number of failed remediation attempts in the last cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediations_failed: Option<u32>,

    /// Names of workloads that were remediated (e.g. "deployments/web-app").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediated_workloads: Option<Vec<String>>,
}

/* ============================= AUDIT RESULT CRD ============================= */

/// PolicyAuditResult stores the outcome of a policy evaluation cycle.
///
/// Created by the reconciler after each evaluation, with retention of the last N results.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "devops.stochastic.io",
    version = "v1",
    kind = "PolicyAuditResult",
    plural = "policyauditresults",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct PolicyAuditResultSpec {
    /// Name of the DevOpsPolicy that produced this result.
    pub policy_name: String,

    /// Cluster context name (for multi-cluster results).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,

    /// ISO 8601 timestamp of the evaluation.
    pub timestamp: String,

    /// Computed health score (0–100).
    pub health_score: u32,

    /// Total number of violations found.
    pub total_violations: u32,

    /// Total number of pods evaluated.
    pub total_pods: u32,

    /// Health classification (Healthy, Stable, Degraded, Critical).
    pub classification: String,

    /// Detailed violations found during this evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<AuditViolation>,
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
            ..Default::default()
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        let deserialized: DevOpsPolicySpec =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.forbid_latest_tag, Some(true));
        assert_eq!(deserialized.require_liveness_probe, Some(true));
        assert_eq!(deserialized.require_readiness_probe, Some(false));
        assert_eq!(deserialized.max_restart_count, Some(3));
        assert_eq!(deserialized.forbid_pending_duration, Some(300));
        assert_eq!(deserialized.enforcement_mode, None);
        assert_eq!(deserialized.default_probe, None);
        assert_eq!(deserialized.default_resources, None);
        assert_eq!(deserialized.severity_overrides, None);
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
        assert_eq!(spec.enforcement_mode, None);
        assert_eq!(spec.default_probe, None);
        assert_eq!(spec.default_resources, None);
        assert_eq!(spec.severity_overrides, None);
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
        assert_eq!(status.remediations_applied, None);
        assert_eq!(status.remediations_failed, None);
        assert_eq!(status.remediated_workloads, None);
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
            remediations_applied: Some(2),
            remediations_failed: Some(0),
            remediated_workloads: Some(vec!["deployments/web-app".to_string()]),
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
        assert_eq!(deserialized.remediations_applied, Some(2));
        assert_eq!(deserialized.remediations_failed, Some(0));
        assert_eq!(
            deserialized.remediated_workloads,
            Some(vec!["deployments/web-app".to_string()])
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
        assert!(!json.contains("remediationsApplied"));
        assert!(!json.contains("remediatedWorkloads"));
    }

    // ── Enforcement type tests ──

    #[test]
    fn test_enforcement_mode_serialize_audit() {
        let mode = EnforcementMode::Audit;
        let json = serde_json::to_string(&mode).expect("should serialize");
        assert_eq!(json, r#""audit""#);
    }

    #[test]
    fn test_enforcement_mode_serialize_enforce() {
        let mode = EnforcementMode::Enforce;
        let json = serde_json::to_string(&mode).expect("should serialize");
        assert_eq!(json, r#""enforce""#);
    }

    #[test]
    fn test_enforcement_mode_deserialize_roundtrip() {
        let json = r#""enforce""#;
        let mode: EnforcementMode = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(mode, EnforcementMode::Enforce);
    }

    #[test]
    fn test_spec_with_enforcement_fields() {
        let spec = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
            enforcement_mode: Some(EnforcementMode::Enforce),
            default_probe: Some(DefaultProbeConfig {
                tcp_port: Some(8080),
                initial_delay_seconds: Some(10),
                period_seconds: Some(15),
            }),
            default_resources: Some(DefaultResourceConfig {
                cpu_request: Some("100m".to_string()),
                cpu_limit: Some("500m".to_string()),
                memory_request: Some("128Mi".to_string()),
                memory_limit: Some("256Mi".to_string()),
            }),
            ..Default::default()
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        let deserialized: DevOpsPolicySpec =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(
            deserialized.enforcement_mode,
            Some(EnforcementMode::Enforce)
        );
        assert_eq!(
            deserialized.default_probe.as_ref().unwrap().tcp_port,
            Some(8080)
        );
        assert_eq!(
            deserialized
                .default_resources
                .as_ref()
                .unwrap()
                .cpu_request
                .as_deref(),
            Some("100m")
        );
    }

    #[test]
    fn test_backward_compat_old_spec_json() {
        // JSON from before Step 6 (no enforcement fields) should still deserialize
        let json = r#"{"forbidLatestTag":true,"requireLivenessProbe":true}"#;
        let spec: DevOpsPolicySpec =
            serde_json::from_str(json).expect("old JSON should deserialize");

        assert_eq!(spec.forbid_latest_tag, Some(true));
        assert_eq!(spec.enforcement_mode, None);
        assert_eq!(spec.default_probe, None);
        assert_eq!(spec.default_resources, None);
    }

    #[test]
    fn test_backward_compat_old_status_json() {
        // Status JSON from before Step 6 (no remediation fields) should still deserialize
        let json = r#"{"healthScore":90,"healthy":true,"violations":1}"#;
        let status: DevOpsPolicyStatus =
            serde_json::from_str(json).expect("old status JSON should deserialize");

        assert_eq!(status.health_score, Some(90));
        assert_eq!(status.remediations_applied, None);
        assert_eq!(status.remediations_failed, None);
        assert_eq!(status.remediated_workloads, None);
    }

    #[test]
    fn test_default_probe_config_partial() {
        // Only tcp_port set, others None
        let config = DefaultProbeConfig {
            tcp_port: Some(3000),
            initial_delay_seconds: None,
            period_seconds: None,
        };
        let json = serde_json::to_string(&config).expect("should serialize");
        let deserialized: DefaultProbeConfig =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.tcp_port, Some(3000));
        assert_eq!(deserialized.initial_delay_seconds, None);
    }

    #[test]
    fn test_default_resource_config_partial() {
        // Only memory_limit set
        let config = DefaultResourceConfig {
            cpu_request: None,
            cpu_limit: None,
            memory_request: None,
            memory_limit: Some("512Mi".to_string()),
        };
        let json = serde_json::to_string(&config).expect("should serialize");
        assert!(json.contains("memoryLimit"));
        assert!(!json.contains("cpuRequest"));
    }

    // ── Severity type tests ──

    #[test]
    fn test_severity_default_is_medium() {
        let s = Severity::default();
        assert_eq!(s, Severity::Medium);
    }

    #[test]
    fn test_severity_serialize_critical() {
        let json = serde_json::to_string(&Severity::Critical).expect("should serialize");
        assert_eq!(json, r#""critical""#);
    }

    #[test]
    fn test_severity_serialize_high() {
        let json = serde_json::to_string(&Severity::High).expect("should serialize");
        assert_eq!(json, r#""high""#);
    }

    #[test]
    fn test_severity_serialize_medium() {
        let json = serde_json::to_string(&Severity::Medium).expect("should serialize");
        assert_eq!(json, r#""medium""#);
    }

    #[test]
    fn test_severity_serialize_low() {
        let json = serde_json::to_string(&Severity::Low).expect("should serialize");
        assert_eq!(json, r#""low""#);
    }

    #[test]
    fn test_severity_deserialize_roundtrip() {
        for severity in [
            Severity::Critical,
            Severity::High,
            Severity::Medium,
            Severity::Low,
        ] {
            let json = serde_json::to_string(&severity).expect("should serialize");
            let deserialized: Severity = serde_json::from_str(&json).expect("should deserialize");
            assert_eq!(deserialized, severity);
        }
    }

    #[test]
    fn test_severity_overrides_partial() {
        let overrides = SeverityOverrides {
            latest_tag: Some(Severity::Critical),
            missing_liveness: None,
            missing_readiness: None,
            high_restarts: Some(Severity::High),
            pending: None,
        };
        let json = serde_json::to_string(&overrides).expect("should serialize");
        assert!(json.contains("latestTag"));
        assert!(json.contains("highRestarts"));
        assert!(!json.contains("missingLiveness"));
        assert!(!json.contains("pending"));

        let deserialized: SeverityOverrides =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.latest_tag, Some(Severity::Critical));
        assert_eq!(deserialized.high_restarts, Some(Severity::High));
        assert_eq!(deserialized.missing_liveness, None);
    }

    #[test]
    fn test_severity_overrides_default_all_none() {
        let overrides = SeverityOverrides::default();
        assert_eq!(overrides.latest_tag, None);
        assert_eq!(overrides.missing_liveness, None);
        assert_eq!(overrides.missing_readiness, None);
        assert_eq!(overrides.high_restarts, None);
        assert_eq!(overrides.pending, None);
    }

    #[test]
    fn test_spec_with_severity_overrides() {
        let spec = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Critical),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).expect("should serialize");
        let deserialized: DevOpsPolicySpec =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(
            deserialized.severity_overrides.as_ref().unwrap().latest_tag,
            Some(Severity::Critical)
        );
    }

    #[test]
    fn test_backward_compat_no_severity_overrides() {
        // JSON from before Step 10 (no severity_overrides) should still deserialize
        let json = r#"{"forbidLatestTag":true,"maxRestartCount":3}"#;
        let spec: DevOpsPolicySpec =
            serde_json::from_str(json).expect("old JSON should deserialize");
        assert_eq!(spec.forbid_latest_tag, Some(true));
        assert_eq!(spec.severity_overrides, None);
    }

    #[test]
    fn test_spec_default_all_none() {
        let spec = DevOpsPolicySpec::default();
        assert_eq!(spec.forbid_latest_tag, None);
        assert_eq!(spec.require_liveness_probe, None);
        assert_eq!(spec.require_readiness_probe, None);
        assert_eq!(spec.max_restart_count, None);
        assert_eq!(spec.forbid_pending_duration, None);
        assert_eq!(spec.enforcement_mode, None);
        assert_eq!(spec.default_probe, None);
        assert_eq!(spec.default_resources, None);
        assert_eq!(spec.severity_overrides, None);
    }

    // ── AuditViolation tests ──

    #[test]
    fn test_audit_violation_serialization_roundtrip() {
        let violation = AuditViolation {
            pod_name: "web-abc123".to_string(),
            container_name: "nginx".to_string(),
            violation_type: "latest_tag".to_string(),
            severity: Severity::High,
            message: "container 'nginx' uses :latest tag".to_string(),
        };
        let json = serde_json::to_string(&violation).expect("should serialize");
        let deserialized: AuditViolation = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.pod_name, "web-abc123");
        assert_eq!(deserialized.severity, Severity::High);
        assert_eq!(deserialized.violation_type, "latest_tag");
    }

    // ── PolicyAuditResult CRD tests ──

    #[test]
    fn test_audit_result_crd_generates_valid_yaml() {
        let crd = PolicyAuditResult::crd();
        let yaml = serde_yaml::to_string(&crd).expect("CRD should serialize to YAML");
        assert!(yaml.contains("devops.stochastic.io"));
        assert!(yaml.contains("PolicyAuditResult"));
        assert!(yaml.contains("policyauditresults"));
    }

    #[test]
    fn test_audit_result_crd_api_group() {
        let crd = PolicyAuditResult::crd();
        assert_eq!(crd.spec.group, "devops.stochastic.io");
    }

    #[test]
    fn test_audit_result_crd_version() {
        let crd = PolicyAuditResult::crd();
        assert!(!crd.spec.versions.is_empty());
        assert_eq!(crd.spec.versions[0].name, "v1");
    }

    #[test]
    fn test_audit_result_crd_is_namespaced() {
        let crd = PolicyAuditResult::crd();
        assert_eq!(crd.spec.scope, "Namespaced");
    }

    #[test]
    fn test_audit_result_spec_serialization_roundtrip() {
        let spec = PolicyAuditResultSpec {
            policy_name: "default-policy".to_string(),
            cluster_name: Some("prod-cluster".to_string()),
            timestamp: "2026-02-24T10:00:00Z".to_string(),
            health_score: 85,
            total_violations: 5,
            total_pods: 20,
            classification: "Healthy".to_string(),
            violations: vec![AuditViolation {
                pod_name: "web-pod".to_string(),
                container_name: "nginx".to_string(),
                violation_type: "latest_tag".to_string(),
                severity: Severity::High,
                message: "uses :latest".to_string(),
            }],
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        let deserialized: PolicyAuditResultSpec =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.policy_name, "default-policy");
        assert_eq!(deserialized.cluster_name, Some("prod-cluster".to_string()));
        assert_eq!(deserialized.health_score, 85);
        assert_eq!(deserialized.violations.len(), 1);
    }

    #[test]
    fn test_audit_result_spec_without_cluster_name() {
        let spec = PolicyAuditResultSpec {
            policy_name: "local-policy".to_string(),
            cluster_name: None,
            timestamp: "2026-02-24T10:00:00Z".to_string(),
            health_score: 100,
            total_violations: 0,
            total_pods: 5,
            classification: "Healthy".to_string(),
            violations: vec![],
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        assert!(!json.contains("clusterName"));
    }

    #[test]
    fn test_audit_result_empty_violations_omitted() {
        let spec = PolicyAuditResultSpec {
            policy_name: "test".to_string(),
            cluster_name: None,
            timestamp: "2026-02-24T10:00:00Z".to_string(),
            health_score: 100,
            total_violations: 0,
            total_pods: 0,
            classification: "Healthy".to_string(),
            violations: vec![],
        };

        let json = serde_json::to_string(&spec).expect("should serialize");
        assert!(!json.contains("violations"));
    }

    #[test]
    fn test_two_crds_different_names() {
        let policy_crd = DevOpsPolicy::crd();
        let audit_crd = PolicyAuditResult::crd();
        assert_ne!(policy_crd.spec.names.kind, audit_crd.spec.names.kind);
        assert_ne!(policy_crd.spec.names.plural, audit_crd.spec.names.plural);
    }
}
