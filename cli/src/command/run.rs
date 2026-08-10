use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Run {
    #[arg(help = "Name of the timer or service to run")]
    pub name: String,
}

impl Run {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Existence check only: running a job is valid whether or not it has a
        // schedule, and either way it leaves the unit's started state alone.
        platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?;

        print!("Running '{}' now...", self.name);
        super::run_and_verify(
            || platform::run_service_now(&resolved_name),
            || platform::verify_run_service_now(&resolved_name),
        )
    }
}
