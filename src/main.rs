mod cli;
mod helpers;
mod types;

use cargo_metadata::MetadataCommand;
use clap::Parser;
use cli::Commands;
use colored::Colorize;
use crates_io_api::SyncClient;
use spinners::{Spinner, Spinners};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::Cli::parse();

    let update_required = match cli.cmd {
        Commands::Cratesup { update } => update,
    };

    // Get metadata from Cargo.toml
    let metadata = MetadataCommand::new().exec().map_err(|e| {
        eprintln!("Failed to get metadata: {}", e);

        return "Failed to get metadata";
    })?;

    // Get root package from metadata
    let root_package = metadata
        .root_package()
        .ok_or("Failed to find root package")?;

    // Create a client to query crates.io
    let client = SyncClient::new(
        "crate (crate@example.com)",            // User agent string
        std::time::Duration::from_millis(1000), // Timeout duration
    )?;

    let mut crates: Vec<types::Crate> = vec![];

    // Check each dependencies updates
    for dep in &root_package.dependencies {
        let mut sp = Spinner::new(Spinners::Dots9, format!("Checking {}... ", dep.name));

        let Ok(dep) = helpers::check_dependency_version(&client, dep, &mut crates) else {
            continue;
        };

        let msg = match dep.update_available {
            true => format!(
                "{} Crate {} has an update available: {} -> {}",
                &format!("").normal(),
                dep.name.cyan(),
                dep.local_version.normal(),
                dep.latest_version.green()
            ),

            false => format!(
                "{} Crate {} is up to date",
                &format!("✔").green(),
                dep.name.cyan()
            ),
        };

        sp.stop_and_persist("", msg);
    }

    let update_available = crates.iter().any(|krate| krate.update_available);
    if !update_available {
        println!("All dependencies are up to date");
        return Ok(());
    }

    if update_required {
        helpers::update_cargo_toml(&crates)?;
    }

    return Ok(());
}
