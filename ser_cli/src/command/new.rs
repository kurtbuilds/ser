use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::process::Command;

use ser_lib::platform::{self, ServiceDetails};

#[derive(Debug, Args)]
pub struct New {}

fn resolve_binary_path(binary: &str) -> Result<String> {
    // If it's already an absolute path, validate it exists and return as-is
    if binary.starts_with('/') {
        if std::path::Path::new(binary).exists() {
            return Ok(binary.to_string());
        } else {
            return Err(anyhow::anyhow!("Binary '{}' does not exist", binary));
        }
    }

    // Try using 'which' command first
    if let Ok(output) = Command::new("which").arg(binary).output() {
        if output.status.success() {
            let path = String::from_utf8(output.stdout)
                .context("Invalid UTF-8 in which command output")?
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

impl New {
    pub fn run(&self) -> Result<()> {
        println!("Creating a new service...\n");

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
                .with_context(|| format!("Failed to resolve binary path for '{prog}'"))?;

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
                let resolved_binary = resolve_binary_path(binary_name).with_context(|| {
                    format!("Failed to resolve binary path for '{binary_name}'")
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
        let details = ServiceDetails {
            name: name.clone(),
            path: String::new(), // Will be set during creation
            enabled: true,
            running: false,
            program,
            arguments,
            working_directory,
            run_at_load,
            keep_alive,
        };

        // Show summary
        println!("\nService configuration:");
        println!("  Name: {}", details.name);
        if let Some(ref prog) = details.program {
            println!("  Program: {}", prog);
        }
        if !details.arguments.is_empty() {
            println!("  Arguments: {}", details.arguments.join(" "));
        }
        if let Some(ref wd) = details.working_directory {
            println!("  Working Directory: {}", wd);
        }
        println!("  Run at Load: {}", details.run_at_load);
        println!("  Keep Alive: {}", details.keep_alive);

        let confirm = Confirm::with_theme(&theme)
            .with_prompt("Create this service?")
            .default(true)
            .interact()?;

        if !confirm {
            println!("Service creation cancelled.");
            return Ok(());
        }

        // Create the service
        platform::create_service(&details)?;
        println!("Service '{}' created successfully.", name);

        // Ask if user wants to start it now
        let start_now = Confirm::with_theme(&theme)
            .with_prompt("Start the service now?")
            .default(true)
            .interact()?;

        if start_now {
            print!("Starting service '{}'...", name);
            platform::start_service(&name)?;
            println!(" done.");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
