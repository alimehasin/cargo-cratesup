use crate::types;
use cargo_metadata::DependencyKind;
use crates_io_api::SyncClient;
use semver::{Version, VersionReq};
use std::error::Error;
use std::fs::{read_to_string, write};
use std::path::Path;
use toml_edit::{value, DocumentMut, Item};

/// The Cargo.toml tables to walk through to reach `dep`, so that a dependency
/// is only ever compared and updated inside the section it was declared in.
fn dependency_table_path(dep: &cargo_metadata::Dependency) -> Vec<String> {
    let table = match dep.kind {
        DependencyKind::Development => "dev-dependencies",
        DependencyKind::Build => "build-dependencies",
        _ => "dependencies",
    };

    return match &dep.target {
        None => vec![table.to_string()],

        Some(target) => vec!["target".to_string(), target.to_string(), table.to_string()],
    };
}

pub fn check_dependency_version(
    client: &SyncClient,
    dep: &cargo_metadata::Dependency,
    crates: &mut Vec<types::Crate>,
) -> Result<types::Crate, Box<dyn Error>> {
    let res = client.get_crate(&dep.name).map_err(|e| {
        eprintln!("Failed to get crate info for {}: {}", dep.name, e);

        return "Failed to get crate info";
    })?;

    let version_req = VersionReq::parse(&dep.req.to_string()).map_err(|e| {
        eprintln!(
            "Failed to parse version requirement for {}: {}",
            dep.name, e
        );

        return "Failed to parse version requirement";
    })?;

    // Find the maximum version that satisfies the requirement
    let (local_version, is_prerelease) = match version_req.comparators.get(0) {
        None => {
            return Err(format!("No comparator found for {}", dep.name).into());
        }

        Some(comparator) => {
            let major = comparator.major;
            let minor = comparator.minor.unwrap_or(0);
            let patch = comparator.patch.unwrap_or(0);
            let pre = comparator.pre.clone();

            let mut local_version = Version::new(major, minor, patch);
            local_version.pre = pre.clone();

            let is_prerelease = !pre.is_empty();

            (local_version, is_prerelease)
        }
    };

    let latest_prerelease_version_str = &res.crate_data.max_version;
    let latest_stable_version_str = res
        .crate_data
        .max_stable_version
        .as_deref()
        .unwrap_or("0.0.0");

    let latest_stable_version = Version::parse(latest_stable_version_str).map_err(|e| {
        eprintln!("Failed to parse latest version for {}: {}", dep.name, e);

        return "Failed to parse latest version";
    })?;

    let latest_prerelease_version =
        Version::parse(&latest_prerelease_version_str).map_err(|e| {
            eprintln!(
                "Failed to parse latest prerelease version for {}: {}",
                dep.name, e
            );

            return "Failed to parse latest prereleases version";
        })?;

    let (latest_version, update_available) = match is_prerelease {
        false => (
            latest_stable_version_str.to_string(),
            local_version < latest_stable_version,
        ),

        true => match latest_stable_version > local_version {
            true => (
                latest_stable_version_str.to_string(),
                local_version < latest_stable_version,
            ),

            false => (
                latest_prerelease_version_str.to_string(),
                local_version < latest_prerelease_version,
            ),
        },
    };

    let krate = types::Crate {
        name: dep.name.clone(),
        local_version: dep.req.to_string(),
        latest_version,
        update_available,
        key: dep.rename.clone().unwrap_or_else(|| dep.name.clone()),
        table_path: dependency_table_path(dep),
    };

    crates.push(krate.clone());

    return Ok(krate);
}

/// Look up a dependency in the exact table it was declared in, so that a dev,
/// build or target dependency is never confused with a `[dependencies]` entry
/// of the same name.
fn find_dependency_mut<'a>(doc: &'a mut DocumentMut, krate: &types::Crate) -> Option<&'a mut Item> {
    let mut table = doc.as_item_mut();

    for key in &krate.table_path {
        table = table.get_mut(key)?;
    }

    return table.get_mut(&krate.key);
}

pub fn update_cargo_toml(output: &[types::Crate]) -> Result<(), Box<dyn Error>> {
    // Path to the Cargo.toml file
    let cargo_toml_path = Path::new("Cargo.toml");

    // Read the current contents of Cargo.toml
    let cargo_toml_str = read_to_string(&cargo_toml_path)
        .map_err(|e| format!("Failed to read Cargo.toml: {}", e))?;

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

    return Ok(());
}
