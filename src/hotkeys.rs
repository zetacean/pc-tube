//! Global keyboard shortcuts (work from any app, even with the window hidden in the tray):
//! Ctrl+Alt+Space play/pause, Ctrl+Alt+Right next, Ctrl+Alt+Left previous.

use crate::ipc::{Out, TrayAction};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::sync::{Mutex, Once};
use tao::event_loop::EventLoopProxy;

/// Which hotkey id means what, and where to send it. The crate accepts its event handler only
/// once per process, so the handler is installed once and reads this table.
static TARGET: Mutex<Option<(EventLoopProxy<Out>, Vec<(u32, TrayAction)>)>> = Mutex::new(None);
static INSTALL: Once = Once::new();

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    keys: Vec<HotKey>,
}

impl Hotkeys {
    /// None if the shortcuts could not be registered at all (another app owns them).
    pub fn register(proxy: EventLoopProxy<Out>) -> Option<Self> {
        let manager = GlobalHotKeyManager::new().ok()?;
        let m = Some(Modifiers::CONTROL | Modifiers::ALT);
        let table = [
            (HotKey::new(m, Code::Space), TrayAction::PlayPause),
            (HotKey::new(m, Code::ArrowRight), TrayAction::Next),
            (HotKey::new(m, Code::ArrowLeft), TrayAction::Prev),
        ];
        let mut keys = Vec::new();
        let mut actions = Vec::new();
        for (hk, action) in table {
            if manager.register(hk).is_ok() {
                actions.push((hk.id(), action));
                keys.push(hk);
            }
        }
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
