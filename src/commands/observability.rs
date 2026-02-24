/* ============================= SERVICE GENERATORS ============================= */

const NAMESPACE: &str = "kube-devops";
const APP_NAME: &str = "kube-devops";

pub fn generate_service(component: &str, port: u16) -> String {
    format!(
        r#"apiVersion: v1
kind: Service
metadata:
  name: {APP_NAME}-{component}
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
    app.kubernetes.io/component: {component}
spec:
  selector:
    app.kubernetes.io/name: {APP_NAME}
    app.kubernetes.io/component: {component}
  ports:
    - name: metrics
      port: {port}
      targetPort: {port}
      protocol: TCP
"#
    )
}

pub fn generate_service_watch() -> String {
    generate_service("watch", 8080)
}

pub fn generate_service_reconcile() -> String {
    generate_service("reconcile", 9090)
}

pub fn generate_service_webhook() -> String {
    generate_service("webhook", 8443)
}

/* ============================= SERVICEMONITOR GENERATORS ============================= */

pub fn generate_service_monitor(component: &str, port: u16) -> String {
    let scheme = if port == 8443 { "https" } else { "http" };

    let mut yaml = format!(
        r#"apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: {APP_NAME}-{component}
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
    app.kubernetes.io/component: {component}
    release: stable
spec:
  selector:
    matchLabels:
      app.kubernetes.io/name: {APP_NAME}
      app.kubernetes.io/component: {component}
  endpoints:
    - port: metrics
      path: /metrics
      interval: 15s
      scheme: {scheme}
"#
    );

    if port == 8443 {
        yaml.push_str(
            "      tlsConfig:\n        insecureSkipVerify: true\n",
        );
    }

    yaml
}

pub fn generate_service_monitor_watch() -> String {
    generate_service_monitor("watch", 8080)
}

pub fn generate_service_monitor_reconcile() -> String {
    generate_service_monitor("reconcile", 9090)
}

pub fn generate_service_monitor_webhook() -> String {
    generate_service_monitor("webhook", 8443)
}

/* ============================= GRAFANA DASHBOARD ============================= */

pub fn generate_grafana_dashboard_configmap() -> String {
    let dashboard = build_dashboard_json();
    let dashboard_str = serde_json::to_string_pretty(&dashboard).expect("dashboard JSON is valid");

    // Escape for YAML embedding (indent every line by 4 spaces)
    let indented: String = dashboard_str
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {APP_NAME}-grafana-dashboard
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
    grafana_dashboard: "1"
data:
  kube-devops.json: |
{indented}
"#
    )
}

fn build_dashboard_json() -> serde_json::Value {
    serde_json::json!({
        "annotations": { "list": [] },
        "editable": true,
        "fiscalYearStartMonth": 0,
        "graphTooltip": 1,
        "id": null,
        "links": [],
        "panels": [
            // ── Row 1: Overview ──
            row_panel(0, "Overview"),
            stat_panel(1, "Cluster Health Score", "cluster_health_score", 0),
            graph_panel(2, "Reconcile Cycles", "rate(devopspolicy_reconcile_total[5m])", 0),
            graph_panel(3, "Webhook Requests", "rate(webhook_requests_total[5m])", 0),

            // ── Row 2: Watch ──
            row_panel(4, "Watch"),
            graph_panel(5, "Namespace Health Scores", "namespace_health_score", 1),
            graph_panel(6, "Pod Events Rate", "rate(pod_events_total[5m])", 1),
            stat_panel(7, "Pods Tracked", "pods_tracked_total", 1),

            // ── Row 3: Reconcile ──
            row_panel(8, "Reconcile"),
            graph_panel(9, "Violations by Namespace", "devopspolicy_violations_total", 2),
            graph_panel(10, "Health Scores", "devopspolicy_health_score", 2),
            graph_panel(11, "Reconcile Rate", "rate(devopspolicy_reconcile_total[5m])", 2),
            graph_panel(12, "Reconcile Errors", "rate(devopspolicy_reconcile_errors_total[5m])", 2),
            graph_panel(13, "Reconcile Duration", "histogram_quantile(0.99, rate(devopspolicy_reconcile_duration_seconds_bucket[5m]))", 2),
            stat_panel(14, "Pods Scanned", "devopspolicy_pods_scanned_total", 2),
            graph_panel(15, "Remediations Applied", "rate(devopspolicy_remediations_applied_total[5m])", 2),
            graph_panel(16, "Remediations Failed", "rate(devopspolicy_remediations_failed_total[5m])", 2),
            stat_panel(17, "Enforcement Mode", "devopspolicy_enforcement_mode", 2),

            // ── Row 4: Webhook ──
            row_panel(18, "Webhook"),
            graph_panel(19, "Allow/Deny Rate", "rate(webhook_requests_total[5m])", 3),
            graph_panel(20, "Denial Breakdown", "rate(webhook_denials_total[5m])", 3),
            graph_panel(21, "Request Latency", "histogram_quantile(0.99, rate(webhook_request_duration_seconds_bucket[5m]))", 3),

            // ── Row 5: Severity & Audit ──
            row_panel(22, "Severity & Audit"),
            graph_panel(23, "Violations by Severity", "sum by (severity) (devopspolicy_violations_by_severity)", 4),
            graph_panel(24, "Audit Results Over Time", "devopspolicy_audit_results_total", 4),
            stat_panel(25, "Critical Violations", "sum(devopspolicy_violations_by_severity{severity=\"critical\"})", 4),
        ],
        "schemaVersion": 39,
        "tags": ["kubernetes", "kube-devops"],
        "templating": { "list": [] },
        "time": { "from": "now-1h", "to": "now" },
        "title": "kube-devops Observability",
        "uid": "kube-devops-overview",
        "version": 1
    })
}

