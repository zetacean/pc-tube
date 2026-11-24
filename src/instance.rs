//! Single instance: a second launch brings the running window forward instead of opening
//! another one (important now that closing can hide the app in the tray).

use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW},
};

pub const WINDOW_TITLE: &str = "PC Tube";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True if this is the first instance. Otherwise the running one is shown and false is returned.
pub fn acquire() -> bool {
    let name = wide("Local\\PcTube.SingleInstance");
    // SAFETY: plain Win32 calls with valid, NUL-terminated UTF-16 strings. The mutex handle is
    // deliberately never closed: it must live as long as the process.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            return true; // can't tell; don't block the app from starting
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let title = wide(WINDOW_TITLE);
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
            return false;
        }
    }
    true
}
