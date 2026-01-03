use anyhow::Result;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm};

use serlib::platform;

#[derive(Debug, Args)]
pub struct New {
    command: Vec<String>,
}

impl New {
    pub fn run(&self) -> Result<()> {
        println!("Creating a new service...\n");
        let theme = ColorfulTheme::default();
        let details =
            crate::interactive::collect_service_details(&theme, self.command.clone(), true)?;

        let is_scheduled = details.schedule.is_some();

        // Create the service (and timer on Linux if scheduled)
        platform::create_service(&details)?;

        if is_scheduled {
            let schedule_display = details
                .schedule
                .as_ref()
                .map(|s| s.display())
                .unwrap_or_default();
            println!(
                "Scheduled service '{}' created successfully (schedule: {}).",
                details.name, schedule_display
            );
            #[cfg(target_os = "linux")]
            println!("Timer file: /etc/systemd/system/{}.timer", details.name);
        } else {
            println!("Service '{}' created successfully.", details.name);
        }

        // Ask if user wants to start/enable it now
        let prompt = if is_scheduled {
            "Enable the timer now?"
        } else {
            "Start the service now?"
        };

        let start_now = Confirm::with_theme(&theme)
            .with_prompt(prompt)
            .default(true)
            .interact()?;

        if start_now {
            if is_scheduled {
                print!("Enabling timer '{}'...", details.name);
            } else {
                print!("Starting service '{}'...", details.name);
            }
            platform::start_service(&details.name)?;
            println!(" done.");
        }

        Ok(())
    }
}
