use anyhow::Context;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};

use kube_devops::governance::{
    self, PodMetrics, add_metrics, calculate_health_score, classify_health,
};

pub async fn run() -> anyhow::Result<()> {
    println!("Running DevOps analysis...\n");

    let client = Client::try_default().await
        .context("Failed to connect to Kubernetes cluster. Is your kubeconfig valid?")?;

    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods.list(&ListParams::default()).await
        .context("Failed to list pods. Check RBAC permissions.")?;

    let mut report = PodMetrics::default();

    for pod in pod_list {
        let ns = pod.metadata.namespace.as_deref().unwrap_or("");

        if governance::is_system_namespace(ns) {
            continue;
        }

        let contribution = governance::evaluate_pod(&pod);
        add_metrics(&mut report, &contribution);
    }

    print_summary(&report);

    Ok(())
}

fn print_summary(report: &PodMetrics) {
    let score = calculate_health_score(report);
    let status = classify_health(score);

    println!("===== DevOps Governance Summary =====");
    println!("Workload Pods Analyzed     : {}", report.total_pods);
    println!("Images using :latest       : {}", report.latest_tag);
    println!("Missing liveness probes    : {}", report.missing_liveness);
    println!("Missing readiness probes   : {}", report.missing_readiness);
    println!("Restart severity score     : {}", report.high_restarts);
    println!("Pending pods               : {}", report.pending);
    println!("--------------------------------------");
    println!("Cluster Health Score       : {}/100", score);
    println!("Cluster Status             : {}", status);
    println!("======================================\n");
}
