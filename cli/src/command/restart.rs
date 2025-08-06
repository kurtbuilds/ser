use anyhow::{anyhow, Result};
use clap::Args;

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct Restart {
    #[arg(help = "Name of the service to restart")]
    pub name: String,
}

impl Restart {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Check if service exists
        if platform::get_service_details(&resolved_name).is_err() {
            return Err(anyhow!("Service '{}' not found.", self.name));
        }

        print!("Restarting service '{}'...", self.name);
        platform::restart_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
