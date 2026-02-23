# Step 4 --- High-Availability Kubernetes Governance Watch Controller

## Deep Technical Documentation for New Developers

------------------------------------------------------------------------

# 1. Purpose of Step 4

By the end of Step 4, the project evolved from a simple CLI scanner into
a:

High-Availability, Event-Driven, Real-Time Kubernetes Governance
Controller.

This component now:

-   Continuously watches the Kubernetes API
-   Detects policy violations in real time
-   Maintains namespace-level health state
-   Exposes metrics for Prometheus
-   Supports horizontal scaling safely via leader election
-   Implements structured production-grade logging
-   Shuts down gracefully without data corruption

This document explains how everything works internally.

------------------------------------------------------------------------

# 2. Architectural Overview

Step 4 introduced the following core subsystems:

1.  Leader Election Layer
2.  Watch Loop (Event-Driven Controller)
3.  In-Memory State Model
4.  Governance Scoring Engine
5.  Structured Logging System
6.  Prometheus Metrics Layer
7.  Health & Readiness Endpoints
8.  Coordinated Graceful Shutdown

Each subsystem is described in detail below.

------------------------------------------------------------------------

# 3. Leader Election (High Availability)

## Why It Exists

If two controller replicas run simultaneously without coordination:

-   Both process events
-   Both emit metrics
-   Both log violations
-   State becomes duplicated

Leader election ensures:

Only one active instance performs governance evaluation.

## Implementation Details

-   Kubernetes resource: Lease
-   API group: coordination.k8s.io/v1
-   Namespace: default
-   Lease name: kube-devops-leader

On startup:

1.  Controller attempts to create the Lease object.
2.  If creation succeeds → instance becomes leader.
3.  If Lease already exists → instance waits in passive mode.

This design prevents concurrent controllers.

------------------------------------------------------------------------

# 4. Event-Driven Watch Loop

## Why Watch Instead of Polling?

Polling: - Expensive - Delayed - Not scalable

Watch API: - Event-driven - Immediate reaction - Kubernetes-native
design

## Watch Implementation

We use:

kube_runtime::watcher

The controller subscribes to Pod events across all namespaces.

Handled event types:

-   Applied → Pod created or modified
-   Deleted → Pod removed
-   Restarted → Initial full synchronization

Restarted is critical: It provides the baseline cluster state when the
controller starts.

------------------------------------------------------------------------

# 5. In-Memory State Model

The controller maintains an internal representation of governance state.

## Structures

LiveMetrics: - total_pods - latest_tag count - missing_liveness count -
missing_readiness count

NamespaceState: - Holds LiveMetrics for a namespace

ClusterState: - Map\<namespace, NamespaceState\> - ready flag

## Why In-Memory?

-   Fast aggregation
-   Incremental updates
-   No database required
-   Deterministic behavior

State is updated incrementally on each Pod event.

------------------------------------------------------------------------

# 6. Governance Violation Detection

Each Pod is evaluated for:

1.  Image ending with :latest
2.  Missing liveness probe
3.  Missing readiness probe

Violation detection occurs during Applied events.

Only violations are logged.

This prevents log noise and supports production scalability.

Example structured log:

{ "event": "policy_violation", "namespace": "default", "pod": "nginx",
"violations": \["latest_tag", "missing_liveness"\] }

------------------------------------------------------------------------

# 7. Namespace Health Scoring

Each namespace accumulates violation counts.

Score formula:

raw_score = (latest_tag \* 5) + (missing_liveness \* 3) +
(missing_readiness \* 2)

normalized_score = raw_score / total_pods

Why normalize? So large namespaces are not unfairly penalized.

Lower score = healthier namespace.

------------------------------------------------------------------------

# 8. Cluster Health Score

Cluster score = average(namespace_scores)

This provides a single governance signal across the cluster.

Exported as:

cluster_health_score

------------------------------------------------------------------------

# 9. Prometheus Metrics Integration

Metrics are registered in a global Registry.

Exposed endpoint:

/metrics

Metrics available:

-   cluster_health_score
-   namespace_health_score{namespace="X"}
-   pod_events_total

These can be scraped by Prometheus and visualized in Grafana.

------------------------------------------------------------------------

# 10. Health & Readiness Endpoints

## /healthz

Returns HTTP 200 if process is alive.

Used for livenessProbe.

## /readyz

Returns:

-   503 until initial watch sync completes
-   200 after state is initialized

Used for readinessProbe.

------------------------------------------------------------------------

# 11. Graceful Shutdown Mechanism

Shutdown flow:

1.  SIGINT captured (Ctrl+C)
2.  Broadcast shutdown signal
3.  Watch loop exits
4.  HTTP server exits
5.  Process terminates cleanly

No tasks are abruptly aborted.

This prevents: - Partial state updates - Corrupted metrics - Race
conditions

------------------------------------------------------------------------

# 12. High Availability Deployment Model

Recommended Deployment Spec:

replicas: 2

Behavior:

Replica A → Acquires lease → Active leader\
Replica B → Passive standby

If Replica A crashes:

Replica B can acquire lease and take over.

------------------------------------------------------------------------

# 13. Test Suite

Step 4 includes a comprehensive test suite that validates the governance
engine, HTTP endpoints, and scoring pipeline without requiring a live
Kubernetes cluster. All Pod objects are constructed in-memory.

## Test Summary

| Location | Tests | Scope |
|---|---|---|
| `src/governance.rs` | 38 | Namespace filter, pod evaluation, violation detection, metrics arithmetic, scoring, health classification, defaults |
| `src/commands/watch.rs` | 5 | HTTP endpoints: healthz, readyz (ready/not-ready), metrics, 404 |
| `tests/governance_integration.rs` | 6 | End-to-end pipeline: pod -> evaluate -> accumulate -> score -> classify |

Total: **49 tests**, all offline.

## Running Tests

```bash
cargo test                                    # Full suite
cargo test --lib governance::tests            # Governance unit tests only
cargo test --lib commands::watch::tests       # HTTP endpoint tests only
cargo test --test governance_integration      # Integration tests only
```

## Key Design Decisions

-   **tower::oneshot** sends requests directly to the axum Router without
    binding a TCP port, eliminating flaky network-dependent tests.
-   **Boundary testing** covers every scoring threshold (80, 60, 40) and
    the restart count threshold (> 3) at their exact transition points.
-   **Nil-safety tests** exercise missing `spec`, `status`, and container
    statuses to verify graceful handling of incomplete Pod objects.
-   A shared `make_test_pod()` helper builds configurable Pod structs for
    both unit and integration tests.

For full details, see `docs/Step_4_Testing.md`.

------------------------------------------------------------------------

# 14. What Step 4 Achieved Technically

You now have:

-   A real Kubernetes controller
-   Event-driven architecture
-   Governance scoring engine
-   Structured JSON observability
-   HA-safe execution
-   Prometheus-ready telemetry
-   Clean shutdown semantics
-   Comprehensive test coverage (49 tests)

This is the foundation of any production Kubernetes operator.

------------------------------------------------------------------------

# 15. Limitations of Current Implementation

-   No persistent storage
-   No CRD yet
-   No enforcement, only observation

These will be addressed in Step 5 and beyond.

------------------------------------------------------------------------

# 16. Next Evolution --- Step 5

Step 5 introduces:

-   Custom Resource Definition (CRD)
-   Reconciliation pattern
-   Desired state enforcement
-   Policy-as-code model

At that point, this system transitions from:

Observer

to

True Kubernetes Operator.

------------------------------------------------------------------------

# End of Step 4 Documentation
