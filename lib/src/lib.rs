pub mod platform;
pub mod systemd;
pub mod plist;

#[derive(Debug, Clone)]
pub struct ServiceDetails {
    pub name: String,
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub run_at_load: bool,
    pub keep_alive: bool,
    pub env_file: Option<String>,
    pub env_vars: Vec<(String, String)>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FsServiceDetails {
    pub service: ServiceDetails,
    pub path: String,
    pub enabled: bool,
    pub running: bool,
}
