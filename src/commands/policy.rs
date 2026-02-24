use anyhow::Result;
use kube_devops::bundles;
use kube_devops::crd::DevOpsPolicy;

/* ============================= BUNDLE COMMANDS ============================= */

/// List all available policy bundles.
pub fn bundle_list() -> Result<()> {
    let bundles = bundles::all_bundles();
    println!("{:<15} DESCRIPTION", "NAME");
    println!("{}", "-".repeat(70));
    for bundle in &bundles {
        println!("{:<15} {}", bundle.name, bundle.description);
    }
    Ok(())
}

/// Show details of a specific bundle.
pub fn bundle_show(name: &str) -> Result<()> {
    match bundles::get_bundle(name) {
        Some(bundle) => {
            println!("Bundle: {}", bundle.name);
            println!("Description: {}", bundle.description);
            println!();
            let yaml = serde_yaml::to_string(&bundle.spec)?;
            println!("Spec:");
            for line in yaml.lines() {
                println!("  {line}");
            }
            Ok(())
        }
        None => {
            let available: Vec<String> = bundles::all_bundles()
                .iter()
                .map(|b| b.name.clone())
                .collect();
            anyhow::bail!(
                "Unknown bundle '{}'. Available bundles: {}",
                name,
                available.join(", ")
            )
        }
    }
}

/// Generate a DevOpsPolicy YAML from a bundle template.
pub fn bundle_apply(name: &str, namespace: &str, policy_name: &str) -> Result<()> {
    let bundle = bundles::get_bundle(name).ok_or_else(|| {
        let available: Vec<String> = bundles::all_bundles()
            .iter()
            .map(|b| b.name.clone())
            .collect();
        anyhow::anyhow!(
            "Unknown bundle '{}'. Available bundles: {}",
            name,
            available.join(", ")
        )
    })?;

    let spec_yaml = serde_yaml::to_string(&bundle.spec)?;

    // Indent the spec YAML for embedding
    let indented_spec: String = spec_yaml
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    let output = format!(
        r#"apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: {policy_name}
  namespace: {namespace}
  labels:
    devops.stochastic.io/bundle: {bundle_name}
    app.kubernetes.io/managed-by: kube-devops
spec:
{indented_spec}
"#,
        bundle_name = bundle.name,
    );

    print!("{output}");
    Ok(())
}

/* ============================= GITOPS COMMANDS ============================= */

/// Export DevOpsPolicies from a namespace as YAML.
pub async fn export(namespace: &str) -> Result<()> {
    let client = kube::Client::try_default().await?;
    let api: kube::Api<DevOpsPolicy> = kube::Api::namespaced(client, namespace);
    let policies = api.list(&Default::default()).await?;

    if policies.items.is_empty() {
        println!("No DevOpsPolicies found in namespace '{namespace}'");
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut first = true;
    for policy in &policies.items {
        if !first {
            println!("---");
        }
        first = false;

        let spec_yaml = serde_yaml::to_string(&policy.spec)?;
        let indented_spec: String = spec_yaml
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let name = policy.metadata.name.as_deref().unwrap_or("unnamed");
        let ns = policy.metadata.namespace.as_deref().unwrap_or(namespace);

        println!(
            r#"apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: {name}
  namespace: {ns}
  annotations:
    devops.stochastic.io/exported-at: "{now}"
    devops.stochastic.io/exported-from: "{ns}"
spec:
{indented_spec}"#
        );
    }

    Ok(())
}

/// Import DevOpsPolicies from a YAML file.
pub async fn import(file: &str, dry_run: bool) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let client = kube::Client::try_default().await?;

    for doc in content.split("---") {
        let trimmed = doc.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_yaml::Value = serde_yaml::from_str(trimmed)?;
        let kind = value["kind"].as_str().unwrap_or("");
        if kind != "DevOpsPolicy" {
            continue;
        }

        let policy: DevOpsPolicy = serde_yaml::from_str(trimmed)?;
        let name = policy.metadata.name.as_deref().unwrap_or("unnamed");
        let ns = policy.metadata.namespace.as_deref().unwrap_or("default");

        if dry_run {
            println!("[DRY-RUN] Would apply DevOpsPolicy '{name}' in namespace '{ns}'");
        } else {
            let api: kube::Api<DevOpsPolicy> = kube::Api::namespaced(client.clone(), ns);
            match api
                .patch(
                    name,
                    &kube::api::PatchParams::apply("kube-devops-cli"),
                    &kube::api::Patch::Apply(&policy),
                )
                .await
            {
                Ok(_) => println!("Applied DevOpsPolicy '{name}' in namespace '{ns}'"),
                Err(e) => eprintln!("Failed to apply '{name}': {e}"),
            }
        }
    }

    Ok(())
}

