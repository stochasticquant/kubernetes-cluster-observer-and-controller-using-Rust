use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::ListParams;
use kube::{Api, Client};

pub async fn run() -> anyhow::Result<()> {
    println!("Running cluster connectivity checks...\n");

    // 1. Build Kubernetes client from kubeconfig
    print!("  Kubeconfig .................. ");
    let client = match Client::try_default().await {
        Ok(c) => {
            println!("OK");
            c
        }
        Err(e) => {
            println!("FAIL");
            anyhow::bail!("Cannot load kubeconfig: {}", e);
        }
    };

    // 2. Verify actual cluster connectivity by fetching server version
    print!("  Cluster connection .......... ");
    let version = match client.apiserver_version().await {
        Ok(v) => {
            println!("OK (v{}.{})", v.major, v.minor);
            Some(v)
        }
        Err(e) => {
            println!("FAIL");
            println!("\n  Error: {}", e);
            println!("  Hint:  Is the cluster running? Check with: kubectl cluster-info\n");
            return Ok(());
        }
    };

    // 3. List pods permission
    print!("  List pods permission ........ ");
    let pods: Api<Pod> = Api::all(client.clone());
    match pods.list(&ListParams::default().limit(1)).await {
        Ok(_) => println!("OK"),
        Err(e) => println!("FAIL ({})", e),
    }

    // 4. List nodes permission
    print!("  List nodes permission ....... ");
    let nodes: Api<Node> = Api::all(client.clone());
    match nodes.list(&ListParams::default()).await {
        Ok(node_list) => {
            let count = node_list.items.len();
            println!("OK ({} nodes)", count);
        }
        Err(e) => println!("FAIL ({})", e),
    }

    // 5. Kubernetes version (already fetched above)
    if let Some(v) = version {
        println!("\n  Kubernetes version: {}.{}", v.major, v.minor);
    }

    println!("\nAll checks completed.");
    Ok(())
}
