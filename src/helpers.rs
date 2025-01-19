use crate::types;
use crates_io_api::SyncClient;
use semver::{Version, VersionReq};
use std::error::Error;
use std::fs::{read_to_string, write};
use std::path::Path;
use toml_edit::{value, DocumentMut, Value};

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

    let dep = match is_prerelease {
        false => types::Crate {
            name: dep.name.clone(),
            local_version: dep.req.to_string(),
            latest_version: latest_stable_version_str.to_string(),
            update_available: local_version < latest_stable_version,
        },

        true => match latest_stable_version > local_version {
            true => types::Crate {
                name: dep.name.clone(),
                local_version: dep.req.to_string(),
                latest_version: latest_stable_version_str.to_string(),
                update_available: local_version < latest_stable_version,
            },

            false => types::Crate {
                name: dep.name.clone(),
                local_version: dep.req.to_string(),
                latest_version: latest_prerelease_version_str.to_string(),
                update_available: local_version < latest_prerelease_version,
            },
        },
    };

    crates.push(dep.clone());

    return Ok(dep);
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
        if let Some(dependencies) = cargo_toml_doc.get_mut("dependencies") {
            if let Some(dep) = dependencies.get_mut(&krate.name) {
                match dep.is_inline_table() {
                    false => {
                        *dep = value(krate.latest_version.clone());
                    }

                    true => {
                        let v = dep.as_inline_table_mut().and_then(|t| t.get_mut("version"));

                        if let Some(version) = v {
                            *version = Value::from(krate.latest_version.clone());
                        }
                    }
                }
            }
        }
    }

    // Write the updated contents back to the Cargo.toml file
    write(cargo_toml_path, cargo_toml_doc.to_string())
        .map_err(|e| format!("Failed to write to Cargo.toml: {}", e))?;

    println!("Cargo.toml has been updated successfully");

    return Ok(());
}
