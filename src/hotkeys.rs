//! Global keyboard shortcuts (work from any app, even with the window hidden in the tray).
//! Defaults: Ctrl+Alt+Space play/pause, Ctrl+Alt+Right next, Ctrl+Alt+Left previous. They can be
//! changed in Ajustes -> Atajos (config key "globalShortcuts").

use crate::ipc::{Out, TrayAction};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use serde_json::Value;
use std::sync::{Mutex, Once};
use tao::event_loop::EventLoopProxy;

/// (config key, action, default combination)
pub const ACTIONS: &[(&str, TrayAction, &str)] = &[
    ("playPause", TrayAction::PlayPause, "Ctrl+Alt+Space"),
    ("next", TrayAction::Next, "Ctrl+Alt+Right"),
    ("prev", TrayAction::Prev, "Ctrl+Alt+Left"),
];

/// Which hotkey id means what, and where to send it. The crate accepts its event handler only
/// once per process, so the handler is installed once and reads this table.
static TARGET: Mutex<Option<(EventLoopProxy<Out>, Vec<(u32, TrayAction)>)>> = Mutex::new(None);
static INSTALL: Once = Once::new();
/// Shortcuts that could not be registered the last time ("Name: combination"), for the settings.
static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn failed() -> Vec<String> {
    FAILED.lock().unwrap().clone()
}

fn code_of(key: &str) -> Option<Code> {
    use Code::*;
    let k = key.to_ascii_lowercase();
    if k.len() == 1 {
        let c = k.as_bytes()[0];
        if c.is_ascii_lowercase() {
            return Some(
                [
                    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
                    KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
                ][(c - b'a') as usize],
            );
        }
        if c.is_ascii_digit() {
            return Some([Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9][(c - b'0') as usize]);
        }
    }
    if let Some(n) = k.strip_prefix('f').and_then(|n| n.parse::<usize>().ok()) {
        let f = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
        return (1..=12).contains(&n).then(|| f[n - 1]);
    }
    Some(match k.as_str() {
        "space" => Space,
        "enter" => Enter,
        "tab" => Tab,
        "left" => ArrowLeft,
        "right" => ArrowRight,
        "up" => ArrowUp,
        "down" => ArrowDown,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        "insert" => Insert,
        "delete" => Delete,
        "," => Comma,
        "." => Period,
        "-" => Minus,
        "=" => Equal,
        "[" => BracketLeft,
        "]" => BracketRight,
        ";" => Semicolon,
        "'" => Quote,
        "/" => Slash,
        "\\" => Backslash,
        "`" => Backquote,
        _ => return None,
    })
}

/// "Ctrl+Alt+Space" -> hotkey. At least one of Ctrl / Alt / Win is required, so that a global
/// shortcut can never swallow plain typing.
pub fn parse_combo(s: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut key = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "win" | "meta" | "super" => mods |= Modifiers::META,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(code_of(part)?);
            }
        }
    }
    if !mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) {
        return None;
    }
    Some(HotKey::new(Some(mods), key?))
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    keys: Vec<HotKey>,
}

impl Hotkeys {
    /// Registers the shortcuts in `shortcuts` (config "globalShortcuts"; missing ones use the
    /// default). Ones that are invalid or already owned by another app are skipped and listed in
    /// `failed()`. None if nothing could be registered.
    pub fn register(proxy: EventLoopProxy<Out>, shortcuts: &Value) -> Option<Self> {
        let mut failed = Vec::new();
        let manager = GlobalHotKeyManager::new().ok()?;
        let mut keys = Vec::new();
        let mut actions = Vec::new();
        for (name, action, default) in ACTIONS {
            let combo = shortcuts.get(*name).and_then(Value::as_str).unwrap_or(default);
            if combo.trim().is_empty() {
                continue; // left empty on purpose
            }
            match parse_combo(combo) {
                Some(hk) if manager.register(hk).is_ok() => {
                    actions.push((hk.id(), *action));
                    keys.push(hk);
                }
                _ => failed.push(format!("{name}: {combo}")),
            }
        }
        *FAILED.lock().unwrap() = failed;
        if keys.is_empty() {
            return None;
        }
        *TARGET.lock().unwrap() = Some((proxy, actions));
        INSTALL.call_once(|| {
            GlobalHotKeyEvent::set_event_handler(Some(|e: GlobalHotKeyEvent| {
                if e.state() != HotKeyState::Pressed {
                    return;
                }
                if let Some((proxy, actions)) = TARGET.lock().unwrap().as_ref() {
                    if let Some((_, a)) = actions.iter().find(|(id, _)| *id == e.id()) {
                        let _ = proxy.send_event(Out::Tray(*a));
                    }
                }
            }));
        });
        Some(Self { manager, keys })
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        let _ = self.manager.unregister_all(&self.keys);
        *TARGET.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combinations() {
        let a = parse_combo("Ctrl+Alt+Space").unwrap();
        assert_eq!(a, HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space));
        assert_eq!(parse_combo("ctrl + shift + k").unwrap(), HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyK));
        assert_eq!(parse_combo("Win+F5").unwrap(), HotKey::new(Some(Modifiers::META), Code::F5));
        assert_eq!(parse_combo("Alt+,").unwrap(), HotKey::new(Some(Modifiers::ALT), Code::Comma));
    }

    #[test]
    fn rejects_unsafe_or_garbage_combinations() {
        assert!(parse_combo("Space").is_none(), "no modifier");
        assert!(parse_combo("Shift+A").is_none(), "shift alone would swallow typing");
        assert!(parse_combo("Ctrl+").is_none(), "no key");
        assert!(parse_combo("Ctrl+A+B").is_none(), "two keys");
        assert!(parse_combo("Ctrl+Banana").is_none());
        assert!(parse_combo("").is_none());
    }

    #[test]
    fn defaults_are_valid() {
        for (_, _, d) in ACTIONS {
            assert!(parse_combo(d).is_some(), "{d}");
        }
    }
}
