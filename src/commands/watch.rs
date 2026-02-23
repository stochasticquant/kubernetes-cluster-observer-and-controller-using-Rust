use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::LazyLock,
    time::Duration,
};

use anyhow::{Context, Result};
use futures::StreamExt;
use kube::{Api, Client};
use kube_runtime::watcher::{watcher, Config, Event};
use k8s_openapi::api::{
    core::v1::Pod,
    coordination::v1::{Lease, LeaseSpec},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::chrono::{self, Utc};

use axum::{
    routing::get,
    Router,
    response::IntoResponse,
    http::StatusCode,
};
use prometheus::{Encoder, IntCounter, IntGauge, IntGaugeVec, Registry, TextEncoder};
use tokio::sync::{broadcast, Mutex};
use tokio::{signal, time::sleep};
use tracing::info;

use kube_devops::governance::{
    self, PodMetrics, add_metrics, subtract_metrics,
    calculate_health_score,
};

/* ============================= CONFIG ============================= */

const LEASE_NAME: &str = "kube-devops-leader";
const LEASE_NAMESPACE: &str = "default";
const LEASE_DURATION_SECONDS: i32 = 15;
const LEASE_RENEW_INTERVAL: Duration = Duration::from_secs(5);

/* ============================= PROMETHEUS ============================= */

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

static CLUSTER_SCORE: LazyLock<IntGauge> = LazyLock::new(|| {
    let g = IntGauge::new("cluster_health_score", "Cluster governance health score (0-100)")
        .expect("metric definition is valid");
    REGISTRY.register(Box::new(g.clone())).expect("metric not yet registered");
    g
});

static NAMESPACE_SCORE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new("namespace_health_score", "Namespace governance health score (0-100)"),
        &["namespace"],
    )
    .expect("metric definition is valid");
    REGISTRY.register(Box::new(g.clone())).expect("metric not yet registered");
    g
});

static POD_EVENTS: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new("pod_events_total", "Total pod events processed")
        .expect("metric definition is valid");
    REGISTRY.register(Box::new(c.clone())).expect("metric not yet registered");
    c
});

static PODS_TRACKED: LazyLock<IntGauge> = LazyLock::new(|| {
    let g = IntGauge::new("pods_tracked_total", "Total pods currently tracked by the watch controller")
        .expect("metric definition is valid");
    REGISTRY.register(Box::new(g.clone())).expect("metric not yet registered");
    g
});

/* ============================= STATE ============================= */

pub(crate) struct NamespaceState {
    pub(crate) metrics: PodMetrics,
}

pub(crate) struct ClusterState {
    pub(crate) namespaces: HashMap<String, NamespaceState>,
    pub(crate) ready: bool,
}

/* ============================= ENTRY ============================= */

pub async fn run() -> Result<()> {
    println!("Starting watch controller...\n");
    info!("controller_starting");

    let client = Client::try_default().await
        .context("Failed to connect to Kubernetes cluster")?;

    print!("  Cluster connection .......... ");
    match client.apiserver_version().await {
        Ok(v) => println!("OK (v{}.{})", v.major, v.minor),
        Err(e) => {
            println!("FAIL");
            anyhow::bail!(
                "Cannot reach cluster: {}. Is the cluster running?",
                e
            );
        }
    }

    print!("  Leader election ............. ");
    if !acquire_leader(&client).await? {
        println!("waiting (another instance holds the lease)");
        info!("not_leader_waiting");
        loop {
            sleep(Duration::from_secs(10)).await;
        }
    }
    println!("acquired");
    info!("leader_acquired");

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    println!("  HTTP server ................. http://{addr}");
    println!();
    println!("  Available endpoints:");
    println!("    GET /healthz .............. Liveness probe (always 200 OK)");
    println!("    GET /readyz ............... Readiness probe (503 until initial sync, then 200)");
    println!("    GET /metrics .............. Prometheus metrics scrape endpoint");
    println!();
    println!("Watch controller running. Press Ctrl+C to stop.\n");
    println!("{}", "=".repeat(70));

    let cluster_state = std::sync::Arc::new(Mutex::new(ClusterState {
        namespaces: HashMap::new(),
        ready: false,
    }));

    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Spawn lease renewal
    let renewal_client = client.clone();
    let renewal_shutdown = shutdown_tx.subscribe();
    tokio::spawn(async move {
        lease_renewal_loop(renewal_client, renewal_shutdown).await
    });

    let watch_state = cluster_state.clone();
    let http_state = cluster_state.clone();

    let watch_shutdown = shutdown_tx.subscribe();
    let http_shutdown = shutdown_tx.subscribe();

    let watch_handle = tokio::spawn(async move {
        watch_loop(watch_state, watch_shutdown).await
    });

    let http_handle = tokio::spawn(async move {
        start_http_server(http_state, http_shutdown, addr).await
    });

    signal::ctrl_c().await?;
    info!("shutdown_signal_received");
    println!("\n{}", "=".repeat(70));
    println!("Shutdown signal received. Stopping watch controller...");
    println!("{}", "=".repeat(70));

    let _ = shutdown_tx.send(());

    let _ = watch_handle.await?;
    let _ = http_handle.await?;

    info!("controller_stopped");
    println!("Watch controller stopped.");
    Ok(())
}

