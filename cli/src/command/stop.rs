use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Stop {
    #[arg(help = "Name of the service or timer to stop")]
    pub name: String,
}

impl Stop {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Check if service exists, and whether it's a scheduled (timer) unit.
        let details = platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?;

        // Stopping a timer disarms its schedule.
        let is_timer = details.service.schedule.is_some();

        // A timer reads as "not running" between fires while its schedule is
        // still armed, so short-circuiting here would leave the timer on.
        if !is_timer && !details.running {
            println!("Service '{}' is already stopped.", self.name);
            return Ok(());
        }

        if is_timer {
            print!("Stopping timer '{}'...", self.name);
        } else {
            print!("Stopping service '{}'...", self.name);
        }
        platform::stop_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
