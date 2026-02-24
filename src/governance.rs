use k8s_openapi::api::core::v1::Pod;

use crate::crd::{DevOpsPolicySpec, Severity, SeverityOverrides};

/* ============================= WEIGHTS ============================= */

pub struct ScoringWeights {
    pub latest_tag: u32,
    pub missing_liveness: u32,
    pub missing_readiness: u32,
    pub high_restarts: u32,
    pub pending: u32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            latest_tag: 5,
            missing_liveness: 3,
            missing_readiness: 2,
            high_restarts: 6,
            pending: 4,
        }
    }
}

/* ============================= METRICS ============================= */

#[derive(Default, Clone)]
pub struct PodMetrics {
    pub total_pods: u32,
    pub latest_tag: u32,
    pub missing_liveness: u32,
    pub missing_readiness: u32,
    pub high_restarts: u32,
    pub pending: u32,
}

pub fn add_metrics(cluster: &mut PodMetrics, pod: &PodMetrics) {
    cluster.total_pods += pod.total_pods;
    cluster.latest_tag += pod.latest_tag;
    cluster.missing_liveness += pod.missing_liveness;
    cluster.missing_readiness += pod.missing_readiness;
    cluster.high_restarts += pod.high_restarts;
    cluster.pending += pod.pending;
}

pub fn subtract_metrics(cluster: &mut PodMetrics, pod: &PodMetrics) {
    cluster.total_pods = cluster.total_pods.saturating_sub(pod.total_pods);
    cluster.latest_tag = cluster.latest_tag.saturating_sub(pod.latest_tag);
    cluster.missing_liveness = cluster.missing_liveness.saturating_sub(pod.missing_liveness);
    cluster.missing_readiness = cluster.missing_readiness.saturating_sub(pod.missing_readiness);
    cluster.high_restarts = cluster.high_restarts.saturating_sub(pod.high_restarts);
    cluster.pending = cluster.pending.saturating_sub(pod.pending);
}

/* ============================= POD EVALUATION ============================= */

pub fn evaluate_pod(pod: &Pod) -> PodMetrics {
    let mut m = PodMetrics { total_pods: 1, ..Default::default() };

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if c.image.as_deref().unwrap_or("").ends_with(":latest") {
                m.latest_tag += 1;
            }
            if c.liveness_probe.is_none() {
                m.missing_liveness += 1;
            }
            if c.readiness_probe.is_none() {
                m.missing_readiness += 1;
            }
        }
    }

    if let Some(status) = &pod.status {
        if let Some(container_statuses) = &status.container_statuses {
            for cs in container_statuses {
                if cs.restart_count > 3 {
                    let capped = (cs.restart_count.max(0) as u32).min(5);
                    m.high_restarts += capped;
                }
            }
        }

        if status.phase.as_deref() == Some("Pending") {
            m.pending += 1;
        }
    }

    m
}

pub fn detect_violations(pod: &Pod) -> Vec<&'static str> {
    let mut violations = Vec::new();

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if c.image.as_deref().unwrap_or("").ends_with(":latest") {
                violations.push("latest_tag");
            }
            if c.liveness_probe.is_none() {
                violations.push("missing_liveness");
            }
            if c.readiness_probe.is_none() {
                violations.push("missing_readiness");
            }
        }
    }

    violations
}

/* ============================= NAMESPACE FILTER ============================= */

pub fn is_system_namespace(ns: &str) -> bool {
    ns.starts_with("kube-") || ns.ends_with("-system") || matches!(
        ns,
        "cert-manager" | "istio-system" | "monitoring" | "observability" | "argocd"
    )
}

/* ============================= SCORING ============================= */

pub fn calculate_health_score(metrics: &PodMetrics) -> u32 {
    if metrics.total_pods == 0 {
        return 100;
    }

    let weights = ScoringWeights::default();

    let raw = (metrics.latest_tag * weights.latest_tag)
        + (metrics.missing_liveness * weights.missing_liveness)
        + (metrics.missing_readiness * weights.missing_readiness)
        + (metrics.high_restarts * weights.high_restarts)
        + (metrics.pending * weights.pending);

    let per_pod = raw / metrics.total_pods;
    let capped = per_pod.min(100);

    100 - capped
}

