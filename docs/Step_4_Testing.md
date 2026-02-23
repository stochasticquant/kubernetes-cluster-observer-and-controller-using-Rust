# Step 4 --- Test Suite Documentation

## Overview

This document describes the unit and integration test suite added to the
kube-devops project after Step 4 reached feature-complete status. The
tests cover the governance scoring engine, pod evaluation pipeline,
violation detection, namespace filtering, metrics arithmetic, health
classification, and HTTP endpoints.

No Kubernetes cluster is required to run these tests. All Pod objects are
constructed synthetically using k8s-openapi structs.

------------------------------------------------------------------------

## Running the Tests

Run the full suite:

```bash
cargo test
```

Run only governance unit tests:

```bash
cargo test --lib governance::tests
```

Run only HTTP endpoint tests:

```bash
cargo test --lib commands::watch::tests
```

Run only integration tests:

```bash
cargo test --test governance_integration
```

Show stdout during tests:

```bash
cargo test -- --nocapture
```

------------------------------------------------------------------------

## Dev-Dependencies

Added to `Cargo.toml`:

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

- **tower** --- Provides `ServiceExt::oneshot` for sending synthetic HTTP
  requests to axum routers without binding a TCP port.
- **http-body-util** --- Provides `BodyExt::collect` for reading response
  bodies in tests.

------------------------------------------------------------------------

## Visibility Changes for Testability

| File | Change | Reason |
|---|---|---|
| `src/main.rs` | `mod governance` -> `pub mod governance` | Integration tests in `tests/` need to import the governance module |
| `src/commands/watch.rs` | `ClusterState` and `NamespaceState` -> `pub(crate)` with `pub(crate)` fields | HTTP endpoint tests construct state directly |
| `src/commands/watch.rs` | Extracted `build_router(state) -> Router` as `pub(crate)` | Tests call the router without binding a TCP listener |

------------------------------------------------------------------------

## Test File Structure

```
src/
  governance.rs             # #[cfg(test)] mod tests  (38 unit tests)
  commands/
    watch.rs                # #[cfg(test)] mod tests  (5 HTTP endpoint tests)
tests/
  common/
    mod.rs                  # Shared make_test_pod() helper
  governance_integration.rs # End-to-end pipeline tests (6 tests)
```

------------------------------------------------------------------------

## Test Helper: `make_test_pod()`

Both the unit tests and integration tests use a `make_test_pod()` builder
function that constructs a complete `k8s_openapi::api::core::v1::Pod`
with configurable parameters:

```rust
fn make_test_pod(
    name: &str,           // Pod name
    namespace: &str,      // Namespace
    image: &str,          // Container image (e.g. "nginx:latest")
    has_liveness: bool,   // Whether liveness probe is set
    has_readiness: bool,  // Whether readiness probe is set
    restart_count: i32,   // Container restart count
    phase: &str,          // Pod phase ("Running", "Pending", etc.)
) -> Pod
```

The helper populates `metadata`, `spec` (one container), and `status`
(one container status) so that every governance function can be exercised.

------------------------------------------------------------------------

## Unit Tests --- `src/governance.rs` (38 tests)

### Group 1: `is_system_namespace` (8 tests)

Tests the namespace filter that determines which namespaces are
Kubernetes system namespaces and should be excluded from governance
scoring.

| Test | Input | Expected |
|---|---|---|
| `test_is_system_kube_system` | `"kube-system"` | `true` |
| `test_is_system_kube_flannel` | `"kube-flannel"` | `true` |
| `test_is_system_longhorn_system` | `"longhorn-system"` | `true` |
| `test_is_system_cert_manager` | `"cert-manager"` | `true` |
| `test_is_system_monitoring` | `"monitoring"` | `true` |
| `test_is_system_argocd` | `"argocd"` | `true` |
| `test_not_system_default` | `"default"` | `false` |
| `test_not_system_production` | `"production"` | `false` |

