//! PC Tube Descargas: the download manager. A small window (list of downloads, completed files,
//! settings, tools) plus a tray icon. PC Tube asks it to download and it keeps working when PC Tube
//! is closed; it closes itself when there is nothing left to do and its window is hidden.
//! The work is done by `pc_tube::engine`; `pc_tube::dmserver` is how PC Tube talks to it.

#![windows_subsystem = "windows"]

use pc_tube::{config, dmserver, engine};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Icon, Window, WindowBuilder},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW},
};
use wry::WebViewBuilder;

const TITLE: &str = "PC Tube Descargas";
const ICON_RGBA: &[u8] = include_bytes!("../../../assets/icon-64.rgba");
const PAGE: &str = include_str!("../../../assets/dm/index.html");

/// Is the window in view? (The idle timer only closes the manager when it is not.)
static VISIBLE: AtomicBool = AtomicBool::new(false);

enum Ev {
    /// Script to run in the window.
    Eval(String),
    Show,
    Quit,
    /// Tray tooltip and the text of its "pause all" item.
    Tip { text: String, paused_all: bool },
    OpenFolder,
    TogglePauseAll,
}

type Proxy = Arc<Mutex<EventLoopProxy<Ev>>>;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True if this is the first manager. A second one just shows the running window (unless it was
/// started in the background, i.e. by PC Tube) and exits.
fn acquire(background: bool) -> bool {
    let name = wide(if config::debug_session() {
        "Local\\PcTubeDownloads.SingleInstance.Debug"
    } else {
        "Local\\PcTubeDownloads.SingleInstance"
    });
    // SAFETY: plain Win32 calls with valid, NUL-terminated UTF-16 strings. The mutex handle is
    // deliberately never closed: it must live as long as the process.
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            return true; // can't tell; don't block the manager from starting
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if !background {
                let title = wide(TITLE);
                let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
                if !hwnd.is_null() {
                    ShowWindow(hwnd, SW_SHOW);
                    ShowWindow(hwnd, SW_RESTORE);
                    SetForegroundWindow(hwnd);
                }
            }
            return false;
        }
    }
    true
}

fn error_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let (text, title) = (wide(text), wide(TITLE));
    // SAFETY: both strings are NUL-terminated UTF-16 and outlive the call.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn show_window(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
}

fn send(proxy: &Proxy, ev: Ev) {
    let _ = proxy.lock().unwrap().send_event(ev);
}

/// "2 en curso · 3 en cola" for the tray tooltip.
fn summary() -> (String, bool) {
    let q = engine::queue_snapshot();
    let running = q["active"].as_array().map_or(0, Vec::len);
    let waiting = q["items"].as_array().map_or(0, Vec::len);
    let paused_all = q["pausedAll"].as_bool().unwrap_or(false);
    let text = if running + waiting == 0 {
        TITLE.to_string()
    } else {
        format!("{TITLE} — {running} en curso · {waiting} en cola")
    };
    (text, paused_all)
}

