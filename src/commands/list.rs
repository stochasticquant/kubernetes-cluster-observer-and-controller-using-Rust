use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};

pub async fn run(resource: String) -> Result<(), Box<dyn std::error::Error>> {
    if resource != "pods" {
        println!("Currently only 'pods' is supported");
        return Ok(());
    }

    // Create Kubernetes client from kubeconfig
    let client = Client::try_default().await?;

    // Access all namespaces
    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods.list(&ListParams::default()).await?;

    for p in pod_list {
        let name = p.metadata.name.unwrap_or_default();
        let namespace = p.metadata.namespace.unwrap_or_default();
        let phase = p
            .status
            .as_ref()
            .and_then(|s| s.phase.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let node = p
            .spec
            .as_ref()
            .and_then(|s| s.node_name.clone())
            .unwrap_or_else(|| "Not Scheduled".to_string());

        println!("{:<20} {:<60} {:<12} {:<15}", namespace, name, phase, node);
    }

    Ok(())
}
