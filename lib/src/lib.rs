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

    /// Parse a systemd `OnCalendar=` expression into a `CalendarSchedule`.
    ///
    /// This is the inverse of [`to_systemd_oncalendar`](Self::to_systemd_oncalendar)
    /// and also tolerates the common hand-written forms:
    ///   `*-*-* 03:00:00`, `Mon *-*-* 09:30:00`, `*-03-15 12:00:00`.
    ///
    /// Returns `None` if the expression cannot be understood (e.g. it uses
    /// ranges, lists, or steps that our structured model cannot represent).
    pub fn from_systemd_oncalendar(expr: &str) -> Option<CalendarSchedule> {
        let mut tokens = expr.split_whitespace().peekable();

        // Optional leading day-of-week token, e.g. "Mon".
        let mut weekday = None;
        if let Some(first) = tokens.peek() {
            if let Some(wd) = parse_weekday_name(first) {
                weekday = Some(wd);
                tokens.next();
            }
        }

        let date = tokens.next()?;
        let time = tokens.next()?;
        // Anything left over (extra fields, timezone, etc.) is beyond our model.
        if tokens.next().is_some() {
            return None;
        }

        // Date is `year-month-day`; we only model month and day.
        let mut date_parts = date.split('-');
        let _year = date_parts.next()?;
        let month = parse_calendar_field(date_parts.next()?)?;
        let day = parse_calendar_field(date_parts.next()?)?;
        if date_parts.next().is_some() {
            return None;
        }

        // Time is `hour:minute:second`; we only model hour and minute.
        let mut time_parts = time.split(':');
        let hour = parse_calendar_field(time_parts.next()?)?;
        let minute = parse_calendar_field(time_parts.next()?)?;

        Some(CalendarSchedule {
            month,
            day,
            weekday,
            hour,
            minute,
        })
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

    /// Compute the next time this schedule fires strictly after `after`.
    ///
    /// Fields are matched conjunctively (a candidate must satisfy every field
    /// that is set), mirroring systemd `OnCalendar` semantics. An unset minute
    /// is treated as `:00` to match how schedules are generated. Returns `None`
    /// if no match occurs within ~5 years (e.g. an impossible date).
    pub fn next_fire_after(&self, after: chrono::NaiveDateTime) -> Option<chrono::NaiveDateTime> {
        let target_minute = self.minute.unwrap_or(0) as u32;
        let mut date = after.date();

        // Bound the search to ~5 years so month/day combos like Feb 29 still
        // resolve, while an impossible schedule terminates instead of looping.
        for _ in 0..(366 * 5) {
            if self.day_matches(date) {
                let hours: Vec<u32> = match self.hour {
                    Some(h) => vec![h as u32],
                    None => (0..24).collect(),
                };
                for hour in hours {
                    if let Some(candidate) = date.and_hms_opt(hour, target_minute, 0) {
                        if candidate > after {
                            return Some(candidate);
                        }
                    }
                }
            }
            date = date.succ_opt()?;
        }
        None
    }

    /// Whether the month/day/weekday constraints match the given date.
    fn day_matches(&self, date: chrono::NaiveDate) -> bool {
        use chrono::Datelike;

        if let Some(month) = self.month {
            if date.month() != month as u32 {
                return false;
            }
        }
        if let Some(day) = self.day {
            if date.day() != day as u32 {
                return false;
            }
        }
        if let Some(weekday) = self.weekday {
            // Our weekday numbering matches chrono's Sun=0..Sat=6.
            if date.weekday().num_days_from_sunday() != weekday as u32 {
                return false;
            }
        }
        true
    }
}

/// How a service is scheduled. Either a calendar pattern ("Mondays at 09:30")
/// or a fixed interval ("every 15 minutes").
#[derive(Debug, Clone)]
pub enum Schedule {
    Calendar(CalendarSchedule),
    /// Repeat every N seconds (launchd `StartInterval` / systemd
    /// `OnUnitActiveSec`).
    Interval(u64),
}

impl Schedule {
    /// Human-readable description of the schedule.
    pub fn display(&self) -> String {
        match self {
            Schedule::Calendar(c) => c.display(),
            Schedule::Interval(secs) => format!("every {}", humanize_secs(*secs)),
        }
    }

