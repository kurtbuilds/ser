use super::{list_services, Config, ServiceRef};
pub use crate::systemd::generate_file;
use crate::systemd::parse_systemd;
use crate::{FsServiceDetails, ServiceDetails};
use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub fn get_service_details(name: &str) -> Result<FsServiceDetails> {
    // Find the service first
    let service_ref = super::get_service(name)?;

    // Parse the unit file for detailed information
    let contents = fs::read_to_string(&service_ref.path)
        .with_context(|| format!("Failed to read service file: {}", service_ref.path))?;

    let service = parse_systemd(&contents)?;
    let running = is_service_running(name)?;

    Ok(FsServiceDetails {
        running,
        service,
        enabled: service_ref.enabled,
        path: service_ref.path,
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
    // Reload systemd daemon to pick up any configuration changes
    refresh_daemon()?;

    let output = Command::new("systemctl")
        .args(["start"])
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
        .args(["stop"])
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
    refresh_daemon()?;
    let output = Command::new("systemctl")
        .args(["restart"])
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

    let path = systemd_system_dir.join(format!("{}.service", details.name));

    // Create systemd unit file content
    let content = generate_file(details)?;

    // Write the unit file
    fs::write(&path, content)
        .with_context(|| format!("Failed to write unit file: {}", path.display()))?;

    // Reload systemd daemon
    refresh_daemon();

    Ok(())
}

pub fn is_service_running(name: &str) -> Result<bool> {
    let output = Command::new("systemctl")
        .args(["is-active", "--quiet"])
        .arg(name)
        .output()
        .context("Failed to execute systemctl")?;

    Ok(output.status.success())
}

pub fn show_service_logs(name: &str, lines: u32, follow: bool) -> Result<()> {
    let mut cmd = Command::new("journalctl");
    cmd.args(["-u", name]);

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

fn refresh_daemon() -> anyhow::Result<()> {
    Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .context("Failed to execute systemctl daemon-reload")?;
    Ok(())
}
