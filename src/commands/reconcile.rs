use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::{Action, Controller};
use kube::{Client, ResourceExt};
use prometheus::{Encoder, Histogram, IntCounter, IntGaugeVec, Registry, TextEncoder};
use tokio::signal;
use tokio::sync::{Mutex, broadcast};
use tracing::{info, warn};

use kube_devops::crd::{
    AuditViolation, DevOpsPolicy, DevOpsPolicyStatus, PolicyAuditResult, PolicyAuditResultSpec,
};
use kube_devops::enforcement;
use kube_devops::governance;

/* ============================= CONFIG ============================= */

const FINALIZER: &str = "devops.stochastic.io/cleanup";
const REQUEUE_INTERVAL: Duration = Duration::from_secs(30);

/* ============================= PROMETHEUS ============================= */

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

static RECONCILE_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_reconcile_total",
        "Total DevOpsPolicy reconciliation cycles",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static RECONCILE_ERRORS: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_reconcile_errors_total",
        "Total DevOpsPolicy reconciliation errors",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static POLICY_VIOLATIONS: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_violations_total",
            "Policy violations per namespace and policy",
        ),
        &["namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

static POLICY_HEALTH: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_health_score",
            "Health score per namespace and policy",
        ),
        &["namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

static REMEDIATIONS_APPLIED: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_remediations_applied_total",
        "Total successful remediations applied",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static REMEDIATIONS_FAILED: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_remediations_failed_total",
        "Total failed remediation attempts",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static ENFORCEMENT_MODE: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_enforcement_mode",
            "Enforcement mode per policy (0=audit, 1=enforce)",
        ),
        &["namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

static PODS_SCANNED: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_pods_scanned_total",
        "Total pods scanned across all reconciliation cycles",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static RECONCILE_DURATION: LazyLock<Histogram> = LazyLock::new(|| {
    let h = Histogram::with_opts(prometheus::HistogramOpts::new(
        "devopspolicy_reconcile_duration_seconds",
        "Duration of each reconciliation cycle in seconds",
    ))
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(h.clone()))
        .expect("metric not yet registered");
    h
});

static VIOLATIONS_BY_SEVERITY: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    let g = IntGaugeVec::new(
        prometheus::Opts::new(
            "devopspolicy_violations_by_severity",
            "Policy violations grouped by severity level",
        ),
        &["severity", "namespace", "policy"],
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("metric not yet registered");
    g
});

static AUDIT_RESULTS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    let c = IntCounter::new(
        "devopspolicy_audit_results_total",
        "Total audit results created",
    )
    .expect("metric definition is valid");
    REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

/* ============================= STATE ============================= */

pub(crate) struct ReconcileState {
    pub(crate) ready: bool,
}

/* ============================= CONTEXT ============================= */

struct ReconcileContext {
    client: Client,
}

/* ============================= ENTRY ============================= */

