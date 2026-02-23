# Kubernetes Cluster Observer & Controller using Rust

## Project Overview

**Kubernetes Cluster Observer & Controller using Rust** is a progressive
DevOps engineering project designed to build a production-grade
Kubernetes enhancement tool from the ground up using Rust.

This project is structured as a learning journey that evolves from:

-   Beginner Rust fundamentals
-   CLI application development
-   Kubernetes API interaction
-   Real-time cluster monitoring
-   Controller/Operator development
-   Policy enforcement mechanisms
-   Admission webhooks
-   Observability integration
-   Production hardening and HA design

The end goal is to build a Rust-based Kubernetes controller that
enhances cluster governance, DevOps best practices enforcement, and
operational visibility.

------------------------------------------------------------------------

## Why This Project Exists

Modern Kubernetes clusters require:

-   Strong governance
-   Workload policy enforcement
-   Observability
-   Automated remediation
-   DevOps best practices

While tools like Kyverno or OPA Gatekeeper exist, this project focuses
on building similar capabilities from scratch to deeply understand:

-   Kubernetes control loops
-   Rust systems programming
-   API-driven reconciliation
-   Cluster automation patterns

------------------------------------------------------------------------

## Current Stage: Step 6 -- Policy Enforcement Mode

The project is now a **true Kubernetes Operator** with policy enforcement —
it can detect violations **and** automatically patch non-compliant workloads:

-   Structured Rust CLI with 8 subcommands
-   `DevOpsPolicy` CRD (`devops.stochastic.io/v1`) for user-defined governance rules
-   Controller reconciliation loop via `kube_runtime::Controller`
-   Policy-aware pod evaluation (only checks what the policy enables)
-   **Policy enforcement mode** — automatically patch Deployments, StatefulSets,
    and DaemonSets to inject missing probes and resource limits
-   Audit mode (default) with zero mutations for backward compatibility
-   System namespace protection (never enforce in `kube-system`, `cert-manager`, etc.)
-   Annotation audit trail (`devops.stochastic.io/patched-by`) on patched workloads
-   Workload deduplication — patches each parent workload only once per cycle
-   Status sub-resource updates with health score, violations, and remediation counts
-   Finalizer lifecycle management (`devops.stochastic.io/cleanup`)
-   Kubernetes Watch API integration for real-time pod event streaming
-   Weighted governance scoring engine with namespace-level health tracking
-   Prometheus metrics for watch, reconcile, and enforcement
-   HTTP health endpoints (`/healthz`, `/readyz`, `/metrics`)
-   Leader election via Kubernetes Lease API for HA deployment
-   Structured JSON logging via `tracing`
-   Graceful shutdown with `Ctrl+C` handling (both watch and reconcile modes)
-   Comprehensive test suite (144+ tests)

### CLI Commands

``` bash
kube-devops version       # Display application version
kube-devops check         # Check cluster connectivity and permissions
kube-devops list pods     # List pods across all namespaces
kube-devops analyze       # Run one-shot governance analysis
kube-devops watch         # Start real-time governance watch controller
kube-devops crd generate  # Print DevOpsPolicy CRD YAML to stdout
kube-devops crd install   # Install CRD into connected cluster
kube-devops reconcile     # Start the DevOpsPolicy operator reconcile loop
```

------------------------------------------------------------------------

## Architecture