pub fn classify_health(score: u32) -> &'static str {
    match score {
        80..=100 => "Healthy",
        60..=79 => "Stable",
        40..=59 => "Degraded",
        _ => "Critical",
    }
}

/* ============================= POLICY-AWARE EVALUATION ============================= */

/// Evaluate a pod against a specific DevOpsPolicy.
///
/// Only checks that the policy explicitly enables are counted.
/// Omitted fields (`None`) are treated as disabled (not checked).
pub fn evaluate_pod_with_policy(pod: &Pod, policy: &DevOpsPolicySpec) -> PodMetrics {
    let mut m = PodMetrics { total_pods: 1, ..Default::default() };

    let restart_threshold = policy.max_restart_count.unwrap_or(i32::MAX);

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if policy.forbid_latest_tag.unwrap_or(false)
                && c.image.as_deref().unwrap_or("").ends_with(":latest")
            {
                m.latest_tag += 1;
            }
            if policy.require_liveness_probe.unwrap_or(false) && c.liveness_probe.is_none() {
                m.missing_liveness += 1;
            }
            if policy.require_readiness_probe.unwrap_or(false) && c.readiness_probe.is_none() {
                m.missing_readiness += 1;
            }
        }
    }

    if let Some(status) = &pod.status {
        if policy.max_restart_count.is_some()
            && let Some(container_statuses) = &status.container_statuses
        {
            for cs in container_statuses {
                if cs.restart_count > restart_threshold {
                    let capped = (cs.restart_count.max(0) as u32).min(5);
                    m.high_restarts += capped;
                }
            }
        }

        if policy.forbid_pending_duration.is_some()
            && status.phase.as_deref() == Some("Pending")
        {
            m.pending += 1;
        }
    }

    m
}

/* ============================= SEVERITY-AWARE SCORING ============================= */

/// Detailed violation with severity, pod name, and container info.
#[derive(Debug, Clone, PartialEq)]
pub struct ViolationDetail {
    pub violation_type: String,
    pub severity: Severity,
    pub pod_name: String,
    pub namespace: String,
    pub container_name: String,
    pub message: String,
}

/// Return the default severity for a given violation type.
pub fn default_severity(violation_type: &str) -> Severity {
    match violation_type {
        "latest_tag" => Severity::High,
        "missing_liveness" => Severity::Medium,
        "missing_readiness" => Severity::Low,
        "high_restarts" => Severity::Critical,
        "pending" => Severity::Medium,
        _ => Severity::Medium,
    }
}

/// Return the scoring multiplier for a severity level.
pub fn severity_multiplier(severity: &Severity) -> u32 {
    match severity {
        Severity::Critical => 3,
        Severity::High => 2,
        Severity::Medium => 1,
        Severity::Low => 1,
    }
}

/// Resolve the effective severity for a violation type, using overrides if present.
pub fn effective_severity(
    violation_type: &str,
    overrides: Option<&SeverityOverrides>,
) -> Severity {
    if let Some(ovr) = overrides {
        let specific = match violation_type {
            "latest_tag" => &ovr.latest_tag,
            "missing_liveness" => &ovr.missing_liveness,
            "missing_readiness" => &ovr.missing_readiness,
            "high_restarts" => &ovr.high_restarts,
            "pending" => &ovr.pending,
            _ => &None,
        };
        if let Some(s) = specific {
            return s.clone();
        }
    }
    default_severity(violation_type)
}

/// Calculate health score with severity multipliers applied to base weights.
pub fn calculate_health_score_with_severity(
    metrics: &PodMetrics,
    overrides: Option<&SeverityOverrides>,
) -> u32 {
    if metrics.total_pods == 0 {
        return 100;
    }

    let weights = ScoringWeights::default();

    let raw = (metrics.latest_tag * weights.latest_tag * severity_multiplier(&effective_severity("latest_tag", overrides)))
        + (metrics.missing_liveness * weights.missing_liveness * severity_multiplier(&effective_severity("missing_liveness", overrides)))
        + (metrics.missing_readiness * weights.missing_readiness * severity_multiplier(&effective_severity("missing_readiness", overrides)))
        + (metrics.high_restarts * weights.high_restarts * severity_multiplier(&effective_severity("high_restarts", overrides)))
        + (metrics.pending * weights.pending * severity_multiplier(&effective_severity("pending", overrides)));

    let per_pod = raw / metrics.total_pods;
    let capped = per_pod.min(100);

    100 - capped
}

