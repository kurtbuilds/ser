#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServiceRef {
    pub name: String,
    pub path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub default_dirs: Vec<PathBuf>,
    pub user_dirs: Vec<PathBuf>,
    pub system_dirs: Vec<PathBuf>,
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
pub use linux::*;

#[derive(Copy, Clone)]
pub enum ListLevel {
    Default,
    User,
    System,
}

pub fn list_services(level: ListLevel) -> Result<Vec<ServiceRef>> {
    let config = get_service_directories();
    let mut services = Vec::new();

    match level {
        ListLevel::Default => {
            for dir in &config.default_dirs {
                let user_services = scan_directory(dir)?;
                services.extend(user_services);
            }
        }
        ListLevel::User => {
            for dir in &config.user_dirs {
                let user_services = scan_directory(dir)?;
                services.extend(user_services);
            }
        }
        ListLevel::System => {
            for dir in &config.user_dirs {
                let user_services = scan_directory(dir)?;
                services.extend(user_services);
            }
            for dir in &config.system_dirs {
                let system_services = scan_directory(dir)?;
                services.extend(system_services);
            }
        }
    }
    Ok(services)
}

pub fn normalize_service_name(name: &str) -> &str {
    // Normalize service names by removing leading/trailing whitespace and converting to lowercase
    let name = name.split('@').next().unwrap();
    name.trim_start_matches("homebrew.mxcl.")
        .trim_end_matches(".service")
}

pub fn get_service(name: &str) -> Result<ServiceRef> {
    let normalized_name = normalize_service_name(name);
    let all_services = list_services(ListLevel::System)?;

    if let Some(service) = all_services
        .into_iter()
        .find(|s| normalize_service_name(&s.name) == normalized_name)
    {
        return Ok(service);
    }

    Err(anyhow::anyhow!("Service '{}' not found", name))
}

pub fn resolve_service_name(name: &str) -> Result<String> {
    let service = get_service(name)?;
    Ok(service.name)
}
