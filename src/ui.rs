//! Small themed windows (Settings, About) built from plain Win32 controls.
//!
//! Layouts are given in 96-DPI pixels and rescaled on DPI changes. Checkboxes are owner-drawn
//! because themed checkbox labels ignore custom text colors, which breaks dark mode.
//! Form state hangs off GWLP_USERDATA and is only read through shared references, so window
//! messages that re-enter the procedure are harmless.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    CloseThemeData, DrawThemeBackground, GetThemePartSize, SetWindowTheme, BP_CHECKBOX,
    DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED, TS_DRAW,
};
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, OpenThemeDataForDpi, SystemParametersInfoForDpi,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{theme, wide};

pub const SETTINGS: usize = 0;
pub const ABOUT: usize = 1;

const WM_THEME: u32 = WM_APP + 10;
const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
// Static control styles (windows-sys keeps these behind another feature).
const SS_ICON: u32 = 0x3;
const SS_NOPREFIX: u32 = 0x80;
const SS_NOTIFY: u32 = 0x100;
const SS_CENTERIMAGE: u32 = 0x200;
/// Extra height for a combo box's drop-down list.
const DROP_HEIGHT: i32 = 160;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Text,
    Muted,
    Header,
    Title,
    Link,
    Error,
    Edit,
    Combo,
    Check,
    Button,
    DefaultButton,
    Icon,
}

/// A control: kind, id, text and [x, y, width, height] at 96 DPI.
pub type Spec<'a> = (Kind, u16, &'a str, [i32; 4]);

struct Form {
    /// Client size at 96 DPI.
    size: (i32, i32),
    ctls: RefCell<Vec<(HWND, Kind, [i32; 4])>>,
    /// Normal, bold, title, underlined.
    fonts: Cell<[HFONT; 4]>,
    /// Window background, input fields.
    brushes: Cell<[HBRUSH; 2]>,
    focus: Cell<HWND>,
    command: fn(HWND, u16, u16),
    slot: usize,
}

impl Form {
    fn kind(&self, hwnd: HWND) -> Option<Kind> {
        self.ctls.borrow().iter().find(|c| c.0 == hwnd).map(|c| c.1)
    }

    fn ctls(&self) -> Vec<(HWND, Kind, [i32; 4])> {
        self.ctls.borrow().clone()
    }
}

thread_local! {
    static OPEN: [Cell<HWND>; 2] = const { [Cell::new(null_mut()), Cell::new(null_mut())] };
}

fn form<'a>(hwnd: HWND) -> Option<&'a Form> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Form).as_ref() }
}

fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

/// Creates the window for `slot` (hidden, so the caller can fill in values before `show`).
/// If it is already open, brings it to the front and returns None.
pub fn open(slot: usize, title: &str, size: (i32, i32), specs: &[Spec], command: fn(HWND, u16, u16)) -> Option<HWND> {
    let existing = OPEN.with(|o| o[slot].get());
    unsafe {
        if !existing.is_null() {
            ShowWindow(existing, SW_RESTORE);
            SetForegroundWindow(existing);
            return None;
        }
        let hinstance = GetModuleHandleW(null());
        let class = wide("KeepAwakeForm");
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(form_proc);
        wc.hInstance = hinstance;
        wc.hIcon = LoadIconW(hinstance, 1 as _);
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc); // fails harmlessly when already registered

        // Open on the monitor with the cursor, i.e. where the tray menu was.
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST), &mut mi);
        let work = mi.rcWork;

        let state = Box::new(Form {
            size,
            ctls: RefCell::new(Vec::new()),
            fonts: Cell::new([null_mut(); 4]),
            brushes: Cell::new([null_mut(); 2]),
            focus: Cell::new(null_mut()),
            command,
            slot,
        });
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(title).as_ptr(),
            STYLE,
            work.left,
            work.top,
            100,
            100,
            null_mut(),
            null_mut(),
            hinstance,
            Box::into_raw(state) as _,
        );
        let f = form(hwnd)?;
        for &(kind, id, text, rect) in specs {
            let (class, style) = match kind {
                Kind::Edit => ("EDIT", WS_TABSTOP | ES_AUTOHSCROLL as u32),
                Kind::Combo => ("COMBOBOX", WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32),
                Kind::Check => ("BUTTON", WS_TABSTOP | BS_OWNERDRAW as u32),
                Kind::Button => ("BUTTON", WS_TABSTOP | BS_PUSHBUTTON as u32),
                Kind::DefaultButton => ("BUTTON", WS_TABSTOP | BS_DEFPUSHBUTTON as u32),
                Kind::Icon => ("STATIC", SS_ICON | SS_CENTERIMAGE),
                Kind::Link => ("STATIC", SS_NOTIFY | SS_NOPREFIX | SS_CENTERIMAGE),
                _ => ("STATIC", SS_NOPREFIX | SS_CENTERIMAGE),
            };
            let ex = if kind == Kind::Edit { WS_EX_CLIENTEDGE } else { 0 };
            let ctl = CreateWindowExW(
                ex,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                0,
                0,
                hwnd,
                id as usize as HMENU,
                hinstance,
                null(),
            );
            f.ctls.borrow_mut().push((ctl, kind, rect));
        }
        OPEN.with(|o| o[slot].set(hwnd));
        layout(hwnd, f, GetDpiForWindow(hwnd));
        let mut rc: RECT = zeroed();
        GetWindowRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top - h) / 2;
        SetWindowPos(hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        apply_theme(hwnd, f);
        Some(hwnd)
    }
}

