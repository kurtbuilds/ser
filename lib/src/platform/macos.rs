use super::{Config, ServiceRef};
use crate::platform::ListLevel;
pub use crate::plist::generate_file;
use crate::{print_command, CalendarSchedule, FsServiceDetails, Schedule, ServiceDetails};
use anyhow::{anyhow, Context, Result};
use plist::Value;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub(super) fn get_service_directories() -> Config {
    let mut user_dirs = Vec::new();
    let mut system_dirs = Vec::new();

    // User-specific launch agents
    if let Some(home) = std::env::var_os("HOME") {
        let user_agents = PathBuf::from(home).join("Library/LaunchAgents");
        user_dirs.push(user_agents);
    }

    // System-wide launch agents
    system_dirs.push(PathBuf::from("/System/Library/LaunchAgents"));
    system_dirs.push(PathBuf::from("/Library/LaunchAgents"));

    // Launch daemons (system services)
    system_dirs.push(PathBuf::from("/System/Library/LaunchDaemons"));
    system_dirs.push(PathBuf::from("/Library/LaunchDaemons"));

    Config {
        default_dirs: user_dirs.clone(),
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

        if path.extension().and_then(|s| s.to_str()) == Some("plist") {
            if let Ok(service) = parse_plist_into_service_ref(&path) {
                services.push(service);
            }
        }
    }
    Ok(services)
}

/// Every label launchd holds an enable/disable override for, mapped to whether
/// it is disabled.
///
/// Queried once per run and cached: one `launchctl print-disabled` per domain
/// covers every service, where a per-service query would mean hundreds of
/// launchctl invocations for `ser list`.
fn disabled_overrides() -> &'static HashMap<String, bool> {
    static OVERRIDES: OnceLock<HashMap<String, bool>> = OnceLock::new();
    OVERRIDES.get_or_init(|| {
        let mut overrides = HashMap::new();

        // System daemons first, so a user agent sharing a label wins.
        let mut domains = vec!["system".to_string()];
        if let Some(uid) = current_uid() {
            domains.push(format!("gui/{}", uid));
        }

        for domain in domains {
            let mut cmd = Command::new("launchctl");
            cmd.arg("print-disabled").arg(&domain);
            print_command(&cmd);

            // A domain we can't read (no such GUI session, or a system domain
            // needing more privilege) just contributes nothing.
            let Ok(output) = cmd.output() else { continue };
            if !output.status.success() {
                continue;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            overrides.extend(parse_disabled_overrides(&stdout));
        }

        overrides
    })
}

/// Parse the `"<label>" => disabled` lines of `launchctl print-disabled`.
/// Releases before Sonoma print `true`/`false` in place of `disabled`/`enabled`.
fn parse_disabled_overrides(output: &str) -> HashMap<String, bool> {
    output
        .lines()
        .filter_map(|line| {
            let (label, state) = line.split_once("=>")?;
            let label = label.trim().trim_matches('"');
            if label.is_empty() {
                return None;
            }
            let disabled = match state.trim().trim_end_matches(';').trim() {
                "disabled" | "true" => true,
                "enabled" | "false" => false,
                _ => return None,
            };
            Some((label.to_string(), disabled))
        })
        .collect()
}

/// The uid whose GUI domain holds this user's agents. Taken from the owner of
/// the home directory, which is where `ser`'s own agents live.
fn current_uid() -> Option<u32> {
    let home = dirs::home_dir()?;
    fs::metadata(home).ok().map(|m| m.uid())
}

fn parse_plist_into_service_ref(path: &Path) -> Result<ServiceRef> {
    let contents = fs::read(path)?;
    let plist: Value = plist::from_bytes(&contents)?;
    let name = if let Some(label) = plist
        .as_dictionary()
        .and_then(|d| d.get("Label"))
        .and_then(|v| v.as_string())
    {
        label.to_string()
    } else {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    // The plist's `Disabled` key is only the baseline the job ships with.
    // `launchctl load -w` / `unload -w` — what `ser start` / `ser stop` run —
    // record the state in launchd's overrides database instead, so that wins
    // when it has an entry for this label.
    let disabled_in_plist = plist
        .as_dictionary()
        .and_then(|d| d.get("Disabled"))
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    let enabled = !disabled_overrides()
        .get(&name)
        .copied()
        .unwrap_or(disabled_in_plist);

    Ok(ServiceRef {
        name,
        path: path.to_string_lossy().to_string(),
        enabled,
    })
}

fn get_service_path(name: &str) -> Result<String> {
    let all_services = super::list_services(ListLevel::System)?;
    let service = all_services
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow!("Service '{}' not found", name))?;
    Ok(service.path.clone())
}

