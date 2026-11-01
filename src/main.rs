#![windows_subsystem = "windows"]

mod about;
mod art;
mod autostart;
mod config;
mod icons;
mod power;
mod settings;
mod theme;
mod ui;

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, SYSTEMTIME, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use art::Look;
use config::{fmt_hhmm, now_unix, Config, Mode, Theme};

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_ID: usize = 1;
const TICK_MS: u32 = 5_000;

const ID_OFF: u32 = 100;
const ID_AWAKE: u32 = 101;
const ID_ACTIVITY: u32 = 102;
const ID_DUR_INF: u32 = 200;
const ID_DUR_BASE: u32 = 201;
const ID_DUR_END: u32 = 299;
const ID_SCHEDULE: u32 = 300;
const ID_AUTOSTART: u32 = 301;
const ID_SETTINGS: u32 = 302;
const ID_EXIT: u32 = 303;
const ID_ABOUT: u32 = 304;

const DURATIONS: [(&str, u64); 5] =
    [("30 minutes", 30), ("1 hour", 60), ("2 hours", 120), ("4 hours", 240), ("8 hours", 480)];

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// (day of week with 0 = Sunday, minute of day, second of day) in local time.
fn local_now() -> (usize, u16, u32) {
    let mut st: SYSTEMTIME = unsafe { zeroed() };
    unsafe { GetLocalTime(&mut st) };
    let sec = st.wHour as u32 * 3600 + st.wMinute as u32 * 60 + st.wSecond as u32;
    (st.wDayOfWeek as usize, st.wHour * 60 + st.wMinute, sec)
}

