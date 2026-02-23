# Step 4 --- Kubernetes Watch Engine (Real-Time DevOps Monitoring)

## Introduction

In Steps 1--3, we built a Kubernetes DevOps governance analyzer that:

-   Connects to a Kubernetes cluster
-   Retrieves a snapshot of all Pods
-   Applies governance rules
-   Produces a weighted cluster health score

However, this model is **static**.

Step 4 transforms the tool into a **real-time monitoring engine** using
the Kubernetes Watch API.

This is the architectural shift from:

> Snapshot Analyzer → Event-Driven DevOps Engine

------------------------------------------------------------------------

# 1️⃣ Why the Watch API Matters

## Snapshot Model (What We Built Before)

    GET /api/v1/pods
    → return full list
    → analyze
    → exit

Problems: - Not real-time - Expensive on large clusters - No event
awareness - Cannot detect drift immediately

------------------------------------------------------------------------

## Watch Model (What We Build Now)

    GET /api/v1/pods?watch=true

The API keeps the connection open and streams events:

``` json
{ "type": "ADDED", "object": { ...Pod... } }
{ "type": "MODIFIED", "object": { ...Pod... } }
{ "type": "DELETED", "object": { ...Pod... } }
```

This enables:

-   Real-time governance
-   Drift detection
-   Immediate alerting
-   Event-driven architecture

------------------------------------------------------------------------

# 2️⃣ Architectural Upgrade

Old architecture:

    main → analyze() → exit

New architecture:

    main
      → async runtime (tokio)
      → open watch stream
      → process events in loop
      → continuously update governance score

We now build a long-running service.

------------------------------------------------------------------------

# 3️⃣ Required Dependencies

Update `Cargo.toml`:

``` toml
[dependencies]
kube = { version = "0.88", features = ["runtime", "derive"] }
k8s-openapi = { version = "0.21", features = ["v1_26"] }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
futures = "0.3"
```

Then build:

    cargo build

------------------------------------------------------------------------

# 4️⃣ Async Rust Foundation

Rust does not run async automatically.

We must update `main.rs`:

``` rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // logic
}
```

This:

-   Starts async runtime
-   Enables streaming
-   Allows non-blocking Kubernetes calls

------------------------------------------------------------------------

# 5️⃣ CLI Extension

Add new subcommand in `cli.rs`:

``` rust
Watch,
```

Wire it in `main.rs`:

``` rust
Commands::Watch => commands::watch::run().await?,
```

------------------------------------------------------------------------

# 6️⃣ File Structure

    src/
     ├── main.rs
     ├── cli.rs
     ├── commands/
     │    ├── analyze.rs
     │    └── watch.rs

------------------------------------------------------------------------

# 7️⃣ Minimal Watch Engine (Phase 1)

Create `watch.rs`:

``` rust
use futures::{StreamExt, TryStreamExt};
use kube::{Api, Client};
use kube::api::ListParams;
use k8s_openapi::api::core::v1::Pod;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {

    println!("Starting real-time Kubernetes Watch Engine...");

    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::all(client);

    let mut stream = pods.watch(&ListParams::default(), "0").await?.boxed();

    while let Some(status) = stream.try_next().await? {
        match status {
            kube::api::WatchEvent::Added(pod) => {
                println!("🟢 ADDED: {}", pod.metadata.name.unwrap_or_default());
            }
            kube::api::WatchEvent::Modified(pod) => {
                println!("🟡 MODIFIED: {}", pod.metadata.name.unwrap_or_default());
            }
            kube::api::WatchEvent::Deleted(pod) => {
                println!("🔴 DELETED: {}", pod.metadata.name.unwrap_or_default());
            }
            _ => {}
        }
    }

    Ok(())
}
```

------------------------------------------------------------------------

# 8️⃣ Running the Watch Engine

    cargo run -- watch

Expected behavior:

-   Tool does not exit
-   Prints events in real time
-   Responds to Pod changes immediately

------------------------------------------------------------------------

# 9️⃣ Advanced Evolution (Phase 2)

Next improvements:

-   Maintain in-memory cluster state
-   Recalculate governance score on each event
-   Emit severity changes
-   Add namespace filters
-   Add structured logging
-   Convert into controller-like architecture

------------------------------------------------------------------------

# 🔟 What You Just Learned

You now understand:

-   Difference between polling and streaming
-   Kubernetes Watch API
-   Async Rust with Tokio
-   Event-driven architecture
-   Real-time DevOps monitoring
-   Testing axum HTTP handlers with `tower::oneshot` (no TCP required)
-   Building synthetic k8s-openapi Pod objects for offline testing

------------------------------------------------------------------------

# 1️⃣1️⃣ Test Suite

Step 4 includes 49 automated tests that run without a Kubernetes
cluster. See `docs/Step_4_Testing.md` for full documentation.

| Layer | Location | Tests |
|---|---|---|
| Unit | `src/governance.rs` | 38 |
| HTTP | `src/commands/watch.rs` | 5 |
| Integration | `tests/governance_integration.rs` | 6 |

Run the full suite:

```bash
cargo test
```

------------------------------------------------------------------------

# Final Outcome

Your tool is no longer:

> A CLI script

It is becoming:

> A Kubernetes-native DevOps monitoring engine written in Rust, backed
> by a comprehensive offline test suite.