```
kube-devops/
 ├── Cargo.toml
 ├── Cargo.lock
 ├── rust-toolchain.toml
 ├── src/
 │   ├── main.rs              # Entry point, async runtime, command routing
 │   ├── lib.rs               # Library crate: exports crd + governance + enforcement
 │   ├── cli.rs               # clap CLI definition (8 subcommands)
 │   ├── crd.rs               # DevOpsPolicy CRD definition (spec + status)
 │   ├── enforcement.rs       # Policy enforcement: owner resolution, remediation, patching
 │   ├── governance.rs        # Scoring engine, pod evaluation, policy-aware checks
 │   └── commands/
 │        ├── mod.rs
 │        ├── version.rs      # Version display
 │        ├── check.rs        # Cluster connectivity check
 │        ├── list.rs         # Resource listing
 │        ├── analyze.rs      # One-shot governance analysis
 │        ├── watch.rs        # Watch controller, HTTP server, leader election
 │        ├── crd.rs          # CRD generate/install commands
 │        └── reconcile.rs    # Operator reconcile loop, finalizers, metrics
 ├── tests/
 │   ├── common/
 │   │   └── mod.rs                  # Shared test pod builder helper
 │   ├── enforcement_integration.rs  # Enforcement pipeline tests
 │   ├── governance_integration.rs   # End-to-end governance pipeline tests
 │   └── operator_integration.rs     # Operator reconcile pipeline tests
 ├── kube-tests/
 │   ├── test-pod.yaml               # Test pod manifest
 │   └── sample-devopspolicy.yaml    # Example DevOpsPolicy CR
 └── docs/
     ├── Step_1_Code_Explanation.md
     ├── Step_2_Kubernetes_Integration.md
     ├── Step_3_DevOps_Analyzer_Engine.md
     ├── Step_4_Kubernetes_Watch_Engine.md
     ├── Step_4_Detailed_Developer_Documentation.md
     ├── Step_4_Testing.md
     ├── Step_5_Kubernetes_Operator.md
     ├── Step_6_Policy_Enforcement.md
     ├── Kubernetes_Observability_Controller_Progress.md
     ├── Kubernetes_Observability_Policy_Controller_Roadmap.md
     ├── Rust_Foundations_for_Kubernetes_DevOps.md
     ├── Rust_Borrowing_Rules.md
     └── Build_Fix_MSVC_Toolchain.md
```

### Core Subsystems

| Subsystem | File | Description |
|---|---|---|
| CLI | `cli.rs` | clap-based command parsing with 8 subcommands |
| CRD | `crd.rs` | DevOpsPolicy CRD definition with spec + status + enforcement types |
| Governance Engine | `governance.rs` | Pod evaluation, violation detection, policy-aware checks, weighted scoring |
| Enforcement Engine | `enforcement.rs` | Owner resolution, remediation planning, workload patching |
| Operator Reconciler | `commands/reconcile.rs` | Controller reconcile loop, finalizers, status updates |
| Watch Controller | `commands/watch.rs` | Kubernetes Watch API stream processing, incremental state updates |
| Leader Election | `commands/watch.rs` | Lease-based leader election for HA multi-replica deployment |
| HTTP Server | `commands/watch.rs` | axum server exposing `/healthz`, `/readyz`, `/metrics` |
| Prometheus | `commands/watch.rs`, `commands/reconcile.rs` | Metrics for watch, reconcile, and enforcement |
| Logging | `main.rs` | Structured JSON logging via `tracing` + `tracing-subscriber` |

### Governance Scoring

The scoring engine applies weighted penalties per pod:

| Violation | Weight |
|---|---|
| `:latest` image tag | 5 |
| Missing liveness probe | 3 |
| Missing readiness probe | 2 |
| High restart count (> 3) | 6 |
| Pending phase | 4 |

Health score formula: `100 - min(raw_penalty / total_pods, 100)`

| Score Range | Classification |
|---|---|
| 80 -- 100 | Healthy |
| 60 -- 79 | Stable |
| 40 -- 59 | Degraded |
| 0 -- 39 | Critical |

------------------------------------------------------------------------

## Prerequisites

-   Rust (stable, edition 2024)
-   Cargo
-   Git
-   Access to a Kubernetes cluster (for `check`, `list`, `analyze`, `watch` commands)
-   A valid kubeconfig in `~/.kube/config`

To install Rust:

``` bash
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
```

Verify installation:

``` bash
rustc --version
cargo --version
```

------------------------------------------------------------------------

## How to Clone the Repository

``` bash
git clone https://github.com/stochasticquant/kubernetes-cluster-observer-and-controller-using-Rust.git
cd kubernetes-cluster-observer-and-controller-using-Rust
```

------------------------------------------------------------------------

## How to Build

``` bash
cargo build              # Debug build → target/debug/kube-devops
cargo build --release    # Release build → target/release/kube-devops
```

------------------------------------------------------------------------

## How to Run

Using Cargo (the `--` separator passes arguments to the application):

