use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::{ServiceDetails, FsServiceDetails};
use super::{Config, ServiceRef, list_services};
pub use crate::systemd::generate_file;

pub(super) fn get_service_directories() -> Config {
    let mut user_dirs = Vec::new();
    let mut system_dirs = Vec::new();

    // User-specific systemd directory
    if let Some(home) = std::env::var_os("HOME") {
        let user_systemd = PathBuf::from(home).join(".config/systemd/user");
        user_dirs.push(user_systemd);
    }

    // User unit directories (global user services)
    user_dirs.push(PathBuf::from("/usr/lib/systemd/user"));
    user_dirs.push(PathBuf::from("/etc/systemd/user"));
    user_dirs.push(PathBuf::from("/usr/local/lib/systemd/user"));

    // System unit directories
    system_dirs.push(PathBuf::from("/lib/systemd/system"));
    system_dirs.push(PathBuf::from("/usr/lib/systemd/system"));
    system_dirs.push(PathBuf::from("/etc/systemd/system"));
    system_dirs.push(PathBuf::from("/usr/local/lib/systemd/system"));

    Config {
        user_dirs,
        system_dirs,
    }
}

pub(super) fn scan_directory(dir: &Path) -> Result<Vec<ServiceRef>> {
    let mut services = Vec::new();

    if !dir.exists() {
        return Ok(services);
    }

    let entries = fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
            if matches!(
                extension,
                "service"
                    | "socket"
                    | "timer"
                    | "target"
                    | "mount"
                    | "automount"
                    | "swap"
                    | "path"
                    | "slice"
                    | "scope"
            ) {
                if let Ok(service) = parse_unit_file(&path) {
                    services.push(service);
                }
            }
        }
    }

    Ok(services)
}

fn parse_unit_file(path: &Path) -> Result<ServiceRef> {
    let _contents = fs::read_to_string(path)?;

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Simple heuristic: if the file exists and is readable, consider it "enabled"
    // In reality, we'd need to check symlinks in /etc/systemd/system/*.wants/ directories
    // or parse the unit file more thoroughly
    let enabled = is_service_enabled(path, &name);

    Ok(ServiceRef {
        name,
        path: path.to_string_lossy().to_string(),
        enabled,
    })
}

fn is_service_enabled(_path: &Path, name: &str) -> bool {
    // Check common systemd target directories for symlinks
    let wants_dirs = [
        "/etc/systemd/system/multi-user.target.wants",
        "/etc/systemd/system/graphical.target.wants",
        "/etc/systemd/system/default.target.wants",
    ];

    for wants_dir in &wants_dirs {
        let symlink_path = PathBuf::from(wants_dir).join(name);
        if symlink_path.exists() {
            return true;
        }
    }

    // Also check if there's a symlink in the same directory structure
    let parent_dir = PathBuf::from("/etc/systemd/system");
    let possible_symlink = parent_dir.join(name);
    if possible_symlink.exists() && possible_symlink.is_symlink() {
        return true;
    }

    false
}

pub fn get_service_details(name: &str) -> Result<ServiceDetails> {
    // Find the service first
    let service = super::get_service(name)?;

    // Parse the unit file for detailed information
    let contents = fs::read_to_string(&service.path)
        .with_context(|| format!("Failed to read service file: {}", service.path))?;

    // Basic parsing of systemd unit file
    let mut program = None;
    let mut arguments = Vec::new();
    let mut working_directory = None;
    let mut run_at_load = false;
    let mut keep_alive = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("ExecStart=") {
            let exec_start = line.strip_prefix("ExecStart=").unwrap_or("");
            let parts = exec_start.split_whitespace();
            if parts.is_empty() {
                bail!("ExecStart line is empty in service file: {}", service.path);
            }
            program = parts.next().unwrap().to_string();
            arguments = parts.map(|s| s.to_string()).collect();
        } else if line.starts_with("WorkingDirectory=") {
            working_directory = line
                .strip_prefix("WorkingDirectory=")
                .map(|s| s.to_string());
        } else if line == "WantedBy=multi-user.target" || line == "WantedBy=default.target" {
            run_at_load = true;
        } else if line.starts_with("Restart=") {
            keep_alive = line != "Restart=no";
        }
    }

    let running = is_service_running(name)?;

    Ok(FsServiceDetails {
        running,
        service: ServiceDetails {
            name: service.name.clone(),
            enabled: service.enabled,
            running,
            program,
            arguments,
            working_directory,
            run_at_load,
            keep_alive,
        },
        path: service.path.clone(),
    })
}

pub fn get_service_file_path(name: &str) -> Result<String> {
    let all_services = list_services(true)?;
    let service = all_services
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow!("Service '{}' not found", name))?;
    Ok(service.path.clone())
}

pub fn start_service(name: &str) -> Result<()> {
    let output = Command::new("systemctl")
        .args(["--user", "start"])
        .arg(name)
        .output()
        .context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    let output = Command::new("systemctl")
        .args(["--user", "stop"])
        .arg(name)
        .output()
        .context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to stop service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn restart_service(name: &str) -> Result<()> {
    let output = Command::new("systemctl")
        .args(["--user", "restart"])
        .arg(name)
        .output()
        .context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to restart service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn create_service(details: &ServiceDetails) -> Result<()> {
    let systemd_system_dir = PathBuf::from("/etc/systemd/system");

    // Ensure the directory exists
    fs::create_dir_all(&systemd_system_dir).context("Failed to create systemd user directory")?;

    let unit_path = systemd_system_dir.join(format!("{}.service", details.name));

    // Create systemd unit file content


    // Write the unit file
    fs::write(&unit_path, unit_content)
        .with_context(|| format!("Failed to write unit file: {}", unit_path.display()))?;

    // Reload systemd daemon
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    Ok(())
}

pub fn is_service_running(name: &str) -> Result<bool> {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet"])
        .arg(name)
        .output()
        .context("Failed to execute systemctl")?;

    Ok(output.status.success())
}

pub fn show_service_logs(name: &str, lines: u32, follow: bool) -> Result<()> {
    let mut cmd = Command::new("journalctl");
    cmd.args(["--user", "-u", name]);

    // Limit number of lines
    cmd.arg("-n").arg(lines.to_string());

    if follow {
        cmd.arg("-f");
    }

    // Show output with colors and pager disabled for better integration
    cmd.arg("--no-pager");

    let mut child = cmd
        .spawn()
        .context("Failed to execute journalctl command")?;

    let status = child
        .wait()
        .context("Failed to wait for journalctl command")?;

    if !status.success() {
        return Err(anyhow!("Journalctl command failed with status: {}", status));
    }

    Ok(())
}