pub async fn run() -> Result<()> {
    println!("Starting DevOpsPolicy operator...\n");

    let client = Client::try_default()
        .await
        .context("Failed to load kubeconfig")?;

    // Verify actual cluster connectivity before starting the controller
    print!("  Cluster connection .......... ");
    match client.apiserver_version().await {
        Ok(v) => println!("OK (v{}.{})", v.major, v.minor),
        Err(e) => {
            println!("FAIL");
            anyhow::bail!("Cannot reach cluster: {}. Is the cluster running?", e);
        }
    }

    let policies: Api<DevOpsPolicy> = Api::all(client.clone());
    let pods: Api<Pod> = Api::all(client.clone());

    let ctx = Arc::new(ReconcileContext {
        client: client.clone(),
    });

    // Force-init Prometheus metrics so they appear on /metrics
    LazyLock::force(&RECONCILE_TOTAL);
    LazyLock::force(&RECONCILE_ERRORS);
    LazyLock::force(&POLICY_VIOLATIONS);
    LazyLock::force(&POLICY_HEALTH);
    LazyLock::force(&REMEDIATIONS_APPLIED);
    LazyLock::force(&REMEDIATIONS_FAILED);
    LazyLock::force(&ENFORCEMENT_MODE);
    LazyLock::force(&PODS_SCANNED);
    LazyLock::force(&RECONCILE_DURATION);
    LazyLock::force(&VIOLATIONS_BY_SEVERITY);
    LazyLock::force(&AUDIT_RESULTS_TOTAL);

    let addr = SocketAddr::from(([0, 0, 0, 0], 9090));

    println!("  CRD watch ................... DevOpsPolicy.devops.stochastic.io/v1");
    println!(
        "  Requeue interval ............ {}s",
        REQUEUE_INTERVAL.as_secs()
    );
    println!("  Metrics server .............. http://{addr}");
    println!();
    println!("  Available endpoints:");
    println!("    GET /healthz .............. Liveness probe (always 200 OK)");
    println!(
        "    GET /readyz ............... Readiness probe (503 until first reconcile, then 200)"
    );
    println!("    GET /metrics .............. Prometheus metrics scrape endpoint");
    println!();
    println!("Operator running. Press Ctrl+C to stop.\n");
    println!("{}", "=".repeat(70));

    info!("operator_controller_started");

    let reconcile_state = Arc::new(Mutex::new(ReconcileState { ready: false }));

    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    let http_state = reconcile_state.clone();
    let http_shutdown = shutdown_tx.subscribe();

    let http_handle =
        tokio::spawn(async move { start_metrics_server(http_state, http_shutdown, addr).await });

    let controller_state = reconcile_state.clone();
    let controller = Controller::new(policies, Default::default())
        .owns(pods, Default::default())
        .run(reconcile, error_policy, ctx)
        .for_each(move |result| {
            let state = controller_state.clone();
            async move {
                // Mark ready after first successful reconcile dispatch
                {
                    let mut s = state.lock().await;
                    if !s.ready {
                        s.ready = true;
                    }
                }
                match result {
                    Ok((_obj, _action)) => {}
                    Err(e) => {
                        warn!(error = %e, "reconcile_dispatch_error");
                        eprintln!("[ERROR] Reconcile dispatch: {e}");
                    }
                }
            }
        });

    // Use select! so Ctrl+C drops (cancels) the controller stream.
    // The kube Controller has no built-in shutdown hook, so dropping
    // the future is the only way to stop it cleanly.
    tokio::select! {
        _ = controller => {
            info!("operator_controller_stream_ended");
            println!("\nController stream ended unexpectedly.");
        }
        _ = signal::ctrl_c() => {
            info!("shutdown_signal_received");
            println!("\n{}", "=".repeat(70));
            println!("Shutdown signal received. Stopping operator...");
            println!("{}", "=".repeat(70));
        }
    }

    // Signal the HTTP server to shut down
    let _ = shutdown_tx.send(());
    let _ = http_handle.await?;

    info!("operator_stopped");
    println!("Operator stopped.");

    Ok(())
}

/* ============================= RECONCILE ============================= */

