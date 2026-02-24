[![CI](https://github.com/stochasticquant/kubernetes-cluster-observer-and-controller-using-Rust/actions/workflows/ci.yml/badge.svg)](https://github.com/stochasticquant/kubernetes-cluster-observer-and-controller-using-Rust/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-v1.26+-326CE5.svg)](https://kubernetes.io/)

# kube-devops — Kubernetes Observability & Policy Controller

A production-grade Kubernetes governance platform written in Rust. Enforces
DevOps best practices through CRD-driven policies, real-time monitoring,
automatic workload remediation, admission webhooks, and full Prometheus +
Grafana observability — deployed across multi-cluster environments.

**Version:** 0.2.0 | **Tests:** 314 | **Edition:** Rust 2024

------------------------------------------------------------------------

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
  - [Build from Source](#build-from-source)
  - [Docker Build](#docker-build)
  - [Helm Chart](#helm-chart)
  - [Raw Manifests](#raw-manifests)
- [CLI Reference](#cli-reference)
- [Operations Guide](#operations-guide)
  - [1. Install the CRD](#1-install-the-crd)
  - [2. Create a DevOpsPolicy](#2-create-a-devopspolicy)
  - [3. Run the Reconcile Operator](#3-run-the-reconcile-operator)
  - [4. Run the Watch Controller](#4-run-the-watch-controller)
  - [5. Run the Admission Webhook](#5-run-the-admission-webhook)
  - [6. Set Up Observability](#6-set-up-observability)
- [DevOpsPolicy CRD Reference](#devopspolicy-crd-reference)
  - [Spec Fields](#spec-fields)
  - [Enforcement Modes](#enforcement-modes)
  - [Severity Levels](#severity-levels)
  - [Severity Overrides](#severity-overrides)
  - [Default Probe Config](#default-probe-config)
  - [Default Resource Config](#default-resource-config)
- [Policy Bundles](#policy-bundles)
- [Multi-Cluster Governance](#multi-cluster-governance)
- [GitOps Workflows](#gitops-workflows)
- [Governance Scoring](#governance-scoring)
- [Prometheus Metrics](#prometheus-metrics)
- [HTTP Endpoints](#http-endpoints)
- [Helm Chart Configuration](#helm-chart-configuration)
- [Testing](#testing)
- [Project Structure](#project-structure)
- [Dependencies](#dependencies)
- [Development Workflow](#development-workflow)
- [Documentation](#documentation)
- [License](#license)

------------------------------------------------------------------------

## Features

**Governance Engine** — Weighted scoring across 5 violation types with
namespace-level health classification (Healthy / Stable / Degraded / Critical).

**CRD-Driven Policies** — `DevOpsPolicy` custom resources define per-namespace
governance rules with configurable checks, severity overrides, and enforcement
modes.

**Policy Enforcement** — Audit mode (default, non-mutating) or enforce mode
(auto-patches Deployments, StatefulSets, DaemonSets to inject missing probes
and resource limits).

**Admission Webhook** — Validating webhook rejects non-compliant pods at
creation time. Fail-open design ensures errors never block the cluster.

**Policy Bundles** — Three built-in policy templates (baseline, restricted,
permissive) for quick onboarding.

**Multi-Cluster** — Evaluate governance policies across multiple kubeconfig
contexts with aggregate scoring.

**GitOps** — Export, import, and diff policies between clusters and YAML files.

**Observability** — 16+ Prometheus metrics, ServiceMonitor auto-discovery,
Grafana dashboard with 26 panels across 4 rows.

**High Availability** — Leader election via Kubernetes Lease API, multi-replica
deployments with PodDisruptionBudgets, graceful shutdown.

**Production-Ready** — Multi-stage Dockerfile, Helm chart (18 templates),
non-root container, security-hardened deployments.

------------------------------------------------------------------------

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        kube-devops Platform                        │
├─────────────┬──────────────────┬──────────────────┬─────────────────┤
│ Watch       │ Reconcile        │ Webhook          │ CLI             │
│ Controller  │ Operator         │ Server           │ Tools           │
│ :8080       │ :9090            │ :8443 (HTTPS)    │                 │
├─────────────┼──────────────────┼──────────────────┼─────────────────┤
│ Watch API   │ Controller loop  │ POST /validate   │ analyze         │
│ Pod events  │ CRD reconcile    │ TLS termination  │ policy bundles  │
│ Leader      │ Status updates   │ Fail-open        │ multi-cluster   │
│ election    │ Enforcement      │ Severity filter  │ GitOps          │
│ Health      │ Audit results    │ System ns bypass │ deploy/observe  │
│ tracking    │ Finalizers       │                  │ manifest gen    │
├─────────────┴──────────────────┴──────────────────┴─────────────────┤
│                     Governance Engine (lib)                         │
│  governance.rs │ admission.rs │ enforcement.rs │ crd.rs │ bundles  │
├─────────────────────────────────────────────────────────────────────┤
│                    Kubernetes API (kube 0.88)                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Source Layout

```
kube-devops/
├── Cargo.toml                    # v0.2.0, edition 2024
├── Dockerfile                    # Multi-stage production build
├── src/
│   ├── main.rs                   # Entry point, async runtime, command routing
│   ├── lib.rs                    # Library: admission, bundles, crd, enforcement, governance, multi_cluster
│   ├── cli.rs                    # clap CLI (25 subcommands)
│   ├── admission.rs              # Admission validation logic
│   ├── bundles.rs                # Policy bundle templates (baseline, restricted, permissive)
│   ├── crd.rs                    # DevOpsPolicy + PolicyAuditResult CRDs, Severity, SeverityOverrides
│   ├── enforcement.rs            # Owner resolution, remediation, workload patching
│   ├── governance.rs             # Scoring engine, pod evaluation, violation detection
│   ├── multi_cluster.rs          # Multi-cluster evaluation and reporting
│   └── commands/
│       ├── mod.rs
│       ├── version.rs            # Version display
│       ├── check.rs              # Cluster connectivity and RBAC check
│       ├── list.rs               # Resource listing (pods)
│       ├── analyze.rs            # One-shot governance analysis
│       ├── watch.rs              # Watch controller, leader election, HTTP :8080
│       ├── crd.rs                # CRD generate/install
│       ├── reconcile.rs          # Operator reconcile loop, HTTP :9090
│       ├── webhook.rs            # Admission webhook HTTPS :8443, cert gen
│       ├── observability.rs      # Service, ServiceMonitor, Grafana generators
│       ├── deploy.rs             # Deployment manifest generators
│       ├── policy.rs             # Bundle list/show/apply, export/import/diff
│       └── multi_cluster.rs      # Multi-cluster list-contexts, analyze
├── tests/
│   ├── common/mod.rs             # Shared test helpers
│   ├── admission_integration.rs
│   ├── enforcement_integration.rs
│   ├── governance_integration.rs
│   └── operator_integration.rs
├── kube-tests/                   # Static reference manifests + sample CRs
├── helm/kube-devops/             # Helm chart (v0.3.0, 18 templates)
└── docs/                         # 16 technical guides
```

------------------------------------------------------------------------

## Prerequisites

- **Rust** (stable, edition 2024) — [install](https://rustup.rs/)
- **Cargo** (bundled with Rust)
- **kubectl** configured with a valid `~/.kube/config`
- **Kubernetes cluster** (v1.26+) for runtime commands
- **Docker** (for container builds)
- **Helm 3** (optional, for Helm-based deployment)

```bash
# Install Rust
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env

# Verify
rustc --version
cargo --version
```

------------------------------------------------------------------------

## Installation

### Build from Source

```bash
git clone https://github.com/stochasticquant/kube-devops.git
cd kube-devops

cargo build --release       # Release binary → target/release/kube-devops
```

### Docker Build

```bash
docker build -t <registry>/kube-devops:v0.2.0 .
docker push <registry>/kube-devops:v0.2.0
```

The Dockerfile uses a multi-stage build:
1. **Builder** (`rust:slim-bookworm`) — compiles the release binary
2. **Runtime** (`debian:bookworm-slim`) — minimal image with ca-certificates,
   non-root user (UID 1000), exposes ports 8080/9090/8443

### Helm Chart

```bash
# Install with defaults
helm install kube-devops ./helm/kube-devops \
  -n kube-devops --create-namespace

# Install with custom values
helm install kube-devops ./helm/kube-devops \
  -n kube-devops --create-namespace \
  --set image.repository=<registry>/kube-devops \
  --set image.tag=v0.2.0 \
  --set replicaCount=3

# Preview rendered templates
helm template kube-devops ./helm/kube-devops
```

### Raw Manifests

```bash
# Generate and apply all manifests
kube-devops deploy generate-all | kubectl apply -f -

# Or apply static reference manifests
kubectl apply -f kube-tests/namespace.yaml
kubectl apply -f kube-tests/serviceaccount.yaml
kubectl apply -f kube-tests/clusterrole.yaml
kubectl apply -f kube-tests/clusterrolebinding.yaml
kubectl apply -f kube-tests/deployment-watch.yaml
kubectl apply -f kube-tests/deployment-reconcile.yaml
kubectl apply -f kube-tests/deployment-webhook.yaml
kubectl apply -f kube-tests/pdb-watch.yaml
kubectl apply -f kube-tests/pdb-reconcile.yaml
kubectl apply -f kube-tests/pdb-webhook.yaml
```

------------------------------------------------------------------------

## CLI Reference

All commands are available via `kube-devops <command>` or `cargo run -- <command>`.

### Core Commands

| Command | Description |
|---|---|
| `version` | Display application version |
| `check` | Verify cluster connectivity and RBAC permissions |
| `list pods` | List pods across all namespaces |
| `analyze` | Run one-shot governance analysis on all workloads |

### Long-Running Controllers

| Command | Description | Port |
|---|---|---|
| `watch` | Start real-time governance watch controller | 8080 |
| `reconcile` | Start DevOpsPolicy operator reconcile loop | 9090 |
| `webhook serve` | Start admission webhook HTTPS server | 8443 |

### CRD Management

| Command | Description |
|---|---|
| `crd generate` | Print DevOpsPolicy CRD YAML to stdout |
| `crd install` | Install CRD into the connected cluster |

### Webhook Management

| Command | Description |
|---|---|
| `webhook serve [--addr 0.0.0.0:8443] [--tls-cert tls.crt] [--tls-key tls.key]` | Start HTTPS webhook server |
| `webhook cert-generate [--service-name ...] [--namespace ...] [--output-dir .] [--ip-san <IP>...]` | Generate self-signed TLS certs |
| `webhook install-config --ca-bundle-path <PATH> [--service-name ...] [--namespace ...]` | Print ValidatingWebhookConfiguration YAML |

### Policy Management

| Command | Description |
|---|---|
| `policy bundle-list` | List available policy bundles |
| `policy bundle-show <name>` | Show details of a policy bundle |
| `policy bundle-apply <name> [--namespace default] [--policy-name devops-policy]` | Generate DevOpsPolicy YAML from bundle |
| `policy export [--namespace default]` | Export DevOpsPolicies from namespace as YAML |
| `policy import <file> [--dry-run]` | Import DevOpsPolicies from YAML file |
| `policy diff <file>` | Diff local YAML policies against cluster state |

### Multi-Cluster

| Command | Description |
|---|---|
| `multi-cluster list-contexts` | List available kubeconfig contexts |
| `multi-cluster analyze [--contexts ctx1,ctx2] [--bundle baseline] [--per-cluster]` | Evaluate multiple clusters against a policy bundle |

### Manifest Generation

| Command | Description |
|---|---|
| `deploy generate-all` | Print all deployment manifests (Namespace + RBAC + Deployments + PDBs) |
| `deploy generate-rbac` | Print RBAC manifests only |
| `deploy generate-deployments` | Print Deployment manifests only |
| `observability generate-all` | Print all observability manifests |
| `observability generate-service-monitors` | Print ServiceMonitor manifests |
| `observability generate-dashboard` | Print Grafana dashboard ConfigMap |

------------------------------------------------------------------------

## Operations Guide

This section walks through operating kube-devops on a live cluster.

### 1. Install the CRD

The `DevOpsPolicy` CRD must be installed before creating policies:

```bash
kube-devops crd install
# or: kube-devops crd generate | kubectl apply -f -
```

Verify:

```bash
kubectl get crd devopspolicies.devops.stochastic.io
```

### 2. Create a DevOpsPolicy

**Option A — From a bundle template:**

```bash
# List available bundles
kube-devops policy bundle-list

# Preview the restricted bundle
kube-devops policy bundle-show restricted

# Generate and apply a policy from a bundle
kube-devops policy bundle-apply restricted --namespace production | kubectl apply -f -
```

**Option B — From a YAML file:**

```yaml
apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: default-policy
  namespace: production
spec:
  forbidLatestTag: true
  requireLivenessProbe: true
  requireReadinessProbe: true
  maxRestartCount: 3
  forbidPendingDuration: 300
```

```bash
kubectl apply -f my-policy.yaml
```

**Option C — With enforcement mode (auto-patching):**

```yaml
apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: enforce-policy
  namespace: staging
spec:
  forbidLatestTag: true
  requireLivenessProbe: true
  requireReadinessProbe: true
  maxRestartCount: 3
  forbidPendingDuration: 300
  enforcementMode: enforce
  defaultProbe:
    tcpPort: 8080
    initialDelaySeconds: 5
    periodSeconds: 10
  defaultResources:
    cpuRequest: "100m"
    cpuLimit: "500m"
    memoryRequest: "128Mi"
    memoryLimit: "256Mi"
```

**Option D — With severity overrides:**

```yaml
apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: strict-policy
  namespace: production
spec:
  forbidLatestTag: true
  requireLivenessProbe: true
  requireReadinessProbe: true
  maxRestartCount: 3
  forbidPendingDuration: 300
  severityOverrides:
    latestTag: critical
    missingLiveness: high
    missingReadiness: high
    highRestarts: medium
    pending: low
```

### 3. Run the Reconcile Operator

The reconcile operator continuously evaluates pods against DevOpsPolicy CRs
and updates their `.status`:

```bash
kube-devops reconcile
```

What it does:
- Watches all `DevOpsPolicy` CRs via `kube_runtime::Controller`
- Evaluates pods in the policy's namespace against the policy's enabled checks
- Updates the CR's `.status` with health score, violations, and classification
- In enforce mode, patches parent workloads to inject missing probes/resources
- Creates `PolicyAuditResult` CRs with detailed violation records
- Manages finalizers (`devops.stochastic.io/cleanup`) for clean deletion
- Exposes HTTP endpoints on port 9090

Check the status after the operator runs:

```bash
kubectl get devopspolicies -n production -o yaml
```

Press **Ctrl+C** for graceful shutdown.

### 4. Run the Watch Controller

The watch controller provides real-time governance monitoring:

```bash
kube-devops watch
```

What it does:
- Acquires a Kubernetes Lease for leader election (namespace: `kube-devops`)
- Opens a Watch API stream for all pods in the cluster
- Evaluates governance violations in real time as pods change
- Maintains namespace-level health scores
- Non-leader replicas serve health probes while waiting for leader promotion
- Exposes HTTP endpoints on port 8080

### 5. Run the Admission Webhook

The webhook prevents non-compliant pods from being created:

```bash
# Step 1: Generate TLS certificates
kube-devops webhook cert-generate

# For development (outside cluster), add IP SANs:
kube-devops webhook cert-generate --ip-san 192.168.1.26

# Step 2: Start the webhook server
kube-devops webhook serve --tls-cert tls.crt --tls-key tls.key

# Step 3: Install the webhook configuration (in another terminal)
kube-devops webhook install-config --ca-bundle-path ca.crt | kubectl apply -f -

# Step 4: Test — this should be rejected if the namespace has a policy forbidding :latest
kubectl run test-latest --image=nginx:latest -n production
```

Webhook behavior:
- Rejects pods violating the namespace's `DevOpsPolicy` rules (`:latest` tags, missing probes)
- Severity-aware: only blocks violations at or above the configured severity threshold
- System namespaces (`kube-system`, `cert-manager`, etc.) are always allowed
- **Fail-open**: errors never block the cluster
- Runtime-only checks (restarts, pending) are automatically skipped at admission time

**In-cluster deployment:** The Helm chart and deployment manifests automatically
mount TLS certificates from a Kubernetes Secret and configure the webhook with
`--tls-cert /tls/tls.crt --tls-key /tls/tls.key`.

### 6. Set Up Observability

```bash
# Generate and apply ServiceMonitors + Grafana dashboard
kube-devops observability generate-all | kubectl apply -f -

# Or generate specific components
kube-devops observability generate-service-monitors | kubectl apply -f -
kube-devops observability generate-dashboard | kubectl apply -f -
```

The Grafana dashboard ConfigMap is automatically imported by the Grafana sidecar
when labeled correctly. It contains 26 panels across 4 rows:
- **Watch Metrics** — cluster health, namespace health, pod events
- **Reconcile Metrics** — reconcile counts, errors, violations, health scores
- **Enforcement Metrics** — remediations applied/failed, enforcement mode
- **Webhook Metrics** — requests, denials, duration

------------------------------------------------------------------------

## DevOpsPolicy CRD Reference

**API Group:** `devops.stochastic.io/v1`
**Kind:** `DevOpsPolicy`
**Scope:** Namespaced

### Spec Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `forbidLatestTag` | `bool` | `nil` (skip) | Flag pods using `:latest` image tags |
| `requireLivenessProbe` | `bool` | `nil` (skip) | Flag containers missing liveness probes |
| `requireReadinessProbe` | `bool` | `nil` (skip) | Flag containers missing readiness probes |
| `maxRestartCount` | `int` | `nil` (skip) | Flag pods exceeding this restart count |
| `forbidPendingDuration` | `int` | `nil` (skip) | Flag pods pending longer than N seconds |
| `enforcementMode` | `string` | `audit` | `audit` or `enforce` |
| `defaultProbe` | `object` | `nil` | Probe config for auto-injection (enforce mode) |
| `defaultResources` | `object` | `nil` | Resource config for auto-injection (enforce mode) |
| `severityOverrides` | `object` | `nil` | Per-violation severity customization |

Fields set to `nil` (omitted) are skipped during evaluation — the operator
only checks what the policy explicitly enables.

### Enforcement Modes

| Mode | Behavior |
|---|---|
| `audit` | Detect and report violations. Never mutates workloads. Default. |
| `enforce` | Automatically patch Deployments, StatefulSets, and DaemonSets to inject missing probes and resource limits. Non-patchable violations (`:latest` tags, high restarts, pending) remain detection-only. |

When in enforce mode:
- Parent workloads are resolved via `ownerReferences`
- Each parent is patched at most once per reconcile cycle (deduplication)
- Patched workloads are annotated with `devops.stochastic.io/patched-by`
- System namespaces (`kube-system`, `cert-manager`, `istio-system`, etc.) are never enforced

### Severity Levels

Violations can have one of four severity levels:

| Severity | Description |
|---|---|
| `critical` | Blocks admission webhook. Immediate action required. |
| `high` | Blocks admission webhook. Important violation. |
| `medium` | Default. Reported in audit. |
| `low` | Informational. Reported in audit. |

### Severity Overrides

Customize severity per violation type:

```yaml
spec:
  severityOverrides:
    latestTag: critical       # default: medium
    missingLiveness: high     # default: medium
    missingReadiness: high    # default: medium
    highRestarts: medium      # default: medium
    pending: low              # default: medium
```

### Default Probe Config

Used by enforce mode to inject TCP probes into containers missing them:

```yaml
spec:
  defaultProbe:
    tcpPort: 8080               # TCP port to probe (default: container's first port, then 8080)
    initialDelaySeconds: 5      # Seconds before first probe (default: 5)
    periodSeconds: 10           # Seconds between probes (default: 10)
```

### Default Resource Config

Used by enforce mode to inject resource requests/limits:

```yaml
spec:
  defaultResources:
    cpuRequest: "100m"
    cpuLimit: "500m"
    memoryRequest: "128Mi"
    memoryLimit: "256Mi"
```

### Status Sub-Resource

The operator updates `.status` after each reconcile:

```yaml
status:
  healthScore: 85
  violations:
    - "pod/nginx-abc123: uses :latest image tag"
  healthy: true
  message: "Healthy"
  lastEvaluated: "2026-02-24T12:00:00Z"
  observedGeneration: 1
```

------------------------------------------------------------------------

## Policy Bundles

Three built-in bundles provide pre-configured policy templates:

### baseline

Balanced policy for general use. Audit mode (non-mutating).

```bash
kube-devops policy bundle-apply baseline --namespace production | kubectl apply -f -
```

| Check | Enabled |
|---|---|
| Forbid `:latest` tag | Yes |
| Require readiness probe | Yes |
| Enforcement mode | Audit |

### restricted

Strict enforcement with auto-patching. For security-critical namespaces.

```bash
kube-devops policy bundle-apply restricted --namespace production | kubectl apply -f -
```

| Check | Enabled | Severity |
|---|---|---|
| Forbid `:latest` tag | Yes | Critical |
| Require liveness probe | Yes | High |
| Require readiness probe | Yes | High |
| Max restart count | 3 | Critical |
| Forbid pending duration | 300s | High |
| Enforcement mode | Enforce | — |
| Default probe | TCP :8080, 5s delay, 10s period | — |
| Default resources | 100m/500m CPU, 128Mi/256Mi memory | — |

### permissive

Lenient monitoring for development or staging. Audit mode.

```bash
kube-devops policy bundle-apply permissive --namespace staging | kubectl apply -f -
```

| Check | Enabled | Severity |
|---|---|---|
| Forbid `:latest` tag | Yes | Low |
| Require liveness probe | Yes | Low |
| Require readiness probe | Yes | Low |
| Max restart count | 10 | Medium |
| Forbid pending duration | 600s | Low |
| Enforcement mode | Audit | — |

------------------------------------------------------------------------

## Multi-Cluster Governance

Evaluate policies across multiple kubeconfig contexts:

```bash
# List all available contexts
kube-devops multi-cluster list-contexts

# Analyze all contexts with the baseline bundle
kube-devops multi-cluster analyze

# Analyze specific contexts with the restricted bundle
kube-devops multi-cluster analyze --contexts prod-us,prod-eu --bundle restricted

# Show per-cluster breakdown
kube-devops multi-cluster analyze --bundle restricted --per-cluster
```

The output includes per-cluster health scores, violation counts, and an
aggregate score across all evaluated clusters.

------------------------------------------------------------------------

## GitOps Workflows

Export, import, and diff policies for version-controlled policy management:

```bash
# Export policies from a namespace to YAML
kube-devops policy export --namespace production > policies.yaml

# Preview an import (dry-run)
kube-devops policy import policies.yaml --dry-run

# Import policies from YAML
kube-devops policy import policies.yaml

# Diff local YAML against live cluster state
kube-devops policy diff policies.yaml
```

------------------------------------------------------------------------

## Governance Scoring

The scoring engine applies weighted penalties per pod:

| Violation | Weight |
|---|---|
| `:latest` image tag | 5 |
| Missing liveness probe | 3 |
| Missing readiness probe | 2 |
| High restart count (> threshold) | 6 |
| Pending phase (> threshold) | 4 |

**Health score formula:** `100 - min(raw_penalty / total_pods, 100)`

| Score Range | Classification |
|---|---|
| 80 – 100 | Healthy |
| 60 – 79 | Stable |
| 40 – 59 | Degraded |
| 0 – 39 | Critical |

System namespaces are automatically excluded: `kube-system`, `kube-public`,
`kube-node-lease`, `cert-manager`, `istio-system`, and any namespace ending
in `-system`.

------------------------------------------------------------------------

## Prometheus Metrics

16+ metrics exposed across the three components:

### Watch Controller (`:8080/metrics`)

| Metric | Type | Description |
|---|---|---|
| `cluster_health_score` | Gauge | Current cluster-wide health score |
| `namespace_health_score` | Gauge | Per-namespace health score |
| `pod_events_total` | Counter | Total pod events processed |
| `pods_tracked_total` | Gauge | Current number of tracked pods |

### Reconcile Operator (`:9090/metrics`)

| Metric | Type | Description |
|---|---|---|
| `devopspolicy_reconcile_total` | Counter | Total reconciliations |
| `devopspolicy_reconcile_errors_total` | Counter | Failed reconciliations |
| `devopspolicy_violations_total` | Gauge | Violations per namespace/policy |
| `devopspolicy_health_score` | Gauge | Health score per namespace/policy |
| `devopspolicy_pods_scanned_total` | Counter | Total pods evaluated |
| `devopspolicy_reconcile_duration_seconds` | Histogram | Reconciliation latency |
| `enforcement_remediations_applied_total` | Counter | Successful patches |
| `enforcement_remediations_failed_total` | Counter | Failed patches |
| `enforcement_mode` | Gauge | Current enforcement mode (0=audit, 1=enforce) |
| `violations_by_severity` | Gauge | Violations grouped by severity level |
| `audit_results_total` | Counter | PolicyAuditResult CRs created |

### Webhook Server (`:8443/metrics`)

| Metric | Type | Description |
|---|---|---|
| `webhook_requests_total` | Counter | Total admission requests |
| `webhook_denials_total` | Counter | Denied admission requests |
| `webhook_request_duration_seconds` | Histogram | Admission request latency |

------------------------------------------------------------------------

## HTTP Endpoints

### Watch Controller — Port 8080

| Endpoint | Method | Description |
|---|---|---|
| `/healthz` | GET | Liveness probe (always 200 OK) |
| `/readyz` | GET | Readiness probe (503 until initial sync, then 200) |
| `/metrics` | GET | Prometheus metrics scrape endpoint |

### Reconcile Operator — Port 9090

| Endpoint | Method | Description |
|---|---|---|
| `/healthz` | GET | Liveness probe (always 200 OK) |
| `/readyz` | GET | Readiness probe (503 until first reconcile, then 200) |
| `/metrics` | GET | Prometheus metrics scrape endpoint |

### Webhook Server — Port 8443 (HTTPS)

| Endpoint | Method | Description |
|---|---|---|
| `/validate` | POST | Admission review handler |
| `/healthz` | GET | Liveness probe (200 OK) |
| `/readyz` | GET | Readiness probe (200 when ready) |
| `/metrics` | GET | Prometheus metrics scrape endpoint |

------------------------------------------------------------------------

## Helm Chart Configuration

**Chart version:** 0.3.0 | **App version:** 0.2.0

| Value | Default | Description |
|---|---|---|
| `image.repository` | `192.168.1.68:5000/kube-devops` | Container image repository |
| `image.tag` | `v0.2.0` | Image tag |
| `image.pullPolicy` | `IfNotPresent` | Image pull policy |
| `replicaCount` | `2` | Replicas per component |
| `resources.requests.memory` | `64Mi` | Memory request |
| `resources.requests.cpu` | `100m` | CPU request |
| `resources.limits.memory` | `128Mi` | Memory limit |
| `resources.limits.cpu` | `250m` | CPU limit |
| `serviceMonitor.enabled` | `true` | Create Prometheus ServiceMonitors |
| `serviceMonitor.interval` | `15s` | Prometheus scrape interval |
| `grafanaDashboard.enabled` | `true` | Create Grafana dashboard ConfigMap |
| `auditResults.retention` | `10` | Max PolicyAuditResults per policy |
| `pdb.enabled` | `true` | Create PodDisruptionBudgets |
| `pdb.minAvailable` | `1` | Minimum available pods per component |

The chart deploys 3 components (watch, reconcile, webhook), each with its own
Deployment, Service, ServiceMonitor, and PodDisruptionBudget.

------------------------------------------------------------------------

## Testing

314 automated tests run without a Kubernetes cluster. All Pod and CRD objects
are constructed synthetically in-memory.

```bash
cargo test                  # Full suite (314 tests)
cargo clippy --all-targets  # Zero warnings
```

### Test Breakdown

| Test Layer | Location | Count | Scope |
|---|---|---|---|
| Unit (lib) | `src/admission.rs` | 16 | Verdict logic, policy filtering, denial messages, multi-container |
| Unit (lib) | `src/governance.rs` | 48 | Namespace filter, pod evaluation, violation detection, metrics, scoring |
| Unit (lib) | `src/crd.rs` | 18 | CRD schema, serialization, enforcement types, backward compatibility |
| Unit (lib) | `src/enforcement.rs` | 30 | Owner resolution, probe/resource building, plan generation, patches |
| Unit (lib) | `src/bundles.rs` | — | Bundle definitions, lookups |
| Unit (lib) | `src/multi_cluster.rs` | — | Context listing, report aggregation |
| Unit (bin) | `src/commands/watch.rs` | 6 | healthz, readyz, metrics, 404 handling |
| Unit (bin) | `src/commands/reconcile.rs` | 20 | Aggregation, finalizers, deletion, status, HTTP endpoints |
| Unit (bin) | `src/commands/webhook.rs` | 9 | Admission response, cert gen, TLS validation |
| Unit (bin) | `src/commands/observability.rs` | 12 | Services, ServiceMonitors, Grafana dashboard |
| Unit (bin) | `src/commands/deploy.rs` | 21 | RBAC, Deployments, PDBs, Namespace, YAML validation |
| Unit (bin) | `src/commands/policy.rs` | — | Bundle CLI handlers |
| Unit (bin) | `src/commands/multi_cluster.rs` | — | Multi-cluster CLI handlers |
| Integration | `tests/admission_integration.rs` | 12 | Full admission pipeline, fail-open, multi-container |
| Integration | `tests/governance_integration.rs` | 6 | End-to-end governance pipeline |
| Integration | `tests/operator_integration.rs` | 13 | Full reconcile simulation, policy changes, CRD schema |
| Integration | `tests/enforcement_integration.rs` | 8 | Enforcement pipeline, audit vs enforce, namespace protection |
| **Total** | | **314** | **All passing, no cluster required** |

### Running Specific Tests

```bash
cargo test --lib admission::tests                # Admission unit tests
cargo test --lib governance::tests               # Governance unit tests
cargo test --lib crd::tests                      # CRD unit tests
cargo test --lib enforcement::tests              # Enforcement unit tests
cargo test --lib commands::watch::tests          # Watch HTTP tests
cargo test --lib commands::reconcile::tests      # Reconcile tests
cargo test --lib commands::webhook::tests        # Webhook unit tests
cargo test --lib commands::observability::tests  # Observability tests
cargo test --bin kube-devops commands::deploy::tests  # Deploy tests
cargo test --test admission_integration          # Admission integration
cargo test --test governance_integration         # Governance integration
cargo test --test operator_integration           # Operator integration
cargo test --test enforcement_integration        # Enforcement integration
```

------------------------------------------------------------------------

## Project Structure

### Core Subsystems

| Subsystem | File | Description |
|---|---|---|
| CLI | `cli.rs` | clap-based command parsing with 25 subcommands |
| Admission | `admission.rs` | Pure admission validation logic (policy-driven, fail-open) |
| Bundles | `bundles.rs` | Pre-defined policy templates (baseline, restricted, permissive) |
| CRD | `crd.rs` | DevOpsPolicy + PolicyAuditResult CRDs, Severity, SeverityOverrides |
| Governance | `governance.rs` | Pod evaluation, violation detection, weighted scoring, health classification |
| Enforcement | `enforcement.rs` | Owner resolution, remediation planning, workload patching |
| Multi-Cluster | `multi_cluster.rs` | Multi-context evaluation, aggregate reporting |
| Operator | `commands/reconcile.rs` | Controller reconcile loop, finalizers, status updates, audit results |
| Watch | `commands/watch.rs` | Watch API stream, leader election, incremental state |
| Webhook | `commands/webhook.rs` | HTTPS server, TLS cert gen, webhook config |
| Policy CLI | `commands/policy.rs` | Bundle list/show/apply, export/import/diff |
| Multi-Cluster CLI | `commands/multi_cluster.rs` | list-contexts, analyze handlers |
| Observability | `commands/observability.rs` | Service, ServiceMonitor, Grafana dashboard generators |
| Deploy | `commands/deploy.rs` | Deployment manifest generators (RBAC, Deployments, PDBs) |
| HTTP Servers | `commands/watch.rs`, `reconcile.rs` | axum servers: `/healthz`, `/readyz`, `/metrics` |
| Prometheus | Multiple | 16+ metrics across watch, reconcile, enforcement, admission |
| Logging | `main.rs` | Structured JSON logging via `tracing` + `tracing-subscriber` |

------------------------------------------------------------------------

## Dependencies

| Crate | Purpose |
|---|---|
| `clap` 4 | CLI argument parsing |
| `kube` 0.88 / `kube-runtime` 0.88 | Kubernetes client, Watch API, Controller runtime, CRD derive |
| `k8s-openapi` 0.21 (v1_26) | Kubernetes API type definitions |
| `tokio` 1 | Async runtime |
| `futures` 0.3 | Stream combinators for watch events |
| `axum` 0.7 | HTTP server for health/metrics endpoints |
| `axum-server` 0.7 | HTTPS server with rustls TLS for admission webhook |
| `prometheus` 0.13 | Metrics registry and text encoding |
| `rcgen` 0.13 | Self-signed TLS certificate generation |
| `rustls-pemfile` 2 | PEM certificate/key file loading |
| `base64` 0.22 | Encoding CA bundle for webhook configuration |
| `tracing` 0.1 / `tracing-subscriber` 0.3 | Structured JSON logging |
| `anyhow` 1 | Error handling |
| `serde` 1 / `serde_json` 1 / `serde_yaml` 0.9 | Serialization for CRD structs and YAML output |
| `schemars` 0.8 | JSON Schema generation for CRD validation |
| `chrono` 0.4 | Timestamps for status updates |

Dev-dependencies: `tower` 0.5 (HTTP testing), `http-body-util` 0.1 (response body reading).

------------------------------------------------------------------------

## Development Workflow

```bash
# Create a feature branch
git checkout -b feature/my-feature

# Make changes, then verify
cargo test                     # All 314 tests pass
cargo clippy --all-targets     # Zero warnings

# Commit and push
git commit -m "feat: description"
git push origin feature/my-feature
```

Branch strategy:
- `main` — Stable baseline
- `feature/*` — Feature development branches

Commit style: `feat(step-N): description`, `fix: description`, `chore: description`

------------------------------------------------------------------------

## Completed Steps

| Step | Milestone | Status |
|---|---|---|
| 1 | Rust CLI foundations (clap, modular commands, error handling) | Done |
| 2 | Kubernetes read-only client (list pods, async API) | Done |
| 3 | DevOps governance analyzer (scoring engine, health classification) | Done |
| 4 | Real-time watch engine (Watch API, leader election, Prometheus, HTTP endpoints) | Done |
| 5 | Kubernetes Operator (DevOpsPolicy CRD, reconciliation loop, finalizers, policy-aware evaluation) | Done |
| 6 | Policy enforcement mode (audit/enforce, auto-patch workloads, inject probes + resource limits) | Done |
| 7 | Admission webhook (HTTPS, TLS, self-signed certs, fail-open, system namespace bypass) | Done |
| 8 | Prometheus expansion (metrics HTTP server, ServiceMonitors, Grafana dashboard with 26 panels) | Done |
| 9 | High availability & production hardening (Dockerfile, manifests, Helm chart, live deployment) | Done |
| 10 | Multi-cluster governance, severity levels, policy bundles, audit results, GitOps support | Done |

------------------------------------------------------------------------

## Documentation

| Document | Description |
|---|---|
| `docs/Step_1_Code_Explanation.md` | Rust CLI foundations walkthrough |
| `docs/Step_2_Kubernetes_Integration.md` | Kubernetes client integration |
| `docs/Step_3_DevOps_Analyzer_Engine.md` | Governance analyzer design |
| `docs/Step_4_Kubernetes_Watch_Engine.md` | Watch engine concepts and architecture |
| `docs/Step_4_Detailed_Developer_Documentation.md` | Deep technical reference for Step 4 subsystems |
| `docs/Step_4_Testing.md` | Test suite documentation |
| `docs/Step_5_Kubernetes_Operator.md` | CRD, reconciliation loop, finalizers, operator architecture |
| `docs/Step_6_Policy_Enforcement.md` | Enforcement mode, remediation, patching architecture |
| `docs/Step_7_Admission_Webhook.md` | Admission webhook, TLS, fail-open design |
| `docs/Step_8_Prometheus_Expansion.md` | Metrics, ServiceMonitors, Grafana dashboard |
| `docs/Step_9_HA_Production_Hardening.md` | Dockerfile, deployment manifests, Helm chart |
| `docs/Step_10_Multi_Cluster_Governance.md` | Multi-cluster, severity levels, policy bundles, GitOps |
| `docs/Kubernetes_Observability_Controller_Progress.md` | Implementation progress tracker |
| `docs/Kubernetes_Observability_Policy_Controller_Roadmap.md` | Full 10-step roadmap |
| `docs/Rust_Foundations_for_Kubernetes_DevOps.md` | Rust language foundations reference |
| `docs/Rust_Borrowing_Rules.md` | Ownership and borrowing reference |
| `docs/Build_Fix_MSVC_Toolchain.md` | MSVC toolchain troubleshooting |

------------------------------------------------------------------------

## Expected Output

`version`:
```
kube-devops version 0.2.0
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

`policy bundle-list`:
```
Available policy bundles:
  baseline    — Balanced audit policy for general use
  restricted  — Strict enforcement with auto-patching
  permissive  — Lenient monitoring for development
```

------------------------------------------------------------------------

## License

MIT License

------------------------------------------------------------------------

## Author

StochasticQuant
DevOps & Platform Engineering Lab

------------------------------------------------------------------------

**Last Updated:** 2026-02-24
