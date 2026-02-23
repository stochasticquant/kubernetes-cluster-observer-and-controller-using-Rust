# Step 7 — Validating Admission Webhook

## Overview

Step 7 adds **preventive control** to the kube-devops operator. While Steps 1-6
focused on detecting and remediating policy violations on *existing* workloads,
the admission webhook **rejects non-compliant Pods before they are created**.

The Kubernetes API server sends an `AdmissionReview` to our HTTPS endpoint
whenever a Pod is created or updated. We evaluate the Pod against the namespace's
`DevOpsPolicy` and return allow/deny.

------------------------------------------------------------------------

## Architecture

```
Pod CREATE/UPDATE request
        │
        ▼
 API Server ──► ValidatingWebhookConfiguration
                        │
                        ▼
                kube-devops webhook (HTTPS :8443)
                  POST /validate
                        │
                ┌───────┴───────┐
                │ Parse         │
                │ AdmissionReview│
                └───────┬───────┘
                        │
                ┌───────┴───────┐
                │ Lookup        │
                │ DevOpsPolicy  │
                │ (namespace)   │
                └───────┬───────┘
                        │
                ┌───────┴───────┐
                │ Validate Pod  │
                │ against policy│
                └───────┬───────┘
                        │
                ┌───────┴───────┐
                │ AdmissionReview│
                │ Response      │
                │ allow / deny  │
                └───────────────┘
```

------------------------------------------------------------------------

## Design Principles

| Principle | Description |
|---|---|
| **Fail-open** | If the webhook errors or can't reach the policy, allow the request |
| **Policy-driven** | Only enforce checks the DevOpsPolicy enables; no policy = allow all |
| **System namespace bypass** | Reuses `governance::is_system_namespace()` — never blocks system pods |
| **Reuse existing logic** | The admission module builds on the same policy types from `crd.rs` |
| **TLS required** | Kubernetes API server only calls webhooks over HTTPS |

------------------------------------------------------------------------

## Admission Checks

