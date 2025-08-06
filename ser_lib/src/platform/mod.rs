#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub user_dirs: Vec<PathBuf>,
    pub system_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ServiceDetails {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub running: bool,
    pub program: Option<String>,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub run_at_load: bool,
    pub keep_alive: bool,
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
pub use linux::*;

// Helper function to resolve service names (handles both display and full names)
pub fn resolve_service_name(display_name: &str) -> anyhow::Result<String> {
    let all_services = list_services(true)?;

    // First try exact match
    if let Some(service) = all_services.iter().find(|s| s.name == display_name) {
        return Ok(service.name.clone());
    }

    // If not found and doesn't start with homebrew.mxcl., try prefixing it
    if !display_name.starts_with("homebrew.mxcl.") {
        let full_name = format!("homebrew.mxcl.{display_name}");
        if let Some(service) = all_services.iter().find(|s| s.name == full_name) {
            return Ok(service.name.clone());
        }
    }

    // Try matching the part before @ symbol
    if let Some(service) = all_services.iter().find(|s| {
        if let Some(at_pos) = s.name.find('@') {
            let prefix = &s.name[..at_pos];
            prefix == display_name
        } else {
            false
        }
    }) {
        return Ok(service.name.clone());
    }

    // Try matching homebrew.mxcl. prefix with @ symbol matching
    if !display_name.starts_with("homebrew.mxcl.") {
        if let Some(service) = all_services.iter().find(|s| {
            if s.name.starts_with("homebrew.mxcl.") {
                if let Some(at_pos) = s.name.find('@') {
                    let after_prefix = &s.name[14..at_pos]; // 14 is length of "homebrew.mxcl."
                    after_prefix == display_name
                } else {
                    false
                }
            } else {
                false
            }
        }) {
            return Ok(service.name.clone());
        }
    }

    Err(anyhow::anyhow!("Service '{}' not found", display_name))
}
