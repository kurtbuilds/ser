use super::{list_services, Config, ServiceRef};
pub use crate::systemd::generate_file;
use crate::systemd::parse_systemd;
use crate::{print_command, FsServiceDetails, ServiceDetails};
use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn get_service_directories() -> Config {
    let mut user_dirs = Vec::new();
    let mut system_dirs = Vec::new();
    let mut default_dirs = Vec::new();

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

    default_dirs.push(PathBuf::from("/etc/systemd/system"));

    Config {
        default_dirs,
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
    // let _contents = fs::read_to_string(path)?;

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
    let all_services = list_services(super::ListLevel::System)?;
    let service = all_services
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow!("Service '{}' not found", name))?;
    Ok(service.path.clone())
}

pub fn start_service(name: &str) -> Result<()> {
    // Reload systemd daemon to pick up any configuration changes
    refresh_daemon()?;

    // Check if this is a timer-based service
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);
    let timer_path = PathBuf::from("/etc/systemd/system").join(&timer_name);

    let unit_to_start = if timer_path.exists() {
        // Start and enable the timer, not the service
        &timer_name
    } else {
        name
    };

    let mut cmd = Command::new("systemctl");
    cmd.args(["enable", "--now"]).arg(unit_to_start);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start '{}': {}", unit_to_start, stderr));
    }

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    // Check if this is a timer-based service
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);
    let timer_path = PathBuf::from("/etc/systemd/system").join(&timer_name);

    let unit_to_stop = if timer_path.exists() {
        // Stop and disable the timer
        &timer_name
    } else {
        name
    };

    let mut cmd = Command::new("systemctl");
    cmd.args(["disable", "--now"]).arg(unit_to_stop);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to stop '{}': {}", unit_to_stop, stderr));
    }

    Ok(())
}

pub fn restart_service(name: &str) -> Result<()> {
    refresh_daemon()?;
    let mut cmd = Command::new("systemctl");
    cmd.args(["restart"]).arg(name);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

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

    // Always create the service file
    let service_path = systemd_system_dir.join(format!("{}.service", details.name));
    let service_content = generate_file(details)?;
    fs::write(&service_path, service_content)
        .with_context(|| format!("Failed to write unit file: {}", service_path.display()))?;

    // If scheduled, also create timer file
    if details.schedule.is_some() {
        let timer_path = systemd_system_dir.join(format!("{}.timer", details.name));
        let timer_content = crate::systemd::generate_timer_file(details)?;
        fs::write(&timer_path, timer_content)
            .with_context(|| format!("Failed to write timer file: {}", timer_path.display()))?;
    }

    // Reload systemd daemon
    refresh_daemon()?;

    Ok(())
}

pub fn is_service_running(name: &str) -> Result<bool> {
    let mut cmd = Command::new("systemctl");
    cmd.args(["is-active", "--quiet"]).arg(name);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

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

    print_command(&cmd);
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
    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    print_command(&cmd);
    cmd.status()
        .context("Failed to execute systemctl daemon-reload")?;
    Ok(())
}

/// Check if a service has an associated timer file.
pub fn has_timer(name: &str) -> bool {
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_path = PathBuf::from("/etc/systemd/system").join(format!("{}.timer", base_name));
    timer_path.exists()
}

/// Get the next trigger time for a timer.
pub fn get_timer_next_trigger(name: &str) -> Result<Option<String>> {
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);

    let mut cmd = Command::new("systemctl");
    cmd.args([
        "show",
        &timer_name,
        "--property=NextElapseUSecRealtime",
        "--value",
    ]);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

    if output.status.success() {
        let next = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !next.is_empty() && next != "n/a" {
            return Ok(Some(next));
        }
    }
    Ok(None)
}

/// Check if a timer is enabled.
pub fn is_timer_enabled(name: &str) -> bool {
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);

    let mut cmd = Command::new("systemctl");
    cmd.args(["is-enabled", &timer_name]);
    print_command(&cmd);

    if let Ok(output) = cmd.output() {
        let status = String::from_utf8_lossy(&output.stdout);
        return status.trim() == "enabled";
    }
    false
}
