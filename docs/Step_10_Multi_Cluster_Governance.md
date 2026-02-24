# Step 10 — Multi-Cluster Governance, Severity Levels & Policy Bundles

## Overview

Step 10 completes the kube-devops roadmap by adding multi-cluster governance,
policy severity levels, pre-defined policy bundles, CRD-stored audit results,
and GitOps-compatible policy management. These features transform the platform
from a single-cluster operator into a multi-cluster governance tool suitable
for platform engineering teams.

------------------------------------------------------------------------

## New Modules

| Module | File | Purpose |
|---|---|---|
| Bundles | `src/bundles.rs` | Pre-defined policy bundle templates |
| Multi-Cluster | `src/multi_cluster.rs` | Multi-context evaluation and reporting |
| Policy CLI | `src/commands/policy.rs` | Bundle list/show/apply, export/import/diff |
| Multi-Cluster CLI | `src/commands/multi_cluster.rs` | list-contexts, analyze handlers |

### Library Exports (updated `src/lib.rs`)

```rust
pub mod admission;
pub mod bundles;          // NEW
pub mod crd;
pub mod enforcement;
pub mod governance;
pub mod multi_cluster;    // NEW
```

------------------------------------------------------------------------

## Severity Levels

### Severity Enum

Violations now carry a severity level that controls how they are reported and
whether they block admission:

| Severity | Description | Admission Behavior |
|---|---|---|
| `critical` | Immediate action required | Blocks pod creation |
| `high` | Important violation | Blocks pod creation |
| `medium` | Standard violation (default) | Reported in audit |
| `low` | Informational | Reported in audit |

### CRD Types (`src/crd.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Critical,
    High,
    #[default]
    Medium,
    Low,
}
```

### Severity Overrides

Each violation type can have its severity customized per policy:

```rust
pub struct SeverityOverrides {
    pub latest_tag: Option<Severity>,
    pub missing_liveness: Option<Severity>,
    pub missing_readiness: Option<Severity>,
    pub high_restarts: Option<Severity>,
    pub pending: Option<Severity>,
}
```

**YAML example:**

```yaml
spec:
  severityOverrides:
    latestTag: critical
    missingLiveness: high
    missingReadiness: high
    highRestarts: medium
    pending: low
```

### Audit Violations

Each violation found during evaluation is recorded with full context:

```rust
pub struct AuditViolation {
    pub pod_name: String,
    pub container_name: String,
    pub violation_type: String,
    pub severity: Severity,
    pub message: String,
}
```

------------------------------------------------------------------------

## CRD-Stored Audit Results

### PolicyAuditResult CRD

Evaluation results are stored as Kubernetes custom resources for historical
tracking and compliance auditing:

```yaml
apiVersion: devops.stochastic.io/v1
kind: PolicyAuditResult
metadata:
  name: default-policy-audit-1708790400
  namespace: production
spec:
  policyName: default-policy
  evaluatedAt: "2026-02-24T12:00:00Z"
  healthScore: 85
  totalPods: 12
  violationCount: 3
  violations:
    - podName: nginx-abc123
      containerName: nginx
      violationType: latest_tag
      severity: critical
      message: "Container uses :latest image tag"
```

Audit results are automatically created by the reconcile operator. The
`auditResults.retention` Helm value (default: 10) controls how many results
are retained per policy.

### Prometheus Metrics

Two new metrics track audit result activity:

| Metric | Type | Description |
|---|---|---|
| `violations_by_severity` | Gauge | Violations grouped by severity level |
| `audit_results_total` | Counter | Total PolicyAuditResult CRs created |

------------------------------------------------------------------------

## Policy Bundles

### Architecture (`src/bundles.rs`)

Bundles are pre-configured `DevOpsPolicySpec` templates that provide
quick-start governance configurations:

```rust
pub struct PolicyBundle {
    pub name: &'static str,
    pub description: &'static str,
    pub spec: DevOpsPolicySpec,
}
```

### Available Bundles

#### baseline

Balanced audit policy for general use. Non-mutating.

| Check | Setting |
|---|---|
| Forbid `:latest` tag | `true` |
| Require readiness probe | `true` |
| Enforcement mode | `audit` |

#### restricted

Strict enforcement with auto-patching. For security-critical namespaces.

| Check | Setting | Severity |
|---|---|---|
| Forbid `:latest` tag | `true` | Critical |
| Require liveness probe | `true` | High |
| Require readiness probe | `true` | High |
| Max restart count | 3 | Critical |
| Forbid pending duration | 300s | High |
| Enforcement mode | `enforce` | — |
| Default probe | TCP :8080, 5s delay, 10s period | — |
| Default resources | 100m/500m CPU, 128Mi/256Mi memory | — |

#### permissive

Lenient monitoring for development or staging. Audit mode.

| Check | Setting | Severity |
|---|---|---|
| Forbid `:latest` tag | `true` | Low |
| Require liveness probe | `true` | Low |
| Require readiness probe | `true` | Low |
| Max restart count | 10 | Medium |
| Forbid pending duration | 600s | Low |
| Enforcement mode | `audit` | — |

### CLI Commands

```bash
# List all bundles
kube-devops policy bundle-list

