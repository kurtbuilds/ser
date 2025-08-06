use anyhow::Result;
use clap::Args;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct List {
    #[arg(long, help = "Show all services (system and user)")]
    pub all: bool,
}

#[derive(Tabled)]
struct ServiceRow {
    #[tabled(rename = "Service Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Enabled")]
    enabled: String,
    #[tabled(rename = "Path")]
    path: String,
}

impl List {
    pub fn run(&self) -> Result<()> {
        let mut services = platform::list_services(self.all)?;
        services.sort_by(|a, b| a.name.cmp(&b.name));
        if services.is_empty() {
            eprintln!("No services found.");
            return Ok(());
        }

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

                ServiceRow {
                    name: display_name,
                    status,
                    enabled,
                    path: service.path,
                }
            })
            .collect();

        // Check if output is piped (not a terminal)
        if atty::isnt(atty::Stream::Stdout) {
            // If piped, print without headers
            for row in &rows {
                println!(
                    "{}\t{}\t{}\t{}",
                    row.name, row.status, row.enabled, row.path
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
