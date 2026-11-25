//! The local server of PC Tube Descargas. PC Tube and the manager's own window send it commands; it
//! runs them on the engine and pushes the engine's events back.
//!
//! Wire format: TCP on 127.0.0.1 (a random port), one JSON object per line.
//!   client -> {"hello": "<token>"}                       first, always
//!   client -> {"id": 7, "cmd": "download", ...args}      then commands
//!   server -> {"reply": 7, "ok": true, "data": ...}      or {"reply": 7, "ok": false, "error": "..."}
//!   server -> {"event": {"kind": "progress", ...}}       whenever something happens
//! The port and a random token are written to dm.json in the user's own data folder; a connection
//! that does not start with that token is dropped, so another program (or a web page) that finds the
//! port can do nothing with it.

use crate::{
    config,
    engine::{self, Emit, Opts},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

pub type Reply = Box<dyn FnOnce(Result<Value, String>) + Send>;

/// What the program hosting the server (the manager) plugs in.
pub struct Hooks {
    /// Every engine event, for the manager's own window.
    pub on_event: Box<dyn Fn(&Value) + Send + Sync>,
    /// Commands that only the window can carry out (show it, quit, pick a folder). Returns the
    /// reply back when the command is not one of those.
    pub ui_cmd: Box<dyn Fn(&str, &Value, Reply) -> Option<Reply> + Send + Sync>,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

struct Peer {
    stream: Mutex<TcpStream>,
}
impl Peer {
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut s = self.stream.lock().unwrap();
        s.write_all(line.as_bytes())?;
        s.write_all(b"\n")
    }
}
static CLIENTS: Mutex<Vec<Arc<Peer>>> = Mutex::new(Vec::new());
static LAST_COMMAND: Mutex<Option<Instant>> = Mutex::new(None);

/// How long since the last command arrived (the manager stays open while it is being used).
pub fn idle_for() -> Duration {
    LAST_COMMAND.lock().unwrap().map_or(Duration::from_secs(3600), |t| t.elapsed())
}

/// Is any program (PC Tube) connected right now?
pub fn client_count() -> usize {
    CLIENTS.lock().unwrap().len()
}

// ---------- connection info ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

pub fn info_path() -> PathBuf {
    config::data_dir().join(if config::debug_session() { "dm-debug.json" } else { "dm.json" })
}

pub fn read_info() -> Option<Info> {
    serde_json::from_str(&std::fs::read_to_string(info_path()).ok()?).ok()
}

/// Deletes the file when it is ours (a newer manager may already have replaced it).
pub fn remove_info() {
    if read_info().is_some_and(|i| i.pid == std::process::id()) {
        let _ = std::fs::remove_file(info_path());
    }
}

fn random_token() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // never expected; fall back to something unpredictable enough for a local socket
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        b = t.to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------- events ----------

/// Sends an engine event to the window and to every connected client.
pub fn broadcast(ev: &Value) {
    if let Some(h) = HOOKS.get() {
        (h.on_event)(ev);
    }
    let line = json!({ "event": ev }).to_string();
    CLIENTS.lock().unwrap().retain(|p| p.write_line(&line).is_ok());
}

/// The callback the engine reports to.
pub fn emitter() -> Emit {
    Arc::new(|ev: Value| broadcast(&ev))
}

// ---------- commands ----------

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// A download request from PC Tube, in the field names its page uses.
fn opts_from(a: &Value) -> Opts {
    Opts {
        mode: text(a, "mode"),
        max_height: a.get("maxHeight").and_then(Value::as_u64).unwrap_or(0).min(10_000) as u32,
        container: text(a, "container"),
        audio_format: text(a, "audioFormat"),
        audio_quality: text(a, "audioQuality"),
        duration: a.get("duration").and_then(Value::as_f64).unwrap_or(0.0),
        embed_meta: a.get("embedMeta").and_then(Value::as_bool).unwrap_or(false),
        remove_categories: text(a, "removeCategories"),
        playlist_id: text(a, "playlistId"),
    }
}

fn qid_of(a: &Value) -> Option<u64> {
    a.get("qid").and_then(Value::as_u64)
}

fn in_thread(reply: Reply, work: impl FnOnce() -> Result<Value, String> + Send + 'static) {
    std::thread::spawn(move || reply(work()));
}

/// Runs one command. `reply` is called exactly once (maybe from another thread).
pub fn command(cmd: &str, args: Value, reply: Reply) {
    *LAST_COMMAND.lock().unwrap() = Some(Instant::now());
    let reply = match HOOKS.get() {
        Some(h) => match (h.ui_cmd)(cmd, &args, reply) {
            None => return,
            Some(r) => r,
        },
        None => reply,
    };
    let emit = emitter();
    let ok = |reply: Reply| reply(Ok(Value::Null));
    match cmd {
        "ping" => reply(Ok(json!({ "version": env!("CARGO_PKG_VERSION") }))),
        "download" => {
            engine::enqueue(text(&args, "videoId"), opts_from(&args), text(&args, "label"), qid_of(&args).unwrap_or(0), emit);
            ok(reply);
        }
        "cancel" => {
            engine::cancel(qid_of(&args)); // no id: all of them
            ok(reply);
        }
        "cancelQueued" => {
            engine::remove_queued(qid_of(&args).unwrap_or(0), &emit);
            ok(reply);
        }
        "clearQueue" => {
            engine::clear_queue(&emit);
            ok(reply);
        }
        "resume" => {
            engine::resume(emit);
            ok(reply);
        }
        "pause" => {
            engine::pause(qid_of(&args).unwrap_or(0), &emit);
            ok(reply);
        }
        "resumeItem" => {
            engine::resume_item(qid_of(&args).unwrap_or(0), &emit);
            ok(reply);
        }
        "startNow" => {
            engine::start_now(qid_of(&args).unwrap_or(0), &emit);
            ok(reply);
        }
        "pauseAll" => {
            engine::pause_all(args.get("on").and_then(Value::as_bool).unwrap_or(true), &emit);
            ok(reply);
        }
        "queue" => reply(Ok(engine::queue_snapshot())),
        "state" => reply(Ok(engine::state())),
        "history" => reply(Ok(engine::history_list())),
        "historyRemove" => {
            engine::history_remove(args.get("entry").and_then(Value::as_u64).unwrap_or(0));
            broadcast(&json!({ "kind": "history" }));
            ok(reply);
        }
        "historyClear" => {
            engine::history_clear(args.get("failedOnly").and_then(Value::as_bool).unwrap_or(false));
            broadcast(&json!({ "kind": "history" }));
            ok(reply);
        }
        "retry" => {
            engine::history_retry(args.get("entry").and_then(Value::as_u64).unwrap_or(0), &emit);
            broadcast(&json!({ "kind": "history" }));
            ok(reply);
        }
        "fileAction" => {
            let r = engine::file_action(&text(&args, "action"), args.get("entry").and_then(Value::as_u64).unwrap_or(0));
            if text(&args, "action") == "delete" {
                broadcast(&json!({ "kind": "history" }));
            }
            reply(r.map(|_| Value::Null));
        }
        "openFolder" => {
            engine::open_folder();
            ok(reply);
        }
        "getSettings" => reply(serde_json::to_value(engine::settings()).map_err(|e| e.to_string())),
        "setSettings" => match engine::set_settings(&args) {
            Ok(s) => {
                // a higher limit or an open schedule may let waiting downloads start
                engine::pump(&emit);
                let v = serde_json::to_value(&s).unwrap_or(Value::Null);
                broadcast(&json!({ "kind": "settings", "settings": v }));
                reply(Ok(v));
            }
            Err(e) => reply(Err(e)),
        },
        "formats" => {
            // Starts yt-dlp and waits for YouTube: slow, so off the connection's thread.
            let id = text(&args, "videoId");
            in_thread(reply, move || engine::formats(&id, &emit));
        }
        "installYtdlp" => {
            engine::install_ytdlp_task(emit);
            ok(reply);
        }
        "updateYtdlp" => {
            engine::update_ytdlp_task(emit);
            ok(reply);
        }
        "installFfmpeg" => {
            engine::install_ffmpeg_task(emit);
            ok(reply);
        }
        "ytdlpVersion" => in_thread(reply, || Ok(engine::ytdlp_version().map_or(Value::Null, Value::String))),
        "diagnostics" => in_thread(reply, || Ok(engine::diagnostics())),
        _ => reply(Err(format!("orden desconocida: {cmd}"))),
    }
}

// ---------- the server ----------

/// Reads one line of at most `max` bytes (None at the end of the stream). A longer one is an error.
fn read_line_limited(r: &mut BufReader<TcpStream>, max: u64) -> std::io::Result<Option<String>> {
    let mut buf = String::new();
    let n = r.by_ref().take(max).read_line(&mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if !buf.ends_with('\n') && n as u64 >= max {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "línea demasiado larga"));
    }
    Ok(Some(buf))
}

