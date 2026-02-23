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
}

#[derive(Subcommand)]
pub enum CrdAction {
    /// Print the CRD YAML to stdout
    Generate,

    /// Install the CRD into the connected cluster
    Install,
}
