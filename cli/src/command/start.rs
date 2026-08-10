use anyhow::{anyhow, Result};
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Start {
    #[arg(help = "Name of the service or timer to start")]
    pub name: String,
}

impl Start {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;

        // Check if service exists, and whether it's a scheduled (timer) unit.
        let details = platform::get_service_details(&resolved_name)
            .map_err(|_| anyhow!("Service '{}' not found.", self.name))?;

        // Starting a timer arms its schedule; `ser run` fires the job once now.
        let is_timer = details.service.schedule.is_some();

        // A timer reads as "not running" between fires even when armed, so this
        // shortcut only makes sense for services. Starting an already-armed
        // timer is harmless — both platforms treat it as a no-op.
        if !is_timer && details.running {
            println!("Service '{}' is already running.", self.name);
            return Ok(());
        }

        if is_timer {
            print!("Starting timer '{}'...", self.name);
        } else {
            print!("Starting service '{}'...", self.name);
        }
        super::run_and_verify(
            || platform::start_service(&resolved_name),
            || platform::verify_service_started(&resolved_name),
        )
    }
}
