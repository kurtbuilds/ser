use super::{list_services, Config, ServiceRef};
pub use crate::systemd::generate_file;
use crate::systemd::parse_systemd;
use crate::{print_command, FsServiceDetails, ServiceDetails};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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

    let enabled = is_service_enabled(&name);

    Ok(ServiceRef {
        name,
        path: path.to_string_lossy().to_string(),
        enabled,
    })
}

/// Unit-file enablement states keyed by unit name, as systemd itself reports
/// them. Read once per process: `parse_unit_file` runs for every file in every
/// scanned directory, so a `systemctl is-enabled` per unit would mean hundreds
/// of subprocesses on a `--all` listing.
fn unit_file_states() -> &'static HashMap<String, String> {
    static STATES: OnceLock<HashMap<String, String>> = OnceLock::new();
    STATES.get_or_init(|| {
        let mut cmd = Command::new("systemctl");
        cmd.args(["list-unit-files", "--no-legend", "--no-pager"]);
        print_command(&cmd);
        let Ok(output) = cmd.output() else {
            return HashMap::new();
        };
        if !output.status.success() {
            return HashMap::new();
        }
        parse_unit_file_states(&String::from_utf8_lossy(&output.stdout))
    })
}

/// Parse `systemctl list-unit-files --no-legend` rows, which are
/// `UNIT-FILE STATE [PRESET]`.
fn parse_unit_file_states(stdout: &str) -> HashMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let name = cols.next()?;
            let state = cols.next()?;
            Some((name.to_string(), state.to_string()))
        })
        .collect()
}

fn is_service_enabled(name: &str) -> bool {
    enabled_from_states(unit_file_states(), name)
}

/// Whether systemd will start this unit on its own at boot.
///
/// A timer-backed job is enabled through its `.timer` — that is the unit
/// `start_service` arms, and its `[Install]` puts the symlink under
/// `timers.target.wants`, not under the service's own target. So report the
/// timer's state for a `.service` that has one, or the listing would call every
/// scheduled job disabled.
fn enabled_from_states(states: &HashMap<String, String>, name: &str) -> bool {
    let timer = format!(
        "{}.timer",
        name.trim_end_matches(".service").trim_end_matches(".timer")
    );
    let unit = if name.ends_with(".service") && states.contains_key(&timer) {
        timer.as_str()
    } else {
        name
    };
    matches!(
        states.get(unit).map(String::as_str),
        Some("enabled") | Some("enabled-runtime")
    )
}

pub fn get_service_details(name: &str) -> Result<FsServiceDetails> {
    // Find the service first
    let service_ref = super::get_service(name)?;

    // Parse the unit file for detailed information
    let contents = fs::read_to_string(&service_ref.path)
        .with_context(|| format!("Failed to read service file: {}", service_ref.path))?;

    let mut service = parse_systemd(&contents)?;
    // The schedule lives in the paired `.timer` unit, not the `.service` file,
    // so read it back here to populate `service.schedule`.
    service.schedule = read_timer_schedule(&service_ref.path);
    let running = is_service_running(name)?;

    Ok(FsServiceDetails {
        running,
        service,
        enabled: service_ref.enabled,
        path: service_ref.path,
    })
}

/// Read the schedule from the `.timer` unit paired with the given `.service`
/// file path. Handles both `OnCalendar=` (calendar) and `OnUnitActiveSec=`
/// (interval) timers. Returns `None` when there is no timer or its expression
/// cannot be represented as a [`Schedule`].
fn read_timer_schedule(service_path: &str) -> Option<crate::Schedule> {
    use crate::{CalendarSchedule, Schedule};

    let timer_path = Path::new(service_path).with_extension("timer");
    let contents = fs::read_to_string(&timer_path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(expr) = line.strip_prefix("OnCalendar=") {
            return CalendarSchedule::from_systemd_oncalendar(expr).map(Schedule::Calendar);
        }
        if let Some(span) = line.strip_prefix("OnUnitActiveSec=") {
            return Schedule::parse_interval_secs(span).map(Schedule::Interval);
        }
    }
    None
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

/// Run the underlying service unit once, immediately, regardless of whether it
/// is timer-backed. Unlike `start_service`, this never touches the `.timer`
/// (which only arms the schedule) — it invokes the `.service` directly so the
/// job executes right now.
pub fn run_service_now(name: &str) -> Result<()> {
    refresh_daemon()?;

    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let service_name = format!("{}.service", base_name);

    let mut cmd = Command::new("systemctl");
    cmd.arg("start").arg(&service_name);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to run '{}': {}", service_name, stderr));
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

    // For timer-backed units, restart the timer so a changed schedule is picked
    // up; restarting the .service would just run it once.
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);
    let timer_path = PathBuf::from("/etc/systemd/system").join(&timer_name);

    let unit_to_restart = if timer_path.exists() {
        &timer_name
    } else {
        name
    };

    let mut cmd = Command::new("systemctl");
    cmd.args(["restart"]).arg(unit_to_restart);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute systemctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Failed to restart '{}': {}",
            unit_to_restart,
            stderr
        ));
    }

    Ok(())
}