    /// The next wall-clock fire time strictly after `after`.
    ///
    /// Interval schedules fire relative to the unit's last activation, which we
    /// don't track, so their next fire isn't knowable here and returns `None`.
    pub fn next_fire_after(&self, after: chrono::NaiveDateTime) -> Option<chrono::NaiveDateTime> {
        match self {
            Schedule::Calendar(c) => c.next_fire_after(after),
            Schedule::Interval(_) => None,
        }
    }

    /// Format an interval as a systemd time span (e.g. `900s`).
    pub fn interval_to_systemd(secs: u64) -> String {
        format!("{secs}s")
    }

    /// Parse a subset of systemd time spans into seconds: a bare number (seconds),
    /// or a value suffixed with `s`, `sec`, `m`, `min`, or `h`. Returns `None`
    /// for compound or unrecognized spans.
    pub fn parse_interval_secs(span: &str) -> Option<u64> {
        let span = span.trim();
        let (digits, unit): (String, String) = span.chars().partition(|c| c.is_ascii_digit());
        if digits.is_empty() {
            return None;
        }
        let value: u64 = digits.parse().ok()?;
        let multiplier = match unit.trim() {
            "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "h" | "hr" | "hour" | "hours" => 3600,
            _ => return None,
        };
        Some(value * multiplier)
    }
}

