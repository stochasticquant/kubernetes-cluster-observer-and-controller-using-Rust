mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Version => commands::version::run()?,
        Commands::Check => commands::check::run()?,
        Commands::List { resource } => {
            commands::list::run(resource).await?;
        }
    }

    Ok(())
}