pub fn show(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(GetNextDlgTabItem(hwnd, null_mut(), 0));
    }
}

/// Closes the window once the current message is handled.
pub fn close(hwnd: HWND) {
    unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
}

/// Re-applies colors to all open windows after a theme change.
pub fn refresh() {
    for slot in [SETTINGS, ABOUT] {
        let hwnd = OPEN.with(|o| o[slot].get());
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_THEME, 0, 0) };
        }
    }
}

pub fn open_url(url: &str) {
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(url).as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

fn item(dlg: HWND, id: u16) -> HWND {
    unsafe { GetDlgItem(dlg, id as i32) }
}

pub fn text(dlg: HWND, id: u16) -> String {
    let h = item(dlg, id);
    unsafe {
        let mut buf = vec![0u16; GetWindowTextLengthW(h) as usize + 1];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

pub fn set_text(dlg: HWND, id: u16, text: &str) {
    unsafe { SetWindowTextW(item(dlg, id), wide(text).as_ptr()) };
}

pub fn checked(dlg: HWND, id: u16) -> bool {
    is_checked(item(dlg, id))
}

pub fn set_checked(dlg: HWND, id: u16, on: bool) {
    check(item(dlg, id), on);
}

fn is_checked(ctl: HWND) -> bool {
    unsafe { GetWindowLongPtrW(ctl, GWLP_USERDATA) != 0 }
}

fn check(ctl: HWND, on: bool) {
    unsafe {
        SetWindowLongPtrW(ctl, GWLP_USERDATA, on as isize);
        InvalidateRect(ctl, null(), 0);
    }
}

pub fn combo(dlg: HWND, id: u16, items: &[&str], selected: usize) {
    let h = item(dlg, id);
    unsafe {
        for text in items {
            SendMessageW(h, CB_ADDSTRING, 0, wide(text).as_ptr() as LPARAM);
        }
        SendMessageW(h, CB_SETCURSEL, selected, 0);
    }
}

pub fn combo_sel(dlg: HWND, id: u16) -> usize {
    unsafe { SendMessageW(item(dlg, id), CB_GETCURSEL, 0, 0).max(0) as usize }
}

pub fn enable(dlg: HWND, id: u16, on: bool) {
    unsafe { EnableWindow(item(dlg, id), on as BOOL) };
}

unsafe fn make_fonts(dpi: u32) -> [HFONT; 4] {
    let mut ncm: NONCLIENTMETRICSW = zeroed();
    ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
    SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as _, 0, dpi);
    let base = ncm.lfMessageFont;
    let mut bold = base;
    bold.lfWeight = 600;
    let mut title = bold;
    title.lfHeight = base.lfHeight * 3 / 2;
    let mut link = base;
    link.lfUnderline = 1;
    [base, bold, title, link].map(|lf| CreateFontIndirectW(&lf))
}

/// Sizes the window and positions every control and font for `dpi`.
unsafe fn layout(hwnd: HWND, f: &Form, dpi: u32) {
    let fonts = make_fonts(dpi);
    let hinstance = GetModuleHandleW(null());
    for (ctl, kind, [x, y, w, h]) in f.ctls() {
        let drop = if kind == Kind::Combo { DROP_HEIGHT } else { 0 };
        let (w, h) = (scale(w, dpi), scale(h + drop, dpi));
        SetWindowPos(ctl, null_mut(), scale(x, dpi), scale(y, dpi), w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        let font = match kind {
            Kind::Header => fonts[1],
            Kind::Title => fonts[2],
            Kind::Link => fonts[3],
            _ => fonts[0],
        };
        SendMessageW(ctl, WM_SETFONT, font as WPARAM, 1);
        if kind == Kind::Icon {
            let icon = LoadImageW(hinstance, 1 as _, IMAGE_ICON, w, w, LR_DEFAULTCOLOR);
            let old = SendMessageW(ctl, STM_SETICON, icon as WPARAM, 0);
            if old != 0 {
                DestroyIcon(old as HICON);
            }
        }
    }
    for font in f.fonts.replace(fonts) {
        if !font.is_null() {
            DeleteObject(font);
        }
    }
    let mut rc = RECT { left: 0, top: 0, right: scale(f.size.0, dpi), bottom: scale(f.size.1, dpi) };
    AdjustWindowRectExForDpi(&mut rc, STYLE, 0, 0, dpi);
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        rc.right - rc.left,
        rc.bottom - rc.top,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

unsafe fn apply_theme(hwnd: HWND, f: &Form) {
    let dark = theme::dark();
    let pal = theme::palette();
    for brush in f.brushes.replace([CreateSolidBrush(pal.bg), CreateSolidBrush(pal.field)]) {
        if !brush.is_null() {
            DeleteObject(brush);
        }
    }
    let on = dark as BOOL;
    DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &on as *const BOOL as _, 4);
    for (ctl, kind, _) in f.ctls() {
        let name = match (kind, dark) {
            (Kind::Button | Kind::DefaultButton, true) => "DarkMode_Explorer",
            (Kind::Edit | Kind::Combo, true) => "DarkMode_CFD",
            (Kind::Button | Kind::DefaultButton | Kind::Edit | Kind::Combo, false) => "Explorer",
            _ => continue,
        };
        SetWindowTheme(ctl, wide(name).as_ptr(), null());
    }
    RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN);
}

unsafe fn draw_check(f: &Form, dis: &DRAWITEMSTRUCT) {
    let pal = theme::palette();
    let (ctl, hdc, rc) = (dis.hwndItem, dis.hDC, dis.rcItem);
    FillRect(hdc, &rc, f.brushes.get()[0]);
    let dpi = GetDpiForWindow(ctl);
    let checked = is_checked(ctl);
    let disabled = dis.itemState & ODS_DISABLED != 0;
    // CBS_UNCHECKEDNORMAL = 1, CBS_CHECKEDNORMAL = 5; +2 pressed, +3 disabled.
    let mut state = if checked { 5 } else { 1 };
    if disabled {
        state += 3;
    } else if dis.itemState & ODS_SELECTED != 0 {
        state += 2;
    }
    let class = if theme::dark() { "DarkMode_DarkTheme::Button" } else { "Button" };
    let htheme = OpenThemeDataForDpi(ctl, wide(class).as_ptr(), dpi);
    let mut glyph = SIZE { cx: scale(13, dpi), cy: scale(13, dpi) };
    if htheme != 0 {
        GetThemePartSize(htheme, hdc, BP_CHECKBOX, state, null(), TS_DRAW, &mut glyph);
    }
    let top = rc.top + (rc.bottom - rc.top - glyph.cy) / 2;
    let mut glyph_rc = RECT { left: rc.left, top, right: rc.left + glyph.cx, bottom: top + glyph.cy };
    if htheme != 0 {
        DrawThemeBackground(htheme, hdc, BP_CHECKBOX, state, &glyph_rc, null());
        CloseThemeData(htheme);
    } else {
        let flags = DFCS_BUTTONCHECK | if checked { DFCS_CHECKED } else { 0 };
        DrawFrameControl(hdc, &mut glyph_rc, DFC_BUTTON, flags);
    }

    let mut buf = [0u16; 128];
    let n = GetWindowTextW(ctl, buf.as_mut_ptr(), buf.len() as i32);
    let old = SelectObject(hdc, SendMessageW(ctl, WM_GETFONT, 0, 0) as HGDIOBJ);
    SetBkMode(hdc, TRANSPARENT as _);
    SetTextColor(hdc, if disabled { pal.muted } else { pal.text });
    let mut text_rc = RECT { left: glyph_rc.right + scale(6, dpi), ..rc };
    let flags = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
    DrawTextW(hdc, buf.as_ptr(), n, &mut text_rc, flags);
    if dis.itemState & ODS_FOCUS != 0 && dis.itemState & ODS_NOFOCUSRECT == 0 {
        let mut focus_rc = text_rc;
        DrawTextW(hdc, buf.as_ptr(), n, &mut focus_rc, flags | DT_CALCRECT);
        let h = focus_rc.bottom - focus_rc.top;
        focus_rc.top = rc.top + (rc.bottom - rc.top - h) / 2;
        focus_rc.bottom = focus_rc.top + h;
        focus_rc.left -= 2;
        focus_rc.right += 2;
        DrawFocusRect(hdc, &focus_rc);
    }
    SelectObject(hdc, old);
}

unsafe extern "system" fn form_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let Some(f) = form(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_ERASEBKGND => {
            let mut rc: RECT = zeroed();
            GetClientRect(hwnd, &mut rc);
            FillRect(wp as HDC, &rc, f.brushes.get()[0]);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
            let pal = theme::palette();
            let [bg, field] = f.brushes.get();
            let kind = f.kind(lp as HWND);
            let (color, back, brush) = match (msg, kind) {
                (WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX, _) | (_, Some(Kind::Edit | Kind::Combo)) => {
                    (pal.text, pal.field, field)
                }
                (_, Some(Kind::Muted)) => (pal.muted, pal.bg, bg),
                (_, Some(Kind::Link)) => (pal.link, pal.bg, bg),
                (_, Some(Kind::Error)) => (pal.error, pal.bg, bg),
                _ => (pal.text, pal.bg, bg),
            };
            SetTextColor(wp as HDC, color);
            SetBkColor(wp as HDC, back);
            brush as LRESULT
        }
        WM_DRAWITEM => {
            draw_check(f, &*(lp as *const DRAWITEMSTRUCT));
            1
        }
        WM_COMMAND => {
            let (id, code, ctl) = ((wp & 0xFFFF) as u16, (wp >> 16) as u16, lp as HWND);
            if f.kind(ctl) == Some(Kind::Check) && matches!(code as u32, BN_CLICKED | BN_DOUBLECLICKED) {
                check(ctl, !is_checked(ctl));
            }
            let command = f.command;
            command(hwnd, id, code);
            0
        }
        WM_SETCURSOR if f.kind(wp as HWND) == Some(Kind::Link) => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_ACTIVATE => {
            if wp & 0xFFFF == WA_INACTIVE as usize {
                f.focus.set(GetFocus());
            } else {
                let focus = f.focus.get();
                SetFocus(if focus.is_null() { GetNextDlgTabItem(hwnd, null_mut(), 0) } else { focus });
            }
            0
        }
        WM_DPICHANGED => {
            let rc = &*(lp as *const RECT);
            SetWindowPos(hwnd, null_mut(), rc.left, rc.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            layout(hwnd, f, (wp >> 16) as u32 & 0xFFFF);
            0
        }
        WM_THEME => {
            apply_theme(hwnd, f);
            0
        }
        WM_DESTROY => {
            for (ctl, kind, _) in f.ctls() {
                if kind == Kind::Icon {
                    DestroyIcon(SendMessageW(ctl, STM_GETICON, 0, 0) as HICON);
                }
            }
            0
        }
        WM_NCDESTROY => {
            OPEN.with(|o| o[f.slot].set(null_mut()));
            for font in f.fonts.get() {
                DeleteObject(font);
            }
            for brush in f.brushes.get() {
                DeleteObject(brush);
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(f as *const Form as *mut Form));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