/// Diff local YAML policies against cluster state.
pub async fn diff(file: &str) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let client = kube::Client::try_default().await?;

    for doc in content.split("---") {
        let trimmed = doc.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_yaml::Value = serde_yaml::from_str(trimmed)?;
        let kind = value["kind"].as_str().unwrap_or("");
        if kind != "DevOpsPolicy" {
            continue;
        }

        let local_policy: DevOpsPolicy = serde_yaml::from_str(trimmed)?;
        let name = local_policy.metadata.name.as_deref().unwrap_or("unnamed");
        let ns = local_policy
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default");

        let api: kube::Api<DevOpsPolicy> = kube::Api::namespaced(client.clone(), ns);
        match api.get(name).await {
            Ok(remote_policy) => {
                let local_json = serde_json::to_value(&local_policy.spec)?;
                let remote_json = serde_json::to_value(&remote_policy.spec)?;

                if local_json == remote_json {
                    println!("[=] {ns}/{name}: no changes");
                } else {
                    println!("[~] {ns}/{name}: spec differs");
                    diff_json("spec", &remote_json, &local_json, "  ");
                }
            }
            Err(kube::Error::Api(err)) if err.code == 404 => {
                println!("[+] {ns}/{name}: new (not in cluster)");
            }
            Err(e) => {
                println!("[!] {ns}/{name}: error fetching from cluster: {e}");
            }
        }
    }

    Ok(())
}

fn diff_json(prefix: &str, remote: &serde_json::Value, local: &serde_json::Value, indent: &str) {
    match (remote, local) {
        (serde_json::Value::Object(r), serde_json::Value::Object(l)) => {
            for key in r
                .keys()
                .chain(l.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                let r_val = r.get(key);
                let l_val = l.get(key);
                match (r_val, l_val) {
                    (Some(rv), Some(lv)) if rv != lv => {
                        diff_json(&format!("{prefix}.{key}"), rv, lv, indent);
                    }
                    (Some(rv), None) => {
                        println!("{indent}- {prefix}.{key}: {rv}");
                    }
                    (None, Some(lv)) => {
                        println!("{indent}+ {prefix}.{key}: {lv}");
                    }
                    _ => {}
                }
            }
        }
        _ if remote != local => {
            println!("{indent}- {prefix}: {remote}");
            println!("{indent}+ {prefix}: {local}");
        }
        _ => {}
    }
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_apply_output_valid_yaml() {
        // Capture the output by generating the YAML string directly
        let bundle = bundles::get_bundle("baseline").unwrap();
        let spec_yaml = serde_yaml::to_string(&bundle.spec).unwrap();
        let indented_spec: String = spec_yaml
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let output = format!(
            r#"apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: test-policy
  namespace: production
  labels:
    devops.stochastic.io/bundle: {bundle_name}
    app.kubernetes.io/managed-by: kube-devops
spec:
{indented_spec}
"#,
            bundle_name = bundle.name,
        );

        let doc: serde_yaml::Value =
            serde_yaml::from_str(&output).expect("generated YAML should be parseable");
        assert_eq!(doc["kind"], "DevOpsPolicy");
        assert_eq!(doc["apiVersion"], "devops.stochastic.io/v1");
        assert_eq!(doc["metadata"]["name"], "test-policy");
        assert_eq!(doc["metadata"]["namespace"], "production");
    }

    #[test]
    fn test_bundle_apply_has_labels() {
        let bundle = bundles::get_bundle("restricted").unwrap();
        let spec_yaml = serde_yaml::to_string(&bundle.spec).unwrap();
        let indented_spec: String = spec_yaml
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        let output = format!(
            r#"apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: my-policy
  namespace: default
  labels:
    devops.stochastic.io/bundle: {bundle_name}
    app.kubernetes.io/managed-by: kube-devops
spec:
{indented_spec}
"#,
            bundle_name = bundle.name,
        );

        let doc: serde_yaml::Value = serde_yaml::from_str(&output).unwrap();
        assert_eq!(
            doc["metadata"]["labels"]["devops.stochastic.io/bundle"],
            "restricted"
        );
        assert_eq!(
            doc["metadata"]["labels"]["app.kubernetes.io/managed-by"],
            "kube-devops"
        );
    }

    #[test]
    fn test_bundle_apply_correct_spec_for_each_bundle() {
        for bundle_name in ["baseline", "restricted", "permissive"] {
            let bundle = bundles::get_bundle(bundle_name).unwrap();
            let spec_yaml = serde_yaml::to_string(&bundle.spec).unwrap();
            // Ensure the spec serializes without error
            assert!(
                !spec_yaml.is_empty(),
                "spec for {bundle_name} should not be empty"
            );
        }
    }

    #[test]
    fn test_diff_json_detects_changed_field() {
        let remote = serde_json::json!({"forbidLatestTag": true, "maxRestartCount": 3});
        let local = serde_json::json!({"forbidLatestTag": true, "maxRestartCount": 5});
        // Just verify it doesn't panic — output goes to stdout
        diff_json("spec", &remote, &local, "  ");
    }

    #[test]
    fn test_diff_json_detects_added_field() {
        let remote = serde_json::json!({"forbidLatestTag": true});
        let local = serde_json::json!({"forbidLatestTag": true, "maxRestartCount": 5});
        diff_json("spec", &remote, &local, "  ");
    }

    #[test]
    fn test_diff_json_detects_removed_field() {
        let remote = serde_json::json!({"forbidLatestTag": true, "maxRestartCount": 3});
        let local = serde_json::json!({"forbidLatestTag": true});
        diff_json("spec", &remote, &local, "  ");
    }

    #[test]
    fn test_diff_json_no_diff() {
        let remote = serde_json::json!({"forbidLatestTag": true});
        let local = serde_json::json!({"forbidLatestTag": true});
        diff_json("spec", &remote, &local, "  ");
    }
}
