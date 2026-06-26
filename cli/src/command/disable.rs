use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Disable {
    #[arg(help = "Name of the service or timer to disable")]
    pub name: String,
}

impl Disable {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // If the unit has a schedule, disabling it disarms the timer; otherwise
        // it disables the service. `stop_service` routes to the timer when present.
        let is_timer = platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?
            .service
            .schedule
            .is_some();

        if is_timer {
            print!("Disabling timer '{}'...", self.name);
        } else {
            print!("Disabling service '{}'...", self.name);
        }
        platform::stop_service(&resolved_name)?;
        println!(" done.");

        Ok(())
    }
}