fn row_panel(id: u32, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "row",
        "title": title,
        "collapsed": false,
        "panels": []
    })
}

fn stat_panel(id: u32, title: &str, expr: &str, _row: u32) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "stat",
        "title": title,
        "targets": [{
            "expr": expr,
            "refId": "A"
        }],
        "fieldConfig": {
            "defaults": {
                "thresholds": {
                    "steps": [
                        { "color": "green", "value": null },
                        { "color": "red", "value": 80 }
                    ]
                }
            }
        }
    })
}

fn graph_panel(id: u32, title: &str, expr: &str, _row: u32) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "timeseries",
        "title": title,
        "targets": [{
            "expr": expr,
            "refId": "A"
        }],
        "fieldConfig": {
            "defaults": {}
        }
    })
}

/* ============================= GENERATE ALL ============================= */

pub fn generate_all() -> String {
    let mut output = String::new();

    output.push_str(&generate_service_watch());
    output.push_str("---\n");
    output.push_str(&generate_service_reconcile());
    output.push_str("---\n");
    output.push_str(&generate_service_webhook());
    output.push_str("---\n");
    output.push_str(&generate_service_monitor_watch());
    output.push_str("---\n");
    output.push_str(&generate_service_monitor_reconcile());
    output.push_str("---\n");
    output.push_str(&generate_service_monitor_webhook());
    output.push_str("---\n");
    output.push_str(&generate_grafana_dashboard_configmap());

    output
}