pub fn get_service_file_path(name: &str) -> Result<String> {
    get_service_path(name)
}

pub fn parse_plist_into_service(plist: Value) -> Result<ServiceDetails> {
    let dict = plist
        .as_dictionary()
        .ok_or_else(|| anyhow!("Invalid plist format"))?;

    let name = dict
        .get("Label")
        .and_then(|v| v.as_string())
        .unwrap_or_default()
        .to_string();

    let mut program = dict
        .get("Program")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());

    let mut arguments: Vec<String> = dict
        .get("ProgramArguments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_string())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    if program.is_none() && !arguments.is_empty() {
        program = Some(arguments.remove(0));
    }

    let program = program.context("Missing 'Program' or 'ProgramArguments' in plist")?;

    let working_directory = dict
        .get("WorkingDirectory")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());

    let run_at_load = dict
        .get("RunAtLoad")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    let keep_alive = dict
        .get("KeepAlive")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    // Parse schedule: a simple repeating StartInterval, or a calendar pattern.
    let schedule = if let Some(secs) = dict
        .get("StartInterval")
        .and_then(|v| v.as_signed_integer())
    {
        Some(Schedule::Interval(secs.max(0) as u64))
    } else {
        dict.get("StartCalendarInterval")
            .and_then(parse_calendar_interval)
            .map(Schedule::Calendar)
    };

    let env_vars = dict
        .get("EnvironmentVariables")
        .and_then(|v| v.as_dictionary())
        .map(|d| {
            d.iter()
                .filter_map(|(k, v)| v.as_string().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Ok(ServiceDetails {
        name,
        program,
        arguments,
        working_directory,
        run_at_load,
        keep_alive,
        env_file: None,
        env_vars,
        after: vec![],
        schedule,
    })
}

fn parse_calendar_interval(value: &Value) -> Option<CalendarSchedule> {
    let dict = value.as_dictionary()?;

    Some(CalendarSchedule {
        month: dict
            .get("Month")
            .and_then(|v| v.as_signed_integer())
            .map(|v| v as u8),
        day: dict
            .get("Day")
            .and_then(|v| v.as_signed_integer())
            .map(|v| v as u8),
        weekday: dict
            .get("Weekday")
            .and_then(|v| v.as_signed_integer())
            .map(|v| v as u8),
        hour: dict
            .get("Hour")
            .and_then(|v| v.as_signed_integer())
            .map(|v| v as u8),
        minute: dict
            .get("Minute")
            .and_then(|v| v.as_signed_integer())
            .map(|v| v as u8),
    })
}

/// Check if a service has a schedule (is a timer).
pub fn has_timer(name: &str) -> bool {
    if let Ok(details) = get_service_details(name) {
        return details.service.schedule.is_some();
    }
    false
}

pub fn get_service_details(name: &str) -> Result<FsServiceDetails> {
    // Find the service first
    let sref = super::get_service(name)?;

    // Parse the plist for detailed information
    let contents = fs::read(&sref.path)
        .with_context(|| format!("Failed to read service file: {}", sref.path))?;
    let plist: Value = plist::from_bytes(&contents)
        .with_context(|| format!("Failed to parse plist: {}", sref.path))?;

    let service = parse_plist_into_service(plist)?;

    let running = is_service_running(name)?;

    Ok(FsServiceDetails {
        service,
        path: sref.path,
        enabled: sref.enabled,
        running,
    })
}

pub fn start_service(name: &str) -> Result<()> {
    let mut cmd = Command::new("launchctl");
    cmd.args(["load", "-w"]).arg(get_service_path(name)?);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute launchctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start service '{}': {}", name, stderr));
    }

    Ok(())
}

/// Run a job once, immediately. For a scheduled (timer) job, `start_service`
/// only loads the plist so launchd arms the schedule; this kicks the job off
/// right now via `launchctl start <label>`. The job must be loaded first, so we
/// load it (idempotently) before starting.
pub fn run_service_now(name: &str) -> Result<()> {
    // Ensure the job is loaded; ignore errors since it may already be loaded.
    let path = get_service_path(name)?;
    let _ = Command::new("launchctl").args(["load", &path]).output();

    // The launchd label matches the service name for ser-managed units.
    let mut cmd = Command::new("launchctl");
    cmd.arg("start").arg(name);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute launchctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to run service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    let mut cmd = Command::new("launchctl");
    cmd.args(["unload", "-w"]).arg(get_service_path(name)?);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute launchctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to stop service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn restart_service(name: &str) -> Result<()> {
    stop_service(name)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    start_service(name)?;
    Ok(())
}

/// What `launchctl list <label>` reports about a job.
struct JobStatus {
    loaded: bool,
    last_exit_status: Option<i64>,
}

fn get_job_status(name: &str) -> Result<JobStatus> {
    let mut cmd = Command::new("launchctl");
    cmd.arg("list").arg(name);
    print_command(&cmd);
    let output = cmd.output().context("Failed to execute launchctl list")?;

    // A non-zero exit means launchd has no such job in this domain.
    if !output.status.success() {
        return Ok(JobStatus {
            loaded: false,
            last_exit_status: None,
        });
    }

    // Output is launchd's old-style dict: `"LastExitStatus" = 256;` per line.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_exit_status = stdout.lines().find_map(|line| {
        let (key, value) = line.trim().trim_end_matches(';').split_once('=')?;
        if key.trim().trim_matches('"') != "LastExitStatus" {
            return None;
        }
        value.trim().parse::<i64>().ok()
    });

    Ok(JobStatus {
        loaded: true,
        last_exit_status,
    })
}

