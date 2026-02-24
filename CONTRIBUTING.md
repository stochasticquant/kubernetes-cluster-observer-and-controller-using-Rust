# Contributing to kube-devops

Thanks for your interest in contributing! This document covers the workflow
and conventions used in this project.

## Prerequisites

- **Rust** (stable, edition 2024) — install via [rustup](https://rustup.rs/)
- **kubectl** configured with access to a Kubernetes cluster (v1.26+)
- **Helm 3** (for chart development)
- A running Kubernetes cluster for integration testing

## Building

```bash
cargo build
```

## Testing

Run the full test suite (314 tests):

```bash
cargo test
```

## Linting

All code must pass clippy with no warnings:

```bash
cargo clippy --all-targets -- -D warnings
```

Format check:

```bash
cargo fmt --check
```

## Branch and Commit Conventions

- **Feature branches:** `feature/step-N-short-description` or `feature/short-description`
- **Commit messages:** Use conventional style prefixes:
  - `feat(step-N): description` — new features
  - `fix: description` — bug fixes
  - `chore: description` — maintenance, dependencies
  - `docs: description` — documentation only

## Pull Request Process

1. Fork the repository and create a feature branch from `main`.
2. Make your changes, ensuring all tests pass and clippy is clean.
3. Write or update tests for any new functionality.
4. Submit a pull request against `main` with a clear description of changes.
5. PRs require passing CI checks before merge.

## Project Structure

| Path | Description |
|------|-------------|
| `src/lib.rs` | Module exports |
| `src/cli.rs` | CLI definitions (clap) |
| `src/crd.rs` | CRD types (DevOpsPolicy, PolicyAuditResult) |
| `src/governance.rs` | Scoring engine and violation detection |
| `src/admission.rs` | Admission webhook logic |
| `src/enforcement.rs` | Remediation and patching |
| `src/bundles.rs` | Pre-defined policy bundles |
| `src/multi_cluster.rs` | Multi-cluster evaluation |
| `src/commands/` | CLI command handlers |
| `helm/kube-devops/` | Helm chart |
| `tests/` | Integration tests |

## CRD Group

All custom resources use the API group `devops.stochastic.io/v1`.