``` bash
cargo run -- version
cargo run -- check
cargo run -- list pods
cargo run -- analyze
cargo run -- watch
cargo run -- crd generate
cargo run -- crd install
cargo run -- reconcile
```

Using the compiled binary directly:

``` bash
./target/debug/kube-devops version
./target/debug/kube-devops check
./target/debug/kube-devops list pods
./target/debug/kube-devops analyze
./target/debug/kube-devops watch
./target/debug/kube-devops crd generate
./target/debug/kube-devops crd install
./target/debug/kube-devops reconcile
```

------------------------------------------------------------------------

## Running the Operator (Step 5)

The `reconcile` command starts the DevOpsPolicy operator:

1. Install the CRD: `cargo run -- crd install`
2. Create a policy: `kubectl apply -f kube-tests/sample-devopspolicy.yaml`
3. Start the operator: `cargo run -- reconcile`
4. Check status: `kubectl get devopspolicies -n production -o yaml`

The operator continuously evaluates pods against each DevOpsPolicy and
updates the CR's `.status` with health score, violations, and classification.
Press **Ctrl+C** for graceful shutdown.

------------------------------------------------------------------------

## Running the Watch Controller

The `watch` command starts a long-running controller that:

1. Acquires a Kubernetes Lease for leader election
2. Opens a Watch API stream for all pods
3. Evaluates governance violations in real time
4. Maintains namespace-level health scores
5. Exposes HTTP endpoints on port 8080

``` bash
cargo run -- watch
```

Endpoints available while running:

| Endpoint | Purpose |
|---|---|
| `GET /healthz` | Liveness probe (always 200 OK) |
| `GET /readyz` | Readiness probe (503 until initial sync, then 200) |
| `GET /metrics` | Prometheus metrics scrape endpoint |

------------------------------------------------------------------------

## Testing

The project includes 144+ automated tests that run without a Kubernetes
cluster. All Pod and CRD objects are constructed synthetically in-memory.

``` bash
cargo test                                       # Full suite (144+ tests)
cargo test --lib governance::tests               # Governance unit tests (48)
cargo test --lib crd::tests                      # CRD unit tests (18)
cargo test --lib enforcement::tests              # Enforcement unit tests (30)
cargo test --lib commands::watch::tests          # HTTP endpoint tests (5)
cargo test --lib commands::reconcile::tests      # Reconcile unit tests (13)
cargo test --test governance_integration         # Governance integration tests (6)
cargo test --test operator_integration           # Operator integration tests (13)
cargo test --test enforcement_integration        # Enforcement integration tests (8)
```

| Test Layer | Location | Count | Scope |
|---|---|---|---|
| Unit (lib) | `src/governance.rs` | 48 | Namespace filter, pod evaluation, violation detection, metrics, scoring, policy-aware evaluation |
| Unit (lib) | `src/crd.rs` | 18 | CRD schema, serialization, enforcement types, backward compatibility |
| Unit (lib) | `src/enforcement.rs` | 30 | Owner resolution, probe/resource building, plan generation, patch construction |
| Unit (bin) | `src/commands/watch.rs` | 5 | healthz, readyz (ready/not-ready), metrics, 404 handling |
| Unit (bin) | `src/commands/reconcile.rs` | 13 | Aggregation, finalizers, deletion, status computation, system ns filtering |
| Integration | `tests/governance_integration.rs` | 6 | End-to-end governance pipeline from pod to health classification |
| Integration | `tests/operator_integration.rs` | 13 | Full reconcile simulation, policy changes, CRD schema round-trip |
| Integration | `tests/enforcement_integration.rs` | 8 | Enforcement pipeline, audit vs enforce, namespace protection, deduplication |

See `docs/Step_4_Testing.md` and `docs/Step_5_Kubernetes_Operator.md` for full test documentation.

------------------------------------------------------------------------

## Expected Output

`version`:

```
kube-devops version 0.1.0
```

`list pods`:

```
default             pod-name-12345                   Running         node-1
kube-system         coredns-558bd4d5db-abc12         Running         node-2
```

`analyze`:

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

------------------------------------------------------------------------