async fn reconcile(
    policy: Arc<DevOpsPolicy>,
    ctx: Arc<ReconcileContext>,
) -> std::result::Result<Action, kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let generation = policy.metadata.generation;

    // ── Skip if already reconciled this generation ──
    let already_reconciled =
        policy.status.as_ref().and_then(|s| s.observed_generation) == generation;

    if already_reconciled {
        info!(
            policy = %name,
            namespace = %namespace,
            generation = ?generation,
            "reconcile_skip_unchanged"
        );
        println!(
            "[{}] {namespace}/{name}: unchanged (generation {:?}), requeue in {}s",
            chrono::Utc::now().format("%H:%M:%S"),
            generation,
            REQUEUE_INTERVAL.as_secs()
        );
        return Ok(Action::requeue(REQUEUE_INTERVAL));
    }

    RECONCILE_TOTAL.inc();
    let _timer = RECONCILE_DURATION.start_timer();

    info!(
        policy = %name,
        namespace = %namespace,
        "reconcile_start"
    );

    // ── Handle deletion with finalizer ──
    if policy.metadata.deletion_timestamp.is_some() {
        return handle_deletion(&policy, &ctx.client).await;
    }

    // ── Ensure finalizer is present ──
    if !has_finalizer(&policy) {
        add_finalizer(&policy, &ctx.client).await?;
    }

    // ── List pods in the policy's namespace ──
    let pods_api: Api<Pod> = Api::namespaced(ctx.client.clone(), &namespace);
    let pod_list = pods_api.list(&Default::default()).await?;

    PODS_SCANNED.inc_by(pod_list.items.len() as u64);

    // ── Evaluate pods against the policy spec ──
    let mut aggregate = governance::PodMetrics::default();
    let mut total_violations: u32 = 0;

    for pod in &pod_list.items {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }

        let contribution = governance::evaluate_pod_with_policy(pod, &policy.spec);
        governance::add_metrics(&mut aggregate, &contribution);

        let violations = governance::detect_violations_with_policy(pod, &policy.spec);
        total_violations += violations.len() as u32;
    }

    let health_score = governance::calculate_health_score(&aggregate);
    let classification = governance::classify_health(health_score);
    let healthy = health_score >= 80;

    let message = format!(
        "{} violations across {} pods — {} ({})",
        total_violations, aggregate.total_pods, classification, health_score
    );

    // ── Print human-readable summary ──
    let now = chrono::Utc::now();
    let timestamp = now.format("%H:%M:%S");

    let enforce_mode = enforcement::is_enforcement_enabled(&policy.spec);
    let mode_label = if enforce_mode { "enforce" } else { "audit" };

    println!(
        "[{timestamp}] {namespace}/{name}: {classification} — score {health_score}/100, \
         {total_violations} violations, {pods} pods (mode: {mode_label})",
        pods = aggregate.total_pods
    );

    info!(
        policy = %name,
        namespace = %namespace,
        health_score,
        violations = total_violations,
        pods = aggregate.total_pods,
        classification,
        mode = mode_label,
        "reconcile_evaluated"
    );

    // ── Update Prometheus metrics ──
    POLICY_VIOLATIONS
        .with_label_values(&[&namespace, &name])
        .set(total_violations as i64);
    POLICY_HEALTH
        .with_label_values(&[&namespace, &name])
        .set(health_score as i64);
    ENFORCEMENT_MODE
        .with_label_values(&[&namespace, &name])
        .set(if enforce_mode { 1 } else { 0 });

    // ── Violations by severity ──
    {
        let mut severity_counts = std::collections::HashMap::new();
        for pod in &pod_list.items {
            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
            if governance::is_system_namespace(ns) {
                continue;
            }
            let details = governance::detect_violations_detailed(pod, &policy.spec);
            for d in &details {
                let sev = format!("{:?}", d.severity).to_lowercase();
                *severity_counts.entry(sev).or_insert(0i64) += 1;
            }
        }
        for sev in &["critical", "high", "medium", "low"] {
            VIOLATIONS_BY_SEVERITY
                .with_label_values(&[sev, &namespace, &name])
                .set(*severity_counts.get(*sev).unwrap_or(&0));
        }
    }

    // ── Enforcement phase ──
    let mut remediations_applied: u32 = 0;
    let mut remediations_failed: u32 = 0;
    let mut remediated_workloads: Vec<String> = Vec::new();
    let mut seen_workloads = std::collections::HashSet::new();

    if enforce_mode {
        for pod in &pod_list.items {
            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
            if governance::is_system_namespace(ns) || enforcement::is_protected_namespace(ns) {
                continue;
            }

            if let Some(plan) = enforcement::plan_remediation(pod, &policy.spec) {
                let key = plan.workload.key();

                // Deduplicate: skip if we already patched this workload in this cycle
                if !seen_workloads.insert(key.clone()) {
                    continue;
                }

                let result = enforcement::apply_remediation(&plan, &ctx.client, &policy.spec).await;

                if result.success {
                    remediations_applied += 1;
                    REMEDIATIONS_APPLIED.inc();
                    remediated_workloads.push(key.clone());
                    info!(
                        workload = %key,
                        policy = %name,
                        "enforcement_remediation_applied"
                    );
                    println!(
                        "  [ENFORCE] Patched {key} ({} action(s))",
                        plan.actions.len()
                    );
                } else {
                    remediations_failed += 1;
                    REMEDIATIONS_FAILED.inc();
                    warn!(
                        workload = %key,
                        error = %result.message,
                        policy = %name,
                        "enforcement_remediation_failed"
                    );
                    println!("  [ENFORCE] FAILED {key}: {}", result.message);
                }
            }
        }

        if remediations_applied > 0 || remediations_failed > 0 {
            println!(
                "  [ENFORCE] Summary: {remediations_applied} applied, {remediations_failed} failed"
            );
        }
    }

    // ── Update status sub-resource ──
    let status = DevOpsPolicyStatus {
        observed_generation: generation,
        healthy: Some(healthy),
        health_score: Some(health_score),
        violations: Some(total_violations),
        last_evaluated: Some(now.to_rfc3339()),
        message: Some(message),
        remediations_applied: if enforce_mode {
            Some(remediations_applied)
        } else {
            None
        },
        remediations_failed: if enforce_mode {
            Some(remediations_failed)
        } else {
            None
        },
        remediated_workloads: if remediated_workloads.is_empty() {
            None
        } else {
            Some(remediated_workloads)
        },
    };

    let status_patch = serde_json::json!({ "status": status });
    let policies_api: Api<DevOpsPolicy> = Api::namespaced(ctx.client.clone(), &namespace);

    policies_api
        .patch_status(
            &name,
            &PatchParams::apply("kube-devops-operator"),
            &Patch::Merge(&status_patch),
        )
        .await?;

    info!(
        policy = %name,
        namespace = %namespace,
        "status_updated"
    );

    // ── Create audit result (async, non-blocking) ──
    let audit_client = ctx.client.clone();
    let audit_name = name.clone();
    let audit_ns = namespace.clone();
    let audit_policy_spec = policy.spec.clone();
    let audit_timestamp = now.to_rfc3339();
    let audit_pods: Vec<_> = pod_list.items.clone();

    tokio::spawn(async move {
        if let Err(e) = create_audit_result(
            &audit_client,
            &audit_name,
            &audit_ns,
            &audit_policy_spec,
            &audit_timestamp,
            health_score,
            total_violations,
            &audit_pods,
        )
        .await
        {
            warn!(error = %e, policy = %audit_name, "audit_result_creation_failed");
        }
    });

    Ok(Action::requeue(REQUEUE_INTERVAL))
}