/* ============================= LEADER ELECTION ============================= */

async fn acquire_leader(client: &Client) -> Result<bool> {
    let leases: Api<Lease> = Api::namespaced(client.clone(), LEASE_NAMESPACE);

    let now = MicroTime(Utc::now());

    let lease = Lease {
        metadata: ObjectMeta {
            name: Some(LEASE_NAME.to_string()),
            ..Default::default()
        },
        spec: Some(LeaseSpec {
            holder_identity: Some("kube-devops-instance".to_string()),
            lease_duration_seconds: Some(LEASE_DURATION_SECONDS),
            acquire_time: Some(now.clone()),
            renew_time: Some(now),
            ..Default::default()
        }),
    };

    // Try to create a fresh lease
    match leases.create(&Default::default(), &lease).await {
        Ok(_) => return Ok(true),
        Err(kube::Error::Api(err)) if err.code == 409 => {
            // Lease already exists — check if we can take it over
            info!("lease_exists_checking_expiry");
        }
        Err(_) => return Ok(false),
    }

    // Lease exists — fetch it and check ownership / expiry
    let existing = leases.get(LEASE_NAME).await?;

    let can_take = match &existing.spec {
        Some(spec) => {
            let is_ours = spec.holder_identity.as_deref()
                == Some("kube-devops-instance");

            let is_expired = spec.renew_time.as_ref().is_none_or(|t| {
                let duration_secs = spec.lease_duration_seconds.unwrap_or(15) as i64;
                Utc::now().signed_duration_since(t.0)
                    > chrono::Duration::seconds(duration_secs)
            });

            is_ours || is_expired
        }
        None => true,
    };

    if !can_take {
        return Ok(false);
    }

    // Take over the expired / our lease
    info!("lease_takeover");
    let now = MicroTime(Utc::now());
    let patch = serde_json::json!({
        "spec": {
            "holderIdentity": "kube-devops-instance",
            "leaseDurationSeconds": LEASE_DURATION_SECONDS,
            "acquireTime": now,
            "renewTime": now
        }
    });

    match leases.patch(
        LEASE_NAME,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    ).await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

async fn lease_renewal_loop(
    client: Client,
    mut shutdown: broadcast::Receiver<()>,
) {
    let leases: Api<Lease> = Api::namespaced(client, LEASE_NAMESPACE);

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("lease_renewal_stopped");
                return;
            }
            _ = sleep(LEASE_RENEW_INTERVAL) => {
                let now = MicroTime(Utc::now());

                let patch = serde_json::json!({
                    "spec": {
                        "renewTime": now
                    }
                });

                match leases.patch(
                    LEASE_NAME,
                    &kube::api::PatchParams::default(),
                    &kube::api::Patch::Merge(&patch),
                ).await {
                    Ok(_) => {}
                    Err(e) => {
                        info!(error=%e, "lease_renewal_failed");
                    }
                }
            }
        }
    }
}

/* ============================= WATCH LOOP ============================= */