/// Detect policy violations with full structured detail.
pub fn detect_violations_detailed(pod: &Pod, policy: &DevOpsPolicySpec) -> Vec<ViolationDetail> {
    let mut violations = Vec::new();

    let pod_name = pod
        .metadata
        .name
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let namespace = pod
        .metadata
        .namespace
        .as_deref()
        .unwrap_or("default")
        .to_string();

    let overrides = policy.severity_overrides.as_ref();
    let restart_threshold = policy.max_restart_count.unwrap_or(i32::MAX);

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if policy.forbid_latest_tag.unwrap_or(false)
                && c.image.as_deref().unwrap_or("").ends_with(":latest")
            {
                violations.push(ViolationDetail {
                    violation_type: "latest_tag".to_string(),
                    severity: effective_severity("latest_tag", overrides),
                    pod_name: pod_name.clone(),
                    namespace: namespace.clone(),
                    container_name: c.name.clone(),
                    message: format!("container '{}' uses :latest tag", c.name),
                });
            }
            if policy.require_liveness_probe.unwrap_or(false) && c.liveness_probe.is_none() {
                violations.push(ViolationDetail {
                    violation_type: "missing_liveness".to_string(),
                    severity: effective_severity("missing_liveness", overrides),
                    pod_name: pod_name.clone(),
                    namespace: namespace.clone(),
                    container_name: c.name.clone(),
                    message: format!("container '{}' missing liveness probe", c.name),
                });
            }
            if policy.require_readiness_probe.unwrap_or(false) && c.readiness_probe.is_none() {
                violations.push(ViolationDetail {
                    violation_type: "missing_readiness".to_string(),
                    severity: effective_severity("missing_readiness", overrides),
                    pod_name: pod_name.clone(),
                    namespace: namespace.clone(),
                    container_name: c.name.clone(),
                    message: format!("container '{}' missing readiness probe", c.name),
                });
            }
        }
    }

    if let Some(status) = &pod.status {
        if policy.max_restart_count.is_some()
            && let Some(container_statuses) = &status.container_statuses
        {
            for cs in container_statuses {
                if cs.restart_count > restart_threshold {
                    violations.push(ViolationDetail {
                        violation_type: "high_restarts".to_string(),
                        severity: effective_severity("high_restarts", overrides),
                        pod_name: pod_name.clone(),
                        namespace: namespace.clone(),
                        container_name: cs.name.clone(),
                        message: format!(
                            "container '{}' has {} restarts (threshold: {})",
                            cs.name, cs.restart_count, restart_threshold
                        ),
                    });
                }
            }
        }

        if policy.forbid_pending_duration.is_some()
            && status.phase.as_deref() == Some("Pending")
        {
            violations.push(ViolationDetail {
                violation_type: "pending".to_string(),
                severity: effective_severity("pending", overrides),
                pod_name: pod_name.clone(),
                namespace: namespace.clone(),
                container_name: String::new(),
                message: "pod is in Pending phase".to_string(),
            });
        }
    }

    violations
}

/* ============================= POLICY-AWARE VIOLATION DETECTION ============================= */

