use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::prelude::*;

mod cli;
mod commands;

use cli::{Cli, CrdAction, Commands, ObservabilityAction, WebhookAction};

/// Wrap an async command so Ctrl+C produces a clean shutdown message.
///
/// Used for short-lived commands (check, list, analyze, crd install) that
/// make API calls which may hang when the cluster is unreachable.
/// Long-running commands (watch, reconcile) handle Ctrl+C internally.
async fn interruptible<F: std::future::Future<Output = anyhow::Result<()>>>(
    task: F,
) -> anyhow::Result<()> {
    tokio::select! {
        result = task => result,
        _ = tokio::signal::ctrl_c() => {
            println!("\nInterrupted. Shutting down gracefully.");
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .json()
                .with_current_span(true)
                .with_target(false)
        )
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        // Instant, synchronous — no Ctrl+C handling needed
        Commands::Version => commands::version::run()?,
        Commands::Crd { action: CrdAction::Generate } => commands::crd::generate()?,

        // Long-running — handle Ctrl+C internally with their own shutdown logic
        Commands::Watch => commands::watch::run().await?,
        Commands::Reconcile => commands::reconcile::run().await?,

        // Short-lived async — wrap with interruptible for graceful Ctrl+C
        Commands::Check => interruptible(commands::check::run()).await?,
        Commands::List { resource } => interruptible(commands::list::run(resource)).await?,
        Commands::Analyze => interruptible(commands::analyze::run()).await?,
        Commands::Crd { action: CrdAction::Install } => {
            interruptible(commands::crd::install()).await?
        }

        // Webhook subcommands
        Commands::Webhook { action: WebhookAction::Serve { addr, tls_cert, tls_key } } => {
            commands::webhook::serve(&addr, &tls_cert, &tls_key).await?
        }
        Commands::Webhook { action: WebhookAction::CertGenerate { service_name, namespace, output_dir, ip_sans } } => {
            commands::webhook::generate_certs(&service_name, &namespace, &output_dir, &ip_sans)?
        }
        Commands::Webhook { action: WebhookAction::InstallConfig { service_name, namespace, ca_bundle_path } } => {
            commands::webhook::install_config(&service_name, &namespace, &ca_bundle_path)?
        }

        // Observability subcommands
        Commands::Observability { action: ObservabilityAction::GenerateAll } => {
            print!("{}", commands::observability::generate_all())
        }
        Commands::Observability { action: ObservabilityAction::GenerateServiceMonitors } => {
            print!("{}", commands::observability::generate_service_monitors())
        }
        Commands::Observability { action: ObservabilityAction::GenerateDashboard } => {
            print!("{}", commands::observability::generate_grafana_dashboard_configmap())
        }
    }

    Ok(())
}
