# Step 9 — High Availability & Production Hardening

## Overview

Step 9 adds production deployment infrastructure to the kube-devops platform. While Steps 1–8 built the full governance, enforcement, admission, and observability capabilities, the system only ran locally via `cargo run`. Step 9 delivers:

- **Multi-stage Dockerfile** for minimal production container images
- **Kubernetes deployment manifests** (Namespace, RBAC, Deployments, PDBs)
- **Helm chart** for configurable production deployments
- **CLI integration** for manifest generation via `deploy` subcommand

------------------------------------------------------------------------

## Dockerfile

### Multi-Stage Build

```dockerfile
# Stage 1: Builder (rust:1.84-slim)
# Stage 2: Runtime (debian:bookworm-slim)
```

**Builder stage** installs `libssl-dev` and `pkg-config` for kube-rs TLS compilation, then runs `cargo build --release`.

**Runtime stage** copies only the release binary into a minimal Debian image with:
- Non-root user (`kube-devops`, UID 1000)
- CA certificates for HTTPS
- Exposed ports: 8080 (watch), 9090 (reconcile), 8443 (webhook)

### Building

```bash
docker build -t 192.168.1.68:5000/kube-devops:v0.1.2 .
docker push 192.168.1.68:5000/kube-devops:v0.1.2
```

### Running

```bash
# Watch controller
docker run --rm kube-devops:latest watch

# Reconcile operator
docker run --rm kube-devops:latest reconcile

# Webhook server
docker run --rm kube-devops:latest webhook serve
```

------------------------------------------------------------------------

## Deployment Manifests

All manifests are generated programmatically by `src/commands/deploy.rs` using the same pattern as `observability.rs`.

### Generated Resources

| Resource | Name | Purpose |
|---|---|---|
| Namespace | `kube-devops` | Dedicated namespace |
| ServiceAccount | `kube-devops` | Pod identity |
| ClusterRole | `kube-devops` | RBAC permissions |
| ClusterRoleBinding | `kube-devops` | Binds role to SA |
| Deployment | `kube-devops-watch` | Watch controller (2 replicas) |
| Deployment | `kube-devops-reconcile` | Reconcile operator (2 replicas) |
| Deployment | `kube-devops-webhook` | Admission webhook (2 replicas) |
| PodDisruptionBudget | `kube-devops-watch` | HA guarantee (minAvailable: 1) |
| PodDisruptionBudget | `kube-devops-reconcile` | HA guarantee (minAvailable: 1) |
| PodDisruptionBudget | `kube-devops-webhook` | HA guarantee (minAvailable: 1) |

### RBAC Permissions

The ClusterRole grants:

| API Group | Resources | Verbs |
|---|---|---|
| `devops.stochastic.io` | `devopspolicies` | get, list, watch |
| `devops.stochastic.io` | `devopspolicies/status` | patch |
| `""` (core) | `pods` | get, list, watch |
| `apps` | `deployments`, `statefulsets`, `daemonsets` | get, list, patch |
| `coordination.k8s.io` | `leases` | get, create, update, patch |
| `admissionregistration.k8s.io` | `validatingwebhookconfigurations` | get, list, create, update |

### Deployment Security

All deployments include:
- `runAsNonRoot: true`
- `readOnlyRootFilesystem: true`
- Resource requests: 64Mi memory, 100m CPU
- Resource limits: 128Mi memory, 250m CPU
- Liveness probe (`/healthz`) — HTTP for watch/reconcile, HTTPS for webhook
- Readiness probe (`/readyz`) — HTTP for watch/reconcile, HTTPS for webhook

### Webhook Deployment

The webhook deployment includes additional configuration:
- TLS volume mount from Secret `kube-devops-webhook-tls` at `/tls`
- Args: `webhook serve --tls-cert /tls/tls.crt --tls-key /tls/tls.key`
- Health probes use `scheme: HTTPS` since the webhook only serves HTTPS

### Watch Controller HA

