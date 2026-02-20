use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kube-devops")]
#[command(about = "Kubernetes DevOps Enhancement Tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Version,
    Check,
    List {
        resource: String,
    },
    Analyze,
}
