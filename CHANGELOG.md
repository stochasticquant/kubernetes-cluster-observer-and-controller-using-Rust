# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-24

### Added
- Multi-cluster governance: enumerate contexts, cross-cluster policy evaluation, aggregate reports
- Policy bundles: baseline, restricted, and permissive pre-defined policy sets
- Bundle CLI: `bundle list`, `bundle show`, `bundle apply`
- Policy GitOps: `policy export`, `policy import`, `policy diff`
- Severity-weighted scoring with configurable severity overrides per violation
- `violations_by_severity` and `audit_results_total` Prometheus metrics
- Grafana dashboard expanded to 26 panels with multi-cluster and severity views
- Helm chart v0.3.0 with 18 templates
- Comprehensive documentation (17 docs covering all steps)

### Changed
- CRD updated with `SeverityOverrides`, `PolicyAuditResult` status fields
- Admission webhook now supports severity-threshold filtering via `AdmissionVerdict`
- Reconcile operator emits richer audit results with severity breakdowns

## [0.1.2] - 2026-02-20

### Fixed
- Watch HTTP server starts before leader election so non-leader pods pass health probes
- Non-leader watch pods retry leader acquisition every 10 seconds
- Leader lease moved to `kube-devops` namespace (was `default`)
- ClusterRole: added `patch` verb for leases
- Webhook: corrected TLS cert/key path arguments
- Webhook probes: use HTTPS scheme matching the HTTPS-only server
- ValidatingWebhookConfiguration: set `port: 8443`

## [0.1.0] - 2026-02-15

### Added
- CLI foundation with clap (25 subcommands)
- Kubernetes client integration (kubeconfig and in-cluster)
- DevOpsPolicy CRD (`devops.stochastic.io/v1`) with `kubectl apply` support
- Governance scoring engine: pod evaluation, violation detection, weighted scoring
- Watch controller with leader election and real-time pod monitoring
- Operator reconcile loop with Prometheus metrics and HTTP server (:9090)
- Enforcement engine: owner resolution, remediation planning, dry-run patching
- Admission webhook with HTTPS server (:8443) and auto-generated TLS certs
- Prometheus integration with ServiceMonitor and metric endpoints
- Observability generators: Service, ServiceMonitor, Grafana dashboard
- Deployment manifest generators with RBAC, Deployments, PodDisruptionBudgets
- Dockerfile with multi-stage build (rust:slim-bookworm -> debian:bookworm-slim)
- 314 tests with zero clippy warnings