/* ============================= AUDIT RESULTS ============================= */

const AUDIT_RETENTION: usize = 10;

#[allow(clippy::too_many_arguments)]
async fn create_audit_result(
    client: &Client,
    policy_name: &str,
    namespace: &str,
    policy_spec: &kube_devops::crd::DevOpsPolicySpec,
    timestamp: &str,
    health_score: u32,
    total_violations: u32,
    pods: &[Pod],
) -> anyhow::Result<()> {
    let audit_api: Api<PolicyAuditResult> = Api::namespaced(client.clone(), namespace);

    // Collect detailed violations
    let mut violations = Vec::new();
    let mut total_pods: u32 = 0;
    for pod in pods {
        let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
        if governance::is_system_namespace(ns) {
            continue;
        }
        total_pods += 1;
        let details = governance::detect_violations_detailed(pod, policy_spec);
        for d in details {
            violations.push(AuditViolation {
                pod_name: d.pod_name,
                container_name: d.container_name,
                violation_type: d.violation_type,
                severity: d.severity,
                message: d.message,
            });
        }
    }

    let classification = governance::classify_health(health_score).to_string();

    let ts_millis = chrono::Utc::now().timestamp_millis();
    let result_name = format!("{policy_name}-{ts_millis}");

    let audit_result = PolicyAuditResult::new(
        &result_name,
        PolicyAuditResultSpec {
            policy_name: policy_name.to_string(),
            cluster_name: None,
            timestamp: timestamp.to_string(),
            health_score,
            total_violations,
            total_pods,
            classification,
            violations,
        },
    );

    audit_api.create(&Default::default(), &audit_result).await?;

    AUDIT_RESULTS_TOTAL.inc();

    info!(
        audit_result = %result_name,
        policy = %policy_name,
        "audit_result_created"
    );

    // Retention: keep last N results per policy
    let existing = audit_api.list(&Default::default()).await?;

    let mut policy_results: Vec<_> = existing
        .items
        .iter()
        .filter(|r| r.spec.policy_name == policy_name)
        .collect();

    policy_results.sort_by(|a, b| a.spec.timestamp.cmp(&b.spec.timestamp));

    if policy_results.len() > AUDIT_RETENTION {
        let to_delete = policy_results.len() - AUDIT_RETENTION;
        for result in policy_results.iter().take(to_delete) {
            let name = result.metadata.name.as_deref().unwrap_or_default();
            if let Err(e) = audit_api.delete(name, &Default::default()).await {
                warn!(error = %e, name = %name, "audit_result_delete_failed");
            }
        }
    }

    Ok(())
}

