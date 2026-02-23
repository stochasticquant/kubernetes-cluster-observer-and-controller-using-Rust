# Step 8 — Prometheus Expansion

## Overview

Step 8 closes the observability gap by adding metrics HTTP endpoints, new
operational metrics, Kubernetes ServiceMonitor manifests, and a Grafana
dashboard to make the kube-devops system production-observable.

------------------------------------------------------------------------

## What Changed

### 1. Reconcile Metrics HTTP Server (port 9090)

The reconcile operator previously had 7 Prometheus metrics but no HTTP
endpoint to expose them. A new axum-based server on port **9090** provides:

| Endpoint | Purpose |
|---|---|
| `GET /healthz` | Liveness probe (always 200 OK) |
| `GET /readyz` | Readiness probe (503 until first reconcile, then 200) |
| `GET /metrics` | Prometheus metrics scrape endpoint |

The server starts alongside the controller using `broadcast::channel`
for graceful shutdown — the same pattern used by `watch.rs`.

### 2. New Metrics

| Module | Metric | Type | Description |
|---|---|---|---|
| reconcile.rs | `devopspolicy_pods_scanned_total` | IntCounter | Total pods scanned across all reconciliation cycles |
| reconcile.rs | `devopspolicy_reconcile_duration_seconds` | Histogram | Duration of each reconciliation cycle in seconds |
| watch.rs | `pods_tracked_total` | IntGauge | Total pods currently tracked by the watch controller |
| webhook.rs | `webhook_request_duration_seconds` | Histogram | Duration of admission webhook request processing |

### 3. Full Metrics Inventory

| Module | Port | /metrics | Metrics |
|---|---|---|---|
| watch.rs | 8080 | Yes | `cluster_health_score`, `namespace_health_score`, `pod_events_total`, `pods_tracked_total` |
| reconcile.rs | 9090 | Yes | `devopspolicy_reconcile_total`, `devopspolicy_reconcile_errors_total`, `devopspolicy_violations_total`, `devopspolicy_health_score`, `devopspolicy_remediations_applied_total`, `devopspolicy_remediations_failed_total`, `devopspolicy_enforcement_mode`, `devopspolicy_pods_scanned_total`, `devopspolicy_reconcile_duration_seconds` |
| webhook.rs | 8443 | Yes | `webhook_requests_total`, `webhook_denials_total`, `webhook_request_duration_seconds` |

### 4. Kubernetes Manifests

The `observability` CLI subcommand generates:

- **3 Service manifests** — expose metrics ports for watch (:8080), reconcile (:9090), webhook (:8443)
- **3 ServiceMonitor manifests** — Prometheus auto-discovery via `monitoring.coreos.com/v1`
  - Webhook uses `scheme: https` + `insecureSkipVerify: true`
- **1 Grafana Dashboard ConfigMap** — labeled `grafana_dashboard: "1"` for sidecar auto-discovery

All manifests use consistent labels:
- `app.kubernetes.io/name: kube-devops`
- `app.kubernetes.io/component: {watch|reconcile|webhook}`

### 5. Grafana Dashboard

The dashboard has 4 rows with 22 panels covering:

| Row | Panels |
|---|---|
| **Overview** | Cluster health score, reconcile cycles rate, webhook requests rate |
| **Watch** | Namespace health scores, pod events rate, pods tracked |
| **Reconcile** | Violations by namespace, health scores, reconcile rate/errors, duration histogram, pods scanned, remediations applied/failed, enforcement mode |
| **Webhook** | Allow/deny rate, denial breakdown by violation, request latency |

------------------------------------------------------------------------

## CLI Commands

```bash
# Print all manifests (Services + ServiceMonitors + Dashboard)
kube-devops observability generate-all

# Print only ServiceMonitor manifests
kube-devops observability generate-service-monitors

# Print only Grafana dashboard ConfigMap
kube-devops observability generate-dashboard
```

### Deployment

```bash
# Apply all observability resources
cargo run -- observability generate-all | kubectl apply -f -

# Or apply individual manifests
kubectl apply -f kube-tests/service-watch.yaml
kubectl apply -f kube-tests/servicemonitor-watch.yaml
kubectl apply -f kube-tests/grafana-dashboard-configmap.yaml
```

------------------------------------------------------------------------

## Files Changed

