//! PC Tube's side of the conversation with PC Tube Descargas (the download manager): finds it
//! (starting it when a download is requested), sends commands and passes the events it pushes on.
//! The wire format is described in `dmserver`.

use crate::dmserver::{self, Info};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// The manager's file next to PC Tube's: the installer's name first, then cargo's.
const DM_EXE_NAMES: &[&str] = &["PC Tube Downloads.exe", "pctube_dm.exe"];

struct Conn {
    writer: Mutex<TcpStream>,
    alive: AtomicBool,
}

type Pending = Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>;

struct Inner {
    conn: Mutex<Option<Arc<Conn>>>,
    pending: Pending,
    next: AtomicU64,
    on_event: Box<dyn Fn(Value) + Send + Sync>,
    /// One connect-or-launch at a time.
    connecting: Mutex<()>,
}

#[derive(Clone)]
pub struct Client(Arc<Inner>);

/// The manager's exe, beside this one.
pub fn dm_exe() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    DM_EXE_NAMES.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// Starts the manager. Without `show` it stays in the background (tray only) until it is needed.
pub fn launch(show: bool) -> Result<(), String> {
    let exe = dm_exe().ok_or("no se encontró el gestor de descargas (PC Tube Downloads.exe) junto a PC Tube")?;
    let mut cmd = Command::new(exe);
    if !show {
        cmd.arg("--background");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no se pudo abrir el gestor de descargas: {e}"))
}

impl Client {
    /// `on_event` receives every event the manager pushes ({"kind": ...}) from a background thread.
    pub fn new(on_event: impl Fn(Value) + Send + Sync + 'static) -> Client {
        Client(Arc::new(Inner {
            conn: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            on_event: Box::new(on_event),
            connecting: Mutex::new(()),
        }))
    }

    fn current(&self) -> Option<Arc<Conn>> {
        self.0.conn.lock().unwrap().clone().filter(|c| c.alive.load(Ordering::SeqCst))
    }

    pub fn is_connected(&self) -> bool {
        self.current().is_some()
    }

    /// Connects to the manager described by `info` and starts reading what it pushes.
    pub fn connect_with(&self, info: &Info) -> Result<(), String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], info.port));
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).map_err(|e| e.to_string())?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
        writeln!(writer, "{}", json!({ "hello": info.token })).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let ok = serde_json::from_str::<Value>(&line).is_ok_and(|v| v["hello"] == "ok");
        if !ok {
            return Err("el gestor rechazó la conexión".into());
        }
        let _ = stream.set_read_timeout(None);

        let conn = Arc::new(Conn { writer: Mutex::new(writer), alive: AtomicBool::new(true) });
        *self.0.conn.lock().unwrap() = Some(conn.clone());
        let inner = self.0.clone();
        std::thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                if !matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = v["reply"].as_u64() {
                    let result = if v["ok"].as_bool().unwrap_or(false) {
                        Ok(v["data"].clone())
                    } else {
                        Err(v["error"].as_str().unwrap_or("error del gestor").to_string())
                    };
                    if let Some(tx) = inner.pending.lock().unwrap().remove(&id) {
                        let _ = tx.send(result);
                    }
                } else if v.get("event").is_some() {
                    (inner.on_event)(v["event"].clone());
                }
            }
            // the manager closed (or crashed): fail what was waiting for an answer
            conn.alive.store(false, Ordering::SeqCst);
            {
                let mut slot = inner.conn.lock().unwrap();
                if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, &conn)) {
                    *slot = None;
                }
            }
            for (_, tx) in inner.pending.lock().unwrap().drain() {
                let _ = tx.send(Err("el gestor de descargas se cerró".into()));
            }
            (inner.on_event)(json!({ "kind": "dm", "state": "closed" }));
        });
        Ok(())
    }

    /// A live connection, opening one if needed. With `launch` the manager is started when it is
    /// not running (it needs a few seconds to be ready).
    fn conn(&self, launch_if_needed: bool) -> Result<Arc<Conn>, String> {
        if let Some(c) = self.current() {
            return Ok(c);
        }
        let _one_at_a_time = self.0.connecting.lock().unwrap();
        if let Some(c) = self.current() {
            return Ok(c);
        }
        let try_once = |me: &Client| -> Result<Arc<Conn>, String> {
            let info = dmserver::read_info().ok_or("el gestor de descargas no está abierto")?;
            me.connect_with(&info)?;
            me.current().ok_or_else(|| "sin conexión".to_string())
        };
        match try_once(self) {
            Ok(c) => return Ok(c),
            Err(e) if !launch_if_needed => return Err(e),
            Err(_) => {}
        }
        launch(false)?;
        let t = Instant::now();
        let mut last = String::from("el gestor no respondió");
        while t.elapsed() < Duration::from_secs(12) {
            std::thread::sleep(Duration::from_millis(250));
            match try_once(self) {
                Ok(c) => return Ok(c),
                Err(e) => last = e,
            }
        }
        Err(format!("el gestor de descargas no arrancó ({last})"))
    }

    /// Sends a command and waits for its answer. With `launch` a closed manager is started first.
    pub fn call(&self, cmd: &str, args: Value, launch: bool, timeout: Duration) -> Result<Value, String> {
        let conn = self.conn(launch)?;
        let id = self.0.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.0.pending.lock().unwrap().insert(id, tx);
        let mut msg = if args.is_object() { args } else { json!({}) };
        msg["id"] = json!(id);
        msg["cmd"] = json!(cmd);
        let sent = writeln!(conn.writer.lock().unwrap(), "{msg}");
        if let Err(e) = sent {
            self.0.pending.lock().unwrap().remove(&id);
            conn.alive.store(false, Ordering::SeqCst);
            return Err(format!("no se pudo hablar con el gestor: {e}"));
        }
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(_) => {
                self.0.pending.lock().unwrap().remove(&id);
                Err("el gestor tardó demasiado en responder".into())
            }
        }
    }

    /// Shows the manager's window (starting the manager if it is not running).
    pub fn open_window(&self) -> Result<(), String> {
        match self.call("show", json!({}), false, Duration::from_secs(3)) {
            Ok(_) => Ok(()),
            Err(_) => launch(true),
        }
    }

    /// Asks the manager to close (what is running stays in its saved queue) and waits a moment for
    /// it to go. Used before an update replaces its files.
    pub fn quit_and_wait(&self) {
        if self.call("quit", json!({}), false, Duration::from_secs(3)).is_err() && dmserver::read_info().is_none() {
            return;
        }
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) && dmserver::read_info().is_some() {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn client_and_server_talk_over_loopback_and_the_token_is_required() {
        let info = dmserver::start(dmserver::Hooks {
            on_event: Box::new(|_| {}),
            ui_cmd: Box::new(|cmd, _, reply| {
                if cmd == "show" {
                    reply(Ok(json!("shown")));
                    None
                } else {
                    Some(reply)
                }
            }),
        })
        .unwrap();

        // a wrong token is dropped without an answer
        let mut bad = TcpStream::connect(("127.0.0.1", info.port)).unwrap();
        writeln!(bad, "{}", json!({ "hello": "nope" })).unwrap();
        let _ = bad.set_read_timeout(Some(Duration::from_secs(3)));
        let mut line = String::new();
        let n = BufReader::new(bad).read_line(&mut line).unwrap_or(0);
        assert_eq!(n, 0, "no answer for a wrong token");
        // neither does something that is not a hello at all
        let mut rude = TcpStream::connect(("127.0.0.1", info.port)).unwrap();
        write!(rude, "GET / HTTP/1.1\r\n\r\n").unwrap();
        let _ = rude.set_read_timeout(Some(Duration::from_secs(3)));
        assert_eq!(BufReader::new(rude).read_line(&mut String::new()).unwrap_or(0), 0);

        let seen = Arc::new(AtomicUsize::new(0));
        let s2 = seen.clone();
        let client = Client::new(move |ev| {
            if ev["kind"] == "ping-test" {
                s2.fetch_add(1, Ordering::SeqCst);
            }
        });
        client.connect_with(&info).unwrap();
        assert!(client.is_connected());

        let t = Duration::from_secs(5);
        let pong = client.call("ping", json!({}), false, t).unwrap();
        assert_eq!(pong["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(client.call("show", json!({}), false, t).unwrap(), json!("shown"), "window commands go to the hook");
        let err = client.call("format-c", json!({}), false, t).unwrap_err();
        assert!(err.contains("desconocida"), "{err}");
        let s = client.call("getSettings", json!({}), false, t).unwrap();
        assert!(s["parallel"].is_u64() && s["nameTemplate"].is_string());
        assert!(client.call("setSettings", json!({ "from": "99:99" }), false, t).is_err());
        let q = client.call("queue", json!({}), false, t).unwrap();
        assert!(q["items"].is_array() && q["schedule"].is_object());

        // events pushed by the server reach the client
        dmserver::broadcast(&json!({ "kind": "ping-test" }));
        let started = Instant::now();
        while seen.load(Ordering::SeqCst) == 0 && started.elapsed() < t {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }
}
