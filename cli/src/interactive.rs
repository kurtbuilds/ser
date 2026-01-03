use anyhow::Context;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use serlib::{CalendarSchedule, ServiceDetails};
use std::process::Command;

pub fn collect_service_details(
    theme: &ColorfulTheme,
    mut command: Vec<String>,
    validate: bool,
) -> anyhow::Result<ServiceDetails> {
    println!("Creating service configuration...\n");

    if command.is_empty() {
        let c = Input::with_theme(theme)
            .with_prompt("Command to execute")
            .validate_with(|input: &String| -> anyhow::Result<(), &str> {
                if input.trim().is_empty() {
                    Err("Command cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()?;
        command = c.split_whitespace().map(String::from).collect();
    }

    let program = command.remove(0);
    let arguments = command;

    let bin_path = if validate {
        resolve_binary_path(&program)
            .with_context(|| format!("Failed to resolve binary path for '{}'", program))?
    } else {
        program
    };

    let default_basename = bin_path.rsplit('/').next().unwrap().to_string();
    // Service name
    let name: String = Input::with_theme(theme)
        .with_prompt("Service name (e.g., com.example.myservice)")
        .default(default_basename)
        .validate_with(|input: &String| -> anyhow::Result<(), &str> {
            if input.trim().is_empty() {
                Err("Service name cannot be empty")
            } else if input.contains(' ') {
                Err("Service name cannot contain spaces")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    let working_directory = {
        let input: String = Input::with_theme(theme)
            .with_prompt("Working directory path")
            .allow_empty(true)
            .interact_text()?;
        if input.trim().is_empty() {
            None
        } else {
            Some(input.trim().to_string())
        }
    };

    let env_file = {
        let input: String = Input::with_theme(theme)
            .with_prompt("Environment file path")
            .allow_empty(true)
            .interact_text()?;
        if input.trim().is_empty() {
            None
        } else {
            Some(input.trim().to_string())
        }
    };

    let env_vars = {
        let mut vars = Vec::new();
        loop {
            let kv: String = Input::with_theme(theme)
                .with_prompt("Environment variable key (or leave empty to finish)")
                .allow_empty(true)
                .interact_text()?;
            if kv.trim().is_empty() {
                break;
            }
            let Some((k, v)) = kv.split_once('=') else {
                eprintln!("Format is 'KEY=VALUE'. Please try again.");
                continue;
            };
            let key = k.trim().to_string();
            let value = v.trim().to_string();
            vars.push((key, value));
        }
        vars
    };
    // Run at load
    let run_at_load = Confirm::with_theme(theme)
        .with_prompt("Start automatically when system boots?")
        .default(true)
        .interact()?;

    // Keep alive
    let keep_alive = Confirm::with_theme(theme)
        .with_prompt("Restart automatically if it crashes?")
        .default(true)
        .interact()?;

    let after = {
        let networked = Confirm::with_theme(theme)
            .with_prompt("Networked service?")
            .default(true)
            .interact()?;
        if networked {
            vec![
                "network.target".to_string(),
                "network-online.target".to_string(),
            ]
        } else {
            Vec::new()
        }
    };

    // Schedule configuration
    let schedule = {
        let wants_schedule = Confirm::with_theme(theme)
            .with_prompt("Schedule this service to run at specific times?")
            .default(false)
            .interact()?;

        if wants_schedule {
            collect_schedule(theme)?
        } else {
            None
        }
    };

    Ok(ServiceDetails {
        name,
        program: bin_path,
        arguments,
        working_directory,
        run_at_load,
        keep_alive,
        env_file,
        env_vars,
        after,
        schedule,
    })
}

fn collect_schedule(theme: &ColorfulTheme) -> anyhow::Result<Option<CalendarSchedule>> {
    println!("\nSchedule configuration:");

    let choices = vec![
        "Daily at specific time",
        "Weekly on specific day",
        "Monthly on specific day",
        "Custom schedule",
    ];

    let selection = Select::with_theme(theme)
        .with_prompt("Schedule type")
        .items(&choices)
        .default(0)
        .interact()?;

    match selection {
        0 => {
            // Daily
            let hour = collect_hour(theme)?;
            let minute = collect_minute(theme)?;
            Ok(Some(CalendarSchedule {
                month: None,
                day: None,
                weekday: None,
                hour: Some(hour),
                minute: Some(minute),
            }))
        }
        1 => {
            // Weekly
            let weekday = collect_weekday(theme)?;
            let hour = collect_hour(theme)?;
            let minute = collect_minute(theme)?;
            Ok(Some(CalendarSchedule {
                month: None,
                day: None,
                weekday: Some(weekday),
                hour: Some(hour),
                minute: Some(minute),
            }))
        }
        2 => {
            // Monthly
            let day = collect_day_of_month(theme)?;
            let hour = collect_hour(theme)?;
            let minute = collect_minute(theme)?;
            Ok(Some(CalendarSchedule {
                month: None,
                day: Some(day),
                weekday: None,
                hour: Some(hour),
                minute: Some(minute),
            }))
        }
        3 => {
            // Custom
            println!("Leave fields empty for 'any' (like * in cron)\n");
            let month = collect_optional_number(theme, "Month (1-12)", 1, 12)?;
            let day = collect_optional_number(theme, "Day of month (1-31)", 1, 31)?;
            let weekday = collect_optional_weekday(theme)?;
            let hour = collect_optional_number(theme, "Hour (0-23)", 0, 23)?;
            let minute = collect_optional_number(theme, "Minute (0-59)", 0, 59)?;
            Ok(Some(CalendarSchedule {
                month,
                day,
                weekday,
                hour,
                minute,
            }))
        }
        _ => Ok(None),
    }
}

fn collect_hour(theme: &ColorfulTheme) -> anyhow::Result<u8> {
    let hour: u8 = Input::with_theme(theme)
        .with_prompt("Hour (0-23)")
        .default(0)
        .validate_with(|input: &u8| {
            if *input > 23 {
                Err("Hour must be 0-23")
            } else {
                Ok(())
            }
        })
        .interact_text()?;
    Ok(hour)
}

fn collect_minute(theme: &ColorfulTheme) -> anyhow::Result<u8> {
    let minute: u8 = Input::with_theme(theme)
        .with_prompt("Minute (0-59)")
        .default(0)
        .validate_with(|input: &u8| {
            if *input > 59 {
                Err("Minute must be 0-59")
            } else {
                Ok(())
            }
        })
        .interact_text()?;
    Ok(minute)
}

fn collect_weekday(theme: &ColorfulTheme) -> anyhow::Result<u8> {
    let days = vec![
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let selection = Select::with_theme(theme)
        .with_prompt("Day of week")
        .items(&days)
        .default(1) // Monday
        .interact()?;
    Ok(selection as u8)
}

fn collect_day_of_month(theme: &ColorfulTheme) -> anyhow::Result<u8> {
    let day: u8 = Input::with_theme(theme)
        .with_prompt("Day of month (1-31)")
        .default(1u8)
        .validate_with(|input: &u8| {
            if *input < 1 || *input > 31 {
                Err("Day must be 1-31")
            } else {
                Ok(())
            }
        })
        .interact_text()?;
    Ok(day)
}

fn collect_optional_number(
    theme: &ColorfulTheme,
    prompt: &str,
    min: u8,
    max: u8,
) -> anyhow::Result<Option<u8>> {
    let input: String = Input::with_theme(theme)
        .with_prompt(format!("{} (or empty for any)", prompt))
        .allow_empty(true)
        .interact_text()?;

    if input.trim().is_empty() {
        Ok(None)
    } else {
        let value: u8 = input
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid number"))?;
        if value < min || value > max {
            anyhow::bail!("Value must be between {} and {}", min, max);
        }
        Ok(Some(value))
    }
}

fn collect_optional_weekday(theme: &ColorfulTheme) -> anyhow::Result<Option<u8>> {
    let days = vec![
        "Any day",
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let selection = Select::with_theme(theme)
        .with_prompt("Day of week")
        .items(&days)
        .default(0)
        .interact()?;

    if selection == 0 {
        Ok(None)
    } else {
        Ok(Some((selection - 1) as u8))
    }
}

fn resolve_binary_path(binary: &str) -> anyhow::Result<String> {
    // If it's already an absolute path, validate it exists and return as-is
    if binary.starts_with('/') {
        return if std::path::Path::new(binary).exists() {
            Ok(binary.to_string())
        } else {
            Err(anyhow::anyhow!("Binary '{}' does not exist", binary))
        };
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
