use std::net::SocketAddr;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use kube::api::ListParams;
use kube::{Api, Client};
use prometheus::{Encoder, Histogram, IntCounterVec, Registry, TextEncoder};
use tokio::sync::broadcast;
use tracing::info;

use k8s_openapi::api::core::v1::Pod;
use kube_devops::admission::{self, AdmissionVerdict};
use kube_devops::crd::DevOpsPolicy;
use kube_devops::governance;

/* ============================= PROMETHEUS ============================= */

static WEBHOOK_REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

static WEBHOOK_REQUESTS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let c = IntCounterVec::new(
        prometheus::Opts::new("webhook_requests_total", "Total admission webhook requests"),
        &["operation", "allowed"],
    )
    .expect("metric definition is valid");
    WEBHOOK_REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static WEBHOOK_DENIALS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let c = IntCounterVec::new(
        prometheus::Opts::new(
            "webhook_denials_total",
            "Total admission webhook denials by namespace and violation",
        ),
        &["namespace", "violation"],
    )
    .expect("metric definition is valid");
    WEBHOOK_REGISTRY
        .register(Box::new(c.clone()))
        .expect("metric not yet registered");
    c
});

static WEBHOOK_DURATION: LazyLock<Histogram> = LazyLock::new(|| {
    let h = Histogram::with_opts(prometheus::HistogramOpts::new(
        "webhook_request_duration_seconds",
        "Duration of admission webhook request processing in seconds",
    ))
    .expect("metric definition is valid");
    WEBHOOK_REGISTRY
        .register(Box::new(h.clone()))
        .expect("metric not yet registered");
    h
});

/* ============================= STATE ============================= */

#[derive(Clone)]
pub(crate) struct WebhookState {
    pub(crate) client: Client,
    pub(crate) ready: bool,
}

/* ============================= ENTRY: SERVE ============================= */

pub async fn serve(addr_str: &str, tls_cert: &str, tls_key: &str) -> Result<()> {
    println!("Starting admission webhook server...\n");
    info!("webhook_starting");

    let client = Client::try_default()
        .await
        .context("Failed to connect to Kubernetes cluster")?;

    print!("  Cluster connection .......... ");
    match client.apiserver_version().await {
        Ok(v) => println!("OK (v{}.{})", v.major, v.minor),
        Err(e) => {
            println!("FAIL");
            anyhow::bail!("Cannot reach cluster: {}. Is the cluster running?", e);
        }
    }

    // Validate TLS certificate and key files exist
    print!("  TLS ......................... ");
    validate_tls_files(tls_cert, tls_key)?;
    println!("loaded ({}, {})", tls_cert, tls_key);

    let addr: SocketAddr = addr_str
        .parse()
        .context("Invalid address format")?;

    println!("  HTTPS server ................ https://{addr}");
    println!();
    println!("  Available endpoints:");
    println!("    POST /validate ............ Admission review handler");
    println!("    GET  /healthz ............. Liveness probe");
    println!("    GET  /readyz .............. Readiness probe");
    println!("    GET  /metrics ............. Prometheus metrics");
    println!();
    println!("Admission webhook running. Press Ctrl+C to stop.\n");
    println!("{}", "=".repeat(70));

    let state = WebhookState {
        client,
        ready: true,
    };

    let tls_cert = tls_cert.to_string();
    let tls_key = tls_key.to_string();

    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let http_shutdown = shutdown_tx.subscribe();

    let http_handle = tokio::spawn(async move {
        start_https_server(state, http_shutdown, addr, &tls_cert, &tls_key).await
    });

    tokio::signal::ctrl_c().await?;
    info!("shutdown_signal_received");
    println!("\n{}", "=".repeat(70));
    println!("Shutdown signal received. Stopping webhook server...");
    println!("{}", "=".repeat(70));

    let _ = shutdown_tx.send(());
    let _ = http_handle.await?;

    info!("webhook_stopped");
    println!("Webhook server stopped.");
    Ok(())
}

/* ============================= TLS ============================= */

fn validate_tls_files(cert_path: &str, key_path: &str) -> Result<()> {
    if !std::path::Path::new(cert_path).exists() {
        anyhow::bail!("TLS certificate file not found: {}", cert_path);
    }
    if !std::path::Path::new(key_path).exists() {
        anyhow::bail!("TLS key file not found: {}", key_path);
    }
    Ok(())
}

/* ============================= HTTPS SERVER ============================= */