| Policy Field | Check | Deny Message |
|---|---|---|
| `forbidLatestTag: true` | Container image ends with `:latest` | "container 'X' uses :latest tag" |
| `requireLivenessProbe: true` | Container missing liveness probe | "container 'X' missing liveness probe" |
| `requireReadinessProbe: true` | Container missing readiness probe | "container 'X' missing readiness probe" |
| `maxRestartCount` | N/A at admission (pod hasn't run yet) | Skipped |
| `forbidPendingDuration` | N/A at admission (pod hasn't run yet) | Skipped |

------------------------------------------------------------------------

## New Files

| File | Description |
|---|---|
| `src/admission.rs` | Pure admission validation logic (16 unit tests) |
| `src/commands/webhook.rs` | HTTPS server, cert generation, webhook config output (8 unit tests) |
| `tests/admission_integration.rs` | Integration tests for admission pipeline (12 tests) |
| `kube-tests/webhook-config.yaml` | ValidatingWebhookConfiguration template |

------------------------------------------------------------------------

## CLI Commands

### Start Webhook Server

```bash
kube-devops webhook serve --tls-cert tls.crt --tls-key tls.key
```

Options:
- `--addr` — Listen address (default: `0.0.0.0:8443`)
- `--tls-cert` — Path to TLS certificate PEM file (default: `tls.crt`)
- `--tls-key` — Path to TLS private key PEM file (default: `tls.key`)

### Generate TLS Certificates

```bash
kube-devops webhook cert-generate
# With IP SANs (required when running outside the cluster):
kube-devops webhook cert-generate --ip-san 192.168.1.26
```

Options:
- `--service-name` — Kubernetes Service name (default: `kube-devops-webhook`)
- `--namespace` — Kubernetes namespace (default: `default`)
- `--output-dir` — Directory for output files (default: `.`)
- `--ip-san` — Additional IP SANs for the certificate (repeatable)

Generates:
- `ca.crt` — CA certificate
- `tls.crt` — Server certificate (signed by CA, includes DNS + IP SANs)
- `tls.key` — Server private key

### Print Webhook Configuration

```bash
kube-devops webhook install-config --ca-bundle-path ca.crt
```

Options:
- `--service-name` — Service name (default: `kube-devops-webhook`)
- `--namespace` — Namespace (default: `default`)
- `--ca-bundle-path` — Path to CA certificate (required)

Prints a ready-to-apply `ValidatingWebhookConfiguration` YAML with the CA
bundle base64-encoded.

------------------------------------------------------------------------

## HTTPS Endpoints

| Endpoint | Method | Description |
|---|---|---|
| `/validate` | POST | Admission review handler |
| `/healthz` | GET | Liveness probe (always 200 OK) |
| `/readyz` | GET | Readiness probe (200 when ready) |
| `/metrics` | GET | Prometheus metrics |

------------------------------------------------------------------------

## Prometheus Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `webhook_requests_total` | Counter | `operation`, `allowed` | Total admission requests |
| `webhook_denials_total` | Counter | `namespace`, `violation` | Denied requests by type |

------------------------------------------------------------------------

## Quick Start

### In-cluster deployment

```bash
# 1. Generate self-signed certificates
cargo run -- webhook cert-generate

# 2. Start the webhook server
cargo run -- webhook serve --tls-cert tls.crt --tls-key tls.key

# 3. In another terminal: print and apply the webhook config
cargo run -- webhook install-config --ca-bundle-path ca.crt | kubectl apply -f -

# 4. Test: create a pod with :latest tag (should be denied)
kubectl run test-latest --image=nginx:latest -n production
```

### Running outside the cluster (dev machine)

When the webhook server runs on your dev machine instead of inside the cluster,
the API server connects by IP address. The TLS certificate must include your
machine's IP as a SAN, and the webhook config must use a `url` instead of a
`service` reference.

```bash
# 1. Generate certs with your machine's IP
cargo run -- webhook cert-generate --ip-san 192.168.1.26

# 2. Start the webhook server
cargo run -- webhook serve --tls-cert tls.crt --tls-key tls.key

# 3. Apply webhook config with url-based clientConfig
CA_B64=$(base64 -w 0 ca.crt)
kubectl apply -f - <<EOF
apiVersion: admissionregistration.k8s.io/v1
kind: ValidatingWebhookConfiguration
metadata:
  name: kube-devops-webhook
webhooks:
  - name: validate.devops.stochastic.io
    rules:
      - apiGroups: [""]
        resources: ["pods"]
        apiVersions: ["v1"]
        operations: ["CREATE"]
    clientConfig:
      url: "https://192.168.1.26:8443/validate"
      caBundle: ${CA_B64}
    failurePolicy: Fail
    sideEffects: None
    admissionReviewVersions: ["v1"]
    namespaceSelector:
      matchExpressions:
        - key: kubernetes.io/metadata.name
          operator: NotIn
          values: ["kube-system", "kube-public", "kube-node-lease"]
EOF

# 4. Test: should be denied
kubectl run test-latest --image=nginx:latest -n production
```

------------------------------------------------------------------------

## Test Suite

| Layer | Location | Count | Scope |
|---|---|---|---|
| Unit (lib) | `src/admission.rs` | 16 | Verdict logic, policy filtering, message formatting |
| Unit (bin) | `src/commands/webhook.rs` | 8 | Admission response, cert generation, TLS validation |
| Integration | `tests/admission_integration.rs` | 12 | Full admission pipeline, fail-open, multi-container |

All tests run without a Kubernetes cluster.

------------------------------------------------------------------------

## Dependencies Added

| Crate | Purpose |
|---|---|
| `axum-server` | HTTPS serving with rustls TLS |
| `rcgen` | Self-signed TLS certificate generation |
| `rustls-pemfile` | PEM file parsing for TLS certificates |
| `base64` | Encoding CA bundle for webhook configuration |

------------------------------------------------------------------------

**Last Updated:** 2026-02-23