/* ============================= ERROR POLICY ============================= */

fn error_policy(
    _policy: Arc<DevOpsPolicy>,
    error: &kube::Error,
    _ctx: Arc<ReconcileContext>,
) -> Action {
    RECONCILE_ERRORS.inc();
    warn!(error = %error, "reconcile_error");
    Action::requeue(Duration::from_secs(60))
}

/* ============================= FINALIZER ============================= */

fn has_finalizer(policy: &DevOpsPolicy) -> bool {
    policy
        .metadata
        .finalizers
        .as_ref()
        .is_some_and(|f| f.iter().any(|s| s == FINALIZER))
}

async fn add_finalizer(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<(), kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let api: Api<DevOpsPolicy> = Api::namespaced(client.clone(), &namespace);

    let patch = serde_json::json!({
        "metadata": {
            "finalizers": [FINALIZER]
        }
    });

    api.patch(
        &name,
        &PatchParams::apply("kube-devops-operator"),
        &Patch::Merge(&patch),
    )
    .await?;

    info!(policy = %name, "finalizer_added");
    Ok(())
}

async fn remove_finalizer(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<(), kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();
    let api: Api<DevOpsPolicy> = Api::namespaced(client.clone(), &namespace);

    let patch = serde_json::json!({
        "metadata": {
            "finalizers": []
        }
    });

    api.patch(
        &name,
        &PatchParams::apply("kube-devops-operator"),
        &Patch::Merge(&patch),
    )
    .await?;

    info!(policy = %name, "finalizer_removed");
    Ok(())
}

async fn handle_deletion(
    policy: &DevOpsPolicy,
    client: &Client,
) -> std::result::Result<Action, kube::Error> {
    let name = policy.name_any();
    let namespace = policy.namespace().unwrap_or_default();

    info!(policy = %name, namespace = %namespace, "handling_deletion");

    // Clear Prometheus metrics for this policy
    let _ = POLICY_VIOLATIONS.remove_label_values(&[&namespace, &name]);
    let _ = POLICY_HEALTH.remove_label_values(&[&namespace, &name]);
    let _ = ENFORCEMENT_MODE.remove_label_values(&[&namespace, &name]);

    if has_finalizer(policy) {
        remove_finalizer(policy, client).await?;
    }

    Ok(Action::await_change())
}

/* ============================= HTTP SERVER ============================= */

pub(crate) fn build_reconcile_router(state: Arc<Mutex<ReconcileState>>) -> Router {
    Router::new()
        .route("/metrics", get(reconcile_metrics_handler))
        .route("/healthz", get(|| async { (StatusCode::OK, "OK") }))
        .route(
            "/readyz",
            get({
                let state = state.clone();
                move || reconcile_ready_handler(state.clone())
            }),
        )
}

async fn start_metrics_server(
    state: Arc<Mutex<ReconcileState>>,
    mut shutdown: broadcast::Receiver<()>,
    addr: SocketAddr,
) -> Result<()> {
    let app = build_reconcile_router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind metrics server on :9090")?;

    info!(addr = %addr, "reconcile_metrics_server_started");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        })
        .await?;

    Ok(())
}

