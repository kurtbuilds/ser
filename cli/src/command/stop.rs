use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Stop {
    #[arg(help = "Name of the service to stop")]
    pub name: String,
}

impl Stop {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Check if service exists and is running
        match platform::get_service_details(&resolved_name) {
            Ok(details) => {
                if !details.running {
                    println!("Service '{}' is already stopped.", self.name);
                    return Ok(());
                }
            }
            Err(_) => {
                return Err(anyhow!("Service '{}' not found.", self.name));
            }
        }

        print!("Stopping service '{}'...", self.name);
        platform::stop_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
