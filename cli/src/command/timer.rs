use std::collections::HashSet;

use anyhow::{anyhow, Result};
use chrono::NaiveDateTime;
use clap::{Args, Subcommand};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Confirm;
use tabled::{
    settings::{Padding, Style},
    Table, Tabled,
};

use crate::interactive::ServiceKind;
use serlib::platform::{self, ListLevel};
use serlib::Schedule;

#[derive(Debug, Args)]
pub struct Timer {
    #[command(subcommand)]
    command: TimerCommand,
}

#[derive(Debug, Subcommand)]
enum TimerCommand {
    #[command(about = "Create a new timer interactively")]
    #[command(alias = "new")]
    Create(Create),
    #[command(about = "List scheduled timers")]
    #[command(alias = "ls")]
    List(List),
    #[command(about = "Show schedule and upcoming runs for a timer")]
    Show(Show),
    #[command(about = "Show upcoming run times")]
    Next(Next),
    #[command(about = "Change a timer's schedule")]
    Edit(Edit),
    #[command(about = "Remove a timer")]
    #[command(alias = "remove")]
    Rm(Rm),
}

impl Timer {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            TimerCommand::Create(cmd) => cmd.run(),
            TimerCommand::List(cmd) => cmd.run(),
            TimerCommand::Show(cmd) => cmd.run(),
            TimerCommand::Next(cmd) => cmd.run(),
            TimerCommand::Edit(cmd) => cmd.run(),
            TimerCommand::Rm(cmd) => cmd.run(),
        }
    }
}

#[derive(Debug, Args)]
pub struct Create {
    command: Vec<String>,
}

impl Create {
    pub fn run(&self) -> Result<()> {
        println!("Creating a new timer...\n");
        let theme = ColorfulTheme::default();
        let details = crate::interactive::collect_service_details(
            &theme,
            self.command.clone(),
            true,
            ServiceKind::Timer,
        )?;
        crate::command::new::finish_create(&theme, details)
    }
}

#[derive(Debug, Args)]
pub struct List {
    #[arg(short, long, help = "Include system timers, not just managed ones")]
    all: bool,
}

#[derive(Tabled)]
struct TimerRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Schedule")]
    schedule: String,
    #[tabled(rename = "Next run")]
    next: String,
    #[tabled(rename = "Enabled")]
    enabled: String,
    #[tabled(rename = "Status")]
    status: String,
}

