#![recursion_limit = "256"] // config::defaults() is one big json! literal
#![windows_subsystem = "windows"]

mod autostart;
mod config;
mod download;
mod hotkeys;
mod instance;
mod ipc;
mod net;
mod notify;
mod taskbar;
mod tray;
mod update;
mod winstate;

use std::sync::{Arc, Mutex};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Fullscreen, Icon, Theme, Window, WindowBuilder},
};
use wry::WebViewBuilder;

const START_URL: &str = "https://www.youtube.com";
/// YouTube's dark background, so there is no white flash while the page loads.
const BG: (u8, u8, u8, u8) = (15, 15, 15, 255);
const ICON_RGBA: &[u8] = include_bytes!("../assets/icon-64.rgba");

// Run in the page (also work while the window is hidden in the tray).
const JS_PLAY_PAUSE: &str =
    "(function(){var v=document.querySelector('video');if(!v)return;if(v.paused){v.play();}else{v.pause();}})()";
const JS_NEXT: &str = "(function(){var b=document.querySelector('.ytp-next-button');\
    if(b&&getComputedStyle(b).display!=='none'){b.click();return;}\
    var p=document.getElementById('movie_player');if(p&&p.nextVideo)p.nextVideo();})()";
// Previous: the playlist's previous button, or restart the current video.
const JS_PREV: &str = "(function(){var b=document.querySelector('.ytp-prev-button');\
    if(b&&getComputedStyle(b).display!=='none'){b.click();return;}\
    var v=document.querySelector('video');if(v)v.currentTime=0;})()";

/// Shown immediately, then it redirects to YouTube (so the window is never just black).
fn splash_html() -> String {
    include_str!("../assets/splash.html").replace("{{URL}}", START_URL)
}

fn init_script() -> String {
    [
        include_str!("../scripts/loader.js"), // first: the veil must appear before anything else
        include_str!("../scripts/bootstrap.js"),
        include_str!("../scripts/keys.js"),
        include_str!("../scripts/adblock.js"),
        include_str!("../scripts/sponsorblock.js"),
        include_str!("../scripts/features.js"),
        include_str!("../scripts/appearance.js"),
        include_str!("../scripts/mini.js"),
        include_str!("../scripts/playback.js"),
        include_str!("../scripts/captions.js"),
        include_str!("../scripts/chapters.js"),
        include_str!("../scripts/download.js"),
        include_str!("../scripts/dldialog.js"),
        include_str!("../scripts/navbuttons.js"),
        include_str!("../scripts/quality.js"),
        include_str!("../scripts/boost.js"),
        include_str!("../scripts/native.js"),
        include_str!("../scripts/update.js"),
        include_str!("../scripts/tray.js"),
        include_str!("../scripts/ui.js"),
        include_str!("../scripts/titlebar.js"),
    ]
    .join("\n;\n")
}

fn show_window(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
}

