use semver::{BuildMetadata, Version};
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use ureq::Agent;

/// The crates.io sparse index, the same one Cargo reads from. Unlike the
/// crates.io API, which allows one request per second, it is served from a CDN
/// without rate limits, so crates can be looked up in parallel.
const INDEX_URL: &str = "https://index.crates.io";

/// How many crates are looked up at the same time
const MAX_CONCURRENT_REQUESTS: usize = 16;

const USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

/// The newest published versions of a crate, ignoring yanked ones.
pub struct Releases {
    /// The highest version, prereleases included.
    pub latest: Version,

    /// The highest version that is not a prerelease.
    pub latest_stable: Version,
}

impl Releases {
    /// The version `local` can be updated to. Prereleases are only suggested to
    /// dependencies already on one, and only while they are ahead of the latest
    /// stable release.
    pub fn latest_for(&self, local: &Version) -> &Version {
        match local.pre.is_empty() || self.latest_stable > *local {
            true => &self.latest_stable,
            false => &self.latest,
        }
    }
}

/// A line of an index file, which describes one published version.
#[derive(Deserialize)]
struct IndexEntry {
    vers: String,

    #[serde(default)]
    yanked: bool,
}

/// The path of a crate's index file, e.g. `se/rd/serde` or `3/s/syn`.
fn index_path(name: &str) -> String {
    let name = name.to_lowercase();

    match name.len() {
        1 => format!("1/{}", name),
        2 => format!("2/{}", name),
        3 => format!("3/{}/{}", &name[..1], name),
        _ => format!("{}/{}/{}", &name[..2], &name[2..4], name),
    }
}

fn parse_releases(index_file: &str) -> Releases {
    let mut releases = Releases {
        latest: Version::new(0, 0, 0),
        latest_stable: Version::new(0, 0, 0),
    };

    for line in index_file.lines() {
        let Ok(entry) = serde_json::from_str::<IndexEntry>(line) else {
            continue;
        };

        if entry.yanked {
            continue;
        }

        let Ok(mut version) = Version::parse(&entry.vers) else {
            continue;
        };

        // Build metadata doesn't make a version newer, so without dropping it
        // `0.1.0+meta` would be reported as an update to `0.1.0`
        version.build = BuildMetadata::EMPTY;

        if version.pre.is_empty() && version > releases.latest_stable {
            releases.latest_stable = version.clone();
        }

        if version > releases.latest {
            releases.latest = version;
        }
    }

    releases
}

fn fetch_releases(agent: &Agent, name: &str) -> Result<Releases, String> {
    let url = format!("{}/{}", INDEX_URL, index_path(name));

    let index_file = agent
        .get(&url)
        .call()
        .and_then(|mut res| res.body_mut().read_to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => "crate not found on crates.io".to_string(),
            e => e.to_string(),
        })?;

    Ok(parse_releases(&index_file))
}

/// Look up the releases of every crate in `names`, several at a time, calling
/// `on_fetched` with each crate's releases as soon as they arrive.
pub fn fetch_all_releases(
    names: &[String],
    on_fetched: impl Fn(&str, Result<Releases, String>) + Sync,
) {
    let agent: Agent = Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();

    let next = AtomicUsize::new(0);
    let workers = names.len().min(MAX_CONCURRENT_REQUESTS);

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                // Keep taking the next crate nobody has looked up yet
                while let Some(name) = names.get(next.fetch_add(1, Ordering::Relaxed)) {
                    on_fetched(name, fetch_releases(&agent, name));
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_path_follows_cargo_layout() {
        assert_eq!(index_path("a"), "1/a");
        assert_eq!(index_path("ab"), "2/ab");
        assert_eq!(index_path("syn"), "3/s/syn");
        assert_eq!(index_path("serde"), "se/rd/serde");
        assert_eq!(index_path("Inflector"), "in/fl/inflector");
    }

    #[test]
    fn parse_releases_skips_yanked_and_build_metadata() {
        let index_file = [
            r#"{"name":"foo","vers":"1.0.0","deps":[],"yanked":false}"#,
            r#"{"name":"foo","vers":"1.1.0+meta","deps":[],"yanked":false}"#,
            r#"{"name":"foo","vers":"1.2.0","deps":[],"yanked":true}"#,
            r#"{"name":"foo","vers":"2.0.0-beta.1","deps":[],"yanked":false}"#,
        ]
        .join("\n");

        let releases = parse_releases(&index_file);

        assert_eq!(releases.latest_stable, Version::parse("1.1.0").unwrap());
        assert_eq!(releases.latest, Version::parse("2.0.0-beta.1").unwrap());
    }

    #[test]
    fn latest_for_only_suggests_prereleases_to_prereleases() {
        let v = |version| Version::parse(version).unwrap();

        let releases = Releases {
            latest: v("2.0.0-beta.2"),
            latest_stable: v("1.1.0"),
        };

        assert_eq!(releases.latest_for(&v("1.0.0")), &v("1.1.0"));
        assert_eq!(releases.latest_for(&v("1.0.0-rc.1")), &v("1.1.0"));
        assert_eq!(releases.latest_for(&v("2.0.0-beta.1")), &v("2.0.0-beta.2"));
    }
}