**Coverage:** The `starts_with("kube-")` branch, the `ends_with("-system")`
branch, explicit `matches!` entries, and negative cases for user
namespaces.

### Group 2: `evaluate_pod` (10 tests)

Tests the function that takes a Pod and returns a `PodMetrics` struct
representing the pod's governance violations.

| Test | Scenario | Key Assertions |
|---|---|---|
| `test_evaluate_latest_tag` | Image ends with `:latest` | `latest_tag == 1` |
| `test_evaluate_proper_tag` | Image has version tag | `latest_tag == 0` |
| `test_evaluate_missing_probes` | No liveness/readiness | Both counters == 1 |
| `test_evaluate_with_probes` | Both probes present | Both counters == 0 |
| `test_evaluate_high_restarts` | restart_count = 10 | `high_restarts > 0` |
| `test_evaluate_restarts_at_threshold` | restart_count = 3 | `high_restarts == 0` (threshold is > 3) |
| `test_evaluate_pending_phase` | phase = "Pending" | `pending == 1` |
| `test_evaluate_multi_container` | 2 containers, both `:latest`, no probes | All counters == 2 |
| `test_evaluate_no_spec` | `spec: None` | `total_pods == 1`, zero violations |
| `test_evaluate_no_status` | `status: None` | `:latest` detected, no restart/pending metrics |

**Coverage:** Every branch in `evaluate_pod` --- image tag check,
liveness/readiness probe check, restart threshold logic (boundary at 3),
pending phase detection, multi-container accumulation, and nil-safety for
missing spec/status.

### Group 3: `detect_violations` (4 tests)

Tests the function that returns a list of violation label strings for a
given Pod.

| Test | Scenario | Expected |
|---|---|---|
| `test_detect_violations_compliant` | Proper tag + both probes | Empty vec |
| `test_detect_violations_fully_noncompliant` | `:latest` + no probes | Contains all 3 labels |
| `test_detect_violations_only_latest` | `:latest` + both probes | `["latest_tag"]` only |
| `test_detect_violations_no_spec` | `spec: None` | Empty vec |

### Group 4: `add_metrics` / `subtract_metrics` (4 tests)

Tests the metric accumulation and decrement functions used to maintain
running namespace totals as pods are added and removed.

| Test | Scenario | Key Assertions |
|---|---|---|
| `test_add_metrics_basic` | Add one pod's metrics | All fields incremented |
| `test_subtract_metrics_basic` | Subtract partial metrics | Fields decremented correctly |
| `test_subtract_metrics_saturating_underflow` | Subtract more than exists | Floors at 0, no panic |
| `test_add_then_subtract_roundtrip` | Add then subtract same metrics | All fields return to 0 |

**Coverage:** Normal arithmetic, saturating subtraction preventing
underflow, and roundtrip identity.

### Group 5: `calculate_health_score` (5 tests)

Tests the weighted scoring formula that converts `PodMetrics` into a
0-100 health score.

| Test | Scenario | Expected Score |
|---|---|---|
| `test_score_zero_pods` | No pods (default metrics) | 100 |
| `test_score_fully_healthy` | 5 pods, zero violations | 100 |
| `test_score_fully_degraded` | 1 pod, all violation types | 56 (computed from weights) |
| `test_score_floor_zero` | Extreme violations | 0 |
| `test_score_capped_at_100` | 100 pods, zero violations | 100 |

**Score formula:** `100 - min(raw_penalty / total_pods, 100)` where
`raw_penalty = (latest_tag * 5) + (missing_liveness * 3) +
(missing_readiness * 2) + (high_restarts * 6) + (pending * 4)`.

### Group 6: `classify_health` (8 tests)

Tests the health classification function that maps a numeric score to a
human-readable label. Tests every boundary value.

| Test | Score | Expected Label |
|---|---|---|
| `test_classify_100` | 100 | "Healthy" |
| `test_classify_80` | 80 | "Healthy" |
| `test_classify_79` | 79 | "Stable" |
| `test_classify_60` | 60 | "Stable" |
| `test_classify_59` | 59 | "Degraded" |
| `test_classify_40` | 40 | "Degraded" |
| `test_classify_39` | 39 | "Critical" |
| `test_classify_0` | 0 | "Critical" |

