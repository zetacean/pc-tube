//! Buttons on the taskbar thumbnail (hover the taskbar icon): previous, play/pause, next,
//! like Spotify. Uses ITaskbarList3; the clicks arrive as WM_COMMAND on the window, so the
//! window procedure is wrapped to catch them.

use crate::ipc::{Out, TrayAction};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicIsize, AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};
use tao::event_loop::EventLoopProxy;
use windows::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{CreateBitmap, DeleteObject},
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
        UI::{
            Shell::{ITaskbarList3, TaskbarList, THBF_ENABLED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON},
            WindowsAndMessaging::{
                CallWindowProcW, CreateIconIndirect, GetSystemMetrics, RegisterWindowMessageW,
                SetWindowLongPtrW, GWLP_WNDPROC, HICON, ICONINFO, SM_CXSMICON, WM_COMMAND, WNDPROC,
            },
        },
    },
};

const ID_PREV: u32 = 0;
const ID_PLAY: u32 = 1;
const ID_NEXT: u32 = 2;
const THBN_CLICKED: u32 = 0x1800;

static PROXY: OnceLock<Mutex<EventLoopProxy<Out>>> = OnceLock::new();
static ORIG_PROC: AtomicIsize = AtomicIsize::new(0);
static BUTTON_CREATED: AtomicU32 = AtomicU32::new(0);

struct State {
    list: ITaskbarList3,
    hwnd: HWND,
    prev: HICON,
    next: HICON,
    play: HICON,
    pause: HICON,
    added: bool,
    paused: bool,
}

thread_local! {
    // COM objects are apartment-bound: this only lives on the UI thread.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

// ---------- glyphs (drawn with signed distance fields, white with a dark outline) ----------

#[derive(Clone, Copy)]
enum Glyph {
    Prev,
    Play,
    Pause,
    Next,
}

fn sd_rect(u: f32, v: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let (cx, cy, hx, hy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let (dx, dy) = ((u - cx).abs() - hx, (v - cy).abs() - hy);
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0)
}

fn sd_tri(u: f32, v: f32, p: [(f32, f32); 3]) -> f32 {
    let mut d = f32::MAX;
    let (mut neg, mut pos) = (false, false);
    for i in 0..3 {
        let (a, b) = (p[i], p[(i + 1) % 3]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let (wx, wy) = (u - a.0, v - a.1);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        d = d.min((wx - ex * t).hypot(wy - ey * t));
        // Inside when the point is on the same side of all three edges (any winding order).
        if ex * wy - ey * wx < 0.0 {
            neg = true;
        } else {
            pos = true;
        }
    }
    if neg && pos { d } else { -d }
}

fn distance(g: Glyph, u: f32, v: f32) -> f32 {
    match g {
        Glyph::Play => sd_tri(u, v, [(0.26, 0.16), (0.26, 0.84), (0.82, 0.5)]),
        Glyph::Pause => sd_rect(u, v, 0.22, 0.16, 0.42, 0.84).min(sd_rect(u, v, 0.58, 0.16, 0.78, 0.84)),
        Glyph::Next => sd_tri(u, v, [(0.16, 0.2), (0.16, 0.8), (0.62, 0.5)]).min(sd_rect(u, v, 0.70, 0.2, 0.84, 0.8)),
        Glyph::Prev => sd_tri(u, v, [(0.84, 0.8), (0.84, 0.2), (0.38, 0.5)]).min(sd_rect(u, v, 0.16, 0.2, 0.30, 0.8)),
    }
}

/// Straight-alpha BGRA pixels of a glyph, `n` x `n`.
fn render_glyph(g: Glyph, n: usize) -> Vec<u8> {
    const SS: usize = 4;
    const OUTLINE: f32 = 0.06;
    let mut bgra = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (mut white, mut alpha) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / n as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / n as f32;
                    let d = distance(g, u, v);
                    if d <= 0.0 {
                        white += 1.0;
                        alpha += 1.0;
                    } else if d <= OUTLINE {
                        alpha += 0.55; // dark halo so it reads on light and dark thumbnails
                    }
                }
            }
            let cnt = (SS * SS) as f32;
            let (a, w) = (alpha / cnt, white / cnt);
            if a > 0.0 {
                let c = ((w / a) * 255.0).round() as u8; // straight (non-premultiplied) grey
                let o = (y * n + x) * 4;
                bgra[o] = c;
                bgra[o + 1] = c;
                bgra[o + 2] = c;
                bgra[o + 3] = (a * 255.0).round() as u8;
            }
        }
    }
    bgra
}

