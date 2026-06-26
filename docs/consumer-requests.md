# Consumer requests — invest / scheduled-jobs

Status: open · Filed: 2026-06-26 · From: the `invest` repo (algo01 scheduled jobs)

Context for whoever implements [`timers-roadmap.md`](./timers-roadmap.md): the
`invest` project drives `ser` from a skill (`.claude/skills/scheduled-jobs`) that
runs **systemd timer jobs on a remote Debian host (`algo01`)**. Its convention,
matching the rest of that repo's infra, is **GitOps-style**: the `.sh` + paired
`.service`/`.timer` are **committed in the repo (`infra/jobs/`)** as the source of
truth, deployed to the host, and `ser` manages their lifecycle. So for this
consumer the **unit file is the input**, not the output — `ser` mostly needs to
*read, enable, and run* hand-written units rather than generate them.

The roadmap already covers most of this. Below is what that workflow concretely
needs, in priority order, with the two genuine gaps the roadmap doesn't address
called out.

## 1. `ser timer run <name>` — trigger a timer's service now  ⟵ GAP (not in roadmap)

Fire the timer's oneshot `.service` immediately, ignoring the schedule, for
post-deploy smoke tests and ad-hoc reruns. Today this requires dropping to
`systemctl start <name>.service`, which the skill is meant to avoid. Please add a
verb (`ser timer run`, or `ser timer trigger`). Should stream/await the run or at
least report the exit result so a deploy script can gate on it.

## 2. Lifecycle verbs must work on hand-written, committed units (not only ser-generated)

The skill never calls an interactive create — it `scp`s a committed `.service` +
`.timer` into `/etc/systemd/system/`, then expects `ser` to drive them:

- `ser enable <name>` — `daemon-reload` + `enable --now` the **`.timer`**
- `ser disable <name>` — `disable --now` the `.timer`
- `ser timer logs <name> [-f]` — journald for the unit
- `ser timer list` / `ser timer show <name>` — status, next/last run, schedule

These are all in roadmap M1/M2. The one thing to guarantee: they must operate on
units `ser` **did not create**. The blocker is the round-trip parsing in M1a
(`parse_systemd` hardcodes `schedule: None`) — without it `show`/`list` are blank
for our timers.

## 3. Parse real-world `OnCalendar` (ranges / intervals / timezone)  ⟵ PARTIAL GAP

Our committed timers use expressions the current `CalendarSchedule` model can't
represent, so even with M1a they won't display correctly until M3's parsing side
lands. Concretely, `ser timer show` must read these back correctly:

- `OnCalendar=Mon..Fri 16:05 America/New_York`  (weekday range **+ TZ suffix**)
- `OnCalendar=*:0/15`  (every 15 min)
- `OnCalendar=*-*-* *:00:00`  (hourly)

Timezone matters specifically: market-hours jobs are pinned to
`America/New_York` (mirroring the scraper's cron), so dropping/ignoring the TZ
suffix on parse would be a correctness bug, not cosmetic. This is the **parse**
direction of roadmap M3 + M4 timezone item — flagging that for us it's required,
not polish.

## 4. (Lower priority) A non-interactive path, if `ser` is ever the generator

If you'd rather `ser` own unit generation than have us hand-write units, we'd need
a **fully non-interactive** entry so it works over `ssh` in `infra/deploy.sh` with
no TTY — either:

- `ser timer create <name> --exec /opt/jobs/<name>.sh --on-calendar "Mon..Fri 16:05 America/New_York" --user server --env-file /opt/scraper/.env --persistent` (flags, no prompts), or
- `ser timer apply <unit-file>` — install a committed unit + reload + enable.

This resolves the roadmap's open question in favor of **allowing raw `OnCalendar`
passthrough** — we need ranges/intervals/TZ that the structured field model can't
express. Either form is fine; we don't need both. Until one exists we stay on the
hand-written-units path (#2/#3), which is our preferred model anyway.

## 5. (Nice-to-have) Remote target flag

`ser --host algo01 timer <...>` so deploy scripts don't wrap every call in
`ssh root@algo01 '...'`. The ssh+ser pattern works today; purely ergonomic.

---

**Summary of true gaps vs. the roadmap:** (1) `ser timer run` to trigger now, and
(3) the TZ-suffix on `OnCalendar` parsing being treated as required. Everything
else here is already planned — this doc just records a downstream consumer's
concrete dependency on M1a + M2 + M3's parse direction so they aren't deprioritized.
