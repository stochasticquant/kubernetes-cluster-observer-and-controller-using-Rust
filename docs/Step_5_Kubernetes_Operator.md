# Step 5 — Build Your First Kubernetes Operator

## Overview

Step 5 converts the tool from a standalone watch controller into a **true Kubernetes Operator** by introducing a Custom Resource Definition (CRD). Users define governance rules as Kubernetes resources (`DevOpsPolicy`), and a reconciliation loop continuously ensures the desired policy state matches the observed cluster state.

This is real operator engineering — the same pattern used by tools like Kyverno, OPA Gatekeeper, and every production Kubernetes operator.

---

## What You Learn

- Custom Resource Definitions (CRDs) — extending the Kubernetes API
- Controller reconciliation pattern — the heart of every operator
- Desired vs observed state — declarative infrastructure management
- Finalizers — safe resource deletion with cleanup guarantees
- Status sub-resources — reporting compliance state back to users
- `kube::CustomResource` derive macro — Rust-native CRD generation
- `kube_runtime::Controller` — production-grade reconcile framework
- Policy-aware evaluation — configurable compliance checks

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    DevOpsPolicy Operator                        │
│                                                                 │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────────┐ │
│  │  Controller  │───>│  Reconciler  │───>│  Status Updater    │ │
│  │  (watcher)   │    │              │    │                    │ │
│  └─────────────┘    │  1. Fetch CR │    │  - health_score    │ │
│                      │  2. List Pods│    │  - violations      │ │
│  ┌─────────────┐    │  3. Evaluate │    │  - healthy         │ │
│  │  Finalizer   │    │  4. Score    │    │  - message         │ │
│  │  Manager     │    │  5. Classify │    │  - last_evaluated  │ │
│  └─────────────┘    └──────────────┘    └────────────────────┘ │
│                                                                 │
│  ┌─────────────┐    ┌──────────────┐                           │
│  │  Prometheus  │    │  Governance  │                           │
│  │  Metrics     │    │  Engine      │                           │
│  └─────────────┘    └──────────────┘                           │
└─────────────────────────────────────────────────────────────────┘
```

### Reconcile Loop Flow

1. Controller watches `DevOpsPolicy` CRs across all namespaces
2. On change (create/update/delete) or requeue interval (30s):
   - Check for deletion → handle finalizer cleanup
   - Ensure finalizer is present on the CR
   - List all pods in the policy's namespace
   - Evaluate each pod against the policy spec using policy-aware functions
   - Aggregate metrics and count violations
   - Calculate health score and classification
   - Update Prometheus metrics
   - Patch the CR's `.status` sub-resource
   - Requeue for next evaluation cycle

---

## CRD Definition

### DevOpsPolicy Custom Resource

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

### Spec Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `forbidLatestTag` | `bool?` | `None` (skip) | Flag images using `:latest` tag |
| `requireLivenessProbe` | `bool?` | `None` (skip) | Require liveness probes on all containers |
| `requireReadinessProbe` | `bool?` | `None` (skip) | Require readiness probes on all containers |
| `maxRestartCount` | `i32?` | `None` (skip) | Maximum restart count threshold |
| `forbidPendingDuration` | `u64?` | `None` (skip) | Max seconds a pod may remain Pending |

Omitted fields (`None`) are treated as disabled — the check is skipped entirely.

### Status Sub-resource

The operator updates `.status` on every reconciliation:

| Field | Type | Description |
|---|---|---|
| `observedGeneration` | `i64?` | Last reconciled `.metadata.generation` |
| `healthy` | `bool?` | Whether health score >= 80 |
| `healthScore` | `u32?` | Governance score (0–100) |
| `violations` | `u32?` | Total violations detected |
| `lastEvaluated` | `string?` | ISO 8601 timestamp |
| `message` | `string?` | Human-readable summary |

After reconciliation, `kubectl get devopspolicies` shows the current compliance state.

---

## Implementation Details

### File Structure

```
src/
├── lib.rs                    # Library crate: exports crd + governance
├── crd.rs                    # DevOpsPolicy CRD definition
├── governance.rs             # Scoring engine + policy-aware evaluation
└── commands/
    ├── crd.rs                # CRD generate/install CLI commands
    └── reconcile.rs          # Reconciliation loop, finalizers, metrics