fn serve(stream: TcpStream, token: &str) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);

    // The first line must be the token, and soon.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let hello = read_line_limited(&mut reader, 512).ok().flatten().and_then(|l| serde_json::from_str::<Value>(&l).ok());
    if hello.as_ref().and_then(|h| h["hello"].as_str()) != Some(token) {
        return;
    }
    let _ = stream.set_read_timeout(None);

    let peer = Arc::new(Peer { stream: Mutex::new(stream) });
    if peer.write_line(&json!({ "hello": "ok", "version": env!("CARGO_PKG_VERSION") }).to_string()).is_err() {
        return;
    }
    // The newcomer starts knowing what is going on.
    let _ = peer.write_line(&json!({ "event": { "kind": "queue", "queue": engine::queue_snapshot() } }).to_string());
    CLIENTS.lock().unwrap().push(peer.clone());

    while let Ok(Some(line)) = read_line_limited(&mut reader, 1 << 20) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(cmd) = msg["cmd"].as_str().map(str::to_string) else { continue };
        let id = msg["id"].clone();
        let p = peer.clone();
        command(
            &cmd,
            msg,
            Box::new(move |r| {
                let out = match r {
                    Ok(data) => json!({ "reply": id, "ok": true, "data": data }),
                    Err(e) => json!({ "reply": id, "ok": false, "error": e }),
                };
                let _ = p.write_line(&out.to_string());
            }),
        );
    }
    CLIENTS.lock().unwrap().retain(|p| !Arc::ptr_eq(p, &peer));
}

