use anyhow::Result;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};
use std::path::Path;

use ser_lib::platform::{self, ServiceDetails};

#[derive(Debug, Args)]
pub struct Add {}

impl Add {
    pub fn run(&self) -> Result<()> {
        println!("Adding a new service...\n");

        let theme = ColorfulTheme::default();

        // Get command with arguments
        let command_input: String = Input::with_theme(&theme)
            .with_prompt("Command to run (with arguments)")
            .validate_with(|input: &String| -> Result<(), &str> {
                if input.trim().is_empty() {
                    Err("Command cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()?;

        // Parse command into program and arguments
        let command_parts: Vec<String> = command_input
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if command_parts.is_empty() {
            return Err(anyhow::anyhow!("Command cannot be empty"));
        }

        // Infer service name from the binary (first argument)
        let binary_path = &command_parts[0];
        let inferred_name = Path::new(binary_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("service")
            .to_string();

        // Ask for service name with inferred default
        let service_name: String = Input::with_theme(&theme)
            .with_prompt("Service name")
            .default(inferred_name)
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

        // Split command into program and arguments
        let program = Some(command_parts[0].clone());
        let arguments = if command_parts.len() > 1 {
            command_parts[1..].to_vec()
        } else {
            Vec::new()
        };

        // Create service details with sensible defaults
        let details = ServiceDetails {
            name: service_name.clone(),
            path: String::new(), // Will be set during creation
            enabled: true,
            running: false,
            program,
            arguments,
            working_directory: None,
            run_at_load: true,
            keep_alive: true,
        };

        // Show summary
        println!("\nService configuration:");
        println!("  Name: {}", details.name);
        if let Some(ref prog) = details.program {
            println!("  Program: {prog}");
        }
        if !details.arguments.is_empty() {
            println!("  Arguments: {}", details.arguments.join(" "));
        }

        // Create the service
        print!("Creating service '{service_name}'...");
        platform::create_service(&details)?;
        println!(" done.");

        // Ask if user wants to start it now (default yes)
        let start_now = Confirm::with_theme(&theme)
            .with_prompt("Start the service now?")
            .default(true)
            .interact()?;

        if start_now {
            print!("Starting service '{service_name}'...");
            platform::start_service(&service_name)?;
            println!(" done.");
        }

        println!("Service '{service_name}' added successfully.");

        Ok(())
    }
}
