use anyhow::Result;
use clap::{Args, ValueEnum};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::process::Command;

use ser_lib::ServiceDetails;

#[derive(Debug, Clone, ValueEnum)]
pub enum Format {
    /// Generate native format for current platform
    Native,
    /// Generate systemd service file
    Systemd,
}

#[derive(Debug, Args)]
pub struct Generate {
    #[arg(long, default_value = "systemd", help = "Output format")]
    format: Format,
    command: Vec<String>,
}

fn resolve_binary_path(binary: &str) -> Result<String> {
    // If it's already an absolute path, validate it exists and return as-is
    if binary.starts_with('/') {
        return if std::path::Path::new(binary).exists() {
            Ok(binary.to_string())
        } else {
            Err(anyhow::anyhow!("Binary '{}' does not exist", binary))
        }
    }

    // Try using 'which' command first
    if let Ok(output) = Command::new("which").arg(binary).output() {
        if output.status.success() {
            let path = String::from_utf8(output.stdout)
                .map_err(|_| anyhow::anyhow!("Invalid UTF-8 in which command output"))?
                .trim()
                .to_string();

            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return Ok(path);
            }
        }
    }

    // Fallback: manually search PATH if 'which' is not available or fails
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let full_path = std::path::Path::new(dir).join(binary);
            if full_path.exists() && full_path.is_file() {
                // Check if the file is executable
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = full_path.metadata() {
                        let permissions = metadata.permissions();
                        if permissions.mode() & 0o111 != 0 {
                            return Ok(full_path.to_string_lossy().to_string());
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    return Ok(full_path.to_string_lossy().to_string());
                }
            }
        }
    }

    Err(anyhow::anyhow!("Binary '{}' not found in PATH", binary))
}

