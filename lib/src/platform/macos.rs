use anyhow::{anyhow, Context, Result};
use plist::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::{FsServiceDetails, ServiceDetails};
use super::{Config, ServiceRef};

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

    // For now, assume all found services are "enabled"
    // In reality, we'd need to check launchctl or disabled keys
    let enabled = !plist
        .as_dictionary()
        .and_then(|d| d.get("Disabled"))
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    Ok(ServiceRef {
        name,
        path: path.to_string_lossy().to_string(),
        enabled,
    })
}

fn get_service_path(name: &str) -> Result<String> {
    let all_services = super::list_services(true)?;
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
    
    if program.is_none() {
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

    Ok(ServiceDetails {
        name: "".to_string(),
        program,
        arguments: vec![],
        working_directory,
        run_at_load,
        keep_alive,
        env_file: None,
        env_vars: vec![],
        after: vec![],
    })

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
    let output = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(get_service_path(name)?)
        .output()
        .context("Failed to execute launchctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start service '{}': {}", name, stderr));
    }

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    let output = Command::new("launchctl")
        .args(["unload", "-w"])
        .arg(get_service_path(name)?)
        .output()
        .context("Failed to execute launchctl")?;

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

pub fn create_service(details: &ServiceDetails) -> Result<()> {
    let mut plist_dict = plist::Dictionary::new();

    plist_dict.insert("Label".to_string(), Value::String(details.name.clone()));

    if details.arguments.is_empty() {
        plist_dict.insert("Program".to_string(), Value::String(details.program.clone()));
    } else {
        let mut args = vec![Value::String(details.program.clone())];
        args.extend(details.arguments.iter().map(|v| Value::String(v.clone())));
        plist_dict.insert("ProgramArguments".to_string(), Value::Array(args));
    }
    if let Some(wd) = &details.working_directory {
        plist_dict.insert("WorkingDirectory".to_string(), Value::String(wd.clone()));
    }

    if details.run_at_load {
        plist_dict.insert("RunAtLoad".to_string(), Value::Boolean(true));
    }

    if details.keep_alive {
        plist_dict.insert("KeepAlive".to_string(), Value::Boolean(true));
    }

    let plist_value = Value::Dictionary(plist_dict);

    // Create the plist file in user's LaunchAgents directory
    let home = dirs::home_dir().context("HOME environment variable not set")?;
    let launch_agents_dir = PathBuf::from(home).join("Library/LaunchAgents");

    // Ensure the directory exists
    fs::create_dir_all(&launch_agents_dir).context("Failed to create LaunchAgents directory")?;

    let plist_path = launch_agents_dir.join(format!("{}.plist", details.name));

    // Write the plist file
    let mut plist_data = Vec::new();
    plist::to_writer_xml(&mut plist_data, &plist_value).context("Failed to serialize plist")?;
    fs::write(&plist_path, plist_data)
        .with_context(|| format!("Failed to write plist file: {}", plist_path.display()))?;

    Ok(())
}

pub fn is_service_running(name: &str) -> Result<bool> {
    let output = Command::new("launchctl")
        .args(["list"])
        .output()
        .context("Failed to execute launchctl list")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|line| line.contains(name)))
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
        let mut child = cmd.spawn().context("Failed to execute log show command")?;
        let status = child.wait().context("Failed to wait for log command")?;
        if !status.success() {
            return Err(anyhow!("Log command failed with status: {}", status));
        }
    } else {
        // For static logs, capture output and show last N lines
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
