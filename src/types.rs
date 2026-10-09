#[derive(Clone)]
pub struct Crate {
    pub latest_version: String,
    pub update_available: bool,

    /// The key the dependency is listed under in Cargo.toml, which differs from
    /// the crate name when the dependency is renamed.
    pub key: String,

    /// The tables to walk through in Cargo.toml to reach the dependency, e.g.
    /// `["dev-dependencies"]` or `["target", "cfg(unix)", "dependencies"]`.
    pub table_path: Vec<String>,
}