fn fmt_remaining(secs: u64) -> String {
    let mins = secs.div_ceil(60);
    if mins >= 60 {
        format!("{}h {}m", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Manual,
    Schedule,
}

struct App {
    hwnd: HWND,
    cfg: Config,
    cfg_mtime: Option<SystemTime>,
    /// Mode to use when a duration is picked or the icon is clicked while off.
    last_on: Mode,
    icons: [HICON; 3],
    applied: Option<Mode>,
    shown: Option<(Mode, String)>,
    taskbar_created: u32,
    /// Theme last applied to menus and windows.
    theme: Option<Theme>,
}

impl App {
    fn effective(&self) -> (Mode, Source) {
        if self.cfg.mode != Mode::Off {
            return (self.cfg.mode, Source::Manual);
        }
        if self.cfg.schedule_enabled {
            let (dow, minute, _) = local_now();
            if self.cfg.in_schedule(dow, minute) {
                return (self.cfg.schedule_mode, Source::Schedule);
            }
        }
        (Mode::Off, Source::Manual)
    }

    fn on_mode(&self) -> Mode {
        if self.cfg.mode != Mode::Off { self.cfg.mode } else { self.last_on }
    }

    fn secs_until_schedule_end(&self) -> u64 {
        let (_, _, sec) = local_now();
        (self.cfg.schedule_end as u64 * 60).saturating_sub(sec as u64)
    }

    /// Applies changes made in the Settings window.
    fn update(&mut self, f: impl FnOnce(&mut Config)) {
        f(&mut self.cfg);
        self.save();
        self.tick(false);
    }

    fn save(&mut self) {
        let _ = self.cfg.save();
        self.cfg_mtime = config::mtime();
    }

    fn set_mode(&mut self, mode: Mode, until: u64) {
        if mode != Mode::Off {
            self.last_on = mode;
        }
        self.cfg.mode = mode;
        self.cfg.until = if mode == Mode::Off { 0 } else { until };
        self.save();
        self.tick(false);
    }

    fn toggle(&mut self) {
        if self.cfg.mode != Mode::Off {
            self.set_mode(Mode::Off, 0);
        } else {
            self.set_mode(self.last_on, 0);
        }
    }

    fn tick(&mut self, from_timer: bool) {
        let mtime = config::mtime();
        if mtime != self.cfg_mtime {
            self.cfg = Config::load();
            self.cfg_mtime = mtime;
            if self.cfg.mode != Mode::Off {
                self.last_on = self.cfg.mode;
            }
        }
        if self.cfg.mode != Mode::Off && self.cfg.until != 0 && now_unix() >= self.cfg.until {
            self.cfg.mode = Mode::Off;
            self.cfg.until = 0;
            self.save();
        }

        if self.theme != Some(self.cfg.theme) {
            self.theme = Some(self.cfg.theme);
            theme::set_pref(self.cfg.theme);
            theme::apply_to_menus();
            ui::refresh();
        }

        let (mode, source) = self.effective();
        if self.applied != Some(mode) {
            power::set_awake(mode != Mode::Off);
            self.applied = Some(mode);
        }
        if from_timer && mode == Mode::Activity && power::idle_ms() >= self.cfg.interval_secs.saturating_mul(1000) {
            power::nudge(self.cfg.input);
        }

        let tip = self.tooltip(mode, source);
        if self.shown.as_ref().map(|(m, t)| (*m, t.as_str())) != Some((mode, tip.as_str())) {
            self.notify(if self.shown.is_none() { NIM_ADD } else { NIM_MODIFY }, mode, &tip);
            self.shown = Some((mode, tip));
        }
    }

    fn tooltip(&self, mode: Mode, source: Source) -> String {
        let mut s = format!("KeepAwake: {}", mode.label());
        match source {
            Source::Schedule => s += &format!(" (work hours until {})", fmt_hhmm(self.cfg.schedule_end)),
            Source::Manual if mode != Mode::Off && self.cfg.until != 0 => {
                s += &format!(" ({} left)", fmt_remaining(self.cfg.until.saturating_sub(now_unix())))
            }
            _ => {}
        }
        s
    }

    fn notify(&self, msg: u32, mode: Mode, tip: &str) {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = self.icons[mode as usize];
        for (dst, src) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *dst = src;
        }
        unsafe { Shell_NotifyIconW(msg, &nid) };
    }

    /// Redraws the tray icons for the current taskbar color.
    fn reload_icons(&mut self) {
        let light = theme::taskbar_light();
        for (icon, look) in self.icons.iter_mut().zip([Look::Off, Look::Awake, Look::Activity]) {
            unsafe { DestroyIcon(*icon) };
            *icon = icons::make(look, light);
        }
        if let Some((mode, tip)) = &self.shown {
            self.notify(NIM_MODIFY, *mode, tip);
        }
    }

    /// (Re)creates the tray icon, e.g. at startup or after Explorer restarts.
    fn add_icon(&mut self) {
        self.shown = None;
        self.tick(false);
    }

    fn shutdown(&mut self) {
        if let Some((mode, tip)) = self.shown.take() {
            self.notify(NIM_DELETE, mode, &tip);
        }
        power::set_awake(false);
        for icon in self.icons {
            unsafe { DestroyIcon(icon) };
        }
    }

    fn build_menu(&self) -> HMENU {
        let (mode, source) = self.effective();
        let checked = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
        unsafe {
            let menu = CreatePopupMenu();
            append(menu, MF_STRING | MF_GRAYED, 0, &self.tooltip(mode, source));
            append(menu, MF_SEPARATOR, 0, "");
            for m in [Mode::Off, Mode::Awake, Mode::Activity] {
                append(menu, MF_STRING, ID_OFF + m as u32, m.label());
            }
            CheckMenuRadioItem(menu, ID_OFF, ID_ACTIVITY, ID_OFF + self.cfg.mode as u32, MF_BYCOMMAND);

            let durations = CreatePopupMenu();
            append(durations, checked(self.cfg.mode != Mode::Off && self.cfg.until == 0), ID_DUR_INF, "Indefinitely");
            for (i, (label, _)) in DURATIONS.iter().enumerate() {
                append(durations, MF_STRING, ID_DUR_BASE + i as u32, label);
            }
            let end_flags = if self.secs_until_schedule_end() > 0 { MF_STRING } else { MF_STRING | MF_GRAYED };
            let end_label = format!("Until {} (end of work hours)", fmt_hhmm(self.cfg.schedule_end));
            append(durations, end_flags, ID_DUR_END, &end_label);
            append_submenu(menu, durations, "Duration");

            append(menu, MF_SEPARATOR, 0, "");
            let schedule_label = format!(
                "Work-hours schedule ({} {}\u{2013}{})",
                self.cfg.schedule_days_text,
                fmt_hhmm(self.cfg.schedule_start),
                fmt_hhmm(self.cfg.schedule_end)
            );
            append(menu, checked(self.cfg.schedule_enabled), ID_SCHEDULE, &schedule_label);
            append(menu, checked(autostart::is_enabled()), ID_AUTOSTART, "Start with Windows");
            append(menu, MF_STRING, ID_SETTINGS, "Settings\u{2026}");
            append(menu, MF_STRING, ID_ABOUT, "About KeepAwake");
            append(menu, MF_SEPARATOR, 0, "");
            append(menu, MF_STRING, ID_EXIT, "Exit");
            menu
        }
    }

    fn command(&mut self, id: u32) {
        let now = now_unix();
        match id {
            ID_OFF => self.set_mode(Mode::Off, 0),
            ID_AWAKE => self.set_mode(Mode::Awake, 0),
            ID_ACTIVITY => self.set_mode(Mode::Activity, 0),
            ID_DUR_INF => self.set_mode(self.on_mode(), 0),
            ID_DUR_END => {
                let left = self.secs_until_schedule_end();
                if left > 0 {
                    self.set_mode(self.on_mode(), now + left);
                }
            }
            id if (ID_DUR_BASE..ID_DUR_BASE + DURATIONS.len() as u32).contains(&id) => {
                let minutes = DURATIONS[(id - ID_DUR_BASE) as usize].1;
                self.set_mode(self.on_mode(), now + minutes * 60);
            }
            ID_SCHEDULE => {
                self.cfg.schedule_enabled = !self.cfg.schedule_enabled;
                self.save();
                self.tick(false);
            }
            ID_AUTOSTART => autostart::set(!autostart::is_enabled()),
            _ => {}
        }
    }
}

unsafe fn append(menu: HMENU, flags: u32, id: u32, text: &str) {
    AppendMenuW(menu, flags, id as usize, wide(text).as_ptr());
}

unsafe fn append_submenu(menu: HMENU, submenu: HMENU, text: &str) {
    AppendMenuW(menu, MF_POPUP, submenu as usize, wide(text).as_ptr());
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app state. Returns None if the state isn't set up yet or is
/// already borrowed further up the stack (modal loops can re-enter the wndproc).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut app| app.as_mut().map(f)))
}

fn show_menu(hwnd: HWND) {
    let Some(menu) = with_app(|app| app.build_menu()) else { return };
    let cmd = unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        cmd as u32
    };
    if cmd != 0 {
        run_command(hwnd, cmd);
    }
}