fn make_icon(g: Glyph, size: i32) -> Option<HICON> {
    let n = size as usize;
    let bgra = render_glyph(g, n);
    // SAFETY: the pixel buffers outlive the CreateBitmap calls (which copy them); the bitmaps
    // are deleted once CreateIconIndirect has made its own copy.
    unsafe {
        let color = CreateBitmap(size, size, 1, 32, Some(bgra.as_ptr().cast()));
        let mask_bits = vec![0u8; ((n + 15) / 16 * 2) * n];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

/// Debug builds only: appends to %LOCALAPPDATA%\PcTube\taskbar-debug.log.
fn debug_log(msg: &str) {
    #[cfg(debug_assertions)]
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(crate::config::data_dir().join("taskbar-debug.log"))
        {
            let _ = writeln!(f, "{msg}");
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = msg;
}

// ---------- buttons ----------

fn button(id: u32, icon: HICON, tip: &str) -> THUMBBUTTON {
    let mut tip_buf = [0u16; 260];
    for (i, c) in tip.encode_utf16().take(259).enumerate() {
        tip_buf[i] = c;
    }
    THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: tip_buf,
        dwFlags: THBF_ENABLED,
    }
}

impl State {
    fn play_button(&self) -> THUMBBUTTON {
        if self.paused {
            button(ID_PLAY, self.play, "Reproducir")
        } else {
            button(ID_PLAY, self.pause, "Pausar")
        }
    }

    /// Fails harmlessly until Windows has created the taskbar button for the window.
    fn add_buttons(&mut self) {
        let buttons = [
            button(ID_PREV, self.prev, "Anterior"),
            self.play_button(),
            button(ID_NEXT, self.next, "Siguiente"),
        ];
        // SAFETY: valid window handle and button array for the duration of the call.
        let result = unsafe { self.list.ThumbBarAddButtons(self.hwnd, &buttons) };
        debug_log(&format!("ThumbBarAddButtons -> {result:?}"));
        if result.is_ok() {
            self.added = true;
        }
    }

    fn update_play(&self) {
        // SAFETY: as above.
        let result = unsafe { self.list.ThumbBarUpdateButtons(self.hwnd, &[self.play_button()]) };
        debug_log(&format!("ThumbBarUpdateButtons(paused={}) -> {result:?}", self.paused));
    }
}

unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let created = BUTTON_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        // The taskbar (re)created our button, e.g. after the window came back from the tray.
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                st.added = false;
                st.add_buttons();
            }
        });
    } else if msg == WM_COMMAND && ((wp.0 >> 16) & 0xFFFF) as u32 == THBN_CLICKED {
        let action = match (wp.0 & 0xFFFF) as u32 {
            ID_PREV => Some(TrayAction::Prev),
            ID_PLAY => Some(TrayAction::PlayPause),
            ID_NEXT => Some(TrayAction::Next),
            _ => None,
        };
        if let (Some(a), Some(p)) = (action, PROXY.get()) {
            let _ = p.lock().unwrap().send_event(Out::Tray(a));
            return LRESULT(0);
        }
    }
    // SAFETY: ORIG_PROC holds the window procedure that was installed before ours.
    let orig: WNDPROC = unsafe { std::mem::transmute::<isize, WNDPROC>(ORIG_PROC.load(Ordering::Relaxed)) };
    unsafe { CallWindowProcW(orig, hwnd, msg, wp, lp) }
}

/// Sets up the thumbnail buttons for `hwnd` (a tao window handle). Call on the UI thread.
pub fn init(hwnd: isize, proxy: EventLoopProxy<Out>) {
    let _ = PROXY.set(Mutex::new(proxy));
    let hwnd = HWND(hwnd as *mut _);

    // SAFETY: plain Win32/COM calls on the UI thread with a valid window handle.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); // already initialised is fine

        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        let (Some(prev), Some(play), Some(pause), Some(next)) = (
            make_icon(Glyph::Prev, size),
            make_icon(Glyph::Play, size),
            make_icon(Glyph::Pause, size),
            make_icon(Glyph::Next, size),
        ) else {
            return;
        };
        let Ok(list) = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) else {
            return;
        };
        if list.HrInit().is_err() {
            return;
        }

        BUTTON_CREATED.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::Relaxed);
        let mut state = State { list, hwnd, prev, next, play, pause, added: false, paused: true };
        // The button may already exist (we run after the window is shown): try right away,
        // and again whenever Windows announces it.
        state.add_buttons();
        STATE.with(|s| *s.borrow_mut() = Some(state));

        let orig = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, subclass_proc as *const () as isize);
        ORIG_PROC.store(orig, Ordering::Relaxed);
    }
}

/// Swaps the middle button between play and pause.
pub fn set_paused(paused: bool) {
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            if st.paused != paused {
                st.paused = paused;
                if st.added {
                    st.update_play();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the four glyphs side by side (64 px each, BGRA) to $PCTUBE_GLYPH_DUMP, to look at them.
    #[test]
    fn dump_glyphs() {
        let Ok(path) = std::env::var("PCTUBE_GLYPH_DUMP") else { return };
        let n = 64;
        let mut out = Vec::new();
        for g in [Glyph::Prev, Glyph::Play, Glyph::Pause, Glyph::Next] {
            out.extend(render_glyph(g, n));
        }
        // one row per glyph, glyphs are stored one after another; reorganise into a strip
        let mut strip = vec![0u8; n * n * 4 * 4];
        for gi in 0..4 {
            for y in 0..n {
                let src = (gi * n * n + y * n) * 4;
                let dst = (y * n * 4 + gi * n) * 4;
                strip[dst..dst + n * 4].copy_from_slice(&out[src..src + n * 4]);
            }
        }
        std::fs::write(path, strip).unwrap();
    }
}