```

### CRD Definition (`src/crd.rs`)

Uses `kube::CustomResource` derive macro:

```rust
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[kube(
    group = "devops.stochastic.io",
    version = "v1",
    kind = "DevOpsPolicy",
    plural = "devopspolicies",
    status = "DevOpsPolicyStatus",
    namespaced
)]
pub struct DevOpsPolicySpec { ... }
```

This generates:
- A `DevOpsPolicy` struct with metadata, spec, and status
- A full OpenAPI v3 schema for Kubernetes validation
- Integration with `kube::CustomResourceExt::crd()` for YAML generation

### Policy-Aware Evaluation (`src/governance.rs`)

Two new functions complement the existing evaluation:

```rust
pub fn evaluate_pod_with_policy(pod: &Pod, policy: &DevOpsPolicySpec) -> PodMetrics
pub fn detect_violations_with_policy(pod: &Pod, policy: &DevOpsPolicySpec) -> Vec<&'static str>
```

These only check violations that the policy explicitly enables. For example, if `forbid_latest_tag` is `None` or `Some(false)`, the `:latest` tag check is skipped entirely.

The `max_restart_count` field acts as a custom threshold — pods with restart counts at or below the threshold are not flagged.

### Reconciliation Loop (`src/commands/reconcile.rs`)

Built on `kube_runtime::Controller`:

```rust
Controller::new(policies, Default::default())
    .owns(pods, Default::default())
    .run(reconcile, error_policy, ctx)
```

The `reconcile` function:
1. Checks for deletion (handles finalizer removal)
2. Ensures finalizer is present
3. Lists pods in the policy's namespace
4. Evaluates each pod with `evaluate_pod_with_policy()`
5. Aggregates metrics and counts violations
6. Computes health score and classification
7. Updates Prometheus metrics
8. Patches the CR's `.status` sub-resource
9. Returns `Action::requeue(30s)` for periodic re-evaluation

### Finalizers (`devops.stochastic.io/cleanup`)

Finalizers ensure cleanup runs before a CR is deleted:

- **Add**: On first reconcile, the operator patches the finalizer onto the CR
- **Remove**: When `deletion_timestamp` is set, the operator clears Prometheus metrics and removes the finalizer
- This prevents orphaned metrics when a policy is deleted

### Graceful Shutdown

The reconcile command supports graceful Ctrl+C shutdown using `tokio::select!`:

```rust
tokio::select! {
    _ = controller => { /* stream ended */ }
    _ = signal::ctrl_c() => { /* clean shutdown */ }
}
```

When the user presses Ctrl+C, the operator prints a shutdown banner and exits cleanly — matching the pattern used by the `watch` command.

### Error Policy

Failed reconciliations are requeued after 60 seconds with a logged warning. The `devopspolicy_reconcile_errors_total` counter is incremented.

---

## Prometheus Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `devopspolicy_reconcile_total` | Counter | — | Total reconciliation cycles |
| `devopspolicy_reconcile_errors_total` | Counter | — | Failed reconciliations |
| `devopspolicy_violations_total` | Gauge | `namespace`, `policy` | Violations per policy |
| `devopspolicy_health_score` | Gauge | `namespace`, `policy` | Health score per policy |

These are separate from the Step 4 watch metrics and use their own Prometheus registry.

---

## CLI Commands

### Generate CRD YAML

```bash
kube-devops crd generate
```

Prints the full CRD YAML to stdout. Pipe to `kubectl apply`:

```bash
kube-devops crd generate | kubectl apply -f -
```

### Install CRD to Cluster

```bash
kube-devops crd install
```

Programmatically applies the CRD. Skips if already installed.

### Start Operator

```bash
kube-devops reconcile
```

Starts the reconciliation loop. Watches all `DevOpsPolicy` CRs and continuously evaluates compliance.

---

## Usage Guide

### 1. Install the CRD

```bash
cargo run -- crd install
```

### 2. Create a Policy

```bash
kubectl apply -f kube-tests/sample-devopspolicy.yaml
```

Or create your own:

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
```