**Boundaries:** 80 (Healthy/Stable), 60 (Stable/Degraded), 40
(Degraded/Critical).

### Group 7: Defaults (2 tests)

| Test | What It Verifies |
|---|---|
| `test_scoring_weights_default` | `ScoringWeights::default()` returns 5, 3, 2, 6, 4 |
| `test_pod_metrics_default` | `PodMetrics::default()` returns all zeros |

------------------------------------------------------------------------

## HTTP Endpoint Tests --- `src/commands/watch.rs` (5 tests)

These tests use `tower::ServiceExt::oneshot` to send HTTP requests
directly into the axum `Router` returned by `build_router()`. No TCP
port is bound. Each test constructs a `ClusterState` with the desired
`ready` flag.

| Test | Endpoint | State | Expected Status | Expected Body |
|---|---|---|---|---|
| `test_healthz_returns_ok` | `GET /healthz` | any | 200 OK | `"OK"` |
| `test_readyz_when_ready` | `GET /readyz` | ready=true | 200 OK | `"READY"` |
| `test_readyz_when_not_ready` | `GET /readyz` | ready=false | 503 Service Unavailable | `"NOT READY"` |
| `test_metrics_returns_ok` | `GET /metrics` | any | 200 OK | non-empty |
| `test_unknown_route_returns_404` | `GET /nonexistent` | any | 404 Not Found | --- |

------------------------------------------------------------------------

## Integration Tests --- `tests/governance_integration.rs` (6 tests)

These tests import `kube_devops::governance` and exercise the full
pipeline: construct pods, evaluate them, accumulate metrics, compute
scores, and classify health. They validate the complete flow from Pod
object to health classification without requiring a live cluster.

| Test | Description |
|---|---|
| `test_full_pipeline_healthy_cluster` | 5 compliant pods produce score 100 and "Healthy" classification |
| `test_full_pipeline_degraded_cluster` | Mix of violations produces a computed score matching expected weight math |
| `test_full_pipeline_pod_lifecycle` | Add 3 pods, subtract 1, verify metrics and score update correctly |
| `test_full_pipeline_single_critical_pod` | 1 pod with maximum violations produces score 0 and "Critical" |
| `test_system_namespace_filtering` | Pods in kube-system are filtered out; pods in default are scored |
| `test_namespace_score_independence` | Two namespaces with different violation levels produce independent scores |

------------------------------------------------------------------------

## Test Principles

1. **No cluster required** --- All tests construct Pod objects in-memory
   using k8s-openapi structs. The test suite runs anywhere `cargo test`
   works.

2. **Boundary coverage** --- Scoring thresholds, restart count threshold
   (> 3), and health classification boundaries are all tested at their
   exact transition points.

3. **Nil-safety** --- Tests cover `spec: None`, `status: None`, and
   missing container statuses to ensure the governance engine handles
   incomplete Pod objects gracefully.

4. **Incremental state** --- The `add_metrics` / `subtract_metrics`
   roundtrip test and the pod lifecycle integration test verify that the
   incremental state model used by the watch loop produces correct
   results after sequences of additions and removals.

5. **HTTP without networking** --- The endpoint tests use
   `tower::oneshot` to exercise the axum router as a function, avoiding
   port binding, flaky network tests, and port conflicts in CI.

------------------------------------------------------------------------

## Adding New Tests

When adding a new governance rule or violation type:

1. Add unit tests for the new detection logic in `governance::tests`.
2. Update the `make_test_pod()` helper if new Pod fields are needed.
3. Add an integration test in `governance_integration.rs` that exercises
   the full evaluate -> accumulate -> score -> classify pipeline.
4. If adding a new HTTP endpoint, add a `tower::oneshot` test in the
   `watch::tests` module.

------------------------------------------------------------------------

End of Test Documentation
