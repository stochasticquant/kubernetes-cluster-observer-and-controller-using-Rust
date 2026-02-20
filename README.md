# Kubernetes Cluster Observer & Controller using Rust

## Project Overview

**Kubernetes Cluster Observer & Controller using Rust** is a progressive
DevOps engineering project designed to build a production-grade
Kubernetes enhancement tool from the ground up using Rust.

This project is structured as a learning journey that evolves from:

-   Beginner Rust fundamentals\
-   CLI application development\
-   Kubernetes API interaction\
-   Real-time cluster monitoring\
-   Controller/Operator development\
-   Policy enforcement mechanisms\
-   Admission webhooks\
-   Observability integration\
-   Production hardening and HA design

The end goal is to build a Rust-based Kubernetes controller that
enhances cluster governance, DevOps best practices enforcement, and
operational visibility.

------------------------------------------------------------------------

## Why This Project Exists

Modern Kubernetes clusters require:

-   Strong governance\
-   Workload policy enforcement\
-   Observability\
-   Automated remediation\
-   DevOps best practices

While tools like Kyverno or OPA Gatekeeper exist, this project focuses
on building similar capabilities from scratch to deeply understand:

-   Kubernetes control loops\
-   Rust systems programming\
-   API-driven reconciliation\
-   Cluster automation patterns

This repository represents the foundation (Step 1) of that journey.

------------------------------------------------------------------------

## Current Stage: Step 3 -- DevOps Analyzer Engine

At this stage, the project provides:

-   A structured Rust CLI application
-   Modular architecture
-   Async Kubernetes API integration
-   DevOps workload analysis and scoring
-   Governance-style reporting

The CLI currently supports:

``` bash
kube-devops version
kube-devops check
kube-devops list pods
kube-devops analyze
```

------------------------------------------------------------------------

## Architecture (Current Phase)

    kube-devops/
     ├── Cargo.toml
     ├── Cargo.lock
     ├── src/
     │   ├── main.rs
     │   ├── cli.rs
     │   └── commands/
     │        ├── mod.rs
     │        ├── version.rs
     │        ├── check.rs
     │        ├── list.rs
     │        └── analyze.rs
     └── docs/
     │   ├── Step_1_Code_Explanation.md
     │   ├── Step_2_Kubernetes_Integration.md
     │   └── Step_3_DevOps_Analyzer_Engine.md
     └── README.md

Design Principles:

-   Separation of concerns
-   Modular command delegation
-   Idiomatic Rust error propagation
-   Async-ready architecture for Kubernetes I/O

------------------------------------------------------------------------

## Prerequisites

You must have:

-   Rust (stable)
-   Cargo
-   Git
-   Access to a Kubernetes cluster
-   A valid kubeconfig in `~/.kube/config`

To install Rust:

``` bash
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
```

Verify installation:

``` bash
rustc --version
cargo --version
```

------------------------------------------------------------------------

## How to Clone the Repository

``` bash
git clone https://github.com/stochasticquant/kubernetes-cluster-observer-and-controller-using-Rust.git
cd kubernetes-cluster-observer-and-controller-using-Rust
```

------------------------------------------------------------------------

## How to Build the Application

### Debug Build

``` bash
cargo build
```

Binary location:

    target/debug/kube-devops

### Release Build (Optimized)

``` bash
cargo build --release
```

Binary location:

    target/release/kube-devops

------------------------------------------------------------------------

## How to Run the Application

When using Cargo:

``` bash
cargo run -- version
cargo run -- check
cargo run -- list pods
cargo run -- analyze
```

Important:

The `--` separator tells Cargo to pass arguments to your application.

Without it, Cargo interprets flags as its own parameters.

------------------------------------------------------------------------

## Running the Compiled Binary Directly

After building:

``` bash
./target/debug/kube-devops version
./target/debug/kube-devops check
./target/debug/kube-devops list pods
./target/debug/kube-devops analyze
```

Or for release:

``` bash
./target/release/kube-devops version
./target/release/kube-devops analyze
```

------------------------------------------------------------------------

## Expected Output

``` bash
kube-devops version 0.1.0
Running DevOps checks...
```

Example output for `list`:

``` bash
default             pod-name-12345                   Running         node-1
kube-system         coredns-558bd4d5db-abc12         Running         node-2
```

Example output for `analyze`:

``` bash
===== DevOps Governance Summary =====
Workload Pods Analyzed     : 12
Images using :latest       : 2
Missing liveness probes    : 5
Missing readiness probes   : 3
Restart severity score     : 4
Pending pods               : 1
--------------------------------------
Cluster Health Score       : 73
Cluster Status             : Stable
======================================
```

------------------------------------------------------------------------

## Error Handling Design

The application uses idiomatic Rust error propagation:

``` rust
fn main() -> Result<(), Box<dyn std::error::Error>>
```

Each command returns:

``` rust
Result<(), Box<dyn std::error::Error>>
```

This enables:

-   Clean failure propagation
-   Scalable command extension
-   Production-grade reliability
-   Compatibility with future async + API integrations

------------------------------------------------------------------------

## Development Workflow

Recommended branch strategy:

-   `main` → Stable baseline
-   `dev` → Integration branch
-   `feature/*` → Feature development

Basic workflow:

``` bash
git checkout -b feature/my-feature
git commit -m "Add new feature"
git push origin feature/my-feature
```

Open Pull Request → Merge into main.

------------------------------------------------------------------------

## Roadmap

Completed in current steps:

-   Async Rust with Tokio
-   Kubernetes API client integration
-   Pod inspection capabilities (list)
-   DevOps audit checks (analyze)

Upcoming phases include:

-   CRD creation
-   Reconciliation controller
-   Admission webhook
-   Prometheus metrics endpoint
-   Leader election
-   Multi-replica HA deployment

------------------------------------------------------------------------

## Long-Term Vision

By the end of this project, the repository will contain:

-   A Rust CLI tool
-   A Kubernetes operator
-   Policy enforcement mechanisms
-   Observability integrations
-   Production-grade deployment manifests
-   Helm chart support
-   HA controller configuration

This project serves as a platform engineering laboratory using a real
9-node Kubernetes cluster.

------------------------------------------------------------------------

## License

MIT License

------------------------------------------------------------------------

## Author

StochasticQuant\
DevOps & Platform Engineering Lab

------------------------------------------------------------------------

**Last Updated:** 2026-02-20