### 3. Start the Operator

```bash
cargo run -- reconcile
```

### 4. Check Policy Status

```bash
kubectl get devopspolicies -n production -o yaml
```

The `.status` section shows:
```yaml
status:
  healthy: true
  healthScore: 87
  violations: 3
  lastEvaluated: "2026-02-22T10:00:00Z"
  message: "3 violations across 42 pods — Healthy (87)"
```

---

## Dependencies Added in Step 5

| Crate | Version | Purpose |
|---|---|---|
| `serde` | 1 | Serialization/deserialization for CRD structs |
| `schemars` | 0.8 | JSON Schema generation for Kubernetes validation |
| `serde_yaml` | 0.9 | YAML output for CRD generation |
| `chrono` | 0.4 | Timestamps for status updates |

---

## Test Coverage

### Unit Tests

**Library crate** (`src/lib.rs` → `crd.rs` + `governance.rs`): **61 tests**

- CRD schema generation, API group, version, kind, scope (5)
- Spec/status serialization round-trips (4)
- Status defaults and None-field handling (2)
- Namespace filtering (8)
- Pod evaluation — original (10)
- Violation detection — original (4)
- Metrics arithmetic (4)
- Health scoring (5)
- Health classification (8)
- Scoring weights and defaults (2)
- Policy-aware evaluation (6)
- Policy-aware violation detection (4)

**Binary crate** (`src/main.rs` → commands): **18 tests**

- HTTP endpoints — healthz, readyz, metrics, 404 (5)
- Reconcile: multi-pod aggregation (2)
- Reconcile: system namespace skipping (1)
- Reconcile: status computation (3)
- Finalizer detection (4)
- Deletion timestamp detection (2)
- Status message format (1)

### Integration Tests

**`tests/governance_integration.rs`**: **6 tests**
- Single pod pipeline (healthy + noncompliant)
- Multi-pod aggregation
- Namespace independence
- Pod lifecycle (add/remove)
- System namespace filtering

**`tests/operator_integration.rs`**: **13 tests**
- Full reconcile simulation (compliant, mixed, noncompliant)
- Empty namespace handling
- Empty policy (all checks disabled)
- System namespace exclusion
- Policy change affects score
- Custom restart threshold
- Status message format and field verification
- CRD schema round-trip

### Total: 98 tests — all passing, no cluster required

---

## Key Concepts Mastered

| Concept | Where |
|---|---|
| Custom Resource Definitions | `src/crd.rs` — struct + derive macro |
| Reconciliation loop | `src/commands/reconcile.rs` — Controller + reconcile fn |
| Desired vs observed state | Policy spec vs actual pod compliance |
| Finalizers | Add on first reconcile, remove on deletion |
| Status sub-resource | `.status` patch after each evaluation |
| Policy-aware evaluation | `governance::evaluate_pod_with_policy()` |
| Graceful shutdown | `tokio::select!` + `signal::ctrl_c()` |
| Library crate structure | `src/lib.rs` for testable public modules |

---

## What Changed from Step 4

| Aspect | Step 4 | Step 5 |
|---|---|---|
| Policy source | Hardcoded checks | User-defined CRD |
| Monitoring mode | Watch API stream | Controller reconcile loop |
| State model | In-memory HashMap | CR `.status` sub-resource |
| Configuration | None | Per-namespace DevOpsPolicy CRs |
| Lifecycle | Long-running watcher | Reconcile-on-change + requeue |
| Cleanup | Graceful shutdown | Finalizers + graceful shutdown |
| Project structure | Binary crate only | Library + binary crate |

---

## Next Step

**Step 6 — Policy Enforcement Mode**: Move from detection to remediation. The operator will patch non-compliant workloads automatically (add resource limits, inject missing probes).