fn main() {
    let background = std::env::args().any(|a| a == "--background");
    if !acquire(background) {
        return;
    }

    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy: Proxy = Arc::new(Mutex::new(event_loop.create_proxy()));

    // Everything that is left unfinished from last time comes back paused; nothing starts alone.
    engine::restore_queue();
    engine::auto_update_in_background();

    // The local server: PC Tube connects here, and so does this window (through its own IPC).
    let hooks = {
        let (p_events, p_ui) = (proxy.clone(), proxy.clone());
        dmserver::Hooks {
            on_event: Box::new(move |ev: &Value| send(&p_events, Ev::Eval(format!("window.dm&&window.dm._event({ev})")))),
            ui_cmd: Box::new(move |cmd, _args, reply| match cmd {
                "show" => {
                    send(&p_ui, Ev::Show);
                    reply(Ok(Value::Null));
                    None
                }
                "quit" => {
                    reply(Ok(Value::Null));
                    send(&p_ui, Ev::Quit);
                    None
                }
                "pickFolder" => {
                    std::thread::spawn(move || {
                        let picked = rfd::FileDialog::new().set_title("Carpeta de descargas").pick_folder();
                        let Some(path) = picked else { return reply(Ok(Value::Null)) };
                        let r = engine::set_settings(&json!({ "dir": path.to_string_lossy() }));
                        if let Ok(s) = &r {
                            dmserver::broadcast(&json!({ "kind": "settings", "settings": s }));
                        }
                        reply(r.map(|s| serde_json::to_value(s).unwrap_or(Value::Null)));
                    });
                    None
                }
                _ => Some(reply),
            }),
        }
    };
    if let Err(e) = dmserver::start(hooks) {
        error_box(&format!("No se pudo iniciar el gestor de descargas.\n\n{e}"));
        return;
    }

    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_window_icon(Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok())
        .with_inner_size(LogicalSize::new(980.0, 700.0))
        .with_min_inner_size(LogicalSize::new(760.0, 520.0))
        .with_visible(!background)
        .build(&event_loop)
        .expect("no se pudo crear la ventana");
    VISIBLE.store(!background, Ordering::SeqCst);

    let profile = if config::debug_session() { "webview-dm-debug" } else { "webview-dm" };
    let mut web_context = wry::WebContext::new(Some(config::data_dir().join(profile)));
    let builder = WebViewBuilder::new_with_web_context(&mut web_context);

    // Debug builds only: PCTUBE_DM_DEBUG_PORT=9444 to inspect the window over CDP.
    #[cfg(debug_assertions)]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        match std::env::var("PCTUBE_DM_DEBUG_PORT") {
            Ok(port) => builder.with_additional_browser_args(format!(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
            )),
            Err(_) => builder,
        }
    };

    let ipc_proxy = proxy.clone();
    let webview = builder
        .with_html(PAGE)
        .with_ipc_handler(move |req| {
            // {id, cmd, ...args}: the same shape PC Tube sends over the socket
            let Ok(msg) = serde_json::from_str::<Value>(req.body()) else { return };
            let Some(cmd) = msg["cmd"].as_str().map(str::to_string) else { return };
            let id = msg["id"].clone();
            let p = ipc_proxy.clone();
            dmserver::command(
                &cmd,
                msg,
                Box::new(move |r| {
                    let out = match r {
                        Ok(data) => json!({ "id": id, "ok": true, "data": data }),
                        Err(e) => json!({ "id": id, "ok": false, "error": e }),
                    };
                    send(&p, Ev::Eval(format!("window.dm&&window.dm._reply({out})")));
                }),
            );
        })
        .with_devtools(cfg!(debug_assertions))
        .build(&window);
    let webview = match webview {
        Ok(w) => w,
        Err(e) => {
            error_box(&format!(
                "No se pudo iniciar WebView2.\n\nEl gestor necesita el runtime de Microsoft Edge WebView2 \
                 (viene con Windows 11 y con Microsoft Edge). Instálalo desde:\n\
                 https://developer.microsoft.com/microsoft-edge/webview2/\n\nDetalle: {e}"
            ));
            dmserver::remove_info();
            return;
        }
    };

    // Tray icon: left click opens the window, right click the menu.
    let pause_item = MenuItem::new("Pausar todo", true, None);
    let tray = (|| -> Result<_, String> {
        let icon = tray_icon::Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).map_err(|e| e.to_string())?;
        let open = MenuItem::new("Abrir el gestor", true, None);
        let folder = MenuItem::new("Abrir la carpeta de descargas", true, None);
        let quit = MenuItem::new("Salir", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &pause_item, &folder, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;
        let (id_open, id_pause, id_folder, id_quit) =
            (open.id().clone(), pause_item.id().clone(), folder.id().clone(), quit.id().clone());
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let ev = if e.id == id_open {
                Ev::Show
            } else if e.id == id_pause {
                Ev::TogglePauseAll
            } else if e.id == id_folder {
                Ev::OpenFolder
            } else if e.id == id_quit {
                Ev::Quit
            } else {
                return;
            };
            send(&p, ev);
        }));
        let p = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                send(&p, Ev::Show);
            }
        }));
        TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(TITLE)
            .build()
            .map_err(|e| e.to_string())
    })()
    .ok();
    // Without a tray icon a hidden window could never be brought back: show it.
    if background && tray.is_none() {
        show_window(&window);
        VISIBLE.store(true, Ordering::SeqCst);
    }

    // Timers: start what the schedule lets through, refresh the tray, close when there is nothing to do.
    {
        let p = proxy.clone();
        std::thread::spawn(move || {
            let emit = dmserver::emitter();
            let mut last_tip = String::new();
            let mut ticks = 0u32;
            let keep_open = std::env::var_os("PCTUBE_DM_KEEP").is_some(); // tests
            loop {
                std::thread::sleep(Duration::from_secs(5));
                ticks += 1;
                if ticks % 6 == 0 {
                    engine::tick(&emit);
                }
                let (text, paused_all) = summary();
                let key = format!("{text}|{paused_all}");
                if key != last_tip {
                    last_tip = key;
                    send(&p, Ev::Tip { text, paused_all });
                }
                let idle = !engine::busy() && !VISIBLE.load(Ordering::SeqCst) && dmserver::idle_for() > Duration::from_secs(120);
                if idle && engine::settings().auto_exit && !keep_open {
                    send(&p, Ev::Quit);
                    return;
                }
            }
        });
    }

    let quit = |flow: &mut ControlFlow| {
        engine::shutdown(); // stop yt-dlp, keep the queue for next time
        dmserver::remove_info();
        *flow = ControlFlow::Exit;
    };
    let mut low_memory = false;

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Ev::Eval(js)) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(Ev::Show) => {
                show_window(&window);
                VISIBLE.store(true, Ordering::SeqCst);
            }
            Event::UserEvent(Ev::Quit) => quit(flow),
            Event::UserEvent(Ev::OpenFolder) => engine::open_folder(),
            Event::UserEvent(Ev::TogglePauseAll) => {
                let on = !engine::queue_snapshot()["pausedAll"].as_bool().unwrap_or(false);
                engine::pause_all(on, &dmserver::emitter());
            }
            Event::UserEvent(Ev::Tip { text, paused_all }) => {
                if let Some(t) = &tray {
                    let _ = t.set_tooltip(Some(text));
                }
                pause_item.set_text(if paused_all { "Reanudar todo" } else { "Pausar todo" });
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if tray.is_some() {
                    window.set_visible(false);
                } else {
                    quit(flow);
                }
            }
            // Out of sight: ask WebView2 to trim its memory; back to normal when shown again.
            Event::MainEventsCleared => {
                let visible = window.is_visible() && !window.is_minimized();
                VISIBLE.store(visible, Ordering::SeqCst);
                if !visible != low_memory {
                    low_memory = !visible;
                    let level = if low_memory { wry::MemoryUsageLevel::Low } else { wry::MemoryUsageLevel::Normal };
                    let _ = wry::WebViewExtWindows::set_memory_usage_level(&webview, level);
                }
            }
            _ => {}
        }
    });
}
