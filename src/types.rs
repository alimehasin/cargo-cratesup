#[derive(Clone)]
pub struct Crate {
    pub name: String,
    pub local_version: String,
    pub latest_version: String,
    pub update_available: bool,
}
