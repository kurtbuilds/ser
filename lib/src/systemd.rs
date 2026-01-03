use crate::ServiceDetails;
use anyhow::{bail, Result};

/// Comment added to generated service files to indicate they are managed by ser
pub const MANAGED_BY_COMMENT: &str = "# Managed by ser";

/// Generate a systemd timer file for scheduled execution.
pub fn generate_timer_file(service: &ServiceDetails) -> Result<String> {
    let schedule = service
        .schedule
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No schedule defined"))?;

    let mut content = String::new();
    content.push_str(MANAGED_BY_COMMENT);
    content.push('\n');
    content.push_str("[Unit]\n");
    content.push_str(&format!("Description=Timer for {}\n", service.name));
    content.push_str("\n[Timer]\n");
    content.push_str(&format!(
        "OnCalendar={}\n",
        schedule.to_systemd_oncalendar()
    ));
    content.push_str("Persistent=true\n");
    content.push_str("\n[Install]\n");
    content.push_str("WantedBy=timers.target\n");

    Ok(content)
}

pub fn parse_systemd(contents: &str) -> Result<ServiceDetails> {
    // Basic parsing of systemd unit file
    let mut name = None;
    let mut program = None;
    let mut arguments = Vec::new();
    let mut working_directory = None;
    let mut run_at_load = false;
    let mut keep_alive = false;
    let mut env_file = None;
    let mut env_vars = Vec::new();
    let mut after = Vec::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("Description=") {
            name = line.strip_prefix("Description=").map(|s| s.to_string());
        }
        if line.starts_with("ExecStart=") {
            let exec_start = line.strip_prefix("ExecStart=").unwrap_or("");
            let mut parts = exec_start.split_whitespace();
            if let Some(prog) = parts.next() {
                program = Some(prog.to_string());
                arguments = parts.map(|s| s.to_string()).collect();
            } else {
                bail!("ExecStart line is empty in service file");
            }
        } else if line.starts_with("WorkingDirectory=") {
            working_directory = line
                .strip_prefix("WorkingDirectory=")
                .map(|s| s.to_string());
        } else if line == "WantedBy=multi-user.target" || line == "WantedBy=default.target" {
            run_at_load = true;
        } else if line.starts_with("Restart=") {
            keep_alive = line != "Restart=no";
        } else if line.starts_with("EnvironmentFile=") {
            env_file = line.strip_prefix("EnvironmentFile=").map(|s| s.to_string());
        } else if line.starts_with("Environment=") {
            // Ignored for now
            let env_line = line.strip_prefix("Environment=").unwrap();
            let Some((a, b)) = env_line.split_once('=') else {
                bail!("Environment line is empty in service file");
            };
            env_vars.push((a.to_string(), b.to_string()));
        } else if line.starts_with("After=") {
            let after_line = line.strip_prefix("After=").unwrap_or("");
            after = after_line
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
        }
    }
    Ok(ServiceDetails {
        name: name.expect("No name for service"),
        program: program.expect("No program for service"),
        arguments,
        working_directory,
        run_at_load,
        keep_alive,
        env_file,
        env_vars,
        after,
        schedule: None, // Schedule is parsed from .timer file separately
    })
}

pub fn generate_file(service: &ServiceDetails) -> Result<String> {
    let mut unit_content = String::new();
    unit_content.push_str(MANAGED_BY_COMMENT);
    unit_content.push('\n');
    unit_content.push_str("[Unit]\n");
    unit_content.push_str(&format!("Description={}\n", service.name));
    if !service.after.is_empty() {
        unit_content.push_str("After=");
        for after in &service.after {
            unit_content.push_str(after);
            unit_content.push(' ');
        }
        unit_content.pop(); // Remove trailing space
        unit_content.push('\n');
    }
    unit_content.push_str("\n[Service]\n");

    // For scheduled services, use Type=oneshot
    if service.schedule.is_some() {
        unit_content.push_str("Type=oneshot\n");
    }

    unit_content.push_str("ExecStart=");
    unit_content.push_str(&service.program);
    for arg in &service.arguments {
        unit_content.push(' ');
        unit_content.push_str(arg);
    }
    unit_content.push('\n');

    if let Some(ref wd) = service.working_directory {
        unit_content.push_str(&format!("WorkingDirectory={}\n", wd));
    }

    // Only add Restart for non-scheduled services
    if service.schedule.is_none() && service.keep_alive {
        unit_content.push_str("Restart=always\n");
    }
    if let Some(file) = &service.env_file {
        unit_content.push_str(&format!("EnvironmentFile={}\n", file));
    }
    for (key, value) in &service.env_vars {
        unit_content.push_str(&format!("Environment=\"{}={}\"\n", key, value));
    }

    // Only add [Install] section for non-scheduled services
    if service.schedule.is_none() && service.run_at_load {
        unit_content.push_str("\n[Install]\n");
        unit_content.push_str("WantedBy=default.target\n");
    }

    Ok(unit_content)
}
