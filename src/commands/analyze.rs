use kube::{Api, Client};
use kube::api::ListParams;
use k8s_openapi::api::core::v1::Pod;

#[derive(Default)]
struct AnalysisReport {
    total_pods: u32,
    latest_tag: u32,
    missing_liveness: u32,
    missing_readiness: u32,
    high_restarts: u32,
    pending: u32,
}

struct ScoringWeights {
    latest_tag: u32,
    missing_liveness: u32,
    missing_readiness: u32,
    high_restarts: u32,
    pending: u32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            latest_tag: 5,
            missing_liveness: 3,
            missing_readiness: 2,
            high_restarts: 6, // reduced to avoid dominance
            pending: 4,
        }
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running DevOps analysis...\n");

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods.list(&ListParams::default()).await?;

    let mut report = AnalysisReport::default();

    for pod in pod_list {
        analyze_pod(&pod, &mut report);
    }

    print_summary(&report);

    Ok(())
}

fn is_system_namespace(ns: &str) -> bool {
    matches!(
        ns,
        "kube-system"
            | "kube-flannel"
            | "longhorn-system"
            | "metallb-system"
            | "cert-manager"
            | "istio-system"
    )
}

fn analyze_pod(p: &Pod, report: &mut AnalysisReport) {

    let namespace = p.metadata.namespace.as_deref().unwrap_or("");

    // Ignore infrastructure namespaces
    if is_system_namespace(namespace) {
        return;
    }

    report.total_pods += 1;

    // ---- SPEC ANALYSIS ----
    if let Some(spec) = &p.spec {
        for container in &spec.containers {

            let image = container.image.clone().unwrap_or_default();

            if image.ends_with(":latest") {
                report.latest_tag += 1;
            }

            if container.liveness_probe.is_none() {
                report.missing_liveness += 1;
            }

            if container.readiness_probe.is_none() {
                report.missing_readiness += 1;
            }
        }
    }

    // ---- STATUS ANALYSIS ----
    if let Some(status) = &p.status {

        if let Some(container_statuses) = &status.container_statuses {
            for cs in container_statuses {

                if cs.restart_count > 3 {
                    // Safe conversion i32 → u32
                    let capped = (cs.restart_count.max(0) as u32).min(5);
                    report.high_restarts += capped;
                }
            }
        }

        if let Some(phase) = &status.phase {
            if phase == "Pending" {
                report.pending += 1;
            }
        }
    }
}

fn calculate_score(report: &AnalysisReport) -> u32 {

    if report.total_pods == 0 {
        return 100;
    }

    let weights = ScoringWeights::default();

    let raw_score =
        (report.latest_tag * weights.latest_tag)
            + (report.missing_liveness * weights.missing_liveness)
            + (report.missing_readiness * weights.missing_readiness)
            + (report.high_restarts * weights.high_restarts)
            + (report.pending * weights.pending);

    // Normalize per pod
    let per_pod_score = raw_score / report.total_pods;

    // Cap maximum impact to avoid runaway scoring
    let capped = per_pod_score.min(100);

    // Convert to 0–100 health index (higher is better)
    100 - capped
}

fn classify(score: u32) -> &'static str {
    match score {
        80..=100 => "🟢 Healthy",
        60..=79 => "🟡 Stable",
        40..=59 => "🟠 Degraded",
        _ => "🔴 Critical",
    }
}

fn print_summary(report: &AnalysisReport) {

    let score = calculate_score(report);
    let status = classify(score);

    println!("===== DevOps Governance Summary =====");
    println!("Workload Pods Analyzed     : {}", report.total_pods);
    println!("Images using :latest       : {}", report.latest_tag);
    println!("Missing liveness probes    : {}", report.missing_liveness);
    println!("Missing readiness probes   : {}", report.missing_readiness);
    println!("Restart severity score     : {}", report.high_restarts);
    println!("Pending pods               : {}", report.pending);
    println!("--------------------------------------");
    println!("Cluster Health Score       : {}", score);
    println!("Cluster Status             : {}", status);
    println!("======================================\n");
}