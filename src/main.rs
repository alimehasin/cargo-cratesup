mod cli;
mod helpers;
mod index;
mod report;
mod types;

use cargo_metadata::MetadataCommand;
use clap::Parser;
use cli::Commands;
use colored::Colorize;
use std::error::Error;
use std::sync::Mutex;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::Cli::parse();

    let update_required = match cli.cmd {
        Commands::Cratesup { update } => update,
    };

    // Get metadata from Cargo.toml, without resolving the whole dependency
    // graph since only the root package's own dependencies are checked
    let metadata = MetadataCommand::new().no_deps().exec().map_err(|e| {
        eprintln!("Failed to get metadata: {}", e);

        "Failed to get metadata"
    })?;

    // Get root package from metadata
    let root_package = metadata
        .root_package()
        .ok_or("Failed to find root package")?;

    // Dependencies without a version requirement, like path and git ones,
    // have nothing to compare against
    let deps: Vec<_> = root_package
        .dependencies
        .iter()
        .filter_map(|dep| Some((dep, helpers::required_version(dep)?)))
        .collect();

    // The same crate can be listed in several sections, but is fetched once
    let mut names: Vec<String> = deps.iter().map(|(dep, _)| dep.name.clone()).collect();
    names.sort();
    names.dedup();

    let report = report::Report::new(
        deps.iter()
            .map(|(dep, _)| report::Row {
                name: dep.name.clone(),
                section: helpers::dependency_table_path(dep).join("."),
                req: dep.req.to_string(),
            })
            .collect(),
    );

    let crates = Mutex::new(vec![]);

    // Check each dependencies updates, as soon as its crate is fetched
    index::fetch_all_releases(&names, |name, releases| {
        for (i, (dep, local_version)) in deps.iter().enumerate() {
            if dep.name != name {
                continue;
            }

            match &releases {
                Ok(releases) => {
                    let krate = helpers::check_dependency_version(dep, local_version, releases);
                    report.finish(i, Ok(&krate));
                    crates.lock().unwrap().push(krate);
                }

                Err(e) => report.finish(i, Err(e)),
            }
        }
    });

    let crates = crates.into_inner().unwrap();
    let outdated = crates.iter().filter(|krate| krate.update_available).count();

    if !deps.is_empty() {
        println!();
    }

    if outdated == 0 {
        println!("All dependencies are up to date");
        return Ok(());
    }

    match update_required {
        true => helpers::update_cargo_toml(&crates)?,

        false => println!(
            "{} {} can be updated, run `{}` to update Cargo.toml",
            outdated,
            match outdated {
                1 => "dependency",
                _ => "dependencies",
            },
            "cargo cratesup -u".bold()
        ),
    }

    Ok(())
}
