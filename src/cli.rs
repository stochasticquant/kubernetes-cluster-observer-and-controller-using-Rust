use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kube-devops")]
#[command(about = "Kubernetes DevOps Enhancement Tool")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Display application version
    Version,

    /// Check cluster connectivity and permissions
    Check,

    /// List Kubernetes resources (e.g. pods)
    List {
        /// Resource type to list (pods)
        resource: String,
    },

    /// Run governance analysis on cluster workloads
    Analyze,

    /// Start real-time governance watch controller
    Watch,

    /// Manage the DevOpsPolicy CRD
    Crd {
        #[command(subcommand)]
        action: CrdAction,
    },

    /// Start the DevOpsPolicy operator reconcile loop
    Reconcile,

    /// Manage the admission webhook
    Webhook {
        #[command(subcommand)]
        action: WebhookAction,
    },

    /// Generate observability manifests (Services, ServiceMonitors, Grafana dashboard)
    Observability {
        #[command(subcommand)]
        action: ObservabilityAction,
    },
}

#[derive(Subcommand)]
pub enum WebhookAction {
    /// Start the admission webhook HTTPS server
    Serve {
        #[arg(long, default_value = "0.0.0.0:8443")]
        addr: String,
        #[arg(long, default_value = "tls.crt")]
        tls_cert: String,
        #[arg(long, default_value = "tls.key")]
        tls_key: String,
    },
    /// Generate self-signed TLS certificates for development
    CertGenerate {
        #[arg(long, default_value = "kube-devops-webhook")]
        service_name: String,
        #[arg(long, default_value = "default")]
        namespace: String,
        #[arg(long, default_value = ".")]
        output_dir: String,
        /// Additional IP SANs (e.g. --ip-san 192.168.1.26)
        #[arg(long = "ip-san")]
        ip_sans: Vec<String>,
    },
    /// Print the ValidatingWebhookConfiguration YAML
    InstallConfig {
        #[arg(long, default_value = "kube-devops-webhook")]
        service_name: String,
        #[arg(long, default_value = "default")]
        namespace: String,
        #[arg(long)]
        ca_bundle_path: String,
    },
}

#[derive(Subcommand)]
pub enum CrdAction {
    /// Print the CRD YAML to stdout
    Generate,

    /// Install the CRD into the connected cluster
    Install,
}

#[derive(Subcommand)]
#[allow(clippy::enum_variant_names)]
pub enum ObservabilityAction {
    /// Print all observability manifests (Services + ServiceMonitors + Grafana dashboard)
    GenerateAll,

    /// Print only ServiceMonitor manifests
    GenerateServiceMonitors,

    /// Print only the Grafana dashboard ConfigMap
    GenerateDashboard,
}
