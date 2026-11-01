//! The Settings window. Changes apply on OK and are written to config.ini.

use std::process::Command;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDOK};

use crate::config::{self, fmt_hhmm, format_days, parse_hhmm, InputKind, Mode, Theme};
use crate::ui::{self, Kind::*, Spec};
use crate::{autostart, with_app};

const ID_OK: u16 = IDOK as u16;
const ID_CANCEL: u16 = IDCANCEL as u16;
const ID_AUTOSTART: u16 = 10;
const ID_THEME: u16 = 11;
const ID_INTERVAL: u16 = 12;
const ID_INPUT: u16 = 13;
const ID_SCHEDULE: u16 = 14;
/// Monday to Sunday.
const ID_DAY: u16 = 20;
const ID_START: u16 = 30;
const ID_END: u16 = 31;
const ID_SCHEDULE_MODE: u16 = 32;
const ID_ERROR: u16 = 40;
const ID_OPEN_FILE: u16 = 41;

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const THEMES: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

/// Config day index (0 = Sunday) of the i-th day checkbox (0 = Monday).
fn day(i: u16) -> usize {
    (i as usize + 1) % 7
}

pub fn open() {
    let Some(cfg) = with_app(|app| app.cfg.clone()) else { return };
    let mut specs: Vec<Spec> = vec![
        (Header, 0, "General", [20, 16, 380, 22]),
        (Check, ID_AUTOSTART, "Start with Windows", [20, 42, 380, 24]),
        (Text, 0, "Appearance", [20, 74, 170, 24]),
        (Combo, ID_THEME, "", [200, 74, 200, 24]),
        (Header, 0, "Simulate activity", [20, 114, 380, 22]),
        (Text, 0, "Send input after idle for", [20, 140, 170, 24]),
        (Edit, ID_INTERVAL, "", [200, 140, 64, 24]),
        (Text, 0, "seconds", [272, 140, 128, 24]),
        (Text, 0, "Input to send", [20, 172, 170, 24]),
        (Combo, ID_INPUT, "", [200, 172, 200, 24]),
        (Header, 0, "Work-hours schedule", [20, 212, 380, 22]),
        (Check, ID_SCHEDULE, "Use the schedule while the mode is Off", [20, 238, 380, 24]),
    ];
    for (i, name) in DAYS.iter().enumerate() {
        specs.push((Check, ID_DAY + i as u16, name, [20 + i as i32 * 54, 266, 52, 24]));
    }
    specs.extend_from_slice(&[
        (Text, 0, "Hours", [20, 298, 170, 24]),
        (Edit, ID_START, "", [200, 298, 64, 24]),
        (Text, 0, "to", [272, 298, 24, 24]),
        (Edit, ID_END, "", [300, 298, 64, 24]),
        (Text, 0, "Mode during work hours", [20, 330, 170, 24]),
        (Combo, ID_SCHEDULE_MODE, "", [200, 330, 200, 24]),
        (Error, ID_ERROR, "", [20, 366, 380, 22]),
        (Button, ID_OPEN_FILE, "Open config file", [20, 400, 130, 28]),
        (DefaultButton, ID_OK, "OK", [216, 400, 88, 28]),
        (Button, ID_CANCEL, "Cancel", [312, 400, 88, 28]),
    ]);
    let Some(dlg) = ui::open(ui::SETTINGS, "KeepAwake Settings", (420, 448), &specs, command) else { return };

    ui::set_checked(dlg, ID_AUTOSTART, autostart::is_enabled());
    ui::combo(dlg, ID_THEME, &["Same as Windows", "Light", "Dark"], cfg.theme as usize);
    ui::set_text(dlg, ID_INTERVAL, &cfg.interval_secs.to_string());
    ui::combo(dlg, ID_INPUT, &["F15 key (recommended)", "Mouse move (zero distance)"], (cfg.input == InputKind::Mouse) as usize);
    ui::set_checked(dlg, ID_SCHEDULE, cfg.schedule_enabled);
    for i in 0..7 {
        ui::set_checked(dlg, ID_DAY + i, cfg.schedule_days[day(i)]);
    }
    ui::set_text(dlg, ID_START, &fmt_hhmm(cfg.schedule_start));
    ui::set_text(dlg, ID_END, &fmt_hhmm(cfg.schedule_end));
    let modes = [Mode::Awake.label(), Mode::Activity.label()];
    ui::combo(dlg, ID_SCHEDULE_MODE, &modes, (cfg.schedule_mode == Mode::Activity) as usize);
    sync_schedule(dlg);
    ui::show(dlg);
}

/// Grays out the schedule fields while the schedule is off.
fn sync_schedule(dlg: HWND) {
    let on = ui::checked(dlg, ID_SCHEDULE);
    for id in (ID_DAY..ID_DAY + 7).chain([ID_START, ID_END, ID_SCHEDULE_MODE]) {
        ui::enable(dlg, id, on);
    }
}

fn command(dlg: HWND, id: u16, _code: u16) {
    match id {
        ID_SCHEDULE => sync_schedule(dlg),
        ID_OPEN_FILE => {
            let _ = Command::new("notepad.exe").arg(config::path()).spawn();
            ui::close(dlg);
        }
        ID_OK => match apply(dlg) {
            Ok(()) => ui::close(dlg),
            Err(msg) => ui::set_text(dlg, ID_ERROR, msg),
        },
        ID_CANCEL => ui::close(dlg),
        _ => {}
    }
}

fn apply(dlg: HWND) -> Result<(), &'static str> {
    let interval = ui::text(dlg, ID_INTERVAL)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (5..=3600).contains(n))
        .ok_or("Idle time must be between 5 and 3600 seconds.")?;
    let schedule = ui::checked(dlg, ID_SCHEDULE);
    let mut days = [false; 7];
    for i in 0..7 {
        days[day(i)] = ui::checked(dlg, ID_DAY + i);
    }
    // Times can't be fixed while their fields are disabled, so only check them when used.
    let start = parse_hhmm(&ui::text(dlg, ID_START));
    let end = parse_hhmm(&ui::text(dlg, ID_END));
    if schedule {
        if start.is_none() || end.is_none() {
            return Err("Enter work hours as HH:MM, for example 08:00.");
        }
        if !days.contains(&true) {
            return Err("Pick at least one day for the schedule.");
        }
    }
    let autostart = ui::checked(dlg, ID_AUTOSTART);
    if autostart != autostart::is_enabled() {
        autostart::set(autostart);
    }
    let theme = THEMES[ui::combo_sel(dlg, ID_THEME).min(2)];
    let input = if ui::combo_sel(dlg, ID_INPUT) == 1 { InputKind::Mouse } else { InputKind::F15 };
    let schedule_mode = if ui::combo_sel(dlg, ID_SCHEDULE_MODE) == 0 { Mode::Awake } else { Mode::Activity };
    with_app(|app| {
        app.update(|c| {
            c.theme = theme;
            c.interval_secs = interval;
            c.input = input;
            c.schedule_enabled = schedule;
            c.schedule_days = days;
            c.schedule_days_text = format_days(&days);
            c.schedule_start = start.unwrap_or(c.schedule_start);
            c.schedule_end = end.unwrap_or(c.schedule_end);
            c.schedule_mode = schedule_mode;
        })
    });
    Ok(())
}