/// Starts listening (on its own threads). The returned info is also written to dm.json.
pub fn start(hooks: Hooks) -> Result<Info, String> {
    let _ = HOOKS.set(hooks);
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("no se pudo abrir el puerto local: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let info = Info { port, token: random_token(), pid: std::process::id() };
    if !cfg!(test) {
        let tmp = info_path().with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(&info).unwrap_or_default()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, info_path()).map_err(|e| e.to_string())?;
    }
    let token = info.token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let token = token.clone();
            std::thread::spawn(move || serve(stream, &token));
        }
    });
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_requests_are_read_with_their_page_field_names() {
        let o = opts_from(&json!({
            "mode": "audio", "maxHeight": 720, "container": "mkv", "audioFormat": "flac", "audioQuality": "192K",
            "duration": 61.5, "embedMeta": true, "removeCategories": "sponsor", "playlistId": "PLabcdefghijk",
            "unknown": "ignored",
        }));
        assert_eq!((o.mode.as_str(), o.max_height, o.container.as_str(), o.audio_format.as_str()), ("audio", 720, "mkv", "flac"));
        assert!(o.embed_meta && o.duration == 61.5 && o.playlist_id == "PLabcdefghijk");
        // missing or wrongly typed fields fall back to empty values (the engine sanitizes the rest)
        let d = opts_from(&json!({ "maxHeight": "alto", "embedMeta": "sí" }));
        assert_eq!((d.max_height, d.embed_meta, d.mode.as_str()), (0, false, ""));
    }

    #[test]
    fn tokens_are_long_and_different_each_time() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
