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
            Format::Native => serlib::platform::generate_file(&details)?,
            Format::Systemd => serlib::systemd::generate_file(&details)?,
        };
        println!("{content}");

        let base_path = PathBuf::from("/etc/systemd/system");
        eprintln!(
            "{} is the suggested file path.",
            base_path
                .join(format!("{}.service", details.name))
                .display()
        );

        // Also generate timer file if scheduled (for systemd format)
        if details.schedule.is_some() && matches!(self.format, Format::Systemd) {
            println!("\n# --- Timer File ---\n");
            let timer_content = serlib::systemd::generate_timer_file(&details)?;
            println!("{timer_content}");
            eprintln!(
                "{} is the suggested timer file path.",
                base_path.join(format!("{}.timer", details.name)).display()
            );
        }

        Ok(())
    }
}
