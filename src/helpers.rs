use crate::index::Releases;
use crate::types;
use cargo_metadata::DependencyKind;
use semver::Version;
use std::error::Error;
use std::fs::{read_to_string, write};
use std::path::Path;
use toml_edit::{value, DocumentMut, Item};

/// The Cargo.toml tables to walk through to reach `dep`, so that a dependency
/// is only ever compared and updated inside the section it was declared in.
pub fn dependency_table_path(dep: &cargo_metadata::Dependency) -> Vec<String> {
    let table = match dep.kind {
        DependencyKind::Development => "dev-dependencies",
        DependencyKind::Build => "build-dependencies",
        _ => "dependencies",
    };

    match &dep.target {
        None => vec![table.to_string()],

        Some(target) => vec!["target".to_string(), target.to_string(), table.to_string()],
    }
}

/// The version a dependency's requirement starts from, e.g. `1.2.0` for `^1.2`.
/// Returns `None` when there is no version to compare against, like for a path
/// or git dependency without a `version` key.
pub fn required_version(dep: &cargo_metadata::Dependency) -> Option<Version> {
    let comparator = dep.req.comparators.first()?;

    let mut version = Version::new(
        comparator.major,
        comparator.minor.unwrap_or(0),
        comparator.patch.unwrap_or(0),
    );
    version.pre = comparator.pre.clone();

    Some(version)
}

pub fn check_dependency_version(
    dep: &cargo_metadata::Dependency,
    local_version: &Version,
    releases: &Releases,
) -> types::Crate {
    let latest_version = releases.latest_for(local_version);

    types::Crate {
        latest_version: latest_version.to_string(),
        update_available: local_version < latest_version,
        key: dep.rename.clone().unwrap_or_else(|| dep.name.clone()),
        table_path: dependency_table_path(dep),
    }
}

/// Look up a dependency in the exact table it was declared in, so that a dev,
/// build or target dependency is never confused with a `[dependencies]` entry
/// of the same name.
fn find_dependency_mut<'a>(doc: &'a mut DocumentMut, krate: &types::Crate) -> Option<&'a mut Item> {
    let mut table = doc.as_item_mut();

    for key in &krate.table_path {
        table = table.get_mut(key)?;
    }

    table.get_mut(&krate.key)
}

pub fn update_cargo_toml(output: &[types::Crate]) -> Result<(), Box<dyn Error>> {
    // Path to the Cargo.toml file
    let cargo_toml_path = Path::new("Cargo.toml");

    // Read the current contents of Cargo.toml
    let cargo_toml_str =
        read_to_string(cargo_toml_path).map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;

    // Parse the Cargo.toml into a TOML document
    let mut cargo_toml_doc = cargo_toml_str
        .parse::<DocumentMut>()
        .map_err(|e| format!("Failed to parse Cargo.toml: {}", e))?;

    // Iterate over the output to check for updates
    for krate in output.iter().filter(|krate| krate.update_available) {
        let Some(dep) = find_dependency_mut(&mut cargo_toml_doc, krate) else {
            continue;
        };

        match dep.as_table_like_mut() {
            // `foo = { version = "1.0", .. }` or `[dependencies.foo]`
            Some(table) => {
                if let Some(version) = table.get_mut("version") {
                    *version = value(krate.latest_version.clone());
                }
            }

            // `foo = "1.0"`
            None => {
                *dep = value(krate.latest_version.clone());
            }
        }
    }

    // Write the updated contents back to the Cargo.toml file
    write(cargo_toml_path, cargo_toml_doc.to_string())
        .map_err(|e| format!("Failed to write to Cargo.toml: {}", e))?;

    println!("Cargo.toml has been updated successfully");

    Ok(())
}