The watch controller handles leader election gracefully:
- HTTP server starts **before** leader election (ensures health probes pass for all replicas)
- The leader acquires the lease and runs the watch loop
- Non-leader replicas retry leader acquisition every 10 seconds
- If the leader pod is terminated, a non-leader is automatically promoted
- Leader election lease is stored in the `kube-devops` namespace

### CLI Commands

```bash
# Generate all manifests (Namespace + RBAC + Deployments + PDBs)
cargo run -- deploy generate-all

# Generate RBAC only
cargo run -- deploy generate-rbac

# Generate Deployments only
cargo run -- deploy generate-deployments

# Apply to cluster
cargo run -- deploy generate-all | kubectl apply -f -
```

------------------------------------------------------------------------

## Helm Chart

### Installation

```bash
# Default installation
helm install kube-devops ./helm/kube-devops -n kube-devops --create-namespace

# Custom values
helm install kube-devops ./helm/kube-devops \
  -n kube-devops --create-namespace \
  --set replicaCount=3 \
  --set image.tag=v0.1.2

# Template preview
helm template kube-devops ./helm/kube-devops
```

### Configurable Values

| Value | Default | Description |
|---|---|---|
| `image.repository` | `192.168.1.68:5000/kube-devops` | Container image repository |
| `image.tag` | `v0.1.2` | Image tag |
| `image.pullPolicy` | `IfNotPresent` | Pull policy |
| `replicaCount` | `2` | Replicas per component |
| `resources.requests.memory` | `64Mi` | Memory request |
| `resources.requests.cpu` | `100m` | CPU request |
| `resources.limits.memory` | `128Mi` | Memory limit |
| `resources.limits.cpu` | `250m` | CPU limit |
| `serviceMonitor.enabled` | `true` | Create ServiceMonitors |
| `serviceMonitor.interval` | `15s` | Scrape interval |
| `grafanaDashboard.enabled` | `true` | Create Grafana dashboard ConfigMap |
| `pdb.enabled` | `true` | Create PodDisruptionBudgets |
| `pdb.minAvailable` | `1` | Minimum available pods |

### Chart Structure

```
helm/kube-devops/
├── Chart.yaml
├── values.yaml
├── templates/
│   ├── _helpers.tpl
│   ├── namespace.yaml
│   ├── serviceaccount.yaml
│   ├── clusterrole.yaml
│   ├── clusterrolebinding.yaml
│   ├── deployment-watch.yaml
│   ├── deployment-reconcile.yaml
│   ├── deployment-webhook.yaml
│   ├── pdb-watch.yaml
│   ├── pdb-reconcile.yaml
│   ├── pdb-webhook.yaml
│   ├── service-watch.yaml
│   ├── service-reconcile.yaml
│   ├── service-webhook.yaml
│   ├── servicemonitor-watch.yaml
│   ├── servicemonitor-reconcile.yaml
│   ├── servicemonitor-webhook.yaml
│   └── grafana-dashboard-configmap.yaml
```

------------------------------------------------------------------------

## Static Reference Manifests

Static copies of all deployment manifests are in `kube-tests/`:

```bash
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

## Tests

Step 9 adds 21 tests for the deployment manifest generators:

| Category | Count | Tests |
|---|---|---|
| RBAC | 3 | SA fields, ClusterRole rules count, CRB references |
| Deployments | 3 | Field tests (replicas, ports, image, probes, security context) |
| PDBs | 3 | Field tests (minAvailable, selector labels) |
| Namespace | 1 | Namespace fields |
| YAML parsability | 3 | All deployments, all PDBs, all RBAC |
| Security context | 2 | runAsNonRoot, resource limits present |
| Aggregators | 3 | generate_all kinds, generate_rbac docs, generate_deployments docs |
| Label consistency | 3 | Consistent `app.kubernetes.io/name: kube-devops` across all manifests |

**Total test suite: 228 tests**

------------------------------------------------------------------------

**Last Updated:** 2026-02-24