pub fn generate_service_monitors() -> String {
    let mut output = String::new();

    output.push_str(&generate_service_monitor_watch());
    output.push_str("---\n");
    output.push_str(&generate_service_monitor_reconcile());
    output.push_str("---\n");
    output.push_str(&generate_service_monitor_webhook());

    output
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    // ── Service tests ──

    #[test]
    fn test_service_watch_fields() {
        let yaml = generate_service_watch();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Service");
        assert_eq!(doc["metadata"]["name"], "kube-devops-watch");
        assert_eq!(doc["metadata"]["namespace"], "kube-devops");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/component"], "watch");
        assert_eq!(doc["spec"]["ports"][0]["port"], 8080);
    }

    #[test]
    fn test_service_reconcile_fields() {
        let yaml = generate_service_reconcile();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Service");
        assert_eq!(doc["metadata"]["name"], "kube-devops-reconcile");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/component"], "reconcile");
        assert_eq!(doc["spec"]["ports"][0]["port"], 9090);
    }

    #[test]
    fn test_service_webhook_fields() {
        let yaml = generate_service_webhook();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Service");
        assert_eq!(doc["metadata"]["name"], "kube-devops-webhook");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/component"], "webhook");
        assert_eq!(doc["spec"]["ports"][0]["port"], 8443);
    }

    // ── ServiceMonitor tests ──

    #[test]
    fn test_service_monitor_watch_fields() {
        let yaml = generate_service_monitor_watch();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ServiceMonitor");
        assert_eq!(doc["metadata"]["name"], "kube-devops-watch");
        assert_eq!(doc["spec"]["endpoints"][0]["path"], "/metrics");
        assert_eq!(doc["spec"]["endpoints"][0]["interval"], "15s");
        assert_eq!(doc["spec"]["endpoints"][0]["scheme"], "http");
        assert_eq!(
            doc["spec"]["selector"]["matchLabels"]["app.kubernetes.io/component"],
            "watch"
        );
    }

    #[test]
    fn test_service_monitor_reconcile_fields() {
        let yaml = generate_service_monitor_reconcile();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ServiceMonitor");
        assert_eq!(doc["metadata"]["name"], "kube-devops-reconcile");
        assert_eq!(doc["spec"]["endpoints"][0]["scheme"], "http");
    }

    #[test]
    fn test_service_monitor_webhook_uses_https() {
        let yaml = generate_service_monitor_webhook();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ServiceMonitor");
        assert_eq!(doc["metadata"]["name"], "kube-devops-webhook");
        assert_eq!(doc["spec"]["endpoints"][0]["scheme"], "https");
        assert_eq!(
            doc["spec"]["endpoints"][0]["tlsConfig"]["insecureSkipVerify"],
            true
        );
    }

    #[test]
    fn test_all_services_parseable_yaml() {
        for yaml in [
            generate_service_watch(),
            generate_service_reconcile(),
            generate_service_webhook(),
        ] {
            let _: serde_yaml::Value = serde_yaml::from_str(&yaml)
                .expect("service YAML should be parseable");
        }
    }

    #[test]
    fn test_all_service_monitors_parseable_yaml() {
        for yaml in [
            generate_service_monitor_watch(),
            generate_service_monitor_reconcile(),
            generate_service_monitor_webhook(),
        ] {
            let _: serde_yaml::Value = serde_yaml::from_str(&yaml)
                .expect("ServiceMonitor YAML should be parseable");
        }
    }

    // ── Grafana dashboard tests ──

    #[test]
    fn test_dashboard_configmap_valid_json() {
        let yaml = generate_grafana_dashboard_configmap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        let dashboard_json_str = doc["data"]["kube-devops.json"]
            .as_str()
            .expect("dashboard JSON should be a string");

        let _dashboard: serde_json::Value = serde_json::from_str(dashboard_json_str)
            .expect("embedded dashboard should be valid JSON");
    }

    #[test]
    fn test_dashboard_has_panels() {
        let dashboard = build_dashboard_json();
        let panels = dashboard["panels"].as_array().expect("panels should be an array");
        assert!(panels.len() >= 20, "dashboard should have at least 20 panels");
    }

    #[test]
    fn test_dashboard_configmap_has_grafana_label() {
        let yaml = generate_grafana_dashboard_configmap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ConfigMap");
        assert_eq!(doc["metadata"]["labels"]["grafana_dashboard"], "1");
    }

    #[test]
    fn test_dashboard_references_all_metrics() {
        let dashboard = build_dashboard_json();
        let dashboard_str = serde_json::to_string(&dashboard).expect("valid JSON");

        let expected_metrics = [
            "cluster_health_score",
            "devopspolicy_reconcile_total",
            "webhook_requests_total",
            "namespace_health_score",
            "pod_events_total",
            "pods_tracked_total",
            "devopspolicy_violations_total",
            "devopspolicy_health_score",
            "devopspolicy_reconcile_errors_total",
            "devopspolicy_reconcile_duration_seconds",
            "devopspolicy_pods_scanned_total",
            "devopspolicy_remediations_applied_total",
            "devopspolicy_remediations_failed_total",
            "devopspolicy_enforcement_mode",
            "webhook_denials_total",
            "webhook_request_duration_seconds",
            "devopspolicy_violations_by_severity",
            "devopspolicy_audit_results_total",
        ];

        for metric in &expected_metrics {
            assert!(
                dashboard_str.contains(metric),
                "dashboard should reference metric: {metric}"
            );
        }
    }
}
