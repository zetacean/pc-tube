use crate::{config, dm, net};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Deserialize)]
struct Msg {
    id: Option<u64>,
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
    key: Option<String>,
    value: Option<Value>,
    cmd: Option<String>,
    #[serde(rename = "videoId")]
    video_id: Option<String>,
    mode: Option<String>,
    #[serde(rename = "maxHeight")]
    max_height: Option<u32>,
    container: Option<String>,
    #[serde(rename = "audioFormat")]
    audio_format: Option<String>,
    #[serde(rename = "audioQuality")]
    audio_quality: Option<String>,
    duration: Option<f64>,
    title: Option<String>,
    paused: Option<bool>,
    #[serde(rename = "embedMeta")]
    embed_meta: Option<bool>,
    #[serde(rename = "removeCategories")]
    remove_categories: Option<String>,
    #[serde(rename = "playlistId")]
    playlist_id: Option<String>,
    qid: Option<u64>,
    label: Option<String>,
    channel: Option<String>,
    /// Update: download and verify only, do not run the installer (test sessions).
    dry: Option<bool>,
}

pub type Config = Arc<Mutex<Value>>;
/// Where background jobs report back to the main thread.
pub type Emit = Arc<dyn Fn(Out) + Send + Sync>;

/// Window operations requested by the page (the window is frameless, so the page
/// draws its own title bar buttons).
#[derive(Debug, Clone)]
pub enum WinCmd {
    Minimize,
    ToggleMaximize,
    Close,
    Drag,
    Resize(tao::window::ResizeDirection),
    Fullscreen(bool),
    AlwaysOnTop(bool),
    /// Mini-player: small always-on-top window in the corner (and back).
    Mini(bool),
    /// Briefly lower WebView2's memory level to make it trim caches (then back to normal).
    TrimMemory,
    /// Internal: end of a TrimMemory.
    TrimDone,
    /// Page zoom (1.0 = 100 %).
    Zoom(f64),
    /// Global shortcuts on/off.
    Hotkeys(bool),
    /// Just re-send the maximized/fullscreen state to the page.
    State,
}

/// Actions from the tray menu (they run on the main thread).
#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    PlayPause,
    Next,
    Prev,
    Show,
    Quit,
}

/// Things the IPC layer (and the tray) ask the main thread to do.
pub enum Out {
    /// Script to run in the page.
    Eval(String),
    Window(WinCmd),
    Tray(TrayAction),
    /// What the page is playing (title is empty when nothing is).
    NowPlaying { title: String, paused: bool },
    /// A new video started: maybe show a Windows notification (the main loop knows if the window is in view).
    Notify { title: String, channel: String, video_id: String },
}

fn parse_win_cmd(cmd: &str, value: &Option<Value>) -> Option<WinCmd> {
    use tao::window::ResizeDirection as D;
    Some(match cmd {
        "minimize" => WinCmd::Minimize,
        "toggleMaximize" => WinCmd::ToggleMaximize,
        "close" => WinCmd::Close,
        "drag" => WinCmd::Drag,
        "state" => WinCmd::State,
        "fullscreen" => WinCmd::Fullscreen(value.as_ref().and_then(Value::as_bool).unwrap_or(false)),
        "alwaysOnTop" => WinCmd::AlwaysOnTop(value.as_ref().and_then(Value::as_bool).unwrap_or(false)),
        "hotkeys" => WinCmd::Hotkeys(value.as_ref().and_then(Value::as_bool).unwrap_or(false)),
        "zoom" => WinCmd::Zoom(value.as_ref().and_then(Value::as_f64).unwrap_or(1.0).clamp(0.5, 2.0)),
        "trimMemory" => WinCmd::TrimMemory,
        "mini" => WinCmd::Mini(value.as_ref().and_then(Value::as_bool).unwrap_or(false)),
        "resize" =>WinCmd::Resize(match value.as_ref().and_then(Value::as_str)? {
            "n" => D::North,
            "s" => D::South,
            "e" => D::East,
            "w" => D::West,
            "ne" => D::NorthEast,
            "nw" => D::NorthWest,
            "se" => D::SouthEast,
            "sw" => D::SouthWest,
            _ => return None,
        }),
        _ => return None,
    })
}

