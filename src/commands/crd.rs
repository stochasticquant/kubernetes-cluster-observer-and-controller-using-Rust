use anyhow::Result;
use kube::CustomResourceExt;

use kube_devops::crd::{DevOpsPolicy, PolicyAuditResult};

/// Print both CRD YAMLs to stdout for `kubectl apply -f`.
pub fn generate() -> Result<()> {
    let policy_crd = DevOpsPolicy::crd();
    let audit_crd = PolicyAuditResult::crd();

    let policy_yaml = serde_yaml::to_string(&policy_crd)?;
    let audit_yaml = serde_yaml::to_string(&audit_crd)?;

    println!("{policy_yaml}---\n{audit_yaml}");
    Ok(())
}

/// Apply both CRDs directly to the connected cluster.
pub async fn install() -> Result<()> {
    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
    use kube::{Api, Client};

    let client = Client::try_default().await?;
    let crds: Api<CustomResourceDefinition> = Api::all(client);

    for crd in [DevOpsPolicy::crd(), PolicyAuditResult::crd()] {
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
    }

    Ok(())
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_contains_both_crds() {
        let policy_crd = DevOpsPolicy::crd();
        let audit_crd = PolicyAuditResult::crd();

        let policy_yaml = serde_yaml::to_string(&policy_crd).unwrap();
        let audit_yaml = serde_yaml::to_string(&audit_crd).unwrap();
        let output = format!("{policy_yaml}---\n{audit_yaml}");

        assert!(output.contains("DevOpsPolicy"));
        assert!(output.contains("PolicyAuditResult"));
        assert!(output.contains("---"));
    }

    #[test]
    fn test_generate_both_crds_valid_yaml() {
        let policy_crd = DevOpsPolicy::crd();
        let audit_crd = PolicyAuditResult::crd();

        let policy_yaml = serde_yaml::to_string(&policy_crd).unwrap();
        let audit_yaml = serde_yaml::to_string(&audit_crd).unwrap();

        let _: serde_yaml::Value =
            serde_yaml::from_str(&policy_yaml).expect("policy CRD YAML should be valid");
        let _: serde_yaml::Value =
            serde_yaml::from_str(&audit_yaml).expect("audit result CRD YAML should be valid");
    }

    #[test]
    fn test_both_crds_same_api_group() {
        let policy_crd = DevOpsPolicy::crd();
        let audit_crd = PolicyAuditResult::crd();
        assert_eq!(policy_crd.spec.group, audit_crd.spec.group);
    }
}
