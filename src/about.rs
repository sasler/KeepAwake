//! The About window: version, build and links.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDOK};

use crate::ui::{self, Kind::*};

const ID_OK: u16 = IDOK as u16;
const ID_CANCEL: u16 = IDCANCEL as u16;
const ID_LINK: u16 = 10;
const ID_UPDATES: u16 = 11;

const REPO: &str = env!("CARGO_PKG_REPOSITORY");

pub fn open() {
    let version = concat!("Version ", env!("CARGO_PKG_VERSION"));
    let build = format!(
        "Build {} ({}), {}",
        env!("KEEPAWAKE_COMMIT"),
        env!("KEEPAWAKE_BUILD_DATE"),
        std::env::consts::ARCH
    );
    let copyright = concat!("\u{a9} 2026 ", env!("CARGO_PKG_AUTHORS"), ". MIT License.");
    let specs = [
        (Icon, 0, "", [24, 24, 48, 48]),
        (Title, 0, "KeepAwake", [88, 22, 248, 30]),
        (Muted, 0, version, [88, 52, 248, 20]),
        (Text, 0, "Keeps Windows awake from the system tray.", [24, 92, 312, 20]),
        (Muted, 0, &build, [24, 116, 312, 20]),
        (Muted, 0, copyright, [24, 138, 312, 20]),
        (Link, ID_LINK, REPO.trim_start_matches("https://"), [24, 160, 200, 20]),
        (Button, ID_UPDATES, "Check for updates", [110, 204, 130, 28]),
        (DefaultButton, ID_OK, "OK", [248, 204, 88, 28]),
    ];
    if let Some(dlg) = ui::open(ui::ABOUT, "About KeepAwake", (360, 252), &specs, command) {
        ui::show(dlg);
    }
}

fn command(dlg: HWND, id: u16, _code: u16) {
    match id {
        ID_LINK => ui::open_url(REPO),
        ID_UPDATES => ui::open_url(&format!("{REPO}/releases/latest")),
        ID_OK | ID_CANCEL => ui::close(dlg),
        _ => {}
    }
}
