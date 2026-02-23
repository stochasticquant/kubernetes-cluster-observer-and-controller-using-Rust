use anyhow::Result;
use kube::CustomResourceExt;

use kube_devops::crd::DevOpsPolicy;

/// Print the DevOpsPolicy CRD YAML to stdout for `kubectl apply -f`.
pub fn generate() -> Result<()> {
    let crd = DevOpsPolicy::crd();
    let yaml = serde_yaml::to_string(&crd)?;
    println!("{yaml}");
    Ok(())
}

/// Apply the DevOpsPolicy CRD directly to the connected cluster.
pub async fn install() -> Result<()> {
    use kube::{Api, Client};
    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;

    let client = Client::try_default().await?;
    let crds: Api<CustomResourceDefinition> = Api::all(client);

    let crd = DevOpsPolicy::crd();
    let name = crd.metadata.name.clone().unwrap_or_default();

    match crds.create(&Default::default(), &crd).await {
        Ok(_) => {
            println!("CRD '{name}' installed successfully");
        }
        Err(kube::Error::Api(err)) if err.code == 409 => {
            println!("CRD '{name}' already exists — skipping");
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}
