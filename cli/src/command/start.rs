use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Start {
    #[arg(help = "Name of the service to start")]
    pub name: String,
}

impl Start {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Check if service exists and is already running
        match platform::get_service_details(&resolved_name) {
            Ok(details) => {
                if details.running {
                    println!("Service '{}' is already running.", self.name);
                    return Ok(());
                }
            }
            Err(_) => {
                return Err(anyhow!("Service '{}' not found.", self.name));
            }
        }

        print!("Starting service '{}'...", self.name);
        platform::start_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
