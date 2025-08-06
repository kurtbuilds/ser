use crate::ServiceDetails;
use anyhow::Result;

pub fn parse_systemd(_content: &str) -> Result<ServiceDetails> {
    unimplemented!()
}

pub fn generate_file(service: &ServiceDetails) -> Result<String> {
    let mut unit_content = String::new();
    unit_content.push_str("[Unit]\n");
    unit_content.push_str(&format!("Description={}\n", service.name));
    unit_content.push_str("\n[Service]\n");

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

    if service.keep_alive {
        unit_content.push_str("Restart=always\n");
    }
    if let Some(file) = &service.env_file {
        unit_content.push_str(&format!("EnvironmentFile={}\n", file));
    }
    for (key, value) in &service.env_vars {
        unit_content.push_str(&format!("Environment=\"{}={}\"\n", key, value));
    }

    if service.run_at_load || !service.after.is_empty() {
        unit_content.push_str("\n[Install]\n");
    }
    if service.run_at_load {
        unit_content.push_str("WantedBy=default.target\n");
    }
    if !service.after.is_empty() {
        unit_content.push_str("After=");
        for after in &service.after {
            unit_content.push_str(after);
            unit_content.push(' ');
        }
        unit_content.pop(); // Remove trailing space
        unit_content.push('\n');
    }
    Ok(unit_content)
}

