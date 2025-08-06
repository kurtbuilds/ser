use anyhow::Result;
use clap::{Parser, Subcommand};

mod command;

#[derive(Parser)]
#[command(name = "ser")]
#[command(about = "A CLI tool for managing background services")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "List background services")]
    #[command(alias = "status")]
    List(command::List),
    #[command(about = "Show detailed information about a service")]
    Show(command::Show),
    #[command(about = "Start a service")]
    Start(command::Start),
    #[command(about = "Stop a service")]
    Stop(command::Stop),
    #[command(about = "Restart a service")]
    Restart(command::Restart),
    #[command(about = "Create a new service interactively")]
    #[command(alias = "create")]
    New(command::New),
    #[command(about = "Generate service file content to stdout")]
    Generate(command::Generate),
    #[command(about = "Edit a service file")]
    Edit(command::Edit),
    #[command(about = "Show logs for a service")]
    Logs(command::Logs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List(list_cmd) => list_cmd.run()?,
        Commands::Show(show_cmd) => show_cmd.run()?,
        Commands::Start(start_cmd) => start_cmd.run()?,
        Commands::Stop(stop_cmd) => stop_cmd.run()?,
        Commands::Restart(restart_cmd) => restart_cmd.run()?,
        Commands::New(new_cmd) => new_cmd.run()?,
        Commands::Generate(generate_cmd) => generate_cmd.run()?,
        Commands::Edit(edit_cmd) => edit_cmd.run()?,
        Commands::Logs(logs_cmd) => logs_cmd.run()?,
    }
    Ok(())
}