async fn watch_loop(
    cluster_state: std::sync::Arc<Mutex<ClusterState>>,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    let client = Client::try_default().await
        .context("Failed to connect to Kubernetes cluster for watcher")?;

    let pods: Api<Pod> = Api::all(client);
    let mut pod_store: HashMap<String, (String, PodMetrics)> = HashMap::new();

    let config = Config::default();
    let mut stream = watcher(pods, config).boxed();

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("watcher_shutdown");
                return Ok(());
            }

            event = stream.next() => {
                if let Some(Ok(event)) = event {
                    POD_EVENTS.inc();

                    let mut state = cluster_state.lock().await;

                    match event {
                        Event::Applied(pod) => {
                            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();

                            if governance::is_system_namespace(ns) {
                                continue;
                            }

                            let name = pod.metadata.name.as_deref().unwrap_or_default();
                            let key = format!("{}/{}", ns, name);

                            // Remove old contribution if pod already tracked
                            if let Some((old_ns, old_metrics)) = pod_store.remove(&key)
                                && let Some(ns_state) = state.namespaces.get_mut(&old_ns)
                            {
                                subtract_metrics(&mut ns_state.metrics, &old_metrics);
                            }

                            let contribution = governance::evaluate_pod(&pod);

                            let violations = governance::detect_violations(&pod);
                            if !violations.is_empty() {
                                info!(
                                    event = "policy_violation",
                                    namespace = %ns,
                                    pod = %name,
                                    violations = ?violations,
                                    "policy_violation_detected"
                                );
                            }

                            let ns_state = state.namespaces
                                .entry(ns.to_string())
                                .or_insert(NamespaceState {
                                    metrics: PodMetrics::default(),
                                });

                            add_metrics(&mut ns_state.metrics, &contribution);
                            pod_store.insert(key, (ns.to_string(), contribution));

                            state.ready = true;
                        }

                        Event::Deleted(pod) => {
                            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
                            let name = pod.metadata.name.as_deref().unwrap_or_default();
                            let key = format!("{}/{}", ns, name);

                            if let Some((old_ns, old_metrics)) = pod_store.remove(&key)
                                && let Some(ns_state) = state.namespaces.get_mut(&old_ns)
                            {
                                subtract_metrics(&mut ns_state.metrics, &old_metrics);
                            }
                        }

                        Event::Restarted(pods) => {
                            pod_store.clear();
                            state.namespaces.clear();

                            for pod in pods {
                                let ns = pod.metadata.namespace.as_deref().unwrap_or_default();

                                if governance::is_system_namespace(ns) {
                                    continue;
                                }

                                let name = pod.metadata.name.as_deref().unwrap_or_default();
                                let key = format!("{}/{}", ns, name);

                                let contribution = governance::evaluate_pod(&pod);

                                let ns_state = state.namespaces
                                    .entry(ns.to_string())
                                    .or_insert(NamespaceState {
                                        metrics: PodMetrics::default(),
                                    });

                                add_metrics(&mut ns_state.metrics, &contribution);
                                pod_store.insert(key, (ns.to_string(), contribution));
                            }

                            state.ready = true;
                        }
                    }

                    update_prometheus_metrics(&state);
                    PODS_TRACKED.set(pod_store.len() as i64);
                }
            }
        }
    }
}

/* ============================= PROMETHEUS UPDATE ============================= */

fn update_prometheus_metrics(state: &ClusterState) {
    let mut total: i64 = 0;
    let mut count: i64 = 0;

    for (ns_name, ns_state) in &state.namespaces {
        let score = calculate_health_score(&ns_state.metrics) as i64;
        NAMESPACE_SCORE.with_label_values(&[ns_name]).set(score);
        total += score;
        count += 1;
    }

    if count > 0 {
        CLUSTER_SCORE.set(total / count);
    }
}

/* ============================= HTTP SERVER ============================= */

pub(crate) fn build_router(state: std::sync::Arc<Mutex<ClusterState>>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(|| async { (StatusCode::OK, "OK") }))
        .route("/readyz", get({
            let state = state.clone();
            move || ready_handler(state.clone())
        }))
}

async fn start_http_server(
    state: std::sync::Arc<Mutex<ClusterState>>,
    mut shutdown: broadcast::Receiver<()>,
    addr: SocketAddr,
) -> Result<()> {
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await
        .context("Failed to bind HTTP server on :8080")?;

    info!(addr = %addr, "http_server_started");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        })
        .await?;

    Ok(())
}

async fn ready_handler(state: std::sync::Arc<Mutex<ClusterState>>) -> impl IntoResponse {
    let state = state.lock().await;
    if state.ready {
        (StatusCode::OK, "READY")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
    }
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();

    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => match String::from_utf8(buffer) {
            Ok(body) => (StatusCode::OK, body),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "metrics encoding error".to_string()),
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "metrics encoding error".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state(ready: bool) -> std::sync::Arc<Mutex<ClusterState>> {
        std::sync::Arc::new(Mutex::new(ClusterState {
            namespaces: HashMap::new(),
            ready,
        }))
    }

    #[tokio::test]
    async fn test_healthz_returns_ok() {
        let app = build_router(test_state(false));
        let req = Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"OK");
    }

    #[tokio::test]
    async fn test_readyz_when_ready() {
        let app = build_router(test_state(true));
        let req = Request::builder()
            .uri("/readyz")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"READY");
    }

    #[tokio::test]
    async fn test_readyz_when_not_ready() {
        let app = build_router(test_state(false));
        let req = Request::builder()
            .uri("/readyz")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"NOT READY");
    }

    #[tokio::test]
    async fn test_metrics_returns_ok() {
        let app = build_router(test_state(false));
        let req = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        let app = build_router(test_state(false));
        let req = Request::builder()
            .uri("/nonexistent")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_pods_tracked_metric_registered() {
        LazyLock::force(&PODS_TRACKED);
        let families = REGISTRY.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        assert!(
            names.contains(&"pods_tracked_total"),
            "pods_tracked_total should be registered"
        );
    }
}
