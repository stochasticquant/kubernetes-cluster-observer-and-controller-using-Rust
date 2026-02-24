use anyhow::Result;
use kube_devops::bundles;
use kube_devops::multi_cluster;

/* ============================= COMMANDS ============================= */

/// List available kubeconfig contexts.
pub fn list_contexts() -> Result<()> {
    let contexts = multi_cluster::list_contexts()?;

    if contexts.is_empty() {
        println!("No kubeconfig contexts found.");
        return Ok(());
    }

    println!("{:<40} STATUS", "CONTEXT");
    println!("{}", "-".repeat(55));
    for ctx in &contexts {
        println!("{:<40} available", ctx);
    }
    println!("\n{} context(s) found.", contexts.len());
    Ok(())
}

/// Analyze one or more clusters against a policy or bundle.
pub async fn analyze(
    contexts: Option<Vec<String>>,
    bundle_name: Option<String>,
    per_cluster: bool,
) -> Result<()> {
    // Resolve which contexts to analyze
    let target_contexts = match contexts {
        Some(c) if !c.is_empty() => c,
        _ => multi_cluster::list_contexts()?,
    };

    if target_contexts.is_empty() {
        println!("No kubeconfig contexts to analyze.");
        return Ok(());
    }

    // Resolve the policy spec
    let bundle_name = bundle_name.as_deref().unwrap_or("baseline");
    let bundle = bundles::get_bundle(bundle_name).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown bundle '{}'. Use 'policy bundle-list' to see available bundles.",
            bundle_name
        )
    })?;

    println!(
        "Analyzing {} cluster(s) with '{}' bundle...\n",
        target_contexts.len(),
        bundle.name
    );

    // Evaluate all clusters in parallel
    let handles: Vec<_> = target_contexts
        .into_iter()
        .map(|ctx| {
            let spec = bundle.spec.clone();
            tokio::spawn(async move {
                match multi_cluster::client_for_context(&ctx).await {
                    Ok(client) => multi_cluster::evaluate_cluster(&client, &ctx, &spec).await,
                    Err(e) => Err(e),
                }
            })
        })
        .collect();

    let mut evaluations = Vec::new();
    for handle in handles {
        match handle.await? {
            Ok(eval) => evaluations.push(eval),
            Err(e) => eprintln!("  [ERROR] {e}"),
        }
    }

    if evaluations.is_empty() {
        println!("No clusters could be reached.");
        return Ok(());
    }

    // Print per-cluster results
    if per_cluster {
        println!(
            "{:<30} {:>6} {:>6} {:>12} STATUS",
            "CLUSTER", "SCORE", "PODS", "VIOLATIONS"
        );
        println!("{}", "-".repeat(75));
        for eval in &evaluations {
            println!(
                "{:<30} {:>6} {:>6} {:>12} {}",
                eval.context_name,
                eval.health_score,
                eval.total_pods,
                eval.total_violations,
                eval.classification
            );
        }
        println!();
    }

    // Print aggregate report
    let report = multi_cluster::aggregate_report(evaluations);
    println!(
        "Aggregate: {} — score {}/100 across {} cluster(s)",
        report.aggregate_classification,
        report.aggregate_score,
        report.clusters.len()
    );

    Ok(())
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use kube_devops::governance;
    use kube_devops::multi_cluster::{ClusterEvaluation, aggregate_report};

    fn make_eval(name: &str, score: u32, pods: u32) -> ClusterEvaluation {
        ClusterEvaluation {
            context_name: name.to_string(),
            health_score: score,
            classification: governance::classify_health(score).to_string(),
            total_pods: pods,
            total_violations: 0,
            violations: vec![],
        }
    }

    #[test]
    fn test_analyze_output_format() {
        let evals = vec![
            make_eval("prod-cluster", 95, 50),
            make_eval("staging-cluster", 70, 20),
        ];
        let report = aggregate_report(evals);
        assert_eq!(report.clusters.len(), 2);
        assert!(report.aggregate_score > 0);
        assert!(!report.aggregate_classification.is_empty());
    }

    #[test]
    fn test_analyze_single_cluster() {
        let evals = vec![make_eval("single", 88, 10)];
        let report = aggregate_report(evals);
        assert_eq!(report.aggregate_score, 88);
    }

    #[test]
    fn test_context_names_preserved() {
        let evals = vec![make_eval("cluster-a", 90, 5), make_eval("cluster-b", 80, 5)];
        let report = aggregate_report(evals);
        let names: Vec<&str> = report
            .clusters
            .iter()
            .map(|c| c.context_name.as_str())
            .collect();
        assert!(names.contains(&"cluster-a"));
        assert!(names.contains(&"cluster-b"));
    }
}
