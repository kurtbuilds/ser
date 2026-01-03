use anyhow::Result;
use clap::Args;

use serlib::platform;

#[derive(Debug, Args)]
pub struct Show {
    #[arg(help = "Name of the service to show")]
    pub name: String,
}

impl Show {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;
        let details = platform::get_service_details(&resolved_name)?;

        println!("Service: {}", details.service.name);
        println!("Path: {}", details.path);
        println!(
            "Status: {}",
            if details.running {
                "Running"
            } else {
                "Stopped"
            }
        );
        println!("Enabled: {}", if details.enabled { "Yes" } else { "No" });

        if !details.service.program.is_empty() {
            println!("Program: {}", details.service.program);
        }

        if !details.service.arguments.is_empty() {
            println!("Arguments: {}", details.service.arguments.join(" "));
        }

        if let Some(ref wd) = details.service.working_directory {
            println!("Working Directory: {}", wd);
        }

        println!(
            "Run at Load: {}",
            if details.service.run_at_load {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "Keep Alive: {}",
            if details.service.keep_alive {
                "Yes"
            } else {
                "No"
            }
        );

        Ok(())
    }
}