impl List {
    pub fn run(&self) -> Result<()> {
        let timers = collect_timers(self.all)?;
        if timers.is_empty() {
            eprintln!("No timers found.");
            return Ok(());
        }

        let now = chrono::Local::now().naive_local();
        let rows: Vec<TimerRow> = timers
            .iter()
            .map(|t| TimerRow {
                name: t.display_name.clone(),
                schedule: t.schedule.display(),
                next: format_next(t.schedule.next_fire_after(now)),
                enabled: if t.enabled { "true" } else { "false" }.to_string(),
                status: if t.running { "running" } else { "stopped" }.to_string(),
            })
            .collect();

        if atty::isnt(atty::Stream::Stdout) {
            for row in &rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    row.name, row.schedule, row.next, row.enabled, row.status
                );
            }
        } else {
            let mut table = Table::new(rows);
            table.with(Style::blank()).with(Padding::zero());
            println!("{table}");
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct Show {
    #[arg(help = "Name of the timer to show")]
    name: String,
}

impl Show {
    pub fn run(&self) -> Result<()> {
        let resolved = platform::resolve_service_name(&self.name)?;
        let details = platform::get_service_details(&resolved)?;
        let schedule = details
            .service
            .schedule
            .ok_or_else(|| anyhow!("'{}' is not a timer (no schedule configured)", self.name))?;

        // macOS plist parsing leaves the name blank, so prefer the resolved
        // unit/label name and only fall back to the parsed Description.
        let display_name = if details.service.name.is_empty() {
            platform::normalize_service_name(&resolved).to_string()
        } else {
            details.service.name.clone()
        };
        println!("Timer: {}", display_name);
        println!("Path: {}", details.path);
        println!("Schedule: {}", schedule.display());
        match &schedule {
            Schedule::Calendar(c) => println!("OnCalendar: {}", c.to_systemd_oncalendar()),
            Schedule::Interval(secs) => {
                println!("Interval: {}", Schedule::interval_to_systemd(*secs))
            }
        }
        println!("Enabled: {}", if details.enabled { "Yes" } else { "No" });

        if !details.service.program.is_empty() {
            let mut cmd = details.service.program.clone();
            if !details.service.arguments.is_empty() {
                cmd.push(' ');
                cmd.push_str(&details.service.arguments.join(" "));
            }
            println!("Runs: {}", cmd);
        }

        match &schedule {
            // Interval timers fire relative to activation, so wall-clock times
            // aren't computable here.
            Schedule::Interval(secs) => {
                println!(
                    "Upcoming: runs every {}",
                    Schedule::interval_to_systemd(*secs)
                );
            }
            Schedule::Calendar(_) => {
                let upcoming = upcoming_runs(&schedule, 5);
                if upcoming.is_empty() {
                    println!("Upcoming: none");
                } else {
                    println!("Upcoming:");
                    for dt in upcoming {
                        println!("  {}", format_dt(dt));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct Next {
    #[arg(help = "Timer name (omit to show the next run of every timer)")]
    name: Option<String>,
    #[arg(
        short,
        long,
        default_value = "5",
        help = "Number of upcoming runs to show for a single timer"
    )]
    count: usize,
}

impl Next {
    pub fn run(&self) -> Result<()> {
        match &self.name {
            // Next N runs for one timer.
            Some(name) => {
                let resolved = platform::resolve_service_name(name)?;
                let details = platform::get_service_details(&resolved)?;
                let schedule = details
                    .service
                    .schedule
                    .ok_or_else(|| anyhow!("'{}' is not a timer (no schedule configured)", name))?;
                let runs = upcoming_runs(&schedule, self.count);
                if runs.is_empty() {
                    eprintln!("No upcoming runs.");
                } else {
                    for dt in runs {
                        println!("{}", format_dt(dt));
                    }
                }
            }
            // Next single run of every timer, soonest first.
            None => {
                let timers = collect_timers(false)?;
                let now = chrono::Local::now().naive_local();
                let mut upcoming: Vec<(NaiveDateTime, String)> = timers
                    .iter()
                    .filter_map(|t| {
                        t.schedule
                            .next_fire_after(now)
                            .map(|dt| (dt, t.display_name.clone()))
                    })
                    .collect();
                upcoming.sort_by_key(|(dt, _)| *dt);

                if upcoming.is_empty() {
                    eprintln!("No timers found.");
                } else {
                    for (dt, name) in upcoming {
                        println!("{}  {}", format_dt(dt), name);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct Edit {
    #[arg(help = "Name of the timer to edit")]
    name: String,
}

impl Edit {
    pub fn run(&self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let resolved = platform::resolve_service_name(&self.name)?;
        let mut details = platform::get_service_details(&resolved)?.service;

        let current = details
            .schedule
            .as_ref()
            .ok_or_else(|| anyhow!("'{}' is not a timer (no schedule configured)", self.name))?;
        println!("Current schedule: {}\n", current.display());

        let new_schedule = crate::interactive::collect_schedule(&theme)?
            .ok_or_else(|| anyhow!("A timer requires a schedule"))?;
        details.schedule = Some(new_schedule.clone());

        // Regenerate the unit/plist with the new schedule, preserving everything else.
        platform::create_service(&details)?;
        println!("\nUpdated schedule: {}", new_schedule.display());

        let apply = Confirm::with_theme(&theme)
            .with_prompt("Apply now (restart the timer)?")
            .default(true)
            .interact()?;
        if apply {
            platform::restart_service(&resolved)?;
            println!("Timer restarted.");
        } else {
            println!("Run `ser enable {}` to apply the new schedule.", self.name);
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct Rm {
    #[arg(help = "Name of the timer to remove")]
    name: String,
    #[arg(short = 'y', long, help = "Skip the confirmation prompt")]
    yes: bool,
}

impl Rm {
    pub fn run(&self) -> Result<()> {
        let resolved = platform::resolve_service_name(&self.name)?;

        if !self.yes {
            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Remove timer '{}'?", self.name))
                .default(false)
                .interact()?;
            if !confirmed {
                println!("Aborted.");
                return Ok(());
            }
        }

        platform::remove_service(&resolved)?;
        println!("Removed timer '{}'.", self.name);
        Ok(())
    }
}

struct TimerEntry {
    display_name: String,
    schedule: Schedule,
    enabled: bool,
    running: bool,
}

/// Gather every scheduled unit on the system as a `TimerEntry`. The schedule is
/// read uniformly via `get_service_details`, which on Linux pairs the `.service`
/// with its `.timer` unit.
fn collect_timers(all: bool) -> Result<Vec<TimerEntry>> {
    let level = if all {
        ListLevel::System
    } else {
        ListLevel::Default
    };
    let mut services = platform::list_services(level)?;
    services.sort_by(|a, b| a.name.cmp(&b.name));
    // The schedule lives on the service; drop bare `.timer` entries to avoid
    // parsing a timer unit as if it were a service.
    services.retain(|s| !s.name.ends_with(".timer"));

    let mut timers = Vec::new();
    let mut seen = HashSet::new();
    for service in services {
        let key = platform::normalize_service_name(&service.name).to_string();
        if !seen.insert(key) {
            continue;
        }
        let Ok(details) = platform::get_service_details(&service.name) else {
            continue;
        };
        let Some(schedule) = details.service.schedule.clone() else {
            continue;
        };
        timers.push(TimerEntry {
            display_name: platform::normalize_service_name(&service.name).to_string(),
            schedule,
            enabled: details.enabled,
            running: details.running,
        });
    }
    Ok(timers)
}

/// The next `count` fire times of a schedule, starting from now.
fn upcoming_runs(schedule: &Schedule, count: usize) -> Vec<NaiveDateTime> {
    let mut runs = Vec::new();
    let mut cursor = chrono::Local::now().naive_local();
    for _ in 0..count {
        match schedule.next_fire_after(cursor) {
            Some(dt) => {
                runs.push(dt);
                cursor = dt;
            }
            None => break,
        }
    }
    runs
}

fn format_next(dt: Option<NaiveDateTime>) -> String {
    dt.map(format_dt).unwrap_or_else(|| "-".to_string())
}

fn format_dt(dt: NaiveDateTime) -> String {
    dt.format("%a %Y-%m-%d %H:%M").to_string()
}