pub fn collect_service_details() -> Result<ServiceDetails> {
    println!("Creating service configuration...\n");

    let theme = ColorfulTheme::default();

    // Service name
    let name: String = Input::with_theme(&theme)
        .with_prompt("Service name (e.g., com.example.myservice)")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Service name cannot be empty")
            } else if input.contains(' ') {
                Err("Service name cannot contain spaces")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    // Program or script to run
    let program_type = Select::with_theme(&theme)
        .with_prompt("What type of program do you want to run?")
        .default(0)
        .items(&["Executable file", "Script with arguments"])
        .interact()?;

    let (program, arguments) = if program_type == 0 {
        // Single executable
        let prog: String = Input::with_theme(&theme)
            .with_prompt("Path to executable")
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.trim().is_empty() {
                    Err("Path cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()?;

        // Resolve the binary path
        let resolved_prog = resolve_binary_path(&prog)
            .map_err(|e| anyhow::anyhow!("Failed to resolve binary path for '{}': {}", prog, e))?;

        (Some(resolved_prog), Vec::new())
    } else {
        // Script with arguments
        let args_input: String = Input::with_theme(&theme)
            .with_prompt("Command and arguments (space-separated)")
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.trim().is_empty() {
                    Err("Command cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()?;

        let mut args: Vec<String> = args_input
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        // Resolve the first argument (the binary) if it exists
        if !args.is_empty() {
            let binary_name = &args[0];
            let resolved_binary = resolve_binary_path(binary_name).map_err(|e| {
                anyhow::anyhow!("Failed to resolve binary path for '{}': {}", binary_name, e)
            })?;
            args[0] = resolved_binary;
        }

        (None, args)
    };

    // Working directory
    let working_directory: Option<String> = {
        let has_wd = Confirm::with_theme(&theme)
            .with_prompt("Set a working directory?")
            .default(false)
            .interact()?;

        if has_wd {
            Some(
                Input::with_theme(&theme)
                    .with_prompt("Working directory path")
                    .interact_text()?,
            )
        } else {
            None
        }
    };

    // Run at load
    let run_at_load = Confirm::with_theme(&theme)
        .with_prompt("Start automatically when system boots?")
        .default(true)
        .interact()?;

    // Keep alive
    let keep_alive = Confirm::with_theme(&theme)
        .with_prompt("Restart automatically if it crashes?")
        .default(true)
        .interact()?;

    // Create service details
    Ok(ServiceDetails {
        name,
        path: String::new(), // Not used for generation
        enabled: true,
        running: false,
        program,
        arguments,
        working_directory,
        run_at_load,
        keep_alive,
        env_file: None,
        env_vars: vec![],
    })
}

fn generate_systemd_service(details: &ServiceDetails) -> String {
    let mut unit_content = String::new();

    unit_content.push_str("[Unit]\n");
    unit_content.push_str(&format!("Description={}\n", details.name));
    unit_content.push_str("\n[Service]\n");

    if let Some(ref program) = details.program {
        if details.arguments.is_empty() {
            unit_content.push_str(&format!("ExecStart={program}\n"));
        } else {
            let mut cmd = program.clone();
            for arg in &details.arguments {
                cmd.push(' ');
                cmd.push_str(arg);
            }
            unit_content.push_str(&format!("ExecStart={cmd}\n"));
        }
    } else if !details.arguments.is_empty() {
        unit_content.push_str(&format!("ExecStart={}\n", details.arguments.join(" ")));
    }

    if let Some(ref wd) = details.working_directory {
        unit_content.push_str(&format!("WorkingDirectory={wd}\n"));
    }

    if details.keep_alive {
        unit_content.push_str("Restart=always\n");
    }

    if details.run_at_load {
        unit_content.push_str("\n[Install]\n");
        unit_content.push_str("WantedBy=default.target\n");
    }

    unit_content
}

fn generate_macos_plist(details: &ServiceDetails) -> Result<String> {
    let mut plist_dict = plist::Dictionary::new();

    plist_dict.insert(
        "Label".to_string(),
        plist::Value::String(details.name.clone()),
    );

    if let Some(ref program) = details.program {
        if details.arguments.is_empty() {
            plist_dict.insert("Program".to_string(), plist::Value::String(program.clone()));
        } else {
            let mut args = vec![program.clone()];
            args.extend(details.arguments.clone());
            let plist_args: Vec<plist::Value> =
                args.into_iter().map(plist::Value::String).collect();
            plist_dict.insert(
                "ProgramArguments".to_string(),
                plist::Value::Array(plist_args),
            );
        }
    } else if !details.arguments.is_empty() {
        let plist_args: Vec<plist::Value> = details
            .arguments
            .iter()
            .cloned()
            .map(plist::Value::String)
            .collect();
        plist_dict.insert(
            "ProgramArguments".to_string(),
            plist::Value::Array(plist_args),
        );
    }

    if let Some(ref wd) = details.working_directory {
        plist_dict.insert(
            "WorkingDirectory".to_string(),
            plist::Value::String(wd.clone()),
        );
    }

    if details.run_at_load {
        plist_dict.insert("RunAtLoad".to_string(), plist::Value::Boolean(true));
    }

    if details.keep_alive {
        plist_dict.insert("KeepAlive".to_string(), plist::Value::Boolean(true));
    }

    let plist_value = plist::Value::Dictionary(plist_dict);

    // Serialize to XML
    let mut plist_data = Vec::new();
    plist::to_writer_xml(&mut plist_data, &plist_value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize plist: {}", e))?;

    String::from_utf8(plist_data)
        .map_err(|e| anyhow::anyhow!("Failed to convert plist to string: {}", e))
}

impl Generate {
    pub fn run(&self) -> Result<()> {
        let details = collect_service_details()?;

        // Show summary
        println!("\nService configuration:");
        println!("  Name: {}", details.name);
        if let Some(ref prog) = details.program {
            println!("  Program: {prog}");
        }
        if !details.arguments.is_empty() {
            println!("  Arguments: {}", details.arguments.join(" "));
        }
        if let Some(ref wd) = details.working_directory {
            println!("  Working Directory: {wd}");
        }
        println!("  Run at Load: {}", details.run_at_load);
        println!("  Keep Alive: {}", details.keep_alive);
        println!("  Format: {:?}", self.format);

        let theme = ColorfulTheme::default();
        let confirm = Confirm::with_theme(&theme)
            .with_prompt("Generate service file?")
            .default(true)
            .interact()?;

        if !confirm {
            println!("Generation cancelled.");
            return Ok(());
        }

        println!();

        // Generate the appropriate format
        match self.format {
            Format::Native => {
                #[cfg(target_os = "macos")]
                {
                    let content = generate_macos_plist(&details)?;
                    println!("{content}");
                }
                #[cfg(target_os = "linux")]
                {
                    let content = generate_systemd_service(&details);
                    println!("{}", content);
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    return Err(anyhow::anyhow!(
                        "Native format not supported on this platform"
                    ));
                }
            }
            Format::Systemd => {
                let content = generate_systemd_service(&details);
                println!("{content}");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_systemd_service() {
        let details = ServiceDetails {
            name: "test-service".to_string(),
            path: String::new(),
            enabled: true,
            running: false,
            program: Some("/usr/bin/test".to_string()),
            arguments: vec!["--flag".to_string(), "value".to_string()],
            working_directory: Some("/tmp".to_string()),
            run_at_load: true,
            keep_alive: true,
        };

        let content = generate_systemd_service(&details);

        assert!(content.contains("[Unit]"));
        assert!(content.contains("Description=test-service"));
        assert!(content.contains("[Service]"));
        assert!(content.contains("ExecStart=/usr/bin/test --flag value"));
        assert!(content.contains("WorkingDirectory=/tmp"));
        assert!(content.contains("Restart=always"));
        assert!(content.contains("[Install]"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    fn test_generate_systemd_service_minimal() {
        let details = ServiceDetails {
            name: "minimal-service".to_string(),
            path: String::new(),
            enabled: true,
            running: false,
            program: None,
            arguments: vec!["/bin/echo".to_string(), "hello".to_string()],
            working_directory: None,
            run_at_load: false,
            keep_alive: false,
        };

        let content = generate_systemd_service(&details);

        assert!(content.contains("[Unit]"));
        assert!(content.contains("Description=minimal-service"));
        assert!(content.contains("[Service]"));
        assert!(content.contains("ExecStart=/bin/echo hello"));
        assert!(!content.contains("WorkingDirectory="));
        assert!(!content.contains("Restart="));
        assert!(!content.contains("[Install]"));
        assert!(!content.contains("WantedBy="));
    }

    #[test]
    fn test_generate_systemd_service_no_args() {
        let details = ServiceDetails {
            name: "no-args-service".to_string(),
            path: String::new(),
            enabled: true,
            running: false,
            program: Some("/usr/bin/python3".to_string()),
            arguments: vec![],
            working_directory: Some("/home/user/app".to_string()),
            run_at_load: true,
            keep_alive: false,
        };

        let content = generate_systemd_service(&details);

        assert!(content.contains("ExecStart=/usr/bin/python3"));
        assert!(content.contains("WorkingDirectory=/home/user/app"));
        assert!(content.contains("[Install]"));
        assert!(content.contains("WantedBy=default.target"));
        assert!(!content.contains("Restart="));
    }

    #[test]
    fn test_resolve_binary_path_absolute() {
        let result = resolve_binary_path("/bin/ls");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/bin/ls");
    }

    #[test]
    fn test_resolve_binary_path_relative() {
        let result = resolve_binary_path("ls");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.starts_with('/'));
        assert!(path.ends_with("ls"));
    }

    #[test]
    fn test_resolve_binary_path_nonexistent() {
        let result = resolve_binary_path("nonexistent_binary_12345");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not found in PATH"));
    }

    #[test]
    fn test_generate_command_systemd_format() {
        // Test the Generate command with systemd format
        let generate_cmd = Generate {
            format: Format::Systemd,
        };

        // Create test service details
        let details = ServiceDetails {
            name: "example-service".to_string(),
            path: String::new(),
            enabled: true,
            running: false,
            program: Some("/usr/bin/node".to_string()),
            arguments: vec!["app.js".to_string(), "--port=3000".to_string()],
            working_directory: Some("/opt/myapp".to_string()),
            run_at_load: true,
            keep_alive: true,
        };

        let content = generate_systemd_service(&details);

        // Verify the generated content
        assert!(content.contains("[Unit]"));
        assert!(content.contains("Description=example-service"));
        assert!(content.contains("[Service]"));
        assert!(content.contains("ExecStart=/usr/bin/node app.js --port=3000"));
        assert!(content.contains("WorkingDirectory=/opt/myapp"));
        assert!(content.contains("Restart=always"));
        assert!(content.contains("[Install]"));
        assert!(content.contains("WantedBy=default.target"));

        // Verify format enum
        assert!(matches!(generate_cmd.format, Format::Systemd));
    }

    #[test]
    fn test_generate_command_native_format() {
        // Test the Generate command with native format
        let generate_cmd = Generate {
            format: Format::Native,
        };

        // Verify format enum
        assert!(matches!(generate_cmd.format, Format::Native));
    }
}