/// Format a number of seconds compactly (e.g. `90s`, `15m`, `2h`).
fn humanize_secs(secs: u64) -> String {
    if secs >= 3600 && secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs >= 60 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Parse a single `OnCalendar` field: `*` (wildcard) becomes `None`, a numeric
/// value becomes `Some(n)`. Returns `None` (parse failure) for anything else,
/// such as ranges/lists/steps we cannot represent (`0/15`, `Mon..Fri`).
fn parse_calendar_field(field: &str) -> Option<Option<u8>> {
    if field == "*" {
        Some(None)
    } else {
        field.parse::<u8>().ok().map(Some)
    }
}

/// Map a systemd day-of-week token to our weekday numbering (0=Sun..6=Sat).
fn parse_weekday_name(token: &str) -> Option<u8> {
    match token {
        "Sun" => Some(0),
        "Mon" => Some(1),
        "Tue" => Some(2),
        "Wed" => Some(3),
        "Thu" => Some(4),
        "Fri" => Some(5),
        "Sat" => Some(6),
        _ => None,
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
    pub schedule: Option<Schedule>,
}

#[derive(Debug, Clone)]
pub struct FsServiceDetails {
    pub service: ServiceDetails,
    pub path: String,
    pub enabled: bool,
    pub running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(sched: CalendarSchedule) {
        let expr = sched.to_systemd_oncalendar();
        let parsed = CalendarSchedule::from_systemd_oncalendar(&expr)
            .unwrap_or_else(|| panic!("failed to parse {expr:?}"));
        assert_eq!(parsed.month, sched.month, "month mismatch for {expr:?}");
        assert_eq!(parsed.day, sched.day, "day mismatch for {expr:?}");
        assert_eq!(
            parsed.weekday, sched.weekday,
            "weekday mismatch for {expr:?}"
        );
        assert_eq!(parsed.hour, sched.hour, "hour mismatch for {expr:?}");
        // Minute defaults to "00" when unset, so a None minute round-trips to Some(0).
        let expected_minute = sched.minute.or(Some(0));
        assert_eq!(
            parsed.minute, expected_minute,
            "minute mismatch for {expr:?}"
        );
    }

    #[test]
    fn oncalendar_roundtrips() {
        roundtrip(CalendarSchedule::default());
        roundtrip(CalendarSchedule {
            hour: Some(3),
            minute: Some(0),
            ..Default::default()
        });
        roundtrip(CalendarSchedule {
            weekday: Some(1),
            hour: Some(9),
            minute: Some(30),
            ..Default::default()
        });
        roundtrip(CalendarSchedule {
            month: Some(3),
            day: Some(15),
            hour: Some(12),
            minute: Some(0),
            ..Default::default()
        });
    }

    #[test]
    fn parses_handwritten_forms() {
        let s = CalendarSchedule::from_systemd_oncalendar("*-*-* 03:00:00").unwrap();
        assert_eq!(s.hour, Some(3));
        assert_eq!(s.minute, Some(0));
        assert_eq!(s.weekday, None);

        let s = CalendarSchedule::from_systemd_oncalendar("Fri *-*-* 17:45:00").unwrap();
        assert_eq!(s.weekday, Some(5));
        assert_eq!(s.hour, Some(17));
        assert_eq!(s.minute, Some(45));
    }

    fn ndt(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn next_fire_daily() {
        let daily_3am = CalendarSchedule {
            hour: Some(3),
            minute: Some(0),
            ..Default::default()
        };
        // Later today.
        assert_eq!(
            daily_3am.next_fire_after(ndt("2026-06-26 01:00:00")),
            Some(ndt("2026-06-26 03:00:00"))
        );
        // Already passed today -> tomorrow.
        assert_eq!(
            daily_3am.next_fire_after(ndt("2026-06-26 09:00:00")),
            Some(ndt("2026-06-27 03:00:00"))
        );
    }

    #[test]
    fn next_fire_weekly() {
        // Monday (weekday=1) at 09:30. 2026-06-26 is a Friday.
        let weekly_mon = CalendarSchedule {
            weekday: Some(1),
            hour: Some(9),
            minute: Some(30),
            ..Default::default()
        };
        assert_eq!(
            weekly_mon.next_fire_after(ndt("2026-06-26 12:00:00")),
            Some(ndt("2026-06-29 09:30:00"))
        );
    }

    #[test]
    fn next_fire_monthly() {
        // Day 15 at noon.
        let monthly = CalendarSchedule {
            day: Some(15),
            hour: Some(12),
            minute: Some(0),
            ..Default::default()
        };
        assert_eq!(
            monthly.next_fire_after(ndt("2026-06-26 00:00:00")),
            Some(ndt("2026-07-15 12:00:00"))
        );
    }

    #[test]
    fn next_fire_handles_leap_day() {
        // Feb 29 only exists in leap years; 2028 is the next after 2026.
        let leap = CalendarSchedule {
            month: Some(2),
            day: Some(29),
            hour: Some(0),
            minute: Some(0),
            ..Default::default()
        };
        assert_eq!(
            leap.next_fire_after(ndt("2026-06-26 00:00:00")),
            Some(ndt("2028-02-29 00:00:00"))
        );
    }

    #[test]
    fn parses_interval_spans() {
        assert_eq!(Schedule::parse_interval_secs("900"), Some(900));
        assert_eq!(Schedule::parse_interval_secs("900s"), Some(900));
        assert_eq!(Schedule::parse_interval_secs("15min"), Some(900));
        assert_eq!(Schedule::parse_interval_secs("15m"), Some(900));
        assert_eq!(Schedule::parse_interval_secs("2h"), Some(7200));
        assert_eq!(Schedule::parse_interval_secs("bogus"), None);
        assert_eq!(Schedule::parse_interval_secs("15days"), None);
    }

    #[test]
    fn interval_display_and_roundtrip() {
        assert_eq!(Schedule::Interval(900).display(), "every 15m");
        assert_eq!(Schedule::Interval(7200).display(), "every 2h");
        assert_eq!(Schedule::Interval(90).display(), "every 90s");
        // systemd round-trip
        let span = Schedule::interval_to_systemd(900);
        assert_eq!(span, "900s");
        assert_eq!(Schedule::parse_interval_secs(&span), Some(900));
    }

    #[test]
    fn rejects_unrepresentable_expressions() {
        // Step/range/list syntax we cannot model structurally.
        assert!(CalendarSchedule::from_systemd_oncalendar("*-*-* *:0/15:00").is_none());
        assert!(CalendarSchedule::from_systemd_oncalendar("Mon..Fri *-*-* 09:00:00").is_none());
        // Garbage.
        assert!(CalendarSchedule::from_systemd_oncalendar("not a schedule").is_none());
    }
}