/// A cloneable, thread-safe handle on `emit` for background jobs.
fn shared_emit<F: Fn(Out) + Send + 'static>(emit: &Arc<Mutex<F>>) -> Emit {
    let emit = emit.clone();
    Arc::new(move |o| (emit.lock().unwrap())(o))
}

/// Fire-and-forget command for the manager (when it is not running there is nothing to act on).
fn manager_cmd(cmd: &'static str, args: Value) {
    std::thread::spawn(move || {
        let _ = dm::try_call(cmd, args);
    });
}

/// Handles one message from the page. `emit` may be called from another thread.
pub fn handle(body: &str, cfg: &Config, emit: impl Fn(Out) + Send + 'static) {
    let Ok(m) = serde_json::from_str::<Msg>(body) else {
        return;
    };
    let emit = Arc::new(Mutex::new(emit));
    let respond = {
        let emit = emit.clone();
        move |id: Option<u64>, ok: bool, data: Value| {
            if let Some(id) = id {
                let payload = json!({ "id": id, "ok": ok, "data": data });
                (emit.lock().unwrap())(Out::Eval(format!(
                    "window.pctube&&window.pctube._resolve({payload})"
                )));
            }
        }
    };
    match m.kind.as_str() {
        "getConfig" => respond(m.id, true, cfg.lock().unwrap().clone()),
        "setConfig" => {
            if let Some(key) = m.key {
                let mut c = cfg.lock().unwrap();
                c[key] = m.value.unwrap_or(Value::Null);
                config::save(&c);
            }
            respond(m.id, true, Value::Null);
        }
        "fetch" => {
            let url = m.url.unwrap_or_default();
            std::thread::spawn(move || match net::fetch(&url) {
                Ok(body) => respond(m.id, true, Value::String(body)),
                Err(e) => respond(m.id, false, Value::String(e)),
            });
        }
        "download" => {
            let args = json!({
                "videoId": m.video_id, "qid": m.qid, "label": m.label, "mode": m.mode, "maxHeight": m.max_height,
                "container": m.container, "audioFormat": m.audio_format, "audioQuality": m.audio_quality,
                "duration": m.duration, "embedMeta": m.embed_meta, "removeCategories": m.remove_categories,
                "playlistId": m.playlist_id,
            });
            let shared = shared_emit(&emit);
            // Starting the manager can take a few seconds: off the UI thread.
            std::thread::spawn(move || {
                if let Err(e) = dm::call("download", args, Duration::from_secs(30)) {
                    shared(Out::Eval(format!(
                        "window.pctube&&window.pctube._download&&window.pctube._download({})",
                        json!({ "kind": "status", "msg": format!("No se pudo descargar: {e}") })
                    )));
                }
            });
        }
        "cancelQueued" => manager_cmd("cancelQueued", json!({ "qid": m.qid })),
        "pauseDownload" => manager_cmd("pause", json!({ "qid": m.qid })),
        "resumeDownload" => manager_cmd("resumeItem", json!({ "qid": m.qid })),
        "pauseAllDownloads" => manager_cmd("pauseAll", json!({ "on": m.value.as_ref().and_then(Value::as_bool).unwrap_or(true) })),
        "clearQueue" => manager_cmd("clearQueue", json!({})),
        "resumeQueue" => manager_cmd("resume", json!({})),
        "queueInfo" => {
            std::thread::spawn(move || {
                let q = dm::try_call("queue", json!({}))
                    .unwrap_or_else(|_| json!({ "items": [], "running": false, "pending": pc_tube::engine::pending_count() }));
                respond(m.id, true, q);
            });
        }
        "hotkeyStatus" => respond(m.id, true, json!(crate::hotkeys::failed())),
        "downloadFormats" => {
            // Starts yt-dlp and waits for YouTube: slow, so off the UI thread.
            let video_id = m.video_id.unwrap_or_default();
            std::thread::spawn(move || {
                match dm::call("formats", json!({ "videoId": video_id }), Duration::from_secs(90)) {
                    Ok(v) => respond(m.id, true, v),
                    Err(e) => respond(m.id, false, Value::String(e)),
                }
            });
        }
        "nowPlaying" => (emit.lock().unwrap())(Out::NowPlaying {
            title: m.title.unwrap_or_default(),
            paused: m.paused.unwrap_or(true),
        }),
        "setAutostart" => crate::autostart::set(m.value.as_ref().and_then(Value::as_bool).unwrap_or(false)),
        "exportConfig" => {
            let cfg = cfg.clone();
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new()
                    .set_title("Exportar ajustes de PC Tube")
                    .set_file_name("pctube-ajustes.json")
                    .add_filter("Ajustes (JSON)", &["json"])
                    .save_file();
                let Some(path) = picked else { return respond(m.id, true, Value::Null) };
                let text = serde_json::to_string_pretty(&*cfg.lock().unwrap()).unwrap_or_default();
                match std::fs::write(&path, text) {
                    Ok(()) => respond(m.id, true, Value::String(path.display().to_string())),
                    Err(e) => respond(m.id, false, Value::String(e.to_string())),
                }
            });
        }
        "importConfig" => {
            let cfg = cfg.clone();
            let shared = shared_emit(&emit);
            std::thread::spawn(move || {
                let picked = rfd::FileDialog::new()
                    .set_title("Importar ajustes de PC Tube")
                    .add_filter("Ajustes (JSON)", &["json"])
                    .pick_file();
                let Some(path) = picked else { return respond(m.id, true, Value::Null) };
                let imported = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| format!("no es un JSON válido: {e}")));
                match imported {
                    Err(e) => respond(m.id, false, Value::String(e)),
                    Ok(v) => {
                        let (n, now) = {
                            let mut c = cfg.lock().unwrap();
                            let n = config::merge_import(&mut c, &v);
                            config::save(&c);
                            (n, c.clone())
                        };
                        // Settings with an effect outside the page.
                        crate::autostart::set(now["autostart"].as_bool().unwrap_or(false));
                        shared(Out::Window(WinCmd::AlwaysOnTop(now["alwaysOnTop"].as_bool().unwrap_or(false))));
                        shared(Out::Window(WinCmd::Zoom(now["uiZoom"].as_f64().unwrap_or(100.0) / 100.0)));
                        respond(m.id, true, json!({ "applied": n, "config": now }));
                    }
                }
            });
        }
        // From the page, e.g. "back to tab" in the PiP window: show the main window.
        "checkUpdate" => {
            let force = m.value.as_ref().and_then(Value::as_bool).unwrap_or(false);
            std::thread::spawn(move || match crate::update::check(force) {
                Ok(v) => respond(m.id, true, v),
                Err(e) => respond(m.id, false, Value::String(e)),
            });
        }
        "applyUpdate" => crate::update::apply(
            shared_emit(&emit),
            m.dry.unwrap_or(false) && config::debug_session(), // only test sessions may skip installing
        ),
        "openRelease" => crate::update::open_release_page(),
        "notifyNowPlaying" => (emit.lock().unwrap())(Out::Notify {
            title: m.title.unwrap_or_default(),
            channel: m.channel.unwrap_or_default(),
            video_id: m.video_id.unwrap_or_default(),
        }),
        "showWindow" => (emit.lock().unwrap())(Out::Tray(TrayAction::Show)),
        "cancelDownload" => manager_cmd("cancel", json!({ "qid": m.qid })), // no id: all of them
        // The download manager's window (its own program, see src/bin/pctube_dm).
        "openManager" | "openDownloads" => {
            let folder = m.kind == "openDownloads";
            std::thread::spawn(move || {
                let r = if folder {
                    dm::try_call("openFolder", json!({})).map(|_| ()).or_else(|_| {
                        pc_tube::engine::open_folder();
                        Ok::<(), String>(())
                    })
                } else {
                    dm::open_window()
                };
                respond(m.id, r.is_ok(), r.err().map_or(Value::Null, Value::String));
            });
        }
        "window" => {
            if let Some(cmd) = m.cmd.as_deref().and_then(|c| parse_win_cmd(c, &m.value)) {
                (emit.lock().unwrap())(Out::Window(cmd));
            }
        }
        _ => {}
    }
}
