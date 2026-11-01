use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off = 0,
    Awake = 1,
    Activity = 2,
}

impl Mode {
    fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Mode::Off),
            "awake" => Some(Mode::Awake),
            "activity" => Some(Mode::Activity),
            _ => None,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Awake => "awake",
            Mode::Activity => "activity",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::Awake => "Keep screen awake",
            Mode::Activity => "Simulate activity",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    F15,
    Mouse,
}

pub struct Config {
    pub mode: Mode,
    /// Unix seconds when `mode` turns off again; 0 = indefinitely.
    pub until: u64,
    pub interval_secs: u32,
    pub input: InputKind,
    pub schedule_enabled: bool,
    /// Index 0 = Sunday, matching SYSTEMTIME.wDayOfWeek.
    pub schedule_days: [bool; 7],
    pub schedule_days_text: String,
    /// Minutes since midnight.
    pub schedule_start: u16,
    pub schedule_end: u16,
    pub schedule_mode: Mode,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::Off,
            until: 0,
            interval_secs: 60,
            input: InputKind::F15,
            schedule_enabled: false,
            schedule_days: [false, true, true, true, true, true, false],
            schedule_days_text: "Mon-Fri".into(),
            schedule_start: 8 * 60,
            schedule_end: 17 * 60,
            schedule_mode: Mode::Activity,
        }
    }
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config::default();
        let Ok(text) = fs::read_to_string(path()) else { return c };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            match k.as_str() {
                "mode" => {
                    if let Some(m) = Mode::parse(v) {
                        c.mode = m
                    }
                }
                "until" => c.until = v.parse().unwrap_or(0),
                "interval_secs" => {
                    if let Ok(n) = v.parse::<u32>() {
                        c.interval_secs = n.clamp(5, 3600)
                    }
                }
                "input" => {
                    c.input = if v.eq_ignore_ascii_case("mouse") { InputKind::Mouse } else { InputKind::F15 }
                }
                "schedule_enabled" => {
                    c.schedule_enabled = matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1")
                }
                "schedule_days" => {
                    if let Some(d) = parse_days(v) {
                        c.schedule_days = d;
                        c.schedule_days_text = v.to_string();
                    }
                }
                "schedule_start" => {
                    if let Some(t) = parse_hhmm(v) {
                        c.schedule_start = t
                    }
                }
                "schedule_end" => {
                    if let Some(t) = parse_hhmm(v) {
                        c.schedule_end = t
                    }
                }
                "schedule_mode" => match Mode::parse(v) {
                    Some(Mode::Off) | None => {}
                    Some(m) => c.schedule_mode = m,
                },
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = format!(
            "# KeepAwake settings. Edits are picked up automatically within a few seconds.

# Mode: off | awake | activity
#   awake    = keep the screen on and prevent sleep (no fake input)
#   activity = awake + a harmless key press after interval_secs of idle time
mode={mode}
# Unix time when the mode switches back to off; 0 = indefinitely
until={until}

# Idle seconds before simulated input is sent (5-3600)
interval_secs={interval}
# Simulated input: f15 (a key nothing uses) | mouse (zero-distance mouse move)
input={input}

# Work-hours schedule: while mode=off, schedule_mode is used during these hours
schedule_enabled={sched}
# Days, e.g. Mon-Fri or Mon,Wed,Fri
schedule_days={days}
schedule_start={start}
schedule_end={end}
# awake | activity
schedule_mode={smode}
",
            mode = self.mode.key(),
            until = self.until,
            interval = self.interval_secs,
            input = if self.input == InputKind::Mouse { "mouse" } else { "f15" },
            sched = self.schedule_enabled,
            days = self.schedule_days_text,
            start = fmt_hhmm(self.schedule_start),
            end = fmt_hhmm(self.schedule_end),
            smode = self.schedule_mode.key(),
        );
        fs::write(p, text)
    }

    /// Whether the schedule covers the given day of week (0 = Sunday) and minute of day.
    pub fn in_schedule(&self, dow: usize, minute: u16) -> bool {
        let (s, e, d) = (self.schedule_start, self.schedule_end, &self.schedule_days);
        if s < e {
            d[dow] && minute >= s && minute < e
        } else if s > e {
            // Overnight window, e.g. 22:00-06:00: the early part belongs to the previous day.
            (d[dow] && minute >= s) || (d[(dow + 6) % 7] && minute < e)
        } else {
            d[dow]
        }
    }
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("KeepAwake").join("config.ini")
}

pub fn mtime() -> Option<SystemTime> {
    fs::metadata(path()).and_then(|m| m.modified()).ok()
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn fmt_hhmm(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn parse_hhmm(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u16, u16) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn day_index(s: &str) -> Option<usize> {
    let s = s.trim().to_ascii_lowercase();
    let prefix = s.get(..3)?;
    ["sun", "mon", "tue", "wed", "thu", "fri", "sat"].iter().position(|d| *d == prefix)
}

fn parse_days(s: &str) -> Option<[bool; 7]> {
    let mut days = [false; 7];
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let (mut i, b) = (day_index(a)?, day_index(b)?);
            loop {
                days[i] = true;
                if i == b {
                    break;
                }
                i = (i + 1) % 7;
            }
        } else {
            days[day_index(part)?] = true;
        }
    }
    Some(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days() {
        assert_eq!(parse_days("Mon-Fri"), Some([false, true, true, true, true, true, false]));
        assert_eq!(parse_days("Fri-Mon"), Some([true, true, false, false, false, true, true]));
        assert_eq!(parse_days("mon, Wednesday"), Some([false, true, false, true, false, false, false]));
        assert_eq!(parse_days("Funday"), None);
    }

    #[test]
    fn schedule_window() {
        let mut c = Config::default();
        assert!(c.in_schedule(1, 8 * 60));
        assert!(!c.in_schedule(1, 17 * 60));
        assert!(!c.in_schedule(0, 12 * 60));
        c.schedule_start = 22 * 60;
        c.schedule_end = 6 * 60;
        assert!(c.in_schedule(5, 23 * 60)); // Friday night
        assert!(c.in_schedule(6, 60)); // early Saturday belongs to Friday
        assert!(!c.in_schedule(1, 60)); // early Monday belongs to Sunday (off)
    }
}