pub(crate) fn build_webhook_router(state: WebhookState) -> Router {
    Router::new()
        .route("/validate", post(admission_handler))
        .route("/healthz", get(|| async { (StatusCode::OK, "OK") }))
        .route(
            "/readyz",
            get({
                let state = state.clone();
                move || ready_handler(state.clone())
            }),
        )
        .route("/metrics", get(webhook_metrics_handler))
        .with_state(state)
}

async fn start_https_server(
    state: WebhookState,
    mut shutdown: broadcast::Receiver<()>,
    addr: SocketAddr,
    tls_cert: &str,
    tls_key: &str,
) -> Result<()> {
    let app = build_webhook_router(state);

    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(tls_cert, tls_key)
        .await
        .context("Failed to load TLS configuration")?;

    info!(addr = %addr, "https_server_started");

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    tokio::spawn(async move {
        let _ = shutdown.recv().await;
        shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    });

    axum_server::bind_rustls(addr, rustls_config)
        .handle(handle)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn ready_handler(state: WebhookState) -> impl IntoResponse {
    if state.ready {
        (StatusCode::OK, "READY")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
    }
}

async fn webhook_metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = WEBHOOK_REGISTRY.gather();
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

/* ============================= ADMISSION HANDLER ============================= */

async fn admission_handler(
    State(state): State<WebhookState>,
    body: String,
) -> impl IntoResponse {
    let _timer = WEBHOOK_DURATION.start_timer();

    let review: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            info!(error = %e, "invalid_admission_review");
            return (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "apiVersion": "admission.k8s.io/v1",
                    "kind": "AdmissionReview",
                    "response": {
                        "uid": "",
                        "allowed": true
                    }
                })
                .to_string(),
            );
        }
    };

    let uid = review["request"]["uid"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let operation = review["request"]["operation"]
        .as_str()
        .unwrap_or("UNKNOWN")
        .to_string();
    let namespace = review["request"]["namespace"]
        .as_str()
        .unwrap_or("default")
        .to_string();

    // System namespace bypass
    if governance::is_system_namespace(&namespace) {
        info!(namespace = %namespace, "system_namespace_bypass");
        WEBHOOK_REQUESTS
            .with_label_values(&[&operation, "true"])
            .inc();
        return (
            StatusCode::OK,
            build_admission_response(&uid, true, None),
        );
    }

    // Extract pod from the admission request
    let pod: Pod = match serde_json::from_value(review["request"]["object"].clone()) {
        Ok(p) => p,
        Err(e) => {
            info!(error = %e, "failed_to_parse_pod");
            // Fail-open: if we can't parse the pod, allow it
            WEBHOOK_REQUESTS
                .with_label_values(&[&operation, "true"])
                .inc();
            return (
                StatusCode::OK,
                build_admission_response(&uid, true, None),
            );
        }
    };

    // Look up DevOpsPolicy for the namespace
    let verdict = match lookup_policy_and_validate(&state.client, &namespace, &pod).await {
        Ok(v) => v,
        Err(e) => {
            // Fail-open: if we can't look up the policy, allow the request
            info!(error = %e, namespace = %namespace, "policy_lookup_failed_failopen");
            WEBHOOK_REQUESTS
                .with_label_values(&[&operation, "true"])
                .inc();
            return (
                StatusCode::OK,
                build_admission_response(&uid, true, None),
            );
        }
    };

    let allowed_str = if verdict.allowed { "true" } else { "false" };
    WEBHOOK_REQUESTS
        .with_label_values(&[&operation, allowed_str])
        .inc();

    if !verdict.allowed {
        for violation in &verdict.violations {
            // Extract a short violation type label
            let violation_type = if violation.contains(":latest") {
                "latest_tag"
            } else if violation.contains("liveness") {
                "missing_liveness"
            } else if violation.contains("readiness") {
                "missing_readiness"
            } else {
                "unknown"
            };
            WEBHOOK_DENIALS
                .with_label_values(&[&namespace, violation_type])
                .inc();
        }
        info!(
            namespace = %namespace,
            violations = ?verdict.violations,
            "admission_denied"
        );
    }

    (
        StatusCode::OK,
        build_admission_response(&uid, verdict.allowed, verdict.message.as_deref()),
    )
}

