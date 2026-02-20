use kube::{Client, Api};
use kube::api::ListParams;
use k8s_openapi::api::core::v1::Pod;

#[derive(Default)]
struct AnalysisReport {
    latest_tag: u32,
    missing_liveness: u32,
    missing_readiness: u32,
    high_restarts: u32,
    pending: u32,
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {

    println!("Running DevOps analysis...\n");

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods.list(&ListParams::default()).await?;

    let mut report = AnalysisReport::default();

    for p in pod_list {
        analyze_pod(&p, &mut report);
    }

    print_summary(&report);

    Ok(())
}

fn analyze_pod(p: &Pod, report: &mut AnalysisReport) {

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

    if let Some(status) = &p.status {

        if let Some(container_statuses) = &status.container_statuses {
            for cs in container_statuses {
                if cs.restart_count > 3 {
                    report.high_restarts += 1;
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

fn print_summary(report: &AnalysisReport) {

    println!("===== DevOps Governance Summary =====");
    println!("Images using :latest       : {}", report.latest_tag);
    println!("Missing liveness probes    : {}", report.missing_liveness);
    println!("Missing readiness probes   : {}", report.missing_readiness);
    println!("High restart containers    : {}", report.high_restarts);
    println!("Pending pods               : {}", report.pending);
    println!("=====================================");
}