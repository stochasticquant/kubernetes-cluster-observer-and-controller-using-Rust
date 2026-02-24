use crate::crd::DevOpsPolicySpec;
use crate::governance::{self, ViolationDetail};

/* ============================= TYPES ============================= */

/// Evaluation result for a single cluster.
#[derive(Debug, Clone)]
pub struct ClusterEvaluation {
    pub context_name: String,
    pub health_score: u32,
    pub classification: String,
    pub total_pods: u32,
    pub total_violations: u32,
    pub violations: Vec<ViolationDetail>,
}

/// Aggregated report across multiple clusters.
#[derive(Debug, Clone)]
pub struct MultiClusterReport {
    pub clusters: Vec<ClusterEvaluation>,
    pub aggregate_score: u32,
    pub aggregate_classification: String,
}

/* ============================= KUBECONFIG UTILITIES ============================= */

/// List available kubeconfig contexts.
pub fn list_contexts() -> anyhow::Result<Vec<String>> {
    let kubeconfig = kube::config::Kubeconfig::read()?;
    Ok(kubeconfig.contexts.iter().map(|c| c.name.clone()).collect())
}

/// Create a kube Client for a specific kubeconfig context.
pub async fn client_for_context(context: &str) -> anyhow::Result<kube::Client> {
    let kubeconfig = kube::config::Kubeconfig::read()?;
    let config = kube::Config::from_custom_kubeconfig(
        kubeconfig,
        &kube::config::KubeConfigOptions {
            context: Some(context.to_string()),
            ..Default::default()
        },
    )
    .await?;
    Ok(kube::Client::try_from(config)?)
}

/* ============================= EVALUATION ============================= */

/// Evaluate a cluster's pods against a policy (requires a connected client).
pub async fn evaluate_cluster(
    client: &kube::Client,
    context_name: &str,
    policy: &DevOpsPolicySpec,
) -> anyhow::Result<ClusterEvaluation> {
    use k8s_openapi::api::core::v1::Pod;
    use kube::Api;

    let pods_api: Api<Pod> = Api::all(client.clone());
    let pod_list = pods_api.list(&Default::default()).await?;

    let mut aggregate = governance::PodMetrics::default();
    let mut all_violations = Vec::new();
    let mut total_violation_count: u32 = 0;

    for pod in &pod_list.items {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }

        let contribution = governance::evaluate_pod_with_policy(pod, policy);
        governance::add_metrics(&mut aggregate, &contribution);

        let details = governance::detect_violations_detailed(pod, policy);
        total_violation_count += details.len() as u32;
        all_violations.extend(details);
    }

    let health_score = governance::calculate_health_score(&aggregate);
    let classification = governance::classify_health(health_score).to_string();

    Ok(ClusterEvaluation {
        context_name: context_name.to_string(),
        health_score,
        classification,
        total_pods: aggregate.total_pods,
        total_violations: total_violation_count,
        violations: all_violations,
    })
}

/// Aggregate multiple cluster evaluations into a unified report.
pub fn aggregate_report(evaluations: Vec<ClusterEvaluation>) -> MultiClusterReport {
    if evaluations.is_empty() {
        return MultiClusterReport {
            clusters: evaluations,
            aggregate_score: 100,
            aggregate_classification: "Healthy".to_string(),
        };
    }

    let total_pods: u32 = evaluations.iter().map(|e| e.total_pods).sum();
    let aggregate_score = if total_pods == 0 {
        100
    } else {
        let weighted_sum: u64 = evaluations
            .iter()
            .map(|e| e.health_score as u64 * e.total_pods as u64)
            .sum();
        (weighted_sum / total_pods as u64) as u32
    };

    let aggregate_classification = governance::classify_health(aggregate_score).to_string();

    MultiClusterReport {
        clusters: evaluations,
        aggregate_score,
        aggregate_classification,
    }
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    fn make_evaluation(name: &str, score: u32, pods: u32, violations: u32) -> ClusterEvaluation {
        ClusterEvaluation {
            context_name: name.to_string(),
            health_score: score,
            classification: governance::classify_health(score).to_string(),
            total_pods: pods,
            total_violations: violations,
            violations: vec![],
        }
    }

    #[test]
    fn test_aggregate_empty_clusters() {
        let report = aggregate_report(vec![]);
        assert_eq!(report.aggregate_score, 100);
        assert_eq!(report.aggregate_classification, "Healthy");
        assert!(report.clusters.is_empty());
    }

    #[test]
    fn test_aggregate_single_cluster() {
        let eval = make_evaluation("cluster-1", 85, 10, 3);
        let report = aggregate_report(vec![eval]);
        assert_eq!(report.aggregate_score, 85);
        assert_eq!(report.aggregate_classification, "Healthy");
        assert_eq!(report.clusters.len(), 1);
    }

    #[test]
    fn test_aggregate_weighted_average() {
        // cluster-1: score 100, 10 pods
        // cluster-2: score 50, 10 pods
        // weighted average: (100*10 + 50*10) / 20 = 75
        let evals = vec![
            make_evaluation("cluster-1", 100, 10, 0),
            make_evaluation("cluster-2", 50, 10, 5),
        ];
        let report = aggregate_report(evals);
        assert_eq!(report.aggregate_score, 75);
        assert_eq!(report.aggregate_classification, "Stable");
    }

    #[test]
    fn test_aggregate_weighted_by_pod_count() {
        // cluster-1: score 90, 100 pods (large)
        // cluster-2: score 20, 1 pod (small)
        // weighted average heavily weighted toward cluster-1
        let evals = vec![
            make_evaluation("cluster-1", 90, 100, 10),
            make_evaluation("cluster-2", 20, 1, 5),
        ];
        let report = aggregate_report(evals);
        // (90*100 + 20*1) / 101 = 9020/101 = 89
        assert_eq!(report.aggregate_score, 89);
        assert_eq!(report.aggregate_classification, "Healthy");
    }

    #[test]
    fn test_aggregate_all_zero_pods() {
        let evals = vec![
            make_evaluation("cluster-1", 100, 0, 0),
            make_evaluation("cluster-2", 100, 0, 0),
        ];
        let report = aggregate_report(evals);
        assert_eq!(report.aggregate_score, 100);
    }

    #[test]
    fn test_aggregate_three_clusters() {
        let evals = vec![
            make_evaluation("prod", 95, 50, 5),
            make_evaluation("staging", 60, 20, 15),
            make_evaluation("dev", 40, 10, 25),
        ];
        let report = aggregate_report(evals);
        // (95*50 + 60*20 + 40*10) / 80 = (4750 + 1200 + 400) / 80 = 6350/80 = 79
        assert_eq!(report.aggregate_score, 79);
        assert_eq!(report.aggregate_classification, "Stable");
        assert_eq!(report.clusters.len(), 3);
    }

    #[test]
    fn test_cluster_evaluation_fields() {
        let eval = make_evaluation("test-cluster", 72, 15, 8);
        assert_eq!(eval.context_name, "test-cluster");
        assert_eq!(eval.health_score, 72);
        assert_eq!(eval.classification, "Stable");
        assert_eq!(eval.total_pods, 15);
        assert_eq!(eval.total_violations, 8);
    }

    #[test]
    fn test_report_classification_matches_score() {
        let evals = vec![make_evaluation("cluster", 35, 10, 20)];
        let report = aggregate_report(evals);
        assert_eq!(report.aggregate_score, 35);
        assert_eq!(report.aggregate_classification, "Critical");
    }
}
