use anyhow::Context;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};

pub async fn run(resource: String) -> anyhow::Result<()> {
    if resource != "pods" {
        anyhow::bail!("Unsupported resource '{}'. Supported: pods", resource);
    }

    let client = Client::try_default()
        .await
        .context("Failed to connect to Kubernetes cluster. Is your kubeconfig valid?")?;

    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods
        .list(&ListParams::default())
        .await
        .context("Failed to list pods. Check RBAC permissions.")?;

    let mut rows: Vec<(String, String, String, String)> = pod_list
        .into_iter()
        .map(|p| {
            let namespace = p.metadata.namespace.unwrap_or_default();
            let name = p.metadata.name.unwrap_or_default();
            let phase = p
                .status
                .as_ref()
                .and_then(|s| s.phase.as_deref())
                .unwrap_or("Unknown")
                .to_string();
            let node = p
                .spec
                .as_ref()
                .and_then(|s| s.node_name.as_deref())
                .unwrap_or("Not Scheduled")
                .to_string();
            (namespace, name, phase, node)
        })
        .collect();

    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    println!(
        "{:<20} {:<60} {:<12} {:<15}",
        "NAMESPACE", "NAME", "STATUS", "NODE"
    );
    println!("{}", "-".repeat(107));

    for (namespace, name, phase, node) in &rows {
        println!("{:<20} {:<60} {:<12} {:<15}", namespace, name, phase, node);
    }

    println!("\nTotal: {} pods", rows.len());

    Ok(())
}