async fn lookup_policy_and_validate(
    client: &Client,
    namespace: &str,
    pod: &Pod,
) -> Result<AdmissionVerdict> {
    let policies: Api<DevOpsPolicy> = Api::namespaced(client.clone(), namespace);
    let policy_list = policies.list(&ListParams::default()).await?;

    if policy_list.items.is_empty() {
        // No policy → allow (fail-open)
        return Ok(AdmissionVerdict {
            allowed: true,
            message: None,
            violations: Vec::new(),
        });
    }

    // Use the first policy in the namespace
    let policy = &policy_list.items[0];
    Ok(admission::validate_pod_admission(pod, &policy.spec))
}

fn build_admission_response(uid: &str, allowed: bool, message: Option<&str>) -> String {
    let mut response = serde_json::json!({
        "apiVersion": "admission.k8s.io/v1",
        "kind": "AdmissionReview",
        "response": {
            "uid": uid,
            "allowed": allowed
        }
    });

    if let Some(msg) = message {
        response["response"]["status"] = serde_json::json!({
            "message": msg
        });
    }

    response.to_string()
}

/* ============================= CERT GENERATION ============================= */

pub fn generate_certs(service_name: &str, namespace: &str, output_dir: &str, ip_sans: &[String]) -> Result<()> {
    println!("Generating self-signed TLS certificates...\n");

    let (ca_pem, cert_pem, key_pem) = generate_self_signed_certs(service_name, namespace, ip_sans)?;

    let output_path = std::path::Path::new(output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path)
            .context("Failed to create output directory")?;
    }

    let ca_path = output_path.join("ca.crt");
    let cert_path = output_path.join("tls.crt");
    let key_path = output_path.join("tls.key");

    std::fs::write(&ca_path, &ca_pem).context("Failed to write ca.crt")?;
    std::fs::write(&cert_path, &cert_pem).context("Failed to write tls.crt")?;
    std::fs::write(&key_path, &key_pem).context("Failed to write tls.key")?;

    println!("  CA certificate .............. {}", ca_path.display());
    println!("  Server certificate .......... {}", cert_path.display());
    println!("  Server key .................. {}", key_path.display());
    println!();
    println!("  Service name ................ {service_name}");
    println!("  Namespace ................... {namespace}");
    println!("  SANs:");
    println!("    - {service_name}.{namespace}.svc");
    println!("    - {service_name}.{namespace}.svc.cluster.local");
    for ip in ip_sans {
        println!("    - {ip} (IP)");
    }
    println!();
    println!("TLS certificates generated successfully.");

    Ok(())
}

pub fn generate_self_signed_certs(
    service_name: &str,
    namespace: &str,
    ip_sans: &[String],
) -> Result<(String, String, String)> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
    use std::net::IpAddr;

    // Generate CA key pair and certificate
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "kube-devops-webhook-ca");
    ca_dn.push(DnType::OrganizationName, "kube-devops");
    ca_params.distinguished_name = ca_dn;

    let ca_key = KeyPair::generate().context("Failed to generate CA key pair")?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .context("Failed to self-sign CA certificate")?;

    // Generate server key pair and certificate signed by the CA
    let mut server_params = CertificateParams::default();
    let mut server_dn = DistinguishedName::new();
    server_dn.push(
        DnType::CommonName,
        format!("{service_name}.{namespace}.svc"),
    );
    server_params.distinguished_name = server_dn;

    let mut sans = vec![
        SanType::DnsName(
            format!("{service_name}.{namespace}.svc")
                .try_into()
                .context("Invalid DNS name for SAN")?,
        ),
        SanType::DnsName(
            format!("{service_name}.{namespace}.svc.cluster.local")
                .try_into()
                .context("Invalid DNS name for SAN")?,
        ),
    ];

    for ip_str in ip_sans {
        let ip: IpAddr = ip_str
            .parse()
            .context(format!("Invalid IP address for SAN: {ip_str}"))?;
        sans.push(SanType::IpAddress(ip));
    }

    server_params.subject_alt_names = sans;

    let server_key = KeyPair::generate().context("Failed to generate server key pair")?;
    let server_cert = server_params
        .signed_by(&server_key, &ca_cert, &ca_key)
        .context("Failed to sign server certificate")?;

    let ca_pem = ca_cert.pem();
    let cert_pem = server_cert.pem();
    let key_pem = server_key.serialize_pem();

    Ok((ca_pem, cert_pem, key_pem))
}

/* ============================= INSTALL CONFIG ============================= */

