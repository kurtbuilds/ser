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

        // Check if service exists, and whether it's a scheduled (timer) unit.
        let details = platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?;

        // For a timer, `start` runs the job once now rather than arming the
        // schedule — use `ser enable` to turn the schedule on.
        if details.service.schedule.is_some() {
            print!("Running '{}' now...", self.name);
            platform::run_service_now(&resolved_name)?;
            println!(" done.");
            return Ok(());
        }

        if details.running {
            println!("Service '{}' is already running.", self.name);
            return Ok(());
        }

        print!("Starting service '{}'...", self.name);
        platform::start_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
