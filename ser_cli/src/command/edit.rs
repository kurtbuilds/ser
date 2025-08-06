use anyhow::Result;
use clap::Args;
use std::process::Command;

use ser_lib::platform;

#[derive(Debug, Args)]
pub struct Edit {
    #[arg(help = "Name of the service to edit")]
    pub name: String,
    #[arg(short, long, help = "Editor to use (default: $EDITOR or vim)")]
    pub editor: Option<String>,
}

impl Edit {
    pub fn run(&self) -> Result<()> {
        let resolved_name = platform::resolve_service_name(&self.name)?;
        let service_path = platform::get_service_file_path(&resolved_name)?;

        let editor = self
            .editor
            .clone()
            .or_else(|| std::env::var("EDITOR").ok())
            .unwrap_or_else(|| "vim".to_string());

        let mut cmd = Command::new(&editor);
        cmd.arg(&service_path);

        let status = cmd.status()?;

        if !status.success() {
            return Err(anyhow::anyhow!("Editor exited with non-zero status"));
        }

        println!("Service file edited: {service_path}");
        Ok(())
    }
}
