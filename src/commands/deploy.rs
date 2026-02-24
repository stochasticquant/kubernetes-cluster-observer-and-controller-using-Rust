/* ============================= CONSTANTS ============================= */

const NAMESPACE: &str = "kube-devops";
const APP_NAME: &str = "kube-devops";
const IMAGE: &str = "192.168.1.68:5000/kube-devops:v0.1.2";

/* ============================= NAMESPACE ============================= */

pub fn generate_namespace() -> String {
    format!(
        r#"apiVersion: v1
kind: Namespace
metadata:
  name: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
"#
    )
}

/* ============================= RBAC ============================= */

pub fn generate_service_account() -> String {
    format!(
        r#"apiVersion: v1
kind: ServiceAccount
metadata:
  name: {APP_NAME}
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
"#
    )
}

pub fn generate_cluster_role() -> String {
    format!(
        r#"apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: {APP_NAME}
  labels:
    app.kubernetes.io/name: {APP_NAME}
rules:
  - apiGroups: ["devops.stochastic.io"]
    resources: ["devopspolicies"]
    verbs: ["get", "list", "watch"]
  - apiGroups: ["devops.stochastic.io"]
    resources: ["devopspolicies/status"]
    verbs: ["patch"]
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list", "watch"]
  - apiGroups: ["apps"]
    resources: ["deployments", "statefulsets", "daemonsets"]
    verbs: ["get", "list", "patch"]
  - apiGroups: ["coordination.k8s.io"]
    resources: ["leases"]
    verbs: ["get", "create", "update", "patch"]
  - apiGroups: ["admissionregistration.k8s.io"]
    resources: ["validatingwebhookconfigurations"]
    verbs: ["get", "list", "create", "update"]
"#
    )
}

pub fn generate_cluster_role_binding() -> String {
    format!(
        r#"apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: {APP_NAME}
  labels:
    app.kubernetes.io/name: {APP_NAME}
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: {APP_NAME}
subjects:
  - kind: ServiceAccount
    name: {APP_NAME}
    namespace: {NAMESPACE}
"#
    )
}

/* ============================= DEPLOYMENT HELPER ============================= */

pub fn generate_deployment(
    component: &str,
    port: u16,
    args: &[&str],
    volume_mounts: &str,
    volumes: &str,
    probe_scheme: &str,
) -> String {
    let args_yaml: String = args.iter().map(|a| format!("            - \"{a}\"\n")).collect();

    let probe_path = "/healthz";
    let readiness_path = "/readyz";

    let volume_mounts_section = if volume_mounts.is_empty() {
        String::new()
    } else {
        format!("          volumeMounts:\n{volume_mounts}")
    };

    let volumes_section = if volumes.is_empty() {
        String::new()
    } else {
        format!("      volumes:\n{volumes}")
    };

    format!(
        r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {APP_NAME}-{component}
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
    app.kubernetes.io/component: {component}
spec:
  replicas: 2
  selector:
    matchLabels:
      app.kubernetes.io/name: {APP_NAME}
      app.kubernetes.io/component: {component}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {APP_NAME}
        app.kubernetes.io/component: {component}
    spec:
      serviceAccountName: {APP_NAME}
      containers:
        - name: {APP_NAME}
          image: {IMAGE}
          imagePullPolicy: IfNotPresent
          args:
{args_yaml}          ports:
            - containerPort: {port}
              protocol: TCP
          livenessProbe:
            httpGet:
              path: {probe_path}
              port: {port}
              scheme: {probe_scheme}
            initialDelaySeconds: 5
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: {readiness_path}
              port: {port}
              scheme: {probe_scheme}
            initialDelaySeconds: 3
            periodSeconds: 5
          resources:
            requests:
              memory: "64Mi"
              cpu: "100m"
            limits:
              memory: "128Mi"
              cpu: "250m"
          securityContext:
            runAsNonRoot: true
            readOnlyRootFilesystem: true
{volume_mounts_section}{volumes_section}"#
    )
}

/* ============================= DEPLOYMENTS ============================= */

pub fn generate_deployment_watch() -> String {
    generate_deployment("watch", 8080, &["watch"], "", "", "HTTP")
}

pub fn generate_deployment_reconcile() -> String {
    generate_deployment("reconcile", 9090, &["reconcile"], "", "", "HTTP")
}

pub fn generate_deployment_webhook() -> String {
    let volume_mounts = "            - name: tls-certs\n              mountPath: /tls\n              readOnly: true\n";
    let volumes = "        - name: tls-certs\n          secret:\n            secretName: kube-devops-webhook-tls\n";
    generate_deployment("webhook", 8443, &["webhook", "serve", "--tls-cert", "/tls/tls.crt", "--tls-key", "/tls/tls.key"], volume_mounts, volumes, "HTTPS")
}

/* ============================= PDB HELPER ============================= */

