use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Enable {
    #[arg(help = "Name of the service or timer to enable")]
    pub name: String,
}

impl Enable {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // If the unit has a schedule, enabling it arms the timer; otherwise it
        // enables the service. `start_service` routes to the timer when present.
        let is_timer = platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?
            .service
            .schedule
            .is_some();

        if is_timer {
            print!("Enabling timer '{}'...", self.name);
        } else {
            print!("Enabling service '{}'...", self.name);
        }
        platform::start_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
