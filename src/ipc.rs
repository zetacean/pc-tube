use crate::{config, net};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

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
    action: Option<String>,
    name: Option<String>,
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
}

pub type Config = Arc<Mutex<Value>>;

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
fn shared_emit<F: Fn(Out) + Send + 'static>(emit: &Arc<Mutex<F>>) -> crate::download::Emit {
    let emit = emit.clone();
    Arc::new(move |o| (emit.lock().unwrap())(o))
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
        "download" => crate::download::start(
            m.video_id.unwrap_or_default(),
            crate::download::Opts {
                mode: m.mode.unwrap_or_default(),
                max_height: m.max_height.unwrap_or(0),
                container: m.container.unwrap_or_default(),
                audio_format: m.audio_format.unwrap_or_default(),
                audio_quality: m.audio_quality.unwrap_or_default(),
                duration: m.duration.unwrap_or(0.0),
                embed_meta: m.embed_meta.unwrap_or(false),
                remove_categories: m.remove_categories.unwrap_or_default(),
                playlist_id: m.playlist_id.unwrap_or_default(),
            },
            shared_emit(&emit),
        ),
        "downloadFormats" => {
            // Starts yt-dlp and waits for YouTube: slow, so off the UI thread.
            let shared = shared_emit(&emit);
            let video_id = m.video_id.unwrap_or_default();
            std::thread::spawn(move || match crate::download::formats(&video_id, &shared) {
                Ok(v) => respond(m.id, true, v),
                Err(e) => respond(m.id, false, Value::String(e)),
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
        "openRelease" => crate::update::open_release_page(),
        "showWindow" => (emit.lock().unwrap())(Out::Tray(TrayAction::Show)),
        "cancelDownload" => crate::download::cancel(),
        "installYtdlp" => crate::download::install_ytdlp_task(shared_emit(&emit)),
        "updateYtdlp" => crate::download::update_ytdlp_task(shared_emit(&emit)),
        "installFfmpeg" => crate::download::install_ffmpeg_task(shared_emit(&emit)),
        "downloadsInfo" => respond(m.id, true, crate::download::info()),
        "ytdlpVersion" => {
            // Starts yt-dlp (slow), so keep it off the UI thread.
            std::thread::spawn(move || {
                let v = crate::download::ytdlp_version();
                respond(m.id, true, v.map_or(Value::Null, Value::String));
            });
        }
        "fileAction" => {
            let res = crate::download::file_action(
                m.action.as_deref().unwrap_or(""),
                m.name.as_deref().unwrap_or(""),
            );
            match res {
                Ok(()) => respond(m.id, true, Value::Null),
                Err(e) => respond(m.id, false, Value::String(e)),
            }
        }
        "openDownloads" => crate::download::open_folder(),
        "window" => {
            if let Some(cmd) = m.cmd.as_deref().and_then(|c| parse_win_cmd(c, &m.value)) {
                (emit.lock().unwrap())(Out::Window(cmd));
            }
        }
        _ => {}
    }
}