async fn reconcile_ready_handler(state: Arc<Mutex<ReconcileState>>) -> impl IntoResponse {
    let state = state.lock().await;
    if state.ready {
        (StatusCode::OK, "READY")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
    }
}

async fn reconcile_metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();

    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => match String::from_utf8(buffer) {
            Ok(body) => (StatusCode::OK, body),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "metrics encoding error".to_string(),
            ),
        },
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "metrics encoding error".to_string(),
        ),
    }
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use k8s_openapi::api::core::v1::{Container, ContainerStatus, PodSpec, PodStatus, Probe};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use kube_devops::crd::DevOpsPolicySpec;
    use tower::ServiceExt;

    fn test_reconcile_state(ready: bool) -> Arc<Mutex<ReconcileState>> {
        Arc::new(Mutex::new(ReconcileState { ready }))
    }

    fn make_test_pod(
        name: &str,
        namespace: &str,
        image: &str,
        has_liveness: bool,
        has_readiness: bool,
        restart_count: i32,
        phase: &str,
    ) -> Pod {
        let probes =
            |has: bool| -> Option<Probe> { if has { Some(Probe::default()) } else { None } };

        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "main".to_string(),
                    image: Some(image.to_string()),
                    liveness_probe: probes(has_liveness),
                    readiness_probe: probes(has_readiness),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some(phase.to_string()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "main".to_string(),
                    restart_count,
                    ready: phase == "Running",
                    image: image.to_string(),
                    image_id: String::new(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn all_enabled_policy() -> DevOpsPolicySpec {
        DevOpsPolicySpec {
            forbid_latest_tag: Some(true),
            require_liveness_probe: Some(true),
            require_readiness_probe: Some(true),
            max_restart_count: Some(3),
            forbid_pending_duration: Some(300),
            ..Default::default()
        }
    }

    // ── Reconcile status computation ──

    #[test]
    fn test_status_healthy_at_80() {
        let score: u32 = 80;
        let healthy = score >= 80;
        assert!(healthy);
    }

    #[test]
    fn test_status_unhealthy_at_79() {
        let score: u32 = 79;
        let healthy = score >= 80;
        assert!(!healthy);
    }

    #[test]
    fn test_aggregate_multiple_pods() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod("a", "prod", "nginx:latest", false, false, 0, "Running"),
            make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
            make_test_pod("c", "prod", "nginx:latest", true, false, 10, "Pending"),
        ];

        let mut aggregate = governance::PodMetrics::default();
        let mut total_violations: u32 = 0;

        for pod in &pods {
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
            let v = governance::detect_violations_with_policy(pod, &policy);
            total_violations += v.len() as u32;
        }

        assert_eq!(aggregate.total_pods, 3);
        assert_eq!(aggregate.latest_tag, 2);
        assert_eq!(aggregate.missing_liveness, 1);
        assert_eq!(aggregate.missing_readiness, 2);
        assert!(aggregate.high_restarts > 0);
        assert_eq!(aggregate.pending, 1);
        assert!(total_violations > 0);
    }

    #[test]
    fn test_aggregate_all_compliant_pods() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod("a", "prod", "nginx:1.25", true, true, 0, "Running"),
            make_test_pod("b", "prod", "redis:7.0", true, true, 0, "Running"),
        ];

        let mut aggregate = governance::PodMetrics::default();
        let mut total_violations: u32 = 0;

        for pod in &pods {
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
            let v = governance::detect_violations_with_policy(pod, &policy);
            total_violations += v.len() as u32;
        }

        assert_eq!(aggregate.total_pods, 2);
        assert_eq!(total_violations, 0);

        let score = governance::calculate_health_score(&aggregate);
        assert_eq!(score, 100);
    }

    #[test]
    fn test_system_namespace_pods_skipped() {
        let policy = all_enabled_policy();

        let pods = vec![
            make_test_pod(
                "a",
                "kube-system",
                "nginx:latest",
                false,
                false,
                0,
                "Running",
            ),
            make_test_pod("b", "prod", "nginx:1.25", true, true, 0, "Running"),
        ];

        let mut aggregate = governance::PodMetrics::default();

        for pod in &pods {
            let ns = pod.metadata.namespace.as_deref().unwrap_or_default();
            if governance::is_system_namespace(ns) {
                continue;
            }
            let contribution = governance::evaluate_pod_with_policy(pod, &policy);
            governance::add_metrics(&mut aggregate, &contribution);
        }

        // Only the "prod" pod should be counted
        assert_eq!(aggregate.total_pods, 1);
        assert_eq!(aggregate.latest_tag, 0);
    }

    #[test]
    fn test_status_message_format() {
        let total_violations: u32 = 5;
        let total_pods: u32 = 10;
        let health_score: u32 = 72;
        let classification = governance::classify_health(health_score);

        let message = format!(
            "{} violations across {} pods — {} ({})",
            total_violations, total_pods, classification, health_score
        );

        assert_eq!(message, "5 violations across 10 pods — Stable (72)");
    }

    #[test]
    fn test_status_fields_populated() {
        let status = DevOpsPolicyStatus {
            observed_generation: Some(3),
            healthy: Some(true),
            health_score: Some(95),
            violations: Some(2),
            last_evaluated: Some("2026-01-01T00:00:00Z".to_string()),
            message: Some("2 violations across 20 pods — Healthy (95)".to_string()),
            remediations_applied: None,
            remediations_failed: None,
            remediated_workloads: None,
        };

        assert_eq!(status.observed_generation, Some(3));
        assert!(status.healthy.unwrap());
        assert_eq!(status.health_score, Some(95));
        assert_eq!(status.violations, Some(2));
        assert!(status.last_evaluated.is_some());
        assert!(status.message.unwrap().contains("Healthy"));
    }

    // ── Finalizer detection ──

    #[test]
    fn test_has_finalizer_when_present() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec![FINALIZER.to_string()]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_when_absent() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: None,
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_with_other_finalizers() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec!["some-other/finalizer".to_string()]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    #[test]
    fn test_has_finalizer_empty_list() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                namespace: Some("default".to_string()),
                finalizers: Some(vec![]),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(!has_finalizer(&policy));
    }

    // ── Deletion detection ──

    #[test]
    fn test_deletion_timestamp_present() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                deletion_timestamp: Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
                    chrono::Utc::now(),
                )),
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(policy.metadata.deletion_timestamp.is_some());
    }

    #[test]
    fn test_deletion_timestamp_absent() {
        let policy = DevOpsPolicy {
            metadata: ObjectMeta {
                name: Some("test".to_string()),
                deletion_timestamp: None,
                ..Default::default()
            },
            spec: all_enabled_policy(),
            status: None,
        };
        assert!(policy.metadata.deletion_timestamp.is_none());
    }

    // ── HTTP endpoint tests ──

    #[tokio::test]
    async fn test_reconcile_healthz_returns_ok() {
        let app = build_reconcile_router(test_reconcile_state(false));
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
    async fn test_reconcile_readyz_when_ready() {
        let app = build_reconcile_router(test_reconcile_state(true));
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
    async fn test_reconcile_readyz_when_not_ready() {
        let app = build_reconcile_router(test_reconcile_state(false));
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
    async fn test_reconcile_metrics_returns_ok() {
        let app = build_reconcile_router(test_reconcile_state(false));
        let req = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_reconcile_unknown_route_returns_404() {
        let app = build_reconcile_router(test_reconcile_state(false));
        let req = Request::builder()
            .uri("/nonexistent")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── New metric registry tests ──

    #[test]
    fn test_pods_scanned_metric_registered() {
        LazyLock::force(&PODS_SCANNED);
        let families = REGISTRY.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        assert!(
            names.contains(&"devopspolicy_pods_scanned_total"),
            "pods_scanned_total should be registered"
        );
    }

    #[test]
    fn test_reconcile_duration_metric_registered() {
        LazyLock::force(&RECONCILE_DURATION);
        let families = REGISTRY.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        assert!(
            names.contains(&"devopspolicy_reconcile_duration_seconds"),
            "reconcile_duration_seconds should be registered"
        );
    }
}