/// A visible error: the release build has no console, so eprintln! would go nowhere.
fn error_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("PC Tube"));
    // SAFETY: both strings are NUL-terminated UTF-16 and outlive the call.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn main() {
    // Debug builds started with PCTUBE_DEBUG_PORT get their own WebView2 profile and skip the
    // single-instance check, so they can run next to a normal session.
    let debug_session = config::debug_session();

    // A second launch just brings the running window forward.
    if !debug_session && !instance::acquire() {
        return;
    }

    let cfg: ipc::Config = Arc::new(Mutex::new(config::load()));

    let event_loop = EventLoopBuilder::<ipc::Out>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window_icon = Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok();

    // Started by Windows at login (see "Iniciar con Windows"): go straight to the tray.
    let minimized_start = std::env::args().any(|a| a == "--minimized");
    let (position, size, maximized) = winstate::initial(&event_loop);
    let always_on_top = cfg.lock().unwrap()["alwaysOnTop"].as_bool().unwrap_or(false);

    // Frameless: the page draws its own window buttons (scripts/titlebar.js).
    let mut builder = WindowBuilder::new()
        .with_title(instance::WINDOW_TITLE)
        .with_window_icon(window_icon)
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_theme(Some(Theme::Dark))
        .with_always_on_top(always_on_top)
        .with_visible(!minimized_start)
        .with_maximized(maximized)
        .with_inner_size(size);
    if let Some(p) = position {
        builder = builder.with_position(p);
    }
    let window = builder.build(&event_loop).expect("no se pudo crear la ventana");

    // Persistent WebView2 profile, so the Google login survives restarts.
    let profile = if debug_session { "webview-debug" } else { "webview" };
    let mut web_context = wry::WebContext::new(Some(config::data_dir().join(profile)));

    let initial_cfg = serde_json::to_string(&*cfg.lock().unwrap()).unwrap();
    // __PCTUBE_BOOT__: when this run started. The page compares it with the time of its last saved
    // setting to know whether its own copy is newer than this snapshot (see bootstrap.js).
    let boot_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let script = format!(
        "window.__PCTUBE_CONFIG__ = {initial_cfg};\nwindow.__PCTUBE_BOOT__ = {boot_ms};\n{}",
        init_script()
    );

    let ipc_cfg = cfg.clone();
    let ipc_proxy = proxy.clone();
    let builder = WebViewBuilder::new_with_web_context(&mut web_context);

    // Debug builds only: set PCTUBE_DEBUG_PORT=9222 to inspect the page over CDP
    // (http://127.0.0.1:9222/json). wry overrides WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS,
    // so the flag has to be passed here (keeping wry's default features flag).
    #[cfg(debug_assertions)]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        match std::env::var("PCTUBE_DEBUG_PORT") {
            Ok(port) => builder.with_additional_browser_args(format!(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
            )),
            Err(_) => builder,
        }
    };

    let webview = builder
        .with_html(splash_html())
        .with_background_color(BG)
        .with_initialization_script(script)
        .with_ipc_handler(move |req| {
            let p = ipc_proxy.clone();
            ipc::handle(req.body(), &ipc_cfg, move |out| {
                let _ = p.send_event(out);
            });
        })
        .with_devtools(cfg!(debug_assertions))
        .build(&window);

    let webview = match webview {
        Ok(w) => w,
        Err(e) => {
            error_box(&format!(
                "No se pudo iniciar WebView2.\n\nPC Tube necesita el runtime de Microsoft Edge WebView2 \
                 (viene con Windows 11 y con Microsoft Edge). Instálalo desde:\n\
                 https://developer.microsoft.com/microsoft-edge/webview2/\n\nDetalle: {e}"
            ));
            return;
        }
    };

    // Tray icon (None if the shell refuses it; then the close button always quits).
    let tray = match tray::Tray::new(proxy.clone()) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("No se pudo crear el icono de la bandeja: {e}");
            None
        }
    };

    // Without a tray icon a hidden window could never be brought back: show it.
    if minimized_start && tray.is_none() {
        show_window(&window);
    }

    // Keep the Run entry pointing at this exe if "Iniciar con Windows" is on (the exe may have moved).
    if cfg.lock().unwrap()["autostart"].as_bool().unwrap_or(false) {
        autostart::set(true);
    }
    download::auto_update_in_background();

    // Previous / play-pause / next on the taskbar thumbnail.
    taskbar::init(window.hwnd(), proxy.clone());

    // Tell the page whether the window is maximized (to swap the button icon).
    let push_state = |window: &Window, webview: &wry::WebView| {
        let js = format!(
            "window.pctube&&window.pctube._winState&&window.pctube._winState({},{})",
            window.is_maximized(),
            window.fullscreen().is_some()
        );
        let _ = webview.evaluate_script(&js);
    };

    // Page zoom from Ajustes -> Apariencia (the mini-player always uses 100 %).
    let mut zoom = (cfg.lock().unwrap()["uiZoom"].as_f64().unwrap_or(100.0) / 100.0).clamp(0.5, 2.0);
    let _ = webview.zoom(zoom);

    // Global shortcuts (Ctrl+Alt+Space / Right / Left).
    let mut _hotkeys = {
        let c = cfg.lock().unwrap();
        if c["globalHotkeys"].as_bool().unwrap_or(true) {
            hotkeys::Hotkeys::register(proxy.clone(), &c["globalShortcuts"])
        } else {
            None
        }
    };
    download::set_limit(cfg.lock().unwrap()["downloadParallel"].as_u64().unwrap_or(2) as usize);
    download::restore_queue(); // what was left unfinished last time (nothing starts by itself)

    // Geometry to go back to when the mini-player is closed: (position, size, maximized).
    let mut mini: Option<(tao::dpi::PhysicalPosition<i32>, tao::dpi::PhysicalSize<u32>, bool)> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Close button / Alt+F4: hide in the tray or quit, depending on the setting.
        let hide_instead_of_quit = tray.is_some()
            && cfg.lock().unwrap().get("closeAction").and_then(|v| v.as_str()) != Some("exit");

        match event {
            Event::UserEvent(ipc::Out::Eval(js)) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(ipc::Out::NowPlaying { title, paused }) => {
                if let Some(t) = &tray {
                    t.update(&title, paused);
                }
                // Nothing playing counts as paused: the button then offers "play".
                taskbar::set_paused(paused || title.is_empty());
            }
            Event::UserEvent(ipc::Out::Notify { title, channel, video_id }) => {
                // "hidden" (default): only when the window is out of sight; "always"; "off".
                let mode = cfg.lock().unwrap()["notifyNowPlaying"].as_str().unwrap_or("hidden").to_string();
                let in_view = window.is_visible() && !window.is_minimized() && window.is_focused();
                if !title.is_empty() && mode != "off" && (mode == "always" || !in_view) {
                    notify::now_playing(title, channel, video_id);
                }
            }
            Event::UserEvent(ipc::Out::Tray(action)) => match action {
                ipc::TrayAction::PlayPause => {
                    let _ = webview.evaluate_script(JS_PLAY_PAUSE);
                }
                ipc::TrayAction::Next => {
                    let _ = webview.evaluate_script(JS_NEXT);
                }
                ipc::TrayAction::Prev => {
                    let _ = webview.evaluate_script(JS_PREV);
                }
                ipc::TrayAction::Show => show_window(&window),
                ipc::TrayAction::Quit => {
                    if mini.is_none() { winstate::save(&window); }
                    download::shutdown(); // stop yt-dlp, keep the queue for next time
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::UserEvent(ipc::Out::Window(cmd)) => {
                use ipc::WinCmd;
                match cmd {
                    WinCmd::Minimize => window.set_minimized(true),
                    WinCmd::ToggleMaximize => window.set_maximized(!window.is_maximized()),
                    WinCmd::Close => {
                        if mini.is_none() { winstate::save(&window); }
                        if hide_instead_of_quit {
                            window.set_visible(false);
                        } else {
                            download::shutdown();
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    WinCmd::AlwaysOnTop(on) => {
                        // while the mini-player is open the window stays on top regardless
                        window.set_always_on_top(on || mini.is_some());
                    }
                    WinCmd::Mini(true) if mini.is_none() && window.fullscreen().is_none() => {
                        if let Ok(pos) = window.outer_position() {
                            mini = Some((pos, window.inner_size(), window.is_maximized()));
                            if window.is_maximized() {
                                window.set_maximized(false);
                            }
                            let scale = window.scale_factor();
                            let (w, h) = ((480.0 * scale) as u32, (270.0 * scale) as u32);
                            window.set_inner_size(tao::dpi::PhysicalSize::new(w, h));
                            // bottom-right corner of the screen the window is on, above the taskbar
                            if let Some(m) = window.current_monitor() {
                                let (mp, ms) = (m.position(), m.size());
                                let margin = (24.0 * scale) as i32;
                                let bar = (48.0 * scale) as i32;
                                window.set_outer_position(tao::dpi::PhysicalPosition::new(
                                    mp.x + ms.width as i32 - w as i32 - margin,
                                    mp.y + ms.height as i32 - h as i32 - margin - bar,
                                ));
                            }
                            window.set_always_on_top(true);
                            let _ = webview.zoom(1.0);
                        }
                    }
                    WinCmd::Mini(false) => {
                        if let Some((pos, size, was_max)) = mini.take() {
                            window.set_inner_size(size);
                            window.set_outer_position(pos);
                            if was_max {
                                window.set_maximized(true);
                            }
                            let on = cfg.lock().unwrap()["alwaysOnTop"].as_bool().unwrap_or(false);
                            window.set_always_on_top(on);
                            let _ = webview.zoom(zoom);
                        }
                    }
                    WinCmd::Mini(true) => {}
                    WinCmd::Zoom(z) => {
                        zoom = z;
                        if mini.is_none() {
                            let _ = webview.zoom(z);
                        }
                    }
                    #[allow(unused_assignments)]
                    WinCmd::Hotkeys(on) => {
                        _hotkeys = None; // unregister first (the same keys can't be taken twice)
                        if on {
                            _hotkeys = hotkeys::Hotkeys::register(proxy.clone(), &cfg.lock().unwrap()["globalShortcuts"]);
                        }
                    }
                    WinCmd::Drag => {
                        let _ = window.drag_window();
                    }
                    WinCmd::Resize(dir) => {
                        let _ = window.drag_resize_window(dir);
                    }
                    WinCmd::Fullscreen(on) => {
                        window.set_fullscreen(on.then(|| Fullscreen::Borderless(None)));
                    }
                    WinCmd::State => {}
                }
                push_state(&window, &webview);
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => push_state(&window, &webview),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if mini.is_none() { winstate::save(&window); }
                if hide_instead_of_quit {
                    window.set_visible(false);
                } else {
                    download::shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}
