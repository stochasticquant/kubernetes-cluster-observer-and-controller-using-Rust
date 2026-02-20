# Step 3 -- DevOps Analyzer Engine

## Kubernetes Cluster Observer & Controller using Rust

**Project:** Kubernetes Cluster Observer & Controller\
**Phase:** Step 3 -- Governance & Policy Detection Engine\
**Last Updated:** 2026-02-20

------------------------------------------------------------------------

# Overview

In Step 3, the project evolves from simply listing Kubernetes resources
to actively analyzing cluster workloads for DevOps best-practice
violations.

You are no longer just consuming Kubernetes data --- you are evaluating
it.

This step introduces:

-   Cluster-wide policy inspection
-   Workload best-practice detection
-   Runtime anomaly identification
-   Foundational governance logic

This forms the basis for a future Kubernetes Controller or Admission
Webhook.

------------------------------------------------------------------------

# Objective

Implement a new CLI command:

``` bash
kube-devops analyze
```

The command evaluates all Pods across all namespaces and detects:

1.  Containers using `:latest` image tags
2.  Missing resource limits
3.  Missing liveness probes
4.  Missing readiness probes
5.  High container restart counts
6.  Pods stuck in `Pending` state

------------------------------------------------------------------------

# Architecture

CLI → Command Handler → Kubernetes API → Analyzer Engine → Console
Report

The analyzer separates:

-   Data retrieval (Kubernetes API)
-   Evaluation logic (DevOps checks)
-   Reporting output

This separation prepares the project for future controller-based
reconciliation.

------------------------------------------------------------------------

# CLI Update

In `cli.rs`:

``` rust
#[derive(Subcommand)]
pub enum Commands {
    Version,
    Check,
    List { resource: String },
    Analyze,
}
```

------------------------------------------------------------------------

# main.rs Update

``` rust
Commands::Analyze => {
    commands::analyze::run().await?;
}
```

`main` remains asynchronous via:

``` rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>
```

------------------------------------------------------------------------

# Module Structure

Create:

    src/commands/analyze.rs

Update `commands/mod.rs`:

``` rust
pub mod analyze;
```

------------------------------------------------------------------------

# Core Analyzer Implementation

``` rust
use kube::{Client, Api};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("Running DevOps analysis...");

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::all(client);

    let pod_list = pods.list(&ListParams::default()).await?;

    for p in pod_list {
        let name = p.metadata.name.clone().unwrap_or_default();
        let namespace = p.metadata.namespace.clone().unwrap_or_default();

        analyze_pod(&p, &namespace, &name);
    }

    Ok(())
}
```

------------------------------------------------------------------------

# Analysis Engine Logic

``` rust
fn analyze_pod(p: &Pod, namespace: &str, name: &str) {

    if let Some(spec) = &p.spec {
        for container in &spec.containers {

            let image = container.image.clone().unwrap_or_default();

            if image.ends_with(":latest") {
                println!("WARNING [{}/{}] uses :latest tag → {}", namespace, name, image);
            }

            if container.resources.is_none() {
                println!("WARNING [{}/{}] has NO resource limits defined", namespace, name);
            }

            if container.liveness_probe.is_none() {
                println!("WARNING [{}/{}] missing liveness probe", namespace, name);
            }

            if container.readiness_probe.is_none() {
                println!("WARNING [{}/{}] missing readiness probe", namespace, name);
            }
        }
    }

    if let Some(status) = &p.status {
        if let Some(container_statuses) = &status.container_statuses {
            for cs in container_statuses {
                if cs.restart_count > 3 {
                    println!(
                        "CRITICAL [{}/{}] high restart count → {} restarts",
                        namespace, name, cs.restart_count
                    );
                }
            }
        }

        if let Some(phase) = &status.phase {
            if phase == "Pending" {
                println!("INFO [{}/{}] is Pending", namespace, name);
            }
        }
    }
}
```

------------------------------------------------------------------------

# How to Run

``` bash
cargo run -- analyze
```

Or:

``` bash
./target/debug/kube-devops analyze
```

------------------------------------------------------------------------

# Example Output

    WARNING [default/my-app] uses :latest tag → nginx:latest
    WARNING [kafka/broker-1] has NO resource limits defined
    CRITICAL [kubeflow/ml-pipeline] high restart count → 7 restarts
    INFO [airflow/scheduler] is Pending

------------------------------------------------------------------------

# DevOps Significance

This analyzer enables:

-   Policy enforcement groundwork
-   Reliability assessment
-   Security posture review
-   Cost governance detection
-   Deployment hygiene validation

It transforms the CLI into a governance engine.

------------------------------------------------------------------------

# What You Learned

-   Inspecting container specifications
-   Parsing runtime status fields
-   Evaluating restart counts
-   Handling nested Option types
-   Structuring reusable analysis logic
-   Building cluster-wide evaluation tools

You progressed from observing cluster state (Step 2) to evaluating
compliance (Step 3).

------------------------------------------------------------------------

# Next Evolution

Future improvements:

-   Severity scoring system
-   Aggregated violation summary
-   Namespace filtering
-   JSON output mode
-   Real-time watch-based monitoring
-   Controller-based reconciliation
-   Admission webhook enforcement

Step 3 is the foundation for Kubernetes policy automation.

------------------------------------------------------------------------

End of Step 3 Documentation
