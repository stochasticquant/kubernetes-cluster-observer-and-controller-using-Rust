# Step 6 — Policy Enforcement Mode

## Overview

Step 6 extends the DevOpsPolicy operator from **detection-only** to
**active remediation**. When a policy's `enforcementMode` is set to
`enforce`, the operator automatically patches non-compliant workloads
to fix patchable violations.

---

## Design Principles

| Principle | Detail |
|---|---|
| Audit by default | `enforcementMode: audit` (or omitted) = zero mutations |
| Patch parents, not pods | Mutations target Deployments / StatefulSets / DaemonSets |
| System namespace protection | Never enforce in `kube-system`, `cert-manager`, etc. |
| Annotation audit trail | Patched workloads receive `devops.stochastic.io/patched-by` |
| Only fix what's fixable | `:latest` tag, high restarts, pending phase = detection only |

---

## Patchable vs Non-Patchable Violations

| Violation | Patchable? | Enforcement Action |
|---|---|---|
| Missing liveness probe | Yes | Inject default TCP probe |
| Missing readiness probe | Yes | Inject default TCP probe |
| Missing resource limits | Yes | Inject default CPU/memory requests+limits |
| `:latest` image tag | No | Detection only |
| High restart count | No | Detection only |
| Pending phase | No | Detection only |

---

## New CRD Fields

### Spec

```yaml
spec:
  enforcementMode: enforce       # "audit" (default) or "enforce"
  defaultProbe:
    tcpPort: 8080                # Falls back to container port, then 8080
    initialDelaySeconds: 5
    periodSeconds: 10
  defaultResources:
    cpuRequest: "100m"
    cpuLimit: "500m"
    memoryRequest: "128Mi"
    memoryLimit: "256Mi"
```

### Status

```yaml
status:
  remediationsApplied: 2
  remediationsFailed: 0
  remediatedWorkloads:
    - deployment/production/web-app
```

---

## Architecture

### Enforcement Module (`src/enforcement.rs`)

Core types:
- `WorkloadRef` — Identifies a parent workload (kind, name, namespace)
- `RemediationAction` — Single action (inject probe, inject resources)
- `RemediationPlan` — Collection of actions for a workload
- `RemediationResult` — Success/failure of applying a plan

Core functions (offline-testable):
- `resolve_owner(pod)` — Walk owner_references to find Deployment/StatefulSet/DaemonSet
- `strip_replicaset_hash(name)` — Derive Deployment name from ReplicaSet name
- `is_enforcement_enabled(policy)` — Check enforcement mode
- `is_protected_namespace(ns)` — Check against protected namespace list
- `build_default_probe(container, config)` — Build TCP probe
- `build_default_resources(config)` — Build resource requirements
- `plan_remediation(pod, policy)` — Determine what to patch
- `build_container_patches(actions, containers, policy)` — Generate patch JSON

Async API functions:
- `apply_remediation(plan, client, policy)` — Patch workload via Kubernetes API
- `resolve_owner_via_api(pod, client)` — Look up ReplicaSet owner via API

### Reconcile Loop Integration

After the detection phase, the reconciler:
1. Checks if `enforcementMode == enforce`
2. For each pod, calls `plan_remediation()`
3. Deduplicates by workload key (skip if already patched)
4. Calls `apply_remediation()` for each unique workload
5. Tracks `remediationsApplied` / `remediationsFailed`
6. Includes remediation fields in status update

### Prometheus Metrics

| Metric | Type | Description |
|---|---|---|
| `devopspolicy_remediations_applied_total` | Counter | Successful remediations |
| `devopspolicy_remediations_failed_total` | Counter | Failed remediation attempts |
| `devopspolicy_enforcement_mode` | Gauge | 0=audit, 1=enforce per policy |

---

## Usage

### Audit Mode (default — backward compatible)

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

### Enforce Mode

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

---

## Test Suite

Step 6 adds ~44 new tests:

| Layer | File | Count | Scope |
|---|---|---|---|
| Unit (lib) | `src/enforcement.rs` | 30 | Owner resolution, probe/resource building, plan generation, patch construction |
| Unit (lib) | `src/crd.rs` | 8 | Enforcement types serialization, backward compat |
| Integration | `tests/enforcement_integration.rs` | 8 | Full pipeline, audit vs enforce, namespace protection, deduplication |

Run enforcement tests:
```bash
cargo test --lib enforcement          # Unit tests (30)
cargo test --test enforcement_integration  # Integration tests (8)
```

---

## Backward Compatibility

- Existing policies with no `enforcementMode` field continue to work in audit mode
- Existing status JSON without remediation fields deserializes correctly
- All 98 pre-existing tests continue to pass unchanged
