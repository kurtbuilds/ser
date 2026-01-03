use anyhow::Result;
use clap::Args;
use std::collections::HashSet;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use serlib::{
    platform::{self, ListLevel, ServiceRef},
    systemd::MANAGED_BY_COMMENT,
};

#[derive(Debug, Args)]
pub struct List {
    #[arg(short, long, help = "Show all services (system and user)")]
    pub all: bool,
}

#[derive(Tabled)]
struct ServiceRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Type")]
    service_type: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Enabled")]
    enabled: String,
    #[tabled(rename = "Schedule")]
    schedule: String,
    #[tabled(rename = "Path")]
    path: String,
}

impl List {
    pub fn run(&self) -> Result<()> {
        let level = if self.all {
            ListLevel::System
        } else {
            ListLevel::Default
        };
        let mut services = platform::list_services(level)?;
        services.sort_by(|a, b| a.name.cmp(&b.name));
        if services.is_empty() {
            eprintln!("No services found.");
            return Ok(());
        }

        if matches!(level, ListLevel::Default) {
            services.retain(|s| {
                if s.path.contains("systemd") {
                    if let Ok(content) = std::fs::read_to_string(&s.path) {
                        content.starts_with(MANAGED_BY_COMMENT)
                    } else {
                        false
                    }
                } else {
                    true
                }
            });
        }

        // Filter out .timer files that have a matching .service file
        // to avoid duplicate display (we'll show the service with timer info instead)
        let timer_base_names: HashSet<_> = services
            .iter()
            .filter(|s| s.name.ends_with(".timer"))
            .map(|s| s.name.trim_end_matches(".timer").to_string())
            .collect();

        let service_base_names: HashSet<_> = services
            .iter()
            .filter(|s| s.name.ends_with(".service"))
            .map(|s| s.name.trim_end_matches(".service").to_string())
            .collect();

        services.retain(|s| {
            if s.name.ends_with(".timer") {
                // Keep timer only if there's no matching service
                let base_name = s.name.trim_end_matches(".timer");
                !service_base_names.contains(base_name)
            } else {
                true
            }
        });

        let rows: Vec<ServiceRow> = services
            .into_iter()
            .map(|service| {
                let display_name = if service.name.starts_with("homebrew.mxcl.") {
                    service
                        .name
                        .strip_prefix("homebrew.mxcl.")
                        .unwrap_or(&service.name)
                        .to_string()
                } else {
                    service.name.clone()
                };

                // Determine status based on running state
                let is_running = platform::is_service_running(&service.name).unwrap_or(false);
                let status = if is_running { "running" } else { "stopped" }.to_string();
                let enabled = if service.enabled { "true" } else { "false" }.to_string();

                // Determine type and schedule info
                let (service_type, schedule) =
                    get_service_type_and_schedule(&service, &timer_base_names);

                ServiceRow {
                    name: display_name,
                    service_type,
                    status,
                    enabled,
                    schedule,
                    path: service.path,
                }
            })
            .collect();

        // Check if output is piped (not a terminal)
        if atty::isnt(atty::Stream::Stdout) {
            // If piped, print without headers
            for row in &rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    row.name, row.service_type, row.status, row.enabled, row.schedule, row.path
                );
            }
        } else {
            // If terminal, print table with headers but no borders
            let mut table = Table::new(rows);
            table.with(Style::blank()).with(Padding::zero());
            println!("{table}");
        }
        Ok(())
    }
}

fn get_service_type_and_schedule(
    service: &ServiceRef,
    #[allow(unused_variables)] timer_base_names: &HashSet<String>,
) -> (String, String) {
    // Check if this is a timer or has an associated timer
    #[allow(unused_variables)]
    let base_name = service
        .name
        .trim_end_matches(".service")
        .trim_end_matches(".timer");

    #[cfg(target_os = "linux")]
    {
        // Check if there's an associated timer
        if timer_base_names.contains(base_name) || service.name.ends_with(".timer") {
            // Try to get next trigger time
            if let Ok(Some(next)) = serlib::platform::get_timer_next_trigger(base_name) {
                return ("timer".to_string(), next);
            }
            return ("timer".to_string(), "-".to_string());
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Check if plist has StartCalendarInterval
        if let Ok(details) = platform::get_service_details(&service.name) {
            if let Some(schedule) = &details.service.schedule {
                return ("timer".to_string(), schedule.display());
            }
        }
    }

    ("service".to_string(), "-".to_string())
}