/// Check that a unit systemd accepted actually stayed up. `systemctl start` and
/// `systemctl restart` return success as soon as the unit is spawned for
/// `Type=simple` services, so a program that dies immediately (bad path,
/// missing dependency, crash on startup) still looks like a successful start.
/// Watch the unit briefly and fail if systemd marks it failed.
///
/// Checks the same unit `start_service`/`restart_service` act on — the `.timer`
/// for timer-backed units.
pub fn verify_service_started(name: &str) -> Result<()> {
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let timer_name = format!("{}.timer", base_name);
    let timer_path = PathBuf::from("/etc/systemd/system").join(&timer_name);

    if timer_path.exists() {
        verify_unit_started(&timer_name)
    } else {
        verify_unit_started(name)
    }
}

/// Like `verify_service_started`, but for the unit `run_service_now` acts on:
/// always the `.service`, never the `.timer`.
pub fn verify_run_service_now(name: &str) -> Result<()> {
    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    verify_unit_started(&format!("{}.service", base_name))
}

fn verify_unit_started(unit: &str) -> Result<()> {
    const SETTLE: std::time::Duration = std::time::Duration::from_secs(2);
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);

    let deadline = std::time::Instant::now() + SETTLE;
    loop {
        let mut cmd = Command::new("systemctl");
        cmd.arg("show")
            .arg(unit)
            .args(["--property=ActiveState", "--property=Result"]);
        print_command(&cmd);
        let output = cmd.output().context("Failed to execute systemctl show")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Failed to query '{}': {}", unit, stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let property = |key: &str| {
            stdout.lines().find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k == key).then(|| v.trim().to_string())
            })
        };

        let active_state = property("ActiveState").unwrap_or_default();
        // `Result` stays "success" until the unit fails; on failure it names
        // the cause (exit-code, signal, core-dump, timeout, start-limit-hit).
        let result = property("Result").unwrap_or_default();

        if active_state == "failed" || (!result.is_empty() && result != "success") {
            return Err(anyhow!(
                "Unit '{}' failed to start: systemd reports {} ({})",
                unit,
                active_state,
                result
            ));
        }

        if std::time::Instant::now() >= deadline {
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
}

pub fn remove_service(name: &str) -> Result<()> {
    // Best-effort stop/disable (also handles the paired timer) before deleting.
    let _ = stop_service(name);

    let base_name = name.trim_end_matches(".service").trim_end_matches(".timer");
    let dir = PathBuf::from("/etc/systemd/system");
    let service_path = dir.join(format!("{}.service", base_name));
    let timer_path = dir.join(format!("{}.timer", base_name));

    let mut removed = false;
    for path in [&service_path, &timer_path] {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Failed to remove unit file: {}", path.display()))?;
            removed = true;
        }
    }
    if !removed {
        bail!("No unit files found for '{}'", name);
    }

    refresh_daemon()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `systemctl list-unit-files --no-legend` output (systemd 255).
    const LIST_UNIT_FILES: &str = "\
admin.service                              disabled        enabled
ibserver.service                           disabled        enabled
seeking-alpha-analysis.service             static          -
seeking-alpha-analysis.timer               enabled         enabled
sync-instruments.service                   static          -
sync-instruments.timer                     enabled         enabled
getty@.service                             enabled         enabled
ufw.service                                enabled         enabled
";

    #[test]
    fn parses_name_and_state_ignoring_preset() {
        let states = parse_unit_file_states(LIST_UNIT_FILES);
        assert_eq!(states["ibserver.service"], "disabled");
        assert_eq!(states["sync-instruments.timer"], "enabled");
        assert_eq!(states["seeking-alpha-analysis.service"], "static");
    }

    /// A timer-backed job is enabled via its `.timer`; the `.service` is
    /// `static`. Reading the service's own state reported every scheduled job
    /// on the host as disabled.
    #[test]
    fn timer_backed_service_reports_the_timers_state() {
        let states = parse_unit_file_states(LIST_UNIT_FILES);
        assert!(enabled_from_states(&states, "sync-instruments.service"));
        assert!(enabled_from_states(&states, "sync-instruments.timer"));
        assert!(enabled_from_states(
            &states,
            "seeking-alpha-analysis.service"
        ));
    }

    #[test]
    fn plain_service_uses_its_own_state() {
        let states = parse_unit_file_states(LIST_UNIT_FILES);
        assert!(enabled_from_states(&states, "ufw.service"));
        // Started by a deploy's `systemctl restart`, never enabled.
        assert!(!enabled_from_states(&states, "ibserver.service"));
    }

    #[test]
    fn unknown_unit_is_not_enabled() {
        let states = parse_unit_file_states(LIST_UNIT_FILES);
        assert!(!enabled_from_states(&states, "nope.service"));
        assert!(!enabled_from_states(&states, ""));
    }
}
