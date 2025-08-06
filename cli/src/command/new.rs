use anyhow::Result;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm};

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct New {
    command: Vec<String>,
}

impl New {
    pub fn run(&self) -> Result<()> {
        println!("Creating a new service...\n");
        let theme = ColorfulTheme::default();
        let details = crate::interactive::collect_service_details(&theme, self.command.clone())?;

        // Create the service
        platform::create_service(&details)?;
        println!("Service '{}' created successfully.", details.name);

        // Ask if user wants to start it now
        let start_now = Confirm::with_theme(&theme)
            .with_prompt("Start the service now?")
            .default(true)
            .interact()?;

        if start_now {
            print!("Starting service '{}'...", details.name);
            platform::start_service(&details.name)?;
            println!(" done.");
        }

        Ok(())
    }
}