| File | Action |
|---|---|
| `src/commands/reconcile.rs` | Modified — HTTP server on :9090, 2 new metrics, restructured run() |
| `src/commands/watch.rs` | Modified — added `pods_tracked_total` metric |
| `src/commands/webhook.rs` | Modified — added `webhook_request_duration_seconds` metric |
| `src/commands/observability.rs` | **New** — manifest generators, Grafana dashboard |
| `src/commands/mod.rs` | Modified — added `pub mod observability;` |
| `src/cli.rs` | Modified — added `Observability` subcommand |
| `src/main.rs` | Modified — dispatch Observability commands |
| `kube-tests/service-watch.yaml` | **New** — Service manifest |
| `kube-tests/service-reconcile.yaml` | **New** — Service manifest |
| `kube-tests/service-webhook.yaml` | **New** — Service manifest |
| `kube-tests/servicemonitor-watch.yaml` | **New** — ServiceMonitor manifest |
| `kube-tests/servicemonitor-reconcile.yaml` | **New** — ServiceMonitor manifest |
| `kube-tests/servicemonitor-webhook.yaml` | **New** — ServiceMonitor manifest |
| `kube-tests/grafana-dashboard-configmap.yaml` | **New** — Grafana dashboard ConfigMap |

------------------------------------------------------------------------

## Test Coverage

| Test | Description |
|---|---|
| `test_reconcile_healthz_returns_ok` | Reconcile HTTP /healthz returns 200 |
| `test_reconcile_readyz_when_ready` | Reconcile HTTP /readyz returns 200 when ready |
| `test_reconcile_readyz_when_not_ready` | Reconcile HTTP /readyz returns 503 when not ready |
| `test_reconcile_metrics_returns_ok` | Reconcile HTTP /metrics returns 200 |
| `test_reconcile_unknown_route_returns_404` | Reconcile HTTP unknown route returns 404 |
| `test_pods_scanned_metric_registered` | pods_scanned_total appears in registry |
| `test_reconcile_duration_metric_registered` | reconcile_duration_seconds appears in registry |
| `test_pods_tracked_metric_registered` | pods_tracked_total appears in registry |
| `test_webhook_duration_metric_registered` | webhook_request_duration_seconds appears in registry |
| `test_service_watch_fields` | Watch Service YAML has correct fields and port |
| `test_service_reconcile_fields` | Reconcile Service YAML has correct fields and port |
| `test_service_webhook_fields` | Webhook Service YAML has correct fields and port |
| `test_service_monitor_watch_fields` | Watch ServiceMonitor has correct selector and endpoint |
| `test_service_monitor_reconcile_fields` | Reconcile ServiceMonitor uses HTTP scheme |
| `test_service_monitor_webhook_uses_https` | Webhook ServiceMonitor uses HTTPS + insecureSkipVerify |
| `test_all_services_parseable_yaml` | All 3 service YAMLs parse successfully |
| `test_all_service_monitors_parseable_yaml` | All 3 ServiceMonitor YAMLs parse successfully |
| `test_dashboard_configmap_valid_json` | Dashboard ConfigMap contains valid embedded JSON |
| `test_dashboard_has_panels` | Dashboard has at least 16 panels |
| `test_dashboard_configmap_has_grafana_label` | ConfigMap has `grafana_dashboard: "1"` label |
| `test_dashboard_references_all_metrics` | Dashboard JSON references all 16 metric names |

**Total test suite: 207 tests** (186 existing + 21 new)

------------------------------------------------------------------------

## Architecture Diagram

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   Watch      │     │  Reconcile   │     │   Webhook   │
│  Controller  │     │  Operator    │     │   Server    │
│              │     │              │     │             │
│ :8080/metrics│     │ :9090/metrics│     │:8443/metrics│
└──────┬───────┘     └──────┬───────┘     └──────┬──────┘
       │                    │                    │
       ▼                    ▼                    ▼
┌──────────────────────────────────────────────────────┐
│              Prometheus (ServiceMonitors)             │
│  watch-sm          reconcile-sm        webhook-sm    │
│  15s interval      15s interval        15s/HTTPS     │
└──────────────────────────┬───────────────────────────┘
                           │
                           ▼
                  ┌─────────────────┐
                  │     Grafana     │
                  │  (auto-import)  │
                  │  4 rows, 22     │
                  │  panels         │
                  └─────────────────┘
```

------------------------------------------------------------------------

**Last Updated:** 2026-02-23
