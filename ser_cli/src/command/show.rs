use anyhow::Result;
use clap::Args;

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct Show {
    #[arg(help = "Name of the service to show")]
    pub name: String,
}

impl Show {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;
        let details = platform::get_service_details(&resolved_name)?;

        println!("Service: {}", details.name);
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

        if let Some(ref program) = details.program {
            println!("Program: {}", program);
        }

        if !details.arguments.is_empty() {
            println!("Arguments: {}", details.arguments.join(" "));
        }

        if let Some(ref wd) = details.working_directory {
            println!("Working Directory: {}", wd);
        }

        println!(
            "Run at Load: {}",
            if details.run_at_load { "Yes" } else { "No" }
        );
        println!(
            "Keep Alive: {}",
            if details.keep_alive { "Yes" } else { "No" }
        );

        Ok(())
    }
}
