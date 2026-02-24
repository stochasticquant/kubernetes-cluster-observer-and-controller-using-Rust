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

## Current Stage: Step 9 -- High Availability & Production Hardening (Deployed)

The project is **deployed and running in production** on a 9-node Kubernetes cluster
with a multi-stage Dockerfile, Kubernetes deployment manifests (RBAC, Deployments,
PDBs), Helm chart, and verified Prometheus + Grafana observability:

-   Structured Rust CLI with 17 subcommands
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
-   Prometheus metrics across all 3 components (16 metrics total)
-   HTTP health endpoints on watch (:8080), reconcile (:9090), webhook (:8443)
-   Leader election via Kubernetes Lease API (with automatic non-leader promotion)
-   Structured JSON logging via `tracing`
-   Graceful shutdown with `Ctrl+C` handling (watch, reconcile, and webhook modes)
-   Validating Admission Webhook (HTTPS, TLS, self-signed cert generation)
-   Admission checks: reject `:latest` tags, missing probes at creation time
-   Fail-open design — webhook errors never block the cluster
-   System namespace bypass — never blocks `kube-system`, `cert-manager`, etc.
-   Kubernetes ServiceMonitor manifests for Prometheus auto-discovery
-   Grafana dashboard ConfigMap with 22 panels across 4 rows
-   Multi-stage Dockerfile for production container images
-   Kubernetes deployment manifests (Namespace, RBAC, Deployments, PDBs)
-   Helm chart for configurable production deployments
-   Comprehensive test suite (228 tests)
-   **Live deployment**: 6 pods (2 per component) on 9-node cluster, image `v0.1.2`
-   **Verified observability**: All 6 Prometheus targets UP, Grafana dashboard with live data

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
kube-devops webhook serve # Start the admission webhook HTTPS server
kube-devops webhook cert-generate   # Generate self-signed TLS certificates
kube-devops webhook cert-generate --ip-san 192.168.1.26  # With IP SANs for dev
kube-devops webhook install-config  # Print ValidatingWebhookConfiguration YAML
kube-devops observability generate-all              # Print all observability manifests
kube-devops observability generate-service-monitors # Print ServiceMonitor manifests
kube-devops observability generate-dashboard        # Print Grafana dashboard ConfigMap
kube-devops deploy generate-all          # Print all deployment manifests
kube-devops deploy generate-rbac         # Print RBAC manifests only
kube-devops deploy generate-deployments  # Print Deployment manifests only
```

------------------------------------------------------------------------

## Architecture

```
kube-devops/
 ├── Cargo.toml
 ├── Cargo.lock
 ├── rust-toolchain.toml
 ├── Dockerfile              # Multi-stage production build
 ├── .dockerignore
 ├── src/
 │   ├── main.rs              # Entry point, async runtime, command routing
 │   ├── lib.rs               # Library crate: exports admission + crd + governance + enforcement
 │   ├── cli.rs               # clap CLI definition (17 subcommands)
 │   ├── admission.rs         # Pure admission validation logic
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
 │        ├── reconcile.rs    # Operator reconcile loop, finalizers, metrics, HTTP server (:9090)
 │        ├── webhook.rs      # Admission webhook HTTPS server, cert gen, config
 │        ├── observability.rs # Service, ServiceMonitor, Grafana dashboard generators
 │        └── deploy.rs       # Deployment manifest generators (RBAC, Deployments, PDBs)
 ├── tests/
 │   ├── common/
 │   │   └── mod.rs                  # Shared test pod builder helper
 │   ├── admission_integration.rs    # Admission pipeline tests
 │   ├── enforcement_integration.rs  # Enforcement pipeline tests
 │   ├── governance_integration.rs   # End-to-end governance pipeline tests
 │   └── operator_integration.rs     # Operator reconcile pipeline tests
 ├── kube-tests/
 │   ├── test-pod.yaml               # Test pod manifest
 │   ├── sample-devopspolicy.yaml    # Example DevOpsPolicy CR
 │   ├── webhook-config.yaml         # ValidatingWebhookConfiguration template
 │   ├── service-watch.yaml          # Watch Service manifest
 │   ├── service-reconcile.yaml      # Reconcile Service manifest
 │   ├── service-webhook.yaml        # Webhook Service manifest
 │   ├── servicemonitor-watch.yaml   # Watch ServiceMonitor
 │   ├── servicemonitor-reconcile.yaml # Reconcile ServiceMonitor
 │   ├── servicemonitor-webhook.yaml # Webhook ServiceMonitor
 │   ├── grafana-dashboard-configmap.yaml # Grafana dashboard ConfigMap
 │   ├── namespace.yaml              # Namespace manifest
 │   ├── serviceaccount.yaml         # ServiceAccount manifest
 │   ├── clusterrole.yaml            # ClusterRole manifest
 │   ├── clusterrolebinding.yaml     # ClusterRoleBinding manifest
 │   ├── deployment-watch.yaml       # Watch Deployment manifest
 │   ├── deployment-reconcile.yaml   # Reconcile Deployment manifest
 │   ├── deployment-webhook.yaml     # Webhook Deployment manifest
 │   ├── pdb-watch.yaml              # Watch PodDisruptionBudget
 │   ├── pdb-reconcile.yaml          # Reconcile PodDisruptionBudget
 │   └── pdb-webhook.yaml            # Webhook PodDisruptionBudget
 ├── helm/kube-devops/               # Helm chart for production deployment
 │   ├── Chart.yaml
 │   ├── values.yaml
 │   └── templates/                  # 18 Helm templates
 └── docs/
     ├── Step_1_Code_Explanation.md
     ├── Step_2_Kubernetes_Integration.md
     ├── Step_3_DevOps_Analyzer_Engine.md
     ├── Step_4_Kubernetes_Watch_Engine.md
     ├── Step_4_Detailed_Developer_Documentation.md
     ├── Step_4_Testing.md
     ├── Step_5_Kubernetes_Operator.md
     ├── Step_6_Policy_Enforcement.md
     ├── Step_8_Prometheus_Expansion.md
     ├── Kubernetes_Observability_Controller_Progress.md
     ├── Kubernetes_Observability_Policy_Controller_Roadmap.md
     ├── Rust_Foundations_for_Kubernetes_DevOps.md
     ├── Rust_Borrowing_Rules.md
     └── Build_Fix_MSVC_Toolchain.md