pub fn generate_pdb(component: &str) -> String {
    format!(
        r#"apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {APP_NAME}-{component}
  namespace: {NAMESPACE}
  labels:
    app.kubernetes.io/name: {APP_NAME}
    app.kubernetes.io/component: {component}
spec:
  minAvailable: 1
  selector:
    matchLabels:
      app.kubernetes.io/name: {APP_NAME}
      app.kubernetes.io/component: {component}
"#
    )
}

/* ============================= PDBs ============================= */

pub fn generate_pdb_watch() -> String {
    generate_pdb("watch")
}

pub fn generate_pdb_reconcile() -> String {
    generate_pdb("reconcile")
}

pub fn generate_pdb_webhook() -> String {
    generate_pdb("webhook")
}

/* ============================= AGGREGATORS ============================= */

pub fn generate_all() -> String {
    let parts = [
        generate_namespace(),
        generate_service_account(),
        generate_cluster_role(),
        generate_cluster_role_binding(),
        generate_deployment_watch(),
        generate_deployment_reconcile(),
        generate_deployment_webhook(),
        generate_pdb_watch(),
        generate_pdb_reconcile(),
        generate_pdb_webhook(),
    ];
    parts.join("---\n")
}

pub fn generate_rbac() -> String {
    let parts = [
        generate_service_account(),
        generate_cluster_role(),
        generate_cluster_role_binding(),
    ];
    parts.join("---\n")
}

pub fn generate_deployments() -> String {
    let parts = [
        generate_deployment_watch(),
        generate_deployment_reconcile(),
        generate_deployment_webhook(),
    ];
    parts.join("---\n")
}

/* ============================= TESTS ============================= */

#[cfg(test)]
mod tests {
    use super::*;

    // ── RBAC tests ──

    #[test]
    fn test_service_account_fields() {
        let yaml = generate_service_account();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ServiceAccount");
        assert_eq!(doc["metadata"]["name"], "kube-devops");
        assert_eq!(doc["metadata"]["namespace"], "kube-devops");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
    }

    #[test]
    fn test_cluster_role_rules_count() {
        let yaml = generate_cluster_role();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ClusterRole");
        let rules = doc["rules"].as_sequence().expect("rules should be a sequence");
        assert_eq!(rules.len(), 6, "ClusterRole should have 6 rules");
    }