/// launchd reports the raw wait(2) status: the low 7 bits hold the terminating
/// signal, the next 8 the exit code.
fn describe_exit_status(status: i64) -> String {
    let signal = status & 0x7f;
    if signal != 0 {
        format!("terminated by signal {}", signal)
    } else {
        format!("exited with code {}", (status >> 8) & 0xff)
    }
}

/// Like `verify_service_started`, but for the job `run_service_now` acts on.
/// launchd addresses a job by its label either way, so this is the same check —
/// it exists to mirror the platform split on Linux, where a timer-backed unit
/// is started through the `.timer` but run through the `.service`.
pub fn verify_run_service_now(name: &str) -> Result<()> {
    verify_service_started(name)
}

/// Check that a job launchd accepted actually stayed up. `launchctl load`
/// reports only whether the plist was accepted, so a program that dies
/// immediately (bad path, missing dependency, crash on startup) still looks
/// like a successful start. Watch the job briefly and fail if it exits.
pub fn verify_service_started(name: &str) -> Result<()> {
    const SETTLE: std::time::Duration = std::time::Duration::from_secs(2);
    const POLL: std::time::Duration = std::time::Duration::from_millis(250);

    let deadline = std::time::Instant::now() + SETTLE;
    loop {
        let status = get_job_status(name)?;

        if !status.loaded {
            return Err(anyhow!(
                "Service '{}' is not loaded after starting it",
                name
            ));
        }

        if let Some(exit_status) = status.last_exit_status.filter(|s| *s != 0) {
            return Err(anyhow!(
                "Service '{}' failed to start: process {}",
                name,
                describe_exit_status(exit_status)
            ));
        }

        if std::time::Instant::now() >= deadline {
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
}

pub fn create_service(details: &ServiceDetails) -> Result<()> {
    let plist_data = generate_file(details)
        .with_context(|| format!("Failed to generate plist for service '{}'", details.name))?;

    let home = dirs::home_dir().context("HOME environment variable not set")?;
    let launch_agents_dir = home.join("Library/LaunchAgents");
    // Ensure the directory exists
    fs::create_dir_all(&launch_agents_dir).context("Failed to create LaunchAgents directory")?;
    let plist_path = launch_agents_dir.join(format!("{}.plist", details.name));

    fs::write(&plist_path, plist_data)
        .with_context(|| format!("Failed to write plist file: {}", plist_path.display()))?;

    Ok(())
}

pub fn remove_service(name: &str) -> Result<()> {
    let path = get_service_path(name)?;

    // Best-effort unload so the job is stopped before its plist disappears.
    let mut cmd = Command::new("launchctl");
    cmd.args(["unload", "-w", &path]);
    print_command(&cmd);
    let _ = cmd.output();

    fs::remove_file(&path).with_context(|| format!("Failed to remove plist file: {path}"))?;
    Ok(())
}

pub fn is_service_running(name: &str) -> Result<bool> {
    // Ask launchd about this label specifically. Scanning the full `launchctl
    // list` for a line *containing* the name reports `foo` as running whenever
    // an unrelated `foobar` is loaded.
    Ok(get_job_status(name)?.loaded)
}

pub fn show_service_logs(name: &str, lines: u32, follow: bool) -> Result<()> {
    // First try to find logs using the unified logging system
    let mut cmd = Command::new("log");
    cmd.arg("show");

    // Show logs from the last hour to capture recent activity
    cmd.arg("--last").arg("1h");

    // Add predicate to filter by service name - try multiple approaches
    let predicate = format!(
        "process CONTAINS[c] '{name}' OR subsystem CONTAINS[c] '{name}' OR category CONTAINS[c] '{name}' OR eventMessage CONTAINS[c] '{name}'"
    );
    cmd.arg("--predicate").arg(predicate);

    cmd.arg("--style").arg("syslog");

    if follow {
        cmd.arg("--stream");
        // For follow mode, spawn and let it run
        print_command(&cmd);
        let mut child = cmd.spawn().context("Failed to execute log show command")?;
        let status = child.wait().context("Failed to wait for log command")?;
        if !status.success() {
            return Err(anyhow!("Log command failed with status: {}", status));
        }
    } else {
        // For static logs, capture output and show last N lines
        print_command(&cmd);
        let output = cmd.output().context("Failed to execute log show command")?;

        if !output.status.success() {
            // Fallback: try to show launchctl logs or suggest manual approaches
            eprintln!("Warning: Could not retrieve logs using 'log show' command");
            eprintln!("Try one of these alternatives:");
            eprintln!("  • Check Console.app and search for '{name}'");
            eprintln!("  • Run: log show --predicate 'process CONTAINS \"{name}\"' --last 1h");
            eprintln!("  • Check service-specific log files in /var/log/ or ~/Library/Logs/");
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let log_lines: Vec<&str> = stdout.lines().collect();

        // Show last N lines
        let start_idx = if log_lines.len() > lines as usize {
            log_lines.len() - lines as usize
        } else {
            0
        };

        for &line in &log_lines[start_idx..] {
            println!("{line}");
        }

        if log_lines.is_empty() {
            println!("No recent logs found for service '{name}'");
            println!("Note: macOS services may log to different locations:");
            println!("  • System logs: Check Console.app");
            println!("  • Service-specific logs: Check /var/log/ or ~/Library/Logs/");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `launchctl print-disabled gui/501` output (Darwin 25).
    const PRINT_DISABLED: &str = "\
disabled services = {
\t\t\"io.tailscale.ipn.macsys.login-item-helper\" => enabled
\t\t\"com.apple.ManagedClientAgent.enrollagent\" => disabled
\t\t\"ser.example\" => disabled
}
";

    /// Older releases spelled the state as a boolean.
    const PRINT_DISABLED_LEGACY: &str = "\
disabled services = {
\t\t\"com.example.old\" => true
\t\t\"com.example.on\" => false
}
";

    #[test]
    fn parses_disabled_state_per_label() {
        let overrides = parse_disabled_overrides(PRINT_DISABLED);
        assert!(overrides["ser.example"]);
        assert!(!overrides["io.tailscale.ipn.macsys.login-item-helper"]);
        // The braces and header carry no `=>` and must not become entries.
        assert_eq!(overrides.len(), 3);
    }

    #[test]
    fn parses_legacy_boolean_state() {
        let overrides = parse_disabled_overrides(PRINT_DISABLED_LEGACY);
        assert!(overrides["com.example.old"]);
        assert!(!overrides["com.example.on"]);
    }

    /// `unload -w` records the disable in launchd's database, not the plist, so
    /// a plist with no `Disabled` key still has to read as disabled.
    #[test]
    fn override_decides_when_plist_has_no_disabled_key() {
        let overrides = parse_disabled_overrides(PRINT_DISABLED);
        let disabled_in_plist = false;
        let enabled = !overrides
            .get("ser.example")
            .copied()
            .unwrap_or(disabled_in_plist);
        assert!(!enabled);
    }
}
