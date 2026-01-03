pub mod platform;
pub mod plist;
pub mod systemd;

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Set verbose mode. When enabled, all executed commands are printed to stderr.
pub fn set_verbose(verbose: bool) {
    VERBOSE.store(verbose, Ordering::SeqCst);
}

/// Print a command to stderr if verbose mode is enabled.
pub fn print_command(cmd: &Command) {
    if VERBOSE.load(Ordering::SeqCst) {
        let program = cmd.get_program().to_string_lossy();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        eprintln!("+ {} {}", program, args.join(" "));
    }
}

/// Represents a calendar-based schedule for running services.
/// Fields are optional - None means "any" (like * in cron).
#[derive(Debug, Clone, Default)]
pub struct CalendarSchedule {
    /// Month (1-12)
    pub month: Option<u8>,
    /// Day of month (1-31)
    pub day: Option<u8>,
    /// Day of week (0=Sunday, 1=Monday, ..., 6=Saturday)
    pub weekday: Option<u8>,
    /// Hour (0-23)
    pub hour: Option<u8>,
    /// Minute (0-59)
    pub minute: Option<u8>,
}

impl CalendarSchedule {
    /// Convert to systemd OnCalendar format.
    /// Examples: "*-*-* 03:00:00" (daily at 3am), "Mon *-*-* 00:00:00" (every Monday)
    pub fn to_systemd_oncalendar(&self) -> String {
        let weekday_str = match self.weekday {
            Some(0) => "Sun ",
            Some(1) => "Mon ",
            Some(2) => "Tue ",
            Some(3) => "Wed ",
            Some(4) => "Thu ",
            Some(5) => "Fri ",
            Some(6) => "Sat ",
            _ => "",
        };

        let month = self
            .month
            .map(|m| format!("{:02}", m))
            .unwrap_or_else(|| "*".to_string());
        let day = self
            .day
            .map(|d| format!("{:02}", d))
            .unwrap_or_else(|| "*".to_string());
        let hour = self
            .hour
            .map(|h| format!("{:02}", h))
            .unwrap_or_else(|| "*".to_string());
        let minute = self
            .minute
            .map(|m| format!("{:02}", m))
            .unwrap_or_else(|| "00".to_string());

        format!("{weekday_str}*-{month}-{day} {hour}:{minute}:00")
    }

    /// Convert to launchd StartCalendarInterval dictionary entries.
    pub fn to_launchd_dict(&self) -> Vec<(String, i64)> {
        let mut entries = Vec::new();
        if let Some(month) = self.month {
            entries.push(("Month".to_string(), month as i64));
        }
        if let Some(day) = self.day {
            entries.push(("Day".to_string(), day as i64));
        }
        if let Some(weekday) = self.weekday {
            entries.push(("Weekday".to_string(), weekday as i64));
        }
        if let Some(hour) = self.hour {
            entries.push(("Hour".to_string(), hour as i64));
        }
        if let Some(minute) = self.minute {
            entries.push(("Minute".to_string(), minute as i64));
        }
        entries
    }

    /// Format schedule for human-readable display.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();

        if let Some(weekday) = self.weekday {
            let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            if let Some(day) = days.get(weekday as usize) {
                parts.push(day.to_string());
            }
        }

        if let Some(day) = self.day {
            parts.push(format!("day {}", day));
        }

        if let Some(hour) = self.hour {
            let minute = self.minute.unwrap_or(0);
            parts.push(format!("{:02}:{:02}", hour, minute));
        }

        if parts.is_empty() {
            "scheduled".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceDetails {
    pub name: String,
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub run_at_load: bool,
    pub keep_alive: bool,
    pub env_file: Option<String>,
    pub env_vars: Vec<(String, String)>,
    pub after: Vec<String>,
    pub schedule: Option<CalendarSchedule>,
}

#[derive(Debug, Clone)]
pub struct FsServiceDetails {
    pub service: ServiceDetails,
    pub path: String,
    pub enabled: bool,
    pub running: bool,
}
