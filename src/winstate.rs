//! Remembers the window's size, position and maximized state between runs
//! (%LOCALAPPDATA%\PcTube\window.json).

use crate::config;
use serde::{Deserialize, Serialize};
use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Size},
    event_loop::EventLoopWindowTarget,
    window::Window,
};

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Saved {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    maximized: bool,
}

fn path() -> std::path::PathBuf {
    config::data_dir().join(if config::debug_session() { "window-debug.json" } else { "window.json" })
}

fn load() -> Option<Saved> {
    serde_json::from_str(&std::fs::read_to_string(path()).ok()?).ok()
}

/// Where and how big the window should start: (position, size, maximized).
/// First run: maximized. A saved position that is no longer on any screen (monitor unplugged,
/// resolution changed) is dropped so the window can't open out of reach.
pub fn initial<T>(target: &EventLoopWindowTarget<T>) -> (Option<PhysicalPosition<i32>>, Size, bool) {
    let Some(s) = load() else {
        return (None, LogicalSize::new(1280.0, 800.0).into(), true);
    };
    let (px, py) = (s.x + 40, s.y + 40); // a point inside the title area
    let on_screen = target.available_monitors().any(|m| {
        let (p, sz) = (m.position(), m.size());
        px >= p.x && py >= p.y && px < p.x + sz.width as i32 && py < p.y + sz.height as i32
    });
    let size = PhysicalSize::new(s.w.max(480), s.h.max(320)).into();
    (on_screen.then(|| PhysicalPosition::new(s.x, s.y)), size, s.maximized)
}

/// Saves the current geometry. While maximized/fullscreen/minimized the previous restored
/// size is kept, so un-maximizing later returns to a sensible window.
pub fn save(window: &Window) {
    let prev = load();
    let normal = !window.is_maximized() && window.fullscreen().is_none() && !window.is_minimized();
    let state = if normal {
        let (p, s) = (window.outer_position().ok(), window.inner_size());
        match p {
            Some(p) => Saved { x: p.x, y: p.y, w: s.width, h: s.height, maximized: false },
            None => return,
        }
    } else {
        match prev {
            Some(mut s) => {
                s.maximized = window.is_maximized() || window.fullscreen().is_some() || s.maximized;
                s
            }
            // never seen a normal window: just remember that it was maximized
            None => Saved { x: 100, y: 100, w: 1280, h: 800, maximized: window.is_maximized() },
        }
    };
    if let Ok(s) = serde_json::to_string(&state) {
        let _ = std::fs::write(path(), s);
    }
}