## Dependencies

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing |
| `kube` / `kube-runtime` | Kubernetes client, Watch API, Controller runtime, CRD derive |
| `k8s-openapi` | Kubernetes API type definitions |
| `tokio` | Async runtime |
| `futures` | Stream combinators for watch events |
| `axum` | HTTP server for health/metrics endpoints |
| `prometheus` | Metrics registry and text encoding |
| `tracing` / `tracing-subscriber` | Structured JSON logging |
| `anyhow` | Error handling |
| `serde` / `serde_json` / `serde_yaml` | Serialization for CRD structs and YAML output |
| `schemars` | JSON Schema generation for CRD validation |
| `chrono` | Timestamps for status updates |

Dev-dependencies: `tower` (HTTP testing), `http-body-util` (response body reading).

------------------------------------------------------------------------

## Development Workflow

Recommended branch strategy:

-   `main` -- Stable baseline
-   `feature/*` -- Feature development

``` bash
git checkout -b feature/my-feature
# make changes
cargo test                     # Verify tests pass
git commit -m "Add new feature"
git push origin feature/my-feature
```

Open Pull Request -> Merge into main.

------------------------------------------------------------------------

## Completed Steps

| Step | Milestone | Status |
|---|---|---|
| 1 | Rust CLI foundations (clap, modular commands, error handling) | Done |
| 2 | Kubernetes read-only client (list pods, async API) | Done |
| 3 | DevOps governance analyzer (scoring engine, health classification) | Done |
| 4 | Real-time watch engine (Watch API, leader election, Prometheus, HTTP endpoints, test suite) | Done |
| 5 | Kubernetes Operator (DevOpsPolicy CRD, reconciliation loop, finalizers, policy-aware evaluation, 98 tests) | Done |
| 6 | Policy enforcement mode (audit/enforce, auto-patch workloads, inject probes+limits, 144+ tests) | Done |

## Roadmap

| Step | Milestone | Status |
|---|---|---|
| 7 | Admission webhook (reject violations at creation time) | Planned |
| 8 | Prometheus expansion (ServiceMonitor, Grafana dashboards) | Planned |
| 9 | High availability & production hardening | Planned |
| 10 | Multi-cluster governance & policy bundles | Planned |

------------------------------------------------------------------------

## Long-Term Vision

By the end of this project, the repository will contain:

-   A Rust CLI tool
-   A Kubernetes operator with CRD
-   Policy enforcement mechanisms
-   Validating admission webhook
-   Observability integrations (Prometheus + Grafana)
-   Production-grade deployment manifests
-   Helm chart support
-   HA controller configuration

This project serves as a platform engineering laboratory using a real
9-node Kubernetes cluster.

------------------------------------------------------------------------

## Documentation

| Document | Description |
|---|---|
| `docs/Step_1_Code_Explanation.md` | Rust CLI foundations walkthrough |
| `docs/Step_2_Kubernetes_Integration.md` | Kubernetes client integration |
| `docs/Step_3_DevOps_Analyzer_Engine.md` | Governance analyzer design |
| `docs/Step_4_Kubernetes_Watch_Engine.md` | Watch engine concepts and architecture |
| `docs/Step_4_Detailed_Developer_Documentation.md` | Deep technical reference for Step 4 subsystems |
| `docs/Step_4_Testing.md` | Test suite documentation (Step 4 tests) |
| `docs/Step_5_Kubernetes_Operator.md` | CRD, reconciliation loop, finalizers, operator architecture |
| `docs/Step_6_Policy_Enforcement.md` | Enforcement mode, remediation planning, patching architecture |
| `docs/Kubernetes_Observability_Controller_Progress.md` | Implementation progress tracker |
| `docs/Kubernetes_Observability_Policy_Controller_Roadmap.md` | Full 10-step roadmap |
| `docs/Rust_Foundations_for_Kubernetes_DevOps.md` | Rust language foundations reference |
| `docs/Rust_Borrowing_Rules.md` | Ownership and borrowing reference |
| `docs/Build_Fix_MSVC_Toolchain.md` | MSVC toolchain troubleshooting |

------------------------------------------------------------------------

## License

MIT License

------------------------------------------------------------------------

## Author

StochasticQuant\
DevOps & Platform Engineering Lab

------------------------------------------------------------------------

**Last Updated:** 2026-02-23