# Show bundle details
kube-devops policy bundle-show restricted

# Generate a DevOpsPolicy from a bundle and apply
kube-devops policy bundle-apply restricted --namespace production | kubectl apply -f -

# Customize the policy resource name
kube-devops policy bundle-apply baseline --namespace staging --policy-name staging-policy | kubectl apply -f -
```

------------------------------------------------------------------------

## GitOps Support

### Export

Export all DevOpsPolicies from a namespace as YAML:

```bash
kube-devops policy export --namespace production > policies.yaml
```

### Import

Import policies from a YAML file:

```bash
# Dry-run — preview changes without applying
kube-devops policy import policies.yaml --dry-run

# Apply
kube-devops policy import policies.yaml
```

### Diff

Compare local YAML against live cluster state:

```bash
kube-devops policy diff policies.yaml
```

This enables version-controlled policy management where policies are stored
in Git and applied through CI/CD pipelines.

------------------------------------------------------------------------

## Multi-Cluster Governance

### Architecture (`src/multi_cluster.rs`)

Multi-cluster support evaluates governance policies across multiple kubeconfig
contexts:

```rust
pub struct ClusterEvaluation {
    pub context_name: String,
    pub health_score: f64,
    pub classification: String,
    pub total_pods: usize,
    pub violations: Vec<String>,
}

pub struct MultiClusterReport {
    pub clusters: Vec<ClusterEvaluation>,
    pub aggregate_score: f64,
    pub aggregate_classification: String,
}
```

### Functions

| Function | Description |
|---|---|
| `list_contexts()` | Parse kubeconfig for all available contexts |
| `client_for_context()` | Create a `kube::Client` for a specific context |
| `evaluate_cluster()` | Evaluate all workload pods in a cluster against a policy |
| `aggregate_report()` | Combine per-cluster evaluations into a unified report |

### CLI Commands

```bash
# List all kubeconfig contexts
kube-devops multi-cluster list-contexts

# Analyze all contexts (default bundle: baseline)
kube-devops multi-cluster analyze

# Analyze specific contexts
kube-devops multi-cluster analyze --contexts prod-us,prod-eu,staging

# Use a specific bundle
kube-devops multi-cluster analyze --bundle restricted

# Show per-cluster breakdown
kube-devops multi-cluster analyze --per-cluster
```

### Example Output

```
===== Multi-Cluster Governance Report =====

Cluster: prod-us
  Health Score: 85
  Classification: Healthy
  Pods Analyzed: 42
  Violations: 3

Cluster: prod-eu
  Health Score: 72
  Classification: Stable
  Pods Analyzed: 38
  Violations: 7

----------------------------------------------
Aggregate Score: 79
Aggregate Status: Stable
===============================================
```

------------------------------------------------------------------------

## Updated CLI Commands

Step 10 adds 9 new subcommands (total: 25):

| Command | Description |
|---|---|
| `policy bundle-list` | List available policy bundles |
| `policy bundle-show <name>` | Show bundle details |
| `policy bundle-apply <name>` | Generate DevOpsPolicy from bundle |
| `policy export` | Export policies as YAML |
| `policy import <file>` | Import policies from YAML |
| `policy diff <file>` | Diff local vs cluster state |
| `multi-cluster list-contexts` | List kubeconfig contexts |
| `multi-cluster analyze` | Evaluate clusters against bundle |

### Updated CLI Definition (`src/cli.rs`)

```rust
pub enum Commands {
    // ... existing commands ...
    Policy { action: PolicyAction },
    MultiCluster { action: MultiClusterAction },
}

pub enum PolicyAction {
    BundleList,
    BundleShow { name: String },
    BundleApply { name: String, namespace: String, policy_name: String },
    Export { namespace: String },
    Import { file: String, dry_run: bool },
    Diff { file: String },
}

pub enum MultiClusterAction {
    ListContexts,
    Analyze { contexts: Option<Vec<String>>, bundle: Option<String>, per_cluster: bool },
}
```

------------------------------------------------------------------------

## Sample DevOpsPolicy with All Step 10 Features

```yaml
apiVersion: devops.stochastic.io/v1
kind: DevOpsPolicy
metadata:
  name: strict-severity-policy
  namespace: production
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
  severityOverrides:
    latestTag: critical
    missingLiveness: high
    missingReadiness: high
    highRestarts: medium
    pending: low
```

------------------------------------------------------------------------

## Tests

Step 10 adds tests for bundles, multi-cluster, policy CLI, and severity
features, bringing the total to **314 tests**.

New test coverage:
- Bundle definitions, lookups, and case-insensitive matching
- Severity enum serialization and defaults
- SeverityOverrides partial configuration
- AuditViolation structure
- PolicyAuditResult CRD schema
- Multi-cluster context listing and report aggregation
- Policy export/import/diff CLI handlers
- New Prometheus metrics (`violations_by_severity`, `audit_results_total`)

All tests run without a Kubernetes cluster using synthetic in-memory objects.

```bash
cargo test                  # Full suite (314 tests)
cargo clippy --all-targets  # Zero warnings
```

------------------------------------------------------------------------

**Last Updated:** 2026-02-24