/// Detect policy violations for a pod, filtered by which checks the policy enables.
///
/// Returns a list of violation labels only for checks the policy has turned on.
pub fn detect_violations_with_policy(pod: &Pod, policy: &DevOpsPolicySpec) -> Vec<&'static str> {
    let mut violations = Vec::new();

    let restart_threshold = policy.max_restart_count.unwrap_or(i32::MAX);

    if let Some(spec) = &pod.spec {
        for c in &spec.containers {
            if policy.forbid_latest_tag.unwrap_or(false)
                && c.image.as_deref().unwrap_or("").ends_with(":latest")
            {
                violations.push("latest_tag");
            }
            if policy.require_liveness_probe.unwrap_or(false) && c.liveness_probe.is_none() {
                violations.push("missing_liveness");
            }
            if policy.require_readiness_probe.unwrap_or(false) && c.readiness_probe.is_none() {
                violations.push("missing_readiness");
            }
        }
    }

    if let Some(status) = &pod.status {
        if policy.max_restart_count.is_some()
            && let Some(container_statuses) = &status.container_statuses
        {
            for cs in container_statuses {
                if cs.restart_count > restart_threshold {
                    violations.push("high_restarts");
                }
            }
        }

        if policy.forbid_pending_duration.is_some()
            && status.phase.as_deref() == Some("Pending")
        {
            violations.push("pending");
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{
        Container, ContainerStatus, Pod, PodSpec, PodStatus, Probe,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn make_test_pod(
        name: &str,
        namespace: &str,
        image: &str,
        has_liveness: bool,
        has_readiness: bool,
        restart_count: i32,
        phase: &str,
    ) -> Pod {
        let probes = |has: bool| -> Option<Probe> {
            if has { Some(Probe::default()) } else { None }
        };

        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "main".to_string(),
                    image: Some(image.to_string()),
                    liveness_probe: probes(has_liveness),
                    readiness_probe: probes(has_readiness),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some(phase.to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "main".to_string(),
                    restart_count,
                    ready: phase == "Running",
                    image: image.to_string(),
                    image_id: String::new(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    // ── is_system_namespace ──

    #[test]
    fn test_is_system_kube_system() {
        assert!(is_system_namespace("kube-system"));
    }

    #[test]
    fn test_is_system_kube_flannel() {
        assert!(is_system_namespace("kube-flannel"));
    }

    #[test]
    fn test_is_system_longhorn_system() {
        assert!(is_system_namespace("longhorn-system"));
    }

    #[test]
    fn test_is_system_cert_manager() {
        assert!(is_system_namespace("cert-manager"));
    }

    #[test]
    fn test_is_system_monitoring() {
        assert!(is_system_namespace("monitoring"));
    }

    #[test]
    fn test_is_system_argocd() {
        assert!(is_system_namespace("argocd"));
    }

    #[test]
    fn test_not_system_default() {
        assert!(!is_system_namespace("default"));
    }

    #[test]
    fn test_not_system_production() {
        assert!(!is_system_namespace("production"));
    }

    // ── evaluate_pod ──

    #[test]
    fn test_evaluate_latest_tag() {
        let pod = make_test_pod("p", "default", "nginx:latest", true, true, 0, "Running");
        let m = evaluate_pod(&pod);
        assert_eq!(m.latest_tag, 1);
    }

    #[test]
    fn test_evaluate_proper_tag() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let m = evaluate_pod(&pod);
        assert_eq!(m.latest_tag, 0);
    }

    #[test]
    fn test_evaluate_missing_probes() {
        let pod = make_test_pod("p", "default", "nginx:1.25", false, false, 0, "Running");
        let m = evaluate_pod(&pod);
        assert_eq!(m.missing_liveness, 1);
        assert_eq!(m.missing_readiness, 1);
    }

    #[test]
    fn test_evaluate_with_probes() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let m = evaluate_pod(&pod);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
    }

    #[test]
    fn test_evaluate_high_restarts() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 10, "Running");
        let m = evaluate_pod(&pod);
        assert!(m.high_restarts > 0);
    }

    #[test]
    fn test_evaluate_restarts_at_threshold() {
        // restart_count == 3 should NOT trigger high_restarts (> 3 required)
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 3, "Running");
        let m = evaluate_pod(&pod);
        assert_eq!(m.high_restarts, 0);
    }

    #[test]
    fn test_evaluate_pending_phase() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Pending");
        let m = evaluate_pod(&pod);
        assert_eq!(m.pending, 1);
    }

    #[test]
    fn test_evaluate_multi_container() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("multi".to_string()),
                namespace: Some("default".to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![
                    Container {
                        name: "a".to_string(),
                        image: Some("img:latest".to_string()),
                        ..Default::default()
                    },
                    Container {
                        name: "b".to_string(),
                        image: Some("img:latest".to_string()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            status: Some(PodStatus::default()),
        };
        let m = evaluate_pod(&pod);
        assert_eq!(m.latest_tag, 2);
        assert_eq!(m.missing_liveness, 2);
        assert_eq!(m.missing_readiness, 2);
    }

    #[test]
    fn test_evaluate_no_spec() {
        let pod = Pod {
            metadata: ObjectMeta::default(),
            spec: None,
            status: None,
        };
        let m = evaluate_pod(&pod);
        assert_eq!(m.total_pods, 1);
        assert_eq!(m.latest_tag, 0);
    }

    #[test]
    fn test_evaluate_no_status() {
        let pod = Pod {
            metadata: ObjectMeta::default(),
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "c".to_string(),
                    image: Some("img:latest".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: None,
        };
        let m = evaluate_pod(&pod);
        assert_eq!(m.latest_tag, 1);
        assert_eq!(m.high_restarts, 0);
        assert_eq!(m.pending, 0);
    }

    // ── detect_violations ──

    #[test]
    fn test_detect_violations_compliant() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let v = detect_violations(&pod);
        assert!(v.is_empty());
    }

    #[test]
    fn test_detect_violations_fully_noncompliant() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 0, "Running");
        let v = detect_violations(&pod);
        assert!(v.contains(&"latest_tag"));
        assert!(v.contains(&"missing_liveness"));
        assert!(v.contains(&"missing_readiness"));
    }

    #[test]
    fn test_detect_violations_only_latest() {
        let pod = make_test_pod("p", "default", "nginx:latest", true, true, 0, "Running");
        let v = detect_violations(&pod);
        assert_eq!(v, vec!["latest_tag"]);
    }

    #[test]
    fn test_detect_violations_no_spec() {
        let pod = Pod {
            metadata: ObjectMeta::default(),
            spec: None,
            status: None,
        };
        let v = detect_violations(&pod);
        assert!(v.is_empty());
    }

    // ── add_metrics / subtract_metrics ──

    #[test]
    fn test_add_metrics_basic() {
        let mut cluster = PodMetrics::default();
        let pod = PodMetrics { total_pods: 1, latest_tag: 1, missing_liveness: 1, ..Default::default() };
        add_metrics(&mut cluster, &pod);
        assert_eq!(cluster.total_pods, 1);
        assert_eq!(cluster.latest_tag, 1);
        assert_eq!(cluster.missing_liveness, 1);
    }

    #[test]
    fn test_subtract_metrics_basic() {
        let mut cluster = PodMetrics { total_pods: 5, latest_tag: 3, ..Default::default() };
        let pod = PodMetrics { total_pods: 2, latest_tag: 1, ..Default::default() };
        subtract_metrics(&mut cluster, &pod);
        assert_eq!(cluster.total_pods, 3);
        assert_eq!(cluster.latest_tag, 2);
    }

    #[test]
    fn test_subtract_metrics_saturating_underflow() {
        let mut cluster = PodMetrics { total_pods: 1, ..Default::default() };
        let pod = PodMetrics { total_pods: 5, ..Default::default() };
        subtract_metrics(&mut cluster, &pod);
        assert_eq!(cluster.total_pods, 0);
    }

    #[test]
    fn test_add_then_subtract_roundtrip() {
        let mut cluster = PodMetrics::default();
        let pod = PodMetrics {
            total_pods: 1, latest_tag: 1, missing_liveness: 1,
            missing_readiness: 1, high_restarts: 2, pending: 1,
        };
        add_metrics(&mut cluster, &pod);
        subtract_metrics(&mut cluster, &pod);
        assert_eq!(cluster.total_pods, 0);
        assert_eq!(cluster.latest_tag, 0);
        assert_eq!(cluster.missing_liveness, 0);
        assert_eq!(cluster.missing_readiness, 0);
        assert_eq!(cluster.high_restarts, 0);
        assert_eq!(cluster.pending, 0);
    }

    // ── calculate_health_score ──

    #[test]
    fn test_score_zero_pods() {
        let m = PodMetrics::default();
        assert_eq!(calculate_health_score(&m), 100);
    }

    #[test]
    fn test_score_fully_healthy() {
        let m = PodMetrics { total_pods: 5, ..Default::default() };
        assert_eq!(calculate_health_score(&m), 100);
    }

    #[test]
    fn test_score_fully_degraded() {
        // 1 pod with every violation maxed out
        let m = PodMetrics {
            total_pods: 1,
            latest_tag: 1,
            missing_liveness: 1,
            missing_readiness: 1,
            high_restarts: 5,
            pending: 1,
        };
        let score = calculate_health_score(&m);
        // raw = 5+3+2+30+4 = 44, per_pod = 44, capped = 44 → 100-44 = 56
        assert_eq!(score, 56);
    }

    #[test]
    fn test_score_floor_zero() {
        // Extreme violations → score should floor at 0
        let m = PodMetrics {
            total_pods: 1,
            latest_tag: 10,
            missing_liveness: 10,
            missing_readiness: 10,
            high_restarts: 10,
            pending: 10,
        };
        let score = calculate_health_score(&m);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_score_capped_at_100() {
        // Zero violations → 100
        let m = PodMetrics { total_pods: 100, ..Default::default() };
        assert_eq!(calculate_health_score(&m), 100);
    }

    // ── classify_health ──

    #[test]
    fn test_classify_100() {
        assert_eq!(classify_health(100), "Healthy");
    }

    #[test]
    fn test_classify_80() {
        assert_eq!(classify_health(80), "Healthy");
    }

    #[test]
    fn test_classify_79() {
        assert_eq!(classify_health(79), "Stable");
    }

    #[test]
    fn test_classify_60() {
        assert_eq!(classify_health(60), "Stable");
    }

    #[test]
    fn test_classify_59() {
        assert_eq!(classify_health(59), "Degraded");
    }

    #[test]
    fn test_classify_40() {
        assert_eq!(classify_health(40), "Degraded");
    }

    #[test]
    fn test_classify_39() {
        assert_eq!(classify_health(39), "Critical");
    }

    #[test]
    fn test_classify_0() {
        assert_eq!(classify_health(0), "Critical");
    }

    // ── defaults ──

    #[test]
    fn test_scoring_weights_default() {
        let w = ScoringWeights::default();
        assert_eq!(w.latest_tag, 5);
        assert_eq!(w.missing_liveness, 3);
        assert_eq!(w.missing_readiness, 2);
        assert_eq!(w.high_restarts, 6);
        assert_eq!(w.pending, 4);
    }

    #[test]
    fn test_pod_metrics_default() {
        let m = PodMetrics::default();
        assert_eq!(m.total_pods, 0);
        assert_eq!(m.latest_tag, 0);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
        assert_eq!(m.high_restarts, 0);
        assert_eq!(m.pending, 0);
    }

    // ── policy-aware evaluate_pod_with_policy ──

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

    #[test]
    fn test_policy_eval_all_enabled_catches_violations() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let m = evaluate_pod_with_policy(&pod, &all_enabled_policy());
        assert_eq!(m.total_pods, 1);
        assert_eq!(m.latest_tag, 1);
        assert_eq!(m.missing_liveness, 1);
        assert_eq!(m.missing_readiness, 1);
        assert!(m.high_restarts > 0);
        assert_eq!(m.pending, 1);
    }

    #[test]
    fn test_policy_eval_empty_policy_skips_all_checks() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let m = evaluate_pod_with_policy(&pod, &empty_policy());
        assert_eq!(m.total_pods, 1);
        assert_eq!(m.latest_tag, 0);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
        assert_eq!(m.high_restarts, 0);
        assert_eq!(m.pending, 0);
    }

    #[test]
    fn test_policy_eval_only_latest_tag_enabled() {
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            ..empty_policy()
        };
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let m = evaluate_pod_with_policy(&pod, &policy);
        assert_eq!(m.latest_tag, 1);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
        assert_eq!(m.high_restarts, 0);
        assert_eq!(m.pending, 0);
    }

    #[test]
    fn test_policy_eval_disabled_false_same_as_none() {
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(false),
            require_liveness_probe: Some(false),
            require_readiness_probe: Some(false),
            ..Default::default()
        };
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let m = evaluate_pod_with_policy(&pod, &policy);
        assert_eq!(m.latest_tag, 0);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
    }

    #[test]
    fn test_policy_eval_compliant_pod_zero_violations() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let m = evaluate_pod_with_policy(&pod, &all_enabled_policy());
        assert_eq!(m.latest_tag, 0);
        assert_eq!(m.missing_liveness, 0);
        assert_eq!(m.missing_readiness, 0);
        assert_eq!(m.high_restarts, 0);
        assert_eq!(m.pending, 0);
    }

    #[test]
    fn test_policy_eval_custom_restart_threshold() {
        let policy = DevOpsPolicySpec {
            max_restart_count: Some(5),
            ..empty_policy()
        };
        // restart_count 4 is under threshold of 5 → no violation
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 4, "Running");
        let m = evaluate_pod_with_policy(&pod, &policy);
        assert_eq!(m.high_restarts, 0);

        // restart_count 6 exceeds threshold of 5 → violation
        let pod2 = make_test_pod("p", "default", "nginx:1.25", true, true, 6, "Running");
        let m2 = evaluate_pod_with_policy(&pod2, &policy);
        assert!(m2.high_restarts > 0);
    }

    // ── policy-aware detect_violations_with_policy ──

    #[test]
    fn test_policy_detect_all_enabled_catches_all() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let v = detect_violations_with_policy(&pod, &all_enabled_policy());
        assert!(v.contains(&"latest_tag"));
        assert!(v.contains(&"missing_liveness"));
        assert!(v.contains(&"missing_readiness"));
        assert!(v.contains(&"high_restarts"));
        assert!(v.contains(&"pending"));
    }

    #[test]
    fn test_policy_detect_empty_policy_no_violations() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let v = detect_violations_with_policy(&pod, &empty_policy());
        assert!(v.is_empty());
    }

    #[test]
    fn test_policy_detect_compliant_pod_no_violations() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let v = detect_violations_with_policy(&pod, &all_enabled_policy());
        assert!(v.is_empty());
    }

    #[test]
    fn test_policy_detect_only_probes_enabled() {
        let policy = DevOpsPolicySpec {
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            ..empty_policy()
        };
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let v = detect_violations_with_policy(&pod, &policy);
        assert!(v.contains(&"missing_liveness"));
        assert!(v.contains(&"missing_readiness"));
        assert!(!v.contains(&"latest_tag"));
        assert!(!v.contains(&"high_restarts"));
        assert!(!v.contains(&"pending"));
    }

    // ── severity tests ──

    #[test]
    fn test_default_severity_values() {
        assert_eq!(default_severity("latest_tag"), Severity::High);
        assert_eq!(default_severity("missing_liveness"), Severity::Medium);
        assert_eq!(default_severity("missing_readiness"), Severity::Low);
        assert_eq!(default_severity("high_restarts"), Severity::Critical);
        assert_eq!(default_severity("pending"), Severity::Medium);
        assert_eq!(default_severity("unknown"), Severity::Medium);
    }

    #[test]
    fn test_severity_multiplier_values() {
        assert_eq!(severity_multiplier(&Severity::Critical), 3);
        assert_eq!(severity_multiplier(&Severity::High), 2);
        assert_eq!(severity_multiplier(&Severity::Medium), 1);
        assert_eq!(severity_multiplier(&Severity::Low), 1);
    }

    #[test]
    fn test_effective_severity_no_overrides() {
        assert_eq!(effective_severity("latest_tag", None), Severity::High);
        assert_eq!(effective_severity("high_restarts", None), Severity::Critical);
    }

    #[test]
    fn test_effective_severity_with_override() {
        let overrides = SeverityOverrides {
            latest_tag: Some(Severity::Low),
            ..Default::default()
        };
        assert_eq!(
            effective_severity("latest_tag", Some(&overrides)),
            Severity::Low
        );
        // Non-overridden check uses default
        assert_eq!(
            effective_severity("high_restarts", Some(&overrides)),
            Severity::Critical
        );
    }

    #[test]
    fn test_health_score_with_severity_no_pods() {
        let m = PodMetrics::default();
        assert_eq!(calculate_health_score_with_severity(&m, None), 100);
    }

    #[test]
    fn test_health_score_with_severity_healthy() {
        let m = PodMetrics { total_pods: 5, ..Default::default() };
        assert_eq!(calculate_health_score_with_severity(&m, None), 100);
    }

    #[test]
    fn test_health_score_with_severity_multipliers_increase_penalty() {
        // One pod with 1 latest_tag violation
        let m = PodMetrics {
            total_pods: 1,
            latest_tag: 1,
            ..Default::default()
        };
        let without = calculate_health_score(&m);
        let with = calculate_health_score_with_severity(&m, None);
        // latest_tag default severity is High (x2), so with severity should penalize more
        assert!(with < without, "severity score {} should be less than base score {}", with, without);
    }

    #[test]
    fn test_health_score_severity_overrides_lower_penalty() {
        let m = PodMetrics {
            total_pods: 1,
            latest_tag: 1,
            ..Default::default()
        };
        let overrides_low = SeverityOverrides {
            latest_tag: Some(Severity::Low),
            ..Default::default()
        };
        let overrides_critical = SeverityOverrides {
            latest_tag: Some(Severity::Critical),
            ..Default::default()
        };
        let score_low = calculate_health_score_with_severity(&m, Some(&overrides_low));
        let score_critical = calculate_health_score_with_severity(&m, Some(&overrides_critical));
        assert!(
            score_low > score_critical,
            "Low severity score {} should be higher than Critical {}",
            score_low,
            score_critical
        );
    }

    #[test]
    fn test_health_score_severity_backward_compat() {
        // Score with all Low severity overrides and multiplier=1 should match base
        let m = PodMetrics {
            total_pods: 3,
            latest_tag: 1,
            missing_liveness: 1,
            ..Default::default()
        };
        // Base scoring and severity scoring with all multiplier=1 should give different results
        // because default severities are not all Low
        let base = calculate_health_score(&m);
        let overrides = SeverityOverrides {
            latest_tag: Some(Severity::Low),
            missing_liveness: Some(Severity::Low),
            missing_readiness: Some(Severity::Low),
            high_restarts: Some(Severity::Low),
            pending: Some(Severity::Low),
        };
        let with_all_low = calculate_health_score_with_severity(&m, Some(&overrides));
        // With all Low (multiplier=1), it should match the base score
        assert_eq!(base, with_all_low);
    }

    // ── detect_violations_detailed tests ──

    #[test]
    fn test_detect_violations_detailed_all_enabled() {
        let pod = make_test_pod("web-pod", "prod", "nginx:latest", false, false, 10, "Pending");
        let policy = all_enabled_policy();
        let details = detect_violations_detailed(&pod, &policy);
        assert!(details.len() >= 4, "should have at least 4 violations, got {}", details.len());
        assert!(details.iter().any(|v| v.violation_type == "latest_tag"));
        assert!(details.iter().any(|v| v.violation_type == "missing_liveness"));
        assert!(details.iter().any(|v| v.violation_type == "missing_readiness"));
        assert!(details.iter().any(|v| v.violation_type == "high_restarts"));
    }

    #[test]
    fn test_detect_violations_detailed_pod_name() {
        let pod = make_test_pod("my-pod", "my-ns", "nginx:latest", true, true, 0, "Running");
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            ..Default::default()
        };
        let details = detect_violations_detailed(&pod, &policy);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].pod_name, "my-pod");
        assert_eq!(details[0].namespace, "my-ns");
        assert_eq!(details[0].container_name, "main");
    }

    #[test]
    fn test_detect_violations_detailed_empty_policy() {
        let pod = make_test_pod("p", "default", "nginx:latest", false, false, 10, "Pending");
        let details = detect_violations_detailed(&pod, &DevOpsPolicySpec::default());
        assert!(details.is_empty());
    }

    #[test]
    fn test_detect_violations_detailed_compliant_pod() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Running");
        let details = detect_violations_detailed(&pod, &all_enabled_policy());
        assert!(details.is_empty());
    }

    #[test]
    fn test_detect_violations_detailed_severity_overrides() {
        let pod = make_test_pod("p", "default", "nginx:latest", true, true, 0, "Running");
        let policy = DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            severity_overrides: Some(SeverityOverrides {
                latest_tag: Some(Severity::Low),
                ..Default::default()
            }),
            ..Default::default()
        };
        let details = detect_violations_detailed(&pod, &policy);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].severity, Severity::Low);
    }

    #[test]
    fn test_detect_violations_detailed_pending() {
        let pod = make_test_pod("p", "default", "nginx:1.25", true, true, 0, "Pending");
        let policy = DevOpsPolicySpec {
            forbid_pending_duration: Some(300),
            ..Default::default()
        };
        let details = detect_violations_detailed(&pod, &policy);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].violation_type, "pending");
        assert!(details[0].container_name.is_empty());
    }
}
