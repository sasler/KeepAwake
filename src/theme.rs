//! Light/dark colors that follow the Windows setting (or the `theme` config key).

use std::cell::Cell;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

use crate::config::Theme;
use crate::wide;

thread_local! {
    static PREF: Cell<Theme> = const { Cell::new(Theme::System) };
}

pub struct Palette {
    pub bg: u32,
    pub field: u32,
    pub text: u32,
    pub muted: u32,
    pub link: u32,
    pub error: u32,
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

const LIGHT: Palette = Palette {
    bg: rgb(0xF3, 0xF3, 0xF3),
    field: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x1A, 0x1A, 0x1A),
    muted: rgb(0x5F, 0x5F, 0x5F),
    link: rgb(0x00, 0x5F, 0xB8),
    error: rgb(0xC4, 0x2B, 0x1C),
};

const DARK: Palette = Palette {
    bg: rgb(0x20, 0x20, 0x20),
    field: rgb(0x2D, 0x2D, 0x2D),
    text: rgb(0xFF, 0xFF, 0xFF),
    muted: rgb(0xA8, 0xA8, 0xA8),
    link: rgb(0x4C, 0xC2, 0xFF),
    error: rgb(0xFF, 0x99, 0xA4),
};

/// A DWORD under HKCU\...\Themes\Personalize; Windows treats a missing value as 1 (light).
fn personalize(name: &str) -> bool {
    let mut value = 1u32;
    let mut len = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide(name).as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut u32 as _,
            &mut len,
        )
    };
    r != ERROR_SUCCESS || value != 0
}

/// Whether the taskbar (and so the tray) is light.
pub fn taskbar_light() -> bool {
    personalize("SystemUsesLightTheme")
}

pub fn dark() -> bool {
    match PREF.with(Cell::get) {
        Theme::System => !personalize("AppsUseLightTheme"),
        Theme::Light => false,
        Theme::Dark => true,
    }
}

pub fn palette() -> &'static Palette {
    if dark() { &DARK } else { &LIGHT }
}

pub fn set_pref(pref: Theme) {
    PREF.with(|p| p.set(pref));
}

/// Switches popup menus between light and dark. Windows only exposes this through
/// undocumented uxtheme exports (by ordinal, Windows 10 1903+); older builds keep light menus.
pub fn apply_to_menus() {
    unsafe {
        let ux = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if ux.is_null() {
            return;
        }
        // SetPreferredAppMode(135): 2 = force dark, 3 = force light. FlushMenuThemes(136).
        if let Some(f) = GetProcAddress(ux, 135 as _) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set_mode(if dark() { 2 } else { 3 });
        }
        if let Some(f) = GetProcAddress(ux, 136 as _) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(f);
            flush();
        }
    }
}
