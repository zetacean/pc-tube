//! "Iniciar con Windows": a value under HKCU\...\Run (no admin rights needed). The app starts
//! with --minimized so it goes straight to the tray.

use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "PC Tube";

pub fn set(enabled: bool) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok((key, _)) = hkcu.create_subkey(RUN_KEY) else { return };
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let _ = key.set_value(NAME, &format!("\"{}\" --minimized", exe.display()));
        }
    } else {
        let _ = key.delete_value(NAME);
    }
}

/// Whether the Run entry currently exists (used to keep the setting honest).
#[allow(dead_code)]
pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(NAME))
        .is_ok()
}