    #[test]
    fn test_cluster_role_binding_references() {
        let yaml = generate_cluster_role_binding();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "ClusterRoleBinding");
        assert_eq!(doc["roleRef"]["kind"], "ClusterRole");
        assert_eq!(doc["roleRef"]["name"], "kube-devops");
        assert_eq!(doc["subjects"][0]["kind"], "ServiceAccount");
        assert_eq!(doc["subjects"][0]["name"], "kube-devops");
        assert_eq!(doc["subjects"][0]["namespace"], "kube-devops");
    }

    // ── Deployment field tests ──

    #[test]
    fn test_deployment_watch_fields() {
        let yaml = generate_deployment_watch();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Deployment");
        assert_eq!(doc["metadata"]["name"], "kube-devops-watch");
        assert_eq!(doc["spec"]["replicas"], 2);
        let container = &doc["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["image"], IMAGE);
        assert_eq!(container["ports"][0]["containerPort"], 8080);
        assert_eq!(container["livenessProbe"]["httpGet"]["path"], "/healthz");
        assert_eq!(container["readinessProbe"]["httpGet"]["path"], "/readyz");
    }

    #[test]
    fn test_deployment_reconcile_fields() {
        let yaml = generate_deployment_reconcile();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Deployment");
        assert_eq!(doc["metadata"]["name"], "kube-devops-reconcile");
        assert_eq!(doc["spec"]["replicas"], 2);
        let container = &doc["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["ports"][0]["containerPort"], 9090);
        assert_eq!(container["args"][0], "reconcile");
    }

    #[test]
    fn test_deployment_webhook_fields() {
        let yaml = generate_deployment_webhook();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Deployment");
        assert_eq!(doc["metadata"]["name"], "kube-devops-webhook");
        assert_eq!(doc["spec"]["replicas"], 2);
        let container = &doc["spec"]["template"]["spec"]["containers"][0];
        assert_eq!(container["ports"][0]["containerPort"], 8443);
        assert_eq!(container["args"][0], "webhook");
        assert_eq!(container["args"][1], "serve");
        // Webhook should have TLS volume mount
        assert_eq!(container["volumeMounts"][0]["name"], "tls-certs");
    }

    // ── PDB field tests ──

    #[test]
    fn test_pdb_watch_fields() {
        let yaml = generate_pdb_watch();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "PodDisruptionBudget");
        assert_eq!(doc["metadata"]["name"], "kube-devops-watch");
        assert_eq!(doc["spec"]["minAvailable"], 1);
        assert_eq!(
            doc["spec"]["selector"]["matchLabels"]["app.kubernetes.io/component"],
            "watch"
        );
    }

    #[test]
    fn test_pdb_reconcile_fields() {
        let yaml = generate_pdb_reconcile();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "PodDisruptionBudget");
        assert_eq!(doc["metadata"]["name"], "kube-devops-reconcile");
        assert_eq!(doc["spec"]["minAvailable"], 1);
        assert_eq!(
            doc["spec"]["selector"]["matchLabels"]["app.kubernetes.io/component"],
            "reconcile"
        );
    }

    #[test]
    fn test_pdb_webhook_fields() {
        let yaml = generate_pdb_webhook();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "PodDisruptionBudget");
        assert_eq!(doc["metadata"]["name"], "kube-devops-webhook");
        assert_eq!(doc["spec"]["minAvailable"], 1);
        assert_eq!(
            doc["spec"]["selector"]["matchLabels"]["app.kubernetes.io/component"],
            "webhook"
        );
    }

    // ── Namespace test ──

    #[test]
    fn test_namespace_fields() {
        let yaml = generate_namespace();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");

        assert_eq!(doc["kind"], "Namespace");
        assert_eq!(doc["metadata"]["name"], "kube-devops");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
    }

    // ── YAML parsability tests ──

    #[test]
    fn test_all_deployments_parseable_yaml() {
        for yaml in [
            generate_deployment_watch(),
            generate_deployment_reconcile(),
            generate_deployment_webhook(),
        ] {
            let _: serde_yaml::Value =
                serde_yaml::from_str(&yaml).expect("deployment YAML should be parseable");
        }
    }

    #[test]
    fn test_all_pdbs_parseable_yaml() {
        for yaml in [
            generate_pdb_watch(),
            generate_pdb_reconcile(),
            generate_pdb_webhook(),
        ] {
            let _: serde_yaml::Value =
                serde_yaml::from_str(&yaml).expect("PDB YAML should be parseable");
        }
    }

    #[test]
    fn test_all_rbac_parseable_yaml() {
        for yaml in [
            generate_service_account(),
            generate_cluster_role(),
            generate_cluster_role_binding(),
        ] {
            let _: serde_yaml::Value =
                serde_yaml::from_str(&yaml).expect("RBAC YAML should be parseable");
        }
    }

    // ── Deployment helper: security context and resource limits ──

    #[test]
    fn test_deployment_security_context_run_as_non_root() {
        for yaml in [
            generate_deployment_watch(),
            generate_deployment_reconcile(),
            generate_deployment_webhook(),
        ] {
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
            let sec = &doc["spec"]["template"]["spec"]["containers"][0]["securityContext"];
            assert_eq!(sec["runAsNonRoot"], true, "runAsNonRoot should be true");
            assert_eq!(
                sec["readOnlyRootFilesystem"], true,
                "readOnlyRootFilesystem should be true"
            );
        }
    }

    #[test]
    fn test_deployment_resource_limits_present() {
        for yaml in [
            generate_deployment_watch(),
            generate_deployment_reconcile(),
            generate_deployment_webhook(),
        ] {
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
            let resources = &doc["spec"]["template"]["spec"]["containers"][0]["resources"];
            assert!(!resources["requests"]["memory"].is_null(), "requests.memory should be set");
            assert!(!resources["requests"]["cpu"].is_null(), "requests.cpu should be set");
            assert!(!resources["limits"]["memory"].is_null(), "limits.memory should be set");
            assert!(!resources["limits"]["cpu"].is_null(), "limits.cpu should be set");
        }
    }

    // ── Aggregator tests ──

    #[test]
    fn test_generate_all_contains_all_kinds() {
        let output = generate_all();
        for kind in [
            "kind: Namespace",
            "kind: ServiceAccount",
            "kind: ClusterRole",
            "kind: ClusterRoleBinding",
            "kind: Deployment",
            "kind: PodDisruptionBudget",
        ] {
            assert!(output.contains(kind), "generate_all should contain {kind}");
        }
    }

    #[test]
    fn test_generate_rbac_has_three_docs() {
        let output = generate_rbac();
        let docs: Vec<&str> = output.split("---\n").collect();
        assert_eq!(docs.len(), 3, "generate_rbac should produce 3 documents");
    }

    #[test]
    fn test_generate_deployments_has_three_docs() {
        let output = generate_deployments();
        let docs: Vec<&str> = output.split("---\n").collect();
        assert_eq!(docs.len(), 3, "generate_deployments should produce 3 documents");
    }

    // ── Label consistency tests ──

    #[test]
    fn test_label_consistency_namespace() {
        let yaml = generate_namespace();
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
        assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
    }

    #[test]
    fn test_label_consistency_deployments() {
        for yaml in [
            generate_deployment_watch(),
            generate_deployment_reconcile(),
            generate_deployment_webhook(),
        ] {
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
            assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
            assert_eq!(
                doc["spec"]["template"]["metadata"]["labels"]["app.kubernetes.io/name"],
                "kube-devops"
            );
        }
    }

    #[test]
    fn test_label_consistency_rbac() {
        for yaml in [
            generate_service_account(),
            generate_cluster_role(),
            generate_cluster_role_binding(),
        ] {
            let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
            assert_eq!(doc["metadata"]["labels"]["app.kubernetes.io/name"], "kube-devops");
        }
    }
}
