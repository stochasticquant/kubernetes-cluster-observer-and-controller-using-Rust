# Step 3: DevOps Analyzer Engine

## Kubernetes Cluster Governance Scan (Rust)

**Project:** kube-devops\
**Phase:** Step 3 - Governance & Policy Detection Engine\
**Last Updated:** 2026-02-20

---

## Overview

Step 3 upgrades the CLI from listing resources to analyzing cluster workloads for
DevOps best-practice issues. The new `analyze` command inspects Pods across all
namespaces (excluding known system namespaces), calculates a health score, and
prints a governance summary.

This step introduces:
- Cluster-wide workload analysis
- Best-practice detection (image tags and probes)
- Runtime anomaly identification (restarts, pending)
- A simple scoring model to quantify cluster health

---

## Objective

Implement a new CLI command:

```bash
kube-devops analyze
```

The command evaluates Pods and detects:
1. Containers using `:latest` image tags
2. Missing liveness probes
3. Missing readiness probes
4. High container restart counts
5. Pods stuck in `Pending` state

---

## Architecture

CLI -> Command Handler -> Kubernetes API -> Analyzer -> Console Report

Separation of concerns:
- **Data retrieval:** Kubernetes API client
- **Evaluation logic:** Pod analysis checks
- **Reporting:** Summary with counts and health score

---

## CLI Update

In `src/cli.rs`:

```rust
#[derive(Subcommand)]
pub enum Commands {
    Version,
    Check,
    List { resource: String },
    Analyze,
}
```

---

## main.rs Update

```rust
Commands::Analyze => {
    commands::analyze::run().await?;
}
```

`main` remains async via:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>
```

---

## Module Structure

Create:
```
src/commands/analyze.rs
```

Update `src/commands/mod.rs`:

```rust
pub mod analyze;
```

---

## Core Analyzer Implementation

### Kubernetes API Access

```rust
let client = Client::try_default().await?;
let pods: Api<Pod> = Api::all(client);
let pod_list = pods.list(&ListParams::default()).await?;
```

- Uses the current kubeconfig context
- Scans all namespaces
- Retrieves Pod lists from the API server

### System Namespace Filter

```rust
if is_system_namespace(namespace) {
    return;
}
```

Pods in known infrastructure namespaces are ignored:
- `kube-system`
- `kube-flannel`
- `longhorn-system`
- `metallb-system`
- `cert-manager`
- `istio-system`

### Workload Checks (Per Container)

```rust
if image.ends_with(":latest") {
    report.latest_tag += 1;
}

if container.liveness_probe.is_none() {
    report.missing_liveness += 1;
}

if container.readiness_probe.is_none() {
    report.missing_readiness += 1;
}
```

### Runtime Checks (Per Pod)

```rust
if cs.restart_count > 3 {
    let capped = (cs.restart_count.max(0) as u32).min(5);
    report.high_restarts += capped;
}

if phase == "Pending" {
    report.pending += 1;
}
```

- Restarts are capped to avoid runaway scoring
- Pending pods increment a cluster-level count

---

## Scoring Model

A simple weighted score converts issues into a 0-100 health index.

### Weights

```rust
latest_tag: 5
missing_liveness: 3
missing_readiness: 2
high_restarts: 6
pending: 4
```

### Calculation

```rust
raw_score = (latest_tag * w1)
          + (missing_liveness * w2)
          + (missing_readiness * w3)
          + (high_restarts * w4)
          + (pending * w5)

per_pod_score = raw_score / total_pods
capped = min(per_pod_score, 100)
health_score = 100 - capped
```

### Classification

```rust
80..=100 -> "Healthy"
60..=79  -> "Stable"
40..=59  -> "Degraded"
0..=39   -> "Critical"
```

---

## Output Summary

The command prints a compact governance report:

```
===== DevOps Governance Summary =====
Workload Pods Analyzed     : 12
Images using :latest       : 2
Missing liveness probes    : 5
Missing readiness probes   : 3
Restart severity score     : 4
Pending pods               : 1
--------------------------------------
Cluster Health Score       : 73
Cluster Status             : Stable
======================================
```

---

## Running the Command

```bash
cargo run -- analyze
```

### Requirements
- A valid Kubernetes context in `~/.kube/config`
- Access to the API server from your environment

---

## Notes and Limitations

- Only Pods are analyzed in Step 3
- System namespaces are excluded by design
- The scoring model is intentionally simple and can evolve over time

---

## Next Ideas

1. Add resource limits/requests checks
2. Analyze Deployments and StatefulSets
3. Export JSON reports
4. Include namespace-level breakdowns
5. Add configurable scoring weights
