use anyhow::Result;
use clap::Args;

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct Logs {
    #[arg(help = "Name of the service to show logs for")]
    pub name: String,
    #[arg(
        short = 'n',
        long,
        default_value = "50",
        help = "Number of lines to show"
    )]
    pub lines: u32,
    #[arg(short, long, help = "Follow log output (like tail -f)")]
    pub follow: bool,
}

impl Logs {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;
        platform::show_service_logs(&resolved_name, self.lines, self.follow)?;
        Ok(())
    }
}