```

### Core Subsystems

| Subsystem | File | Description |
|---|---|---|
| CLI | `cli.rs` | clap-based command parsing with 17 subcommands |
| Admission | `admission.rs` | Pure admission validation logic (policy-driven, fail-open) |
| CRD | `crd.rs` | DevOpsPolicy CRD definition with spec + status + enforcement types |
| Governance Engine | `governance.rs` | Pod evaluation, violation detection, policy-aware checks, weighted scoring |
| Enforcement Engine | `enforcement.rs` | Owner resolution, remediation planning, workload patching |
| Operator Reconciler | `commands/reconcile.rs` | Controller reconcile loop, finalizers, status updates |
| Watch Controller | `commands/watch.rs` | Kubernetes Watch API stream processing, incremental state updates |
| Admission Webhook | `commands/webhook.rs` | HTTPS server, TLS cert generation, webhook config |
| Leader Election | `commands/watch.rs` | Lease-based leader election for HA multi-replica deployment |
| Observability | `commands/observability.rs` | Service, ServiceMonitor, Grafana dashboard generators |
| Deploy | `commands/deploy.rs` | Deployment manifest generators (RBAC, Deployments, PDBs) |
| HTTP Server | `commands/watch.rs`, `commands/reconcile.rs` | axum servers exposing `/healthz`, `/readyz`, `/metrics` |
| Prometheus | `commands/watch.rs`, `commands/reconcile.rs`, `commands/webhook.rs` | 16 metrics across watch, reconcile, enforcement, and admission |
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

### Docker Build (on cluster master node)

``` bash
docker build -t 192.168.1.68:5000/kube-devops:v0.1.2 .
docker push 192.168.1.68:5000/kube-devops:v0.1.2
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
cargo run -- webhook serve --tls-cert tls.crt --tls-key tls.key
cargo run -- webhook cert-generate
cargo run -- webhook cert-generate --ip-san 192.168.1.26
cargo run -- webhook install-config --ca-bundle-path ca.crt
cargo run -- observability generate-all
cargo run -- observability generate-service-monitors
cargo run -- observability generate-dashboard
cargo run -- deploy generate-all
cargo run -- deploy generate-rbac
cargo run -- deploy generate-deployments
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
./target/debug/kube-devops webhook serve --tls-cert tls.crt --tls-key tls.key
./target/debug/kube-devops webhook cert-generate
./target/debug/kube-devops webhook cert-generate --ip-san 192.168.1.26
./target/debug/kube-devops webhook install-config --ca-bundle-path ca.crt
./target/debug/kube-devops observability generate-all
./target/debug/kube-devops observability generate-service-monitors
./target/debug/kube-devops observability generate-dashboard
./target/debug/kube-devops deploy generate-all
./target/debug/kube-devops deploy generate-rbac
./target/debug/kube-devops deploy generate-deployments
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

Endpoints available while running (port 9090):

| Endpoint | Purpose |
|---|---|
| `GET /healthz` | Liveness probe (always 200 OK) |
| `GET /readyz` | Readiness probe (503 until first reconcile, then 200) |
| `GET /metrics` | Prometheus metrics scrape endpoint |

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

## Running the Admission Webhook (Step 7)

The `webhook` subcommand manages the Validating Admission Webhook:

1. Generate TLS certificates: `cargo run -- webhook cert-generate`
   - For dev (outside cluster): `cargo run -- webhook cert-generate --ip-san <YOUR_IP>`
2. Start the webhook server: `cargo run -- webhook serve --tls-cert tls.crt --tls-key tls.key`
3. Install the webhook config: `cargo run -- webhook install-config --ca-bundle-path ca.crt | kubectl apply -f -`
4. Test: `kubectl run test-latest --image=nginx:latest -n production`