pub fn install_config(service_name: &str, namespace: &str, ca_bundle_path: &str) -> Result<()> {
    use base64::Engine;

    let ca_bytes =
        std::fs::read(ca_bundle_path).context("Failed to read CA bundle file")?;
    let ca_b64 = base64::engine::general_purpose::STANDARD.encode(&ca_bytes);

    let yaml = format!(
        r#"apiVersion: admissionregistration.k8s.io/v1
kind: ValidatingWebhookConfiguration
metadata:
  name: {service_name}
webhooks:
  - name: validate.devops.stochastic.io
    rules:
      - apiGroups: [""]
        resources: ["pods"]
        apiVersions: ["v1"]
        operations: ["CREATE", "UPDATE"]
    clientConfig:
      service:
        name: {service_name}
        namespace: {namespace}
        path: /validate
      caBundle: {ca_b64}
    failurePolicy: Ignore
    sideEffects: None
    admissionReviewVersions: ["v1"]
    namespaceSelector:
      matchExpressions:
        - key: kubernetes.io/metadata.name
          operator: NotIn
          values: ["kube-system", "kube-public", "kube-node-lease"]
"#
    );

    println!("{yaml}");
    Ok(())
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_admission_response_allowed() {
        let resp = build_admission_response("test-uid-123", true, None);
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["response"]["uid"], "test-uid-123");
        assert_eq!(v["response"]["allowed"], true);
        assert!(v["response"]["status"].is_null());
    }

    #[test]
    fn test_build_admission_response_denied() {
        let resp = build_admission_response(
            "test-uid-456",
            false,
            Some("container 'nginx' uses :latest tag"),
        );
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["response"]["uid"], "test-uid-456");
        assert_eq!(v["response"]["allowed"], false);
        assert_eq!(
            v["response"]["status"]["message"],
            "container 'nginx' uses :latest tag"
        );
    }

    #[test]
    fn test_build_admission_response_preserves_uid() {
        let uid = "550e8400-e29b-41d4-a716-446655440000";
        let resp = build_admission_response(uid, true, None);
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["response"]["uid"], uid);
        assert_eq!(v["apiVersion"], "admission.k8s.io/v1");
        assert_eq!(v["kind"], "AdmissionReview");
    }

    #[test]
    fn test_generate_self_signed_certs() {
        let (ca_pem, cert_pem, key_pem) =
            generate_self_signed_certs("my-webhook", "production", &[]).unwrap();

        assert!(ca_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));

        // CA and server cert should be different
        assert_ne!(ca_pem, cert_pem);
    }

    #[test]
    fn test_generate_certs_writes_files() {
        let temp_dir = std::env::temp_dir().join("kube-devops-test-certgen");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = generate_certs("test-svc", "test-ns", temp_dir.to_str().unwrap(), &[]);
        assert!(result.is_ok());

        assert!(temp_dir.join("ca.crt").exists());
        assert!(temp_dir.join("tls.crt").exists());
        assert!(temp_dir.join("tls.key").exists());

        // Verify PEM content
        let ca = std::fs::read_to_string(temp_dir.join("ca.crt")).unwrap();
        let cert = std::fs::read_to_string(temp_dir.join("tls.crt")).unwrap();
        let key = std::fs::read_to_string(temp_dir.join("tls.key")).unwrap();
        assert!(ca.contains("BEGIN CERTIFICATE"));
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("BEGIN PRIVATE KEY"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_install_config_output() {
        let temp_dir = std::env::temp_dir().join("kube-devops-test-webhook");
        let _ = std::fs::create_dir_all(&temp_dir);
        let ca_path = temp_dir.join("test-ca.crt");
        std::fs::write(&ca_path, "FAKE-CA-CERT").unwrap();

        let result = install_config("test-webhook", "test-ns", ca_path.to_str().unwrap());
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_validate_tls_files_missing_cert() {
        let result = validate_tls_files("/nonexistent/cert.pem", "/nonexistent/key.pem");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("certificate file not found")
        );
    }

    #[test]
    fn test_validate_tls_files_missing_key() {
        let temp_dir = std::env::temp_dir().join("kube-devops-test-tls-validate");
        let _ = std::fs::create_dir_all(&temp_dir);
        let cert_path = temp_dir.join("cert.pem");
        std::fs::write(&cert_path, "CERT").unwrap();

        let result = validate_tls_files(cert_path.to_str().unwrap(), "/nonexistent/key.pem");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("key file not found"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_webhook_duration_metric_registered() {
        LazyLock::force(&WEBHOOK_DURATION);
        let families = WEBHOOK_REGISTRY.gather();
        let names: Vec<&str> = families.iter().map(|f| f.get_name()).collect();
        assert!(
            names.contains(&"webhook_request_duration_seconds"),
            "webhook_request_duration_seconds should be registered"
        );
    }
}
