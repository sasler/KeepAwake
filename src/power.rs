use std::mem::{size_of, zeroed};

use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, SendInput, INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYEVENTF_KEYUP, LASTINPUTINFO,
    MOUSEEVENTF_MOVE, VK_F15,
};

use crate::config::InputKind;

/// Tells Windows the display and system are in use (or releases that request).
/// The request belongs to the calling thread, which lives as long as the app.
pub fn set_awake(on: bool) {
    let flags = if on { ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
    unsafe { SetThreadExecutionState(flags) };
}

/// Milliseconds since the last keyboard or mouse input in this session.
pub fn idle_ms() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime)
    }
}

/// Sends input that resets the idle timer without visibly doing anything.
pub fn nudge(kind: InputKind) {
    unsafe {
        let mut inputs: [INPUT; 2] = zeroed();
        let count = match kind {
            InputKind::F15 => {
                inputs[0].r#type = INPUT_KEYBOARD;
                inputs[0].Anonymous.ki.wVk = VK_F15;
                inputs[1].r#type = INPUT_KEYBOARD;
                inputs[1].Anonymous.ki.wVk = VK_F15;
                inputs[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                2
            }
            InputKind::Mouse => {
                inputs[0].r#type = INPUT_MOUSE;
                inputs[0].Anonymous.mi.dwFlags = MOUSEEVENTF_MOVE;
                1
            }
        };
        SendInput(count, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
}
