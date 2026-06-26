# Timer Management Roadmap

Status: draft · Last updated: 2026-06-26

`ser` already creates working timers on both platforms (launchd
`StartCalendarInterval` plists, systemd `.timer` units). The infrastructure is
solid; the gaps are in **observability** (seeing an existing timer's schedule,
next run, and frequency) and **round-tripping** (parsing a deployed timer back
into our model). This roadmap closes those gaps by building a dedicated
`ser timer` command group, with **best-effort per-platform** scheduling so each
OS exposes its full power.

## Design decisions

- **Dedicated subcommand group.** Scheduled timers get their own verbs under
  `ser timer …` rather than overloading the flat service commands. One-shot/
  long-running services and scheduled timers have different mental models;
  separating them keeps each clean.
- **Best-effort per platform.** We expose each platform's native expressivity
  (systemd `OnCalendar` ranges/intervals, launchd `StartInterval`) and degrade
  gracefully where the other side can't represent it, instead of restricting to
  the lowest common denominator.
- **Schedule is the source of truth, the unit file is the artifact.** Every
  feature must round-trip: a timer created by `ser` must read back identically,
  and a hand-written unit/plist must be inspectable.

## Proposed command surface

```
ser timer list                 # all scheduled units, with next-run + frequency
ser timer show <name>          # schedule, next/last run, enabled state, unit path
ser timer next [<name>]        # upcoming fire times (one or all), sorted
ser timer logs <name> [-f]     # run history / output (journalctl / log show)
ser timer create               # interactive, schedule-first creation flow
ser timer edit <name>          # change schedule without hand-editing files
ser timer enable|disable <name># toggle without removing the definition
ser timer rm <name>            # remove timer (+ paired service on Linux)
```

`ser list` continues to show everything; `ser timer list` is the
scheduling-focused view. Shared logic lives in `lib`; the `timer` command module
is a thin CLI layer.

---

## Milestone 1 — `ser timer` inspection surface (the first deliverable) ✅

Status: **done.** `ser timer list/show/next` ship, backed by a tested
`next_fire_after` and round-trip parsing. Remaining nice-to-have: `ser timer
logs` (1e) is currently served by the existing `ser logs`.

Goal: a user can fully understand any timer on the system without reaching for
`launchctl`/`systemctl`. This also forces us to fix the round-trip bugs.

### 1a. Fix the round-trip foundation (prerequisite)
These are correctness bugs that block everything else.

- **Linux: parse `OnCalendar` back into `CalendarSchedule`.** Today
  `parse_systemd` hardcodes `schedule: None` (`lib/src/platform/linux.rs`). Add
  `CalendarSchedule::from_systemd_oncalendar()` as the inverse of
  `to_systemd_oncalendar()`, and read the paired `.timer` unit when present.
- **macOS: confirm `StartCalendarInterval` parse coverage.** `parse_calendar_
  interval` (`macos.rs:162`) handles the dict form; add array-of-dicts support
  (launchd allows multiple calendar entries) → model as `Vec<CalendarSchedule>`
  or a follow-up (see M4).
- **`Show` must display the schedule.** `cli/src/command/show.rs` parses but
  never prints it. Add a Schedule row using `CalendarSchedule::display()`.

### 1b. `ser timer list`
- Enumerate only scheduled units (reuse `has_timer`).
- Columns: name, schedule (human), next run, enabled, last result.
- Linux next-run from `get_timer_next_trigger`; add last-run via
  `systemctl show --property=LastTriggerUSec`.
- macOS has no native "next run" — compute it ourselves from `CalendarSchedule`
  (see shared helper in 1c).

### 1c. `ser timer next` + next-run computation
- Add `CalendarSchedule::next_fire(after: DateTime) -> Option<DateTime>` in
  `lib` — pure, unit-testable, platform-independent. This powers macOS next-run
  and gives Linux a cross-check.
- `ser timer next` lists upcoming fires sorted ascending; `ser timer next <name>`
  shows the next N fires for one timer.

### 1d. `ser timer show <name>`
- Schedule (human + raw `OnCalendar`/plist form), next run, last run + result,
  enabled state, unit/plist path, target program.

### 1e. `ser timer logs <name>`
- Thin alias over existing logs path (`journalctl -u` / `log show`), scoped and
  documented for timer run history. Add `--follow`.

**Exit criteria for M1:** on a clean machine, create a timer with `ser`, then
`ser timer list/show/next/logs` give a complete picture with zero `systemctl`/
`launchctl`. Round-trip parsing has unit tests on both `to_*`/`from_*` directions.

---

## Milestone 2 — Schedule-first creation & editing ✅

Status: **done.** Full lifecycle now runs through `ser timer`. To make
edit-by-regenerate lossless, the macOS plist parser was fixed to capture `Label`,
`ProgramArguments`, and `EnvironmentVariables` (previously dropped) — which also
fixed empty names in `show`.

- **`ser timer create`** — schedule-first interactive flow (reuses
  `collect_schedule`) that leads with cadence, then program.
- **`ser timer edit <name>`** — loads existing details, re-runs the picker,
  regenerates the unit/plist, and offers to restart so the change applies.
- **`ser timer enable|disable|rm`** — explicit lifecycle verbs (`rm` confirms,
  `-y` to skip). New `platform::remove_service` on both platforms.
- **Fixed `Restart` for timers** (`platform/linux.rs`) so it restarts the
  `.timer`, not just the `.service`.

**Exit criteria met:** create → inspect → edit schedule → disable → remove,
entirely through `ser timer`, with the unit file always matching the model.

---

## Milestone 3 — Per-platform scheduling expressivity

Goal: deliver on "best-effort per platform" — let power users express more.

- **macOS `StartInterval`** — "every N seconds/minutes" simple intervals
  (`plist.rs`). Model as `Schedule::Interval(Duration)` vs
  `Schedule::Calendar(..)`.
- **systemd interval timers** — `OnUnitActiveSec`/`OnBootSec` for "every N
  minutes" semantics; map the same `Schedule::Interval` here.
- **Richer `OnCalendar`** — ranges/lists/steps (`Mon..Fri`, `*:0/15`). Extend
  `CalendarSchedule` fields from `Option<u8>` toward a small expression type, or
  add a raw-passthrough escape hatch with validation.
- **Graceful degradation** — when a schedule isn't representable on the other
  platform, `ser generate` warns clearly rather than emitting something wrong.

**Exit criteria:** "every 15 minutes" and "weekdays at 9am" both work natively
on each platform and round-trip.

---

## Milestone 4 — Polish & robustness

- Timezone awareness (systemd `OnCalendar` TZ suffix; document launchd's
  local-time behavior).
- Multiple calendar entries per timer (launchd array form ↔ several systemd
  `OnCalendar=` lines).
- `ser timer next --json` / machine-readable output for scripting.
- Dry-run / validation: `ser timer validate <name>` checks the deployed unit
  matches what `ser` would generate.
- Missed-run handling surfaced (systemd `Persistent=true` is already set;
  explain catch-up behavior in `show`).

---

## Suggested implementation order

1. M1a round-trip fixes (unblocks correct display) — **start here**
2. `next_fire()` helper + tests
3. `ser timer list` / `show` / `next` / `logs`
4. M2 create/edit/lifecycle
5. M3 expressivity
6. M4 polish

## Open questions

- Should `ser timer create` replace the schedule path in `ser new`, or coexist?
- Naming: `rm` vs `remove` vs `delete` (match existing CLI conventions).
- How much raw `OnCalendar` passthrough to allow before it becomes a maintenance
  burden vs. structured fields.