/// Runs a menu command. Windows are opened outside `with_app`, since creating them
/// sends messages that need the app state.
fn run_command(hwnd: HWND, id: u32) {
    match id {
        ID_EXIT => unsafe {
            DestroyWindow(hwnd);
        },
        ID_SETTINGS => settings::open(),
        ID_ABOUT => about::open(),
        id => {
            with_app(|app| app.command(id));
        }
    }
}

/// Whether a WM_SETTINGCHANGE lParam names the given setting.
unsafe fn setting_is(lp: LPARAM, name: &str) -> bool {
    let p = lp as *const u16;
    if p.is_null() {
        return false;
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    std::slice::from_raw_parts(p, len).iter().copied().eq(name.encode_utf16())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            with_app(|app| app.tick(true));
            0
        }
        WM_TRAY => {
            match (lp & 0xFFFF) as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                WM_LBUTTONUP => {
                    with_app(|app| app.toggle());
                }
                _ => {}
            }
            0
        }
        // Lets the menu IDs be triggered from outside, e.g. by tests.
        WM_COMMAND => {
            run_command(hwnd, (wp & 0xFFFF) as u32);
            0
        }
        WM_SETTINGCHANGE => {
            if setting_is(lp, "ImmersiveColorSet") {
                theme::apply_to_menus();
                ui::refresh();
                with_app(|app| app.reload_icons());
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DESTROY => {
            with_app(|app| app.shutdown());
            PostQuitMessage(0);
            0
        }
        _ => {
            if with_app(|app| app.taskbar_created != 0 && app.taskbar_created == msg) == Some(true) {
                with_app(|app| app.add_icon());
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
}

fn main() {
    unsafe {
        let _mutex = CreateMutexW(null(), 1, wide("Local\\KeepAwake.SingleInstance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            MessageBoxW(
                null_mut(),
                wide("KeepAwake is already running. Look for its icon in the system tray.").as_ptr(),
                wide("KeepAwake").as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
            return;
        }

        // Crisp icons and menus on scaled displays.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(null());
        let class = wide("KeepAwakeTray");
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinstance;
        wc.hIcon = LoadIconW(hinstance, 1 as _); // embedded by build.rs
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        let cfg = Config::load();
        if !config::path().exists() {
            let _ = cfg.save();
        }
        theme::set_pref(cfg.theme);
        theme::apply_to_menus();
        let theme = Some(cfg.theme);
        let light = theme::taskbar_light();
        let last_on = if cfg.mode != Mode::Off { cfg.mode } else { Mode::Activity };
        let app = App {
            hwnd,
            cfg,
            cfg_mtime: config::mtime(),
            last_on,
            icons: [Look::Off, Look::Awake, Look::Activity].map(|look| icons::make(look, light)),
            applied: None,
            shown: None,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            theme,
        };
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        with_app(|app| app.add_icon());
        SetTimer(hwnd, TIMER_ID, TICK_MS, None);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            // Tab, Enter and Esc in the Settings and About windows.
            let active = GetActiveWindow();
            if !active.is_null() && active != hwnd && IsDialogMessageW(active, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
