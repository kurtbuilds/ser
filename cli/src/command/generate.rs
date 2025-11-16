use anyhow::Result;
use clap::{Args, ValueEnum};
use dialoguer::theme::ColorfulTheme;
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
pub enum Format {
    /// Generate native format for current platform
    Native,
    /// Generate systemd service file
    Systemd,
}

#[derive(Debug, Args)]
pub struct Generate {
    #[arg(long, default_value = "systemd", help = "Output format")]
    format: Format,
    command: Vec<String>,
}

impl Generate {
    pub fn run(&self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let details =
            crate::interactive::collect_service_details(&theme, self.command.clone(), false)?;

        let content = match self.format {
            Format::Native => ser_lib::platform::generate_file(&details)?,
            Format::Systemd => ser_lib::systemd::generate_file(&details)?,
        };
        println!("{content}");
        eprintln!(
            "{} is the suggested file path.",
            PathBuf::from("/etc/systemd/system")
                .join(format!("{}.service", details.name))
                .display()
        );
        Ok(())
    }
}