The webhook rejects Pods that violate the namespace's `DevOpsPolicy` rules
(`:latest` tags, missing probes). Pods in system namespaces are always allowed.
If the webhook can't reach the policy or encounters an error, it **fails open**
to avoid blocking the cluster.

**Note:** When running outside the cluster, use `--ip-san` with your machine's IP
during cert generation, and use a `url`-based `clientConfig` in the webhook
configuration instead of a `service` reference. See `docs/Step_7_Admission_Webhook.md`
for details.

Endpoints available while running:

| Endpoint | Purpose |
|---|---|
| `POST /validate` | Admission review handler |
| `GET /healthz` | Liveness probe (always 200 OK) |
| `GET /readyz` | Readiness probe (200 when ready) |
| `GET /metrics` | Prometheus metrics (webhook_requests_total, webhook_denials_total) |

------------------------------------------------------------------------

## Testing

The project includes 228 automated tests that run without a Kubernetes
cluster. All Pod and CRD objects are constructed synthetically in-memory.

``` bash
cargo test                                       # Full suite (228 tests)
cargo test --lib admission::tests                # Admission unit tests (16)
cargo test --lib governance::tests               # Governance unit tests (48)
cargo test --lib crd::tests                      # CRD unit tests (18)
cargo test --lib enforcement::tests              # Enforcement unit tests (30)
cargo test --lib commands::watch::tests          # HTTP endpoint + metrics tests (6)
cargo test --lib commands::reconcile::tests      # Reconcile + HTTP endpoint tests (20)
cargo test --lib commands::webhook::tests        # Webhook unit tests (9)
cargo test --lib commands::observability::tests  # Observability manifest tests (12)
cargo test --bin kube-devops commands::deploy::tests  # Deploy manifest tests (21)
cargo test --test admission_integration          # Admission integration tests (12)
cargo test --test governance_integration         # Governance integration tests (6)
cargo test --test operator_integration           # Operator integration tests (13)
cargo test --test enforcement_integration        # Enforcement integration tests (8)
```

| Test Layer | Location | Count | Scope |
|---|---|---|---|
| Unit (lib) | `src/admission.rs` | 16 | Verdict logic, policy filtering, denial messages, multi-container |
| Unit (lib) | `src/governance.rs` | 48 | Namespace filter, pod evaluation, violation detection, metrics, scoring, policy-aware evaluation |
| Unit (lib) | `src/crd.rs` | 18 | CRD schema, serialization, enforcement types, backward compatibility |
| Unit (lib) | `src/enforcement.rs` | 30 | Owner resolution, probe/resource building, plan generation, patch construction |
| Unit (bin) | `src/commands/watch.rs` | 6 | healthz, readyz, metrics, 404 handling, pods_tracked metric |
| Unit (bin) | `src/commands/reconcile.rs` | 20 | Aggregation, finalizers, deletion, status, HTTP endpoints, new metrics |
| Unit (bin) | `src/commands/webhook.rs` | 9 | Admission response, cert generation, TLS validation, duration metric |
| Unit (bin) | `src/commands/observability.rs` | 12 | Services, ServiceMonitors, Grafana dashboard, YAML validation |
| Unit (bin) | `src/commands/deploy.rs` | 21 | RBAC, Deployments, PDBs, Namespace, YAML validation, labels |
| Integration | `tests/admission_integration.rs` | 12 | Full admission pipeline, fail-open, multi-container, runtime check skip |
| Integration | `tests/governance_integration.rs` | 6 | End-to-end governance pipeline from pod to health classification |
| Integration | `tests/operator_integration.rs` | 13 | Full reconcile simulation, policy changes, CRD schema round-trip |
| Integration | `tests/enforcement_integration.rs` | 8 | Enforcement pipeline, audit vs enforce, namespace protection, deduplication |

See `docs/Step_4_Testing.md`, `docs/Step_5_Kubernetes_Operator.md`, `docs/Step_7_Admission_Webhook.md`, and `docs/Step_8_Prometheus_Expansion.md` for full test documentation.

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
| `axum-server` | HTTPS server with rustls TLS for admission webhook |
| `rcgen` | Self-signed TLS certificate generation |
| `rustls-pemfile` | PEM certificate/key file loading |
| `base64` | Encoding CA bundle for webhook configuration |
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
| 7 | Admission webhook (HTTPS, TLS, self-signed certs, fail-open, system ns bypass, 186 tests) | Done |
| 8 | Prometheus expansion (metrics HTTP server, ServiceMonitors, Grafana dashboard, 207 tests) | Done |
| 9 | High availability & production hardening (Dockerfile, manifests, Helm chart, 228 tests) | Done |

## Roadmap

| Step | Milestone | Status |
|---|---|---|
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

This project serves as a platform engineering laboratory deployed on a real
9-node Kubernetes cluster with full Prometheus and Grafana observability.

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
| `docs/Step_7_Admission_Webhook.md` | Admission webhook, TLS, fail-open design, CLI commands |
| `docs/Step_8_Prometheus_Expansion.md` | Metrics expansion, ServiceMonitors, Grafana dashboard |
| `docs/Step_9_HA_Production_Hardening.md` | Dockerfile, deployment manifests, Helm chart |
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

**Last Updated:** 2026-02-24
