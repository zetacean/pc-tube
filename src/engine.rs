//! The download engine behind PC Tube Descargas (the download manager). Video downloads go
//! through yt-dlp (the standard tool for this; YouTube's signature and token handling changes too
//! often to reimplement), plus the tooling around it:
//! - yt-dlp is fetched from its official GitHub release and verified against SHA2-256SUMS.
//! - ffmpeg (optional, needed for best quality and MP3) comes from yt-dlp's own FFmpeg-Builds
//!   release, verified against its checksums.sha256 and unpacked into %LOCALAPPDATA%\PcTube\bin.
//! - Files land in Downloads\PC Tube (or the folder chosen in the manager's settings).
//! - A queue with pause / resume, a speed limit, an optional time window and a history.
//!
//! The engine knows nothing about windows: everything it reports goes through an `Emit` callback as
//! a JSON event (`{"kind": "progress", "qid": 3, ...}`); the manager shows them and forwards them to
//! PC Tube (see `dmserver`).

use crate::config;
use crate::http::{download_to, expected_hash, get_text, http_agent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::{BufRead, BufReader},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const FFMPEG_BASE: &str = "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download";
const FFMPEG_ZIP: &str = "ffmpeg-master-latest-win64-gpl-shared.zip";
/// Only these file types are opened or deleted from the manager.
const MEDIA_EXT: &[&str] = &[
    "mp4", "mkv", "webm", "mp3", "m4a", "opus", "ogg", "wav", "flac", "mov", "aac",
];

static TOOL_BUSY: AtomicBool = AtomicBool::new(false);
/// The app is quitting: interrupted downloads stay in the saved queue to resume next time.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
/// "Pause all": nothing new starts until it is lifted.
static PAUSED_ALL: AtomicBool = AtomicBool::new(false);
static NEXT_QID: AtomicU64 = AtomicU64::new(1);
/// Slow side jobs in progress (format lookups, tool installs): the manager does not close itself meanwhile.
static SIDE_JOBS: AtomicUsize = AtomicUsize::new(0);
pub const MAX_PARALLEL: usize = 4;
/// Two downloads that both need yt-dlp must not both install it.
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// A download in progress: its queue item plus what is needed to stop it.
struct Active {
    item: QueueItem,
    pid: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    /// Stopped on purpose to be paused (it goes back to the queue, paused, keeping its partial files).
    pausing: Arc<AtomicBool>,
}
struct State {
    queue: VecDeque<QueueItem>,
    active: Vec<Active>,
}
static STATE: Mutex<State> = Mutex::new(State { queue: VecDeque::new(), active: Vec::new() });

pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

/// Counts a slow side job while it lives.
struct SideJob;
impl SideJob {
    fn new() -> Self {
        SIDE_JOBS.fetch_add(1, Ordering::SeqCst);
        SideJob
    }
}
impl Drop for SideJob {
    fn drop(&mut self) {
        SIDE_JOBS.fetch_sub(1, Ordering::SeqCst);
    }
}

// ---------- paths ----------

pub fn bin_dir() -> PathBuf {
    let d = config::data_dir().join("bin");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(config::data_dir).join("PC Tube")
}

/// Where files are saved: the folder chosen in the settings, or Downloads\PC Tube.
pub fn download_dir() -> PathBuf {
    let s = settings();
    let d = if s.dir.is_empty() { default_download_dir() } else { PathBuf::from(&s.dir) };
    if std::fs::create_dir_all(&d).is_ok() {
        return d;
    }
    let d = default_download_dir();
    let _ = std::fs::create_dir_all(&d);
    d
}

fn in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

fn find_ytdlp() -> Option<PathBuf> {
    let local = bin_dir().join("yt-dlp.exe");
    if local.is_file() {
        return Some(local);
    }
    in_path("yt-dlp.exe")
}

/// (available, dir for --ffmpeg-location). The dir is None when ffmpeg is on PATH.
fn find_ffmpeg() -> (bool, Option<PathBuf>) {
    if bin_dir().join("ffmpeg.exe").is_file() {
        return (true, Some(bin_dir()));
    }
    (in_path("ffmpeg.exe").is_some(), None)
}

// ---------- clock and log ----------

/// Local time of day as (hour, minute, second).
fn local_time() -> (u32, u32, u32) {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut t = SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
    // SAFETY: GetLocalTime only writes into the struct we pass.
    unsafe { GetLocalTime(&mut t) };
    (t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// A line for the diagnostics log (the last 200 are kept in memory).
pub fn log(msg: &str) {
    let (h, m, s) = local_time();
    let mut l = LOG.lock().unwrap();
    l.push_back(format!("{h:02}:{m:02}:{s:02} {}", msg.chars().filter(|c| !c.is_control()).take(400).collect::<String>()));
    while l.len() > 200 {
        l.pop_front();
    }
}

// ---------- settings ----------

pub const NAME_DEFAULT: &str = "{title} [{id}]";
pub const LIST_DEFAULT: &str = "{playlist}/{index} - {title} [{id}]";

/// What the manager's settings page edits. Saved to dm-settings.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub parallel: u32,             // downloads at the same time, 1..=4
    pub speed_kbps: u32,           // total speed limit in KB/s (0 = none), shared by the parallel downloads
    pub dir: String,               // "" = Downloads\PC Tube
    pub name_template: String,     // file name of a single video, e.g. "{channel}/{title} [{id}]"
    pub playlist_template: String, // file name inside a playlist
    pub subs: String,              // off | file | embed
    pub subs_langs: String,        // "es,en"
    pub subs_auto: bool,           // also take automatic captions
    pub thumbnail: bool,           // keep the cover as a separate .jpg
    pub schedule: bool,            // only start downloads between `from` and `to`
    pub from: String,              // "HH:MM"
    pub to: String,
    pub auto_exit: bool,           // close the manager when idle and its window is hidden
    pub auto_update: bool,         // keep yt-dlp up to date (weekly)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            parallel: 2,
            speed_kbps: 0,
            dir: String::new(),
            name_template: NAME_DEFAULT.into(),
            playlist_template: LIST_DEFAULT.into(),
            subs: "off".into(),
            subs_langs: "es,en".into(),
            subs_auto: true,
            thumbnail: false,
            schedule: false,
            from: "01:00".into(),
            to: "07:00".into(),
            auto_exit: true,
            auto_update: true,
        }
    }
}

/// "07:30" -> minutes since midnight.
fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Language codes as yt-dlp knows them: "es", "en,pt-BR", at most 8 of them.
fn valid_langs(s: &str) -> bool {
    let parts: Vec<&str> = s.split(',').collect();
    !parts.is_empty()
        && parts.len() <= 8
        && parts.iter().all(|p| {
            (2..=12).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

impl Settings {
    /// Brings every field into range (a hand-edited file can't break anything).
    fn sanitized(mut self) -> Self {
        let d = Settings::default();
        self.parallel = self.parallel.clamp(1, MAX_PARALLEL as u32);
        self.speed_kbps = self.speed_kbps.min(1_000_000);
        if !self.dir.is_empty() && (!Path::new(&self.dir).is_absolute() || self.dir.contains('\0')) {
            self.dir.clear();
        }
        if render_template(&self.name_template).is_err() {
            self.name_template = d.name_template;
        }
        if render_template(&self.playlist_template).is_err() {
            self.playlist_template = d.playlist_template;
        }
        if !["off", "file", "embed"].contains(&self.subs.as_str()) {
            self.subs = d.subs;
        }
        if !valid_langs(&self.subs_langs) {
            self.subs_langs = d.subs_langs;
        }
        if parse_hhmm(&self.from).is_none() {
            self.from = d.from;
        }
        if parse_hhmm(&self.to).is_none() {
            self.to = d.to;
        }
        self
    }
}

fn settings_path() -> PathBuf {
    config::data_dir().join(if config::debug_session() { "dm-settings-debug.json" } else { "dm-settings.json" })
}

static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();

fn load_settings() -> Settings {
    if cfg!(test) {
        return Settings::default();
    }
    if let Ok(text) = std::fs::read_to_string(settings_path()) {
        if let Ok(s) = serde_json::from_str::<Settings>(&text) {
            return s.sanitized();
        }
    }
    // First run: keep the "simultaneous downloads" choice made in PC Tube's own settings.
    let mut s = Settings::default();
    if let Some(n) = config::load().get("downloadParallel").and_then(Value::as_u64) {
        s.parallel = n.min(MAX_PARALLEL as u64) as u32;
    }
    s.sanitized()
}

fn cell() -> &'static Mutex<Settings> {
    SETTINGS.get_or_init(|| Mutex::new(load_settings()))
}

pub fn settings() -> Settings {
    cell().lock().unwrap().clone()
}

/// Applies the fields present in `patch` (unknown keys are ignored). Invalid names, times or
/// folders are refused with a message the manager can show.
pub fn set_settings(patch: &Value) -> Result<Settings, String> {
    let patch = patch.as_object().ok_or("ajustes no válidos")?;
    let mut cur = serde_json::to_value(settings()).map_err(|e| e.to_string())?;
    for (k, v) in patch {
        if cur.get(k).is_some() {
            cur[k] = v.clone();
        }
    }
    let next: Settings = serde_json::from_value(cur).map_err(|_| "valor no válido".to_string())?;
    render_template(&next.name_template).map_err(|e| format!("Nombre del archivo: {e}"))?;
    render_template(&next.playlist_template).map_err(|e| format!("Nombre en las listas: {e}"))?;
    if parse_hhmm(&next.from).is_none() || parse_hhmm(&next.to).is_none() {
        return Err("la hora debe ser HH:MM".into());
    }
    if !valid_langs(&next.subs_langs) {
        return Err("idiomas de subtítulos no válidos (ej. es,en)".into());
    }
    if !next.dir.is_empty() {
        let p = Path::new(&next.dir);
        if !p.is_absolute() || next.dir.contains('\0') {
            return Err("la carpeta debe ser una ruta completa".into());
        }
        if !cfg!(test) {
            std::fs::create_dir_all(p).map_err(|e| format!("no se puede usar esa carpeta: {e}"))?;
        }
    }
    let next = next.sanitized();
    *cell().lock().unwrap() = next.clone();
    if !cfg!(test) {
        if let Ok(s) = serde_json::to_string_pretty(&next) {
            let _ = std::fs::write(settings_path(), s);
        }
    }
    Ok(next)
}

/// Changes the number of simultaneous downloads in memory only (tests). Call `pump` afterwards to
/// use a higher limit right away.
pub fn set_limit(n: usize) {
    cell().lock().unwrap().parallel = n.clamp(1, MAX_PARALLEL) as u32;
}

// ---------- file name templates ----------

/// {token} -> yt-dlp output field.
const TEMPLATE_TOKENS: &[(&str, &str)] = &[
    ("title", "%(title).150B"),
    ("id", "%(id)s"),
    ("channel", "%(channel,uploader|Sin canal).80B"),
    ("date", "%(upload_date)s"),
    ("playlist", "%(playlist_title|Sin lista).80B"),
    ("index", "%(playlist_index)03d"),
];
/// Besides letters and digits, these may appear in a template.
const TEMPLATE_LITERALS: &str = " -_.()[],&'+#@!=";

/// Turns what the user wrote ("{channel}/{title} [{id}]") into yt-dlp's output template, checking
/// that nothing but known fields, plain characters and sub-folders gets through. The extension is
/// added at the end.
pub fn render_template(t: &str) -> Result<String, String> {
    let t = t.trim();
    if t.is_empty() {
        return Err("está vacío".into());
    }
    if t.chars().count() > 120 {
        return Err("es demasiado largo".into());
    }
    let segments: Vec<&str> = t.split('/').map(str::trim).collect();
    if segments.len() > 5 {
        return Err("demasiadas carpetas".into());
    }
    let mut out: Vec<String> = Vec::new();
    for seg in segments {
        if seg.is_empty() || seg.contains("..") || seg.ends_with('.') {
            return Err("las carpetas no pueden estar vacías ni terminar en punto".into());
        }
        let mut s = String::new();
        let mut chars = seg.chars();
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if !closed {
                    return Err("falta un «}»".into());
                }
                let (_, field) = TEMPLATE_TOKENS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .ok_or_else(|| format!("no se reconoce «{{{name}}}»"))?;
                s.push_str(field);
            } else if c.is_alphanumeric() || TEMPLATE_LITERALS.contains(c) {
                s.push(c);
            } else {
                return Err(format!("el carácter «{c}» no se puede usar"));
            }
        }
        out.push(s);
    }
    Ok(format!("{}.%(ext)s", out.join("/")))
}

// ---------- schedule ----------

/// Is `now` (minutes since midnight) inside the window `from`..`to`? A window that ends before it
/// starts runs overnight (22:00-06:00).
pub fn in_window(now: u32, from: u32, to: u32) -> bool {
    if from == to {
        true
    } else if from < to {
        now >= from && now < to
    } else {
        now >= from || now < to
    }
}

/// May downloads start right now?
pub fn schedule_open() -> bool {
    let s = settings();
    if !s.schedule {
        return true;
    }
    let (h, m, _) = local_time();
    in_window(h * 60 + m, parse_hhmm(&s.from).unwrap_or(0), parse_hhmm(&s.to).unwrap_or(0))
}

// ---------- events ----------

/// Everything one running download needs: where to report (tagged with its queue id) and how to
/// stop it. Cloned into the helper threads of that download.
#[derive(Clone)]
struct Ctx {
    emit: Emit,
    qid: u64,
    video_id: String,
    label: String,
    pid: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
}
impl Ctx {
    fn send(&self, mut ev: Value) {
        ev["qid"] = json!(self.qid);
        (self.emit)(ev);
    }
    fn status(&self, msg: &str) {
        self.send(json!({ "kind": "status", "msg": msg }));
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub(crate) fn send(emit: &Emit, ev: Value) {
    emit(ev);
}
fn status(emit: &Emit, msg: &str) {
    send(emit, json!({ "kind": "status", "msg": msg }));
}
fn tool_ev(emit: &Emit, tool: &str, state: &str, msg: &str, pct: Option<u32>) {
    send(emit, json!({ "kind": "tool", "tool": tool, "state": state, "msg": msg, "pct": pct }));
}

// ---------- tools ----------

fn install_ytdlp(emit: &Emit) -> Result<PathBuf, String> {
    let agent = http_agent();
    let sums = get_text(&agent, &format!("{YTDLP_BASE}/SHA2-256SUMS"))?;
    let expected = expected_hash(&sums, "yt-dlp.exe").ok_or("no se encontró el checksum de yt-dlp")?;
    let part = bin_dir().join("yt-dlp.exe.part");
    let got = download_to(&agent, &format!("{YTDLP_BASE}/yt-dlp.exe"), &part, 100 << 20, |pct| {
        tool_ev(emit, "ytdlp", "working", "Descargando yt-dlp…", Some(pct));
    })?;
    if got != expected {
        let _ = std::fs::remove_file(&part);
        return Err("el checksum de yt-dlp no coincide; instalación cancelada".into());
    }
    let dest = bin_dir().join("yt-dlp.exe");
    let _ = std::fs::remove_file(&dest);
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn install_ffmpeg(emit: &Emit) -> Result<(), String> {
    let agent = http_agent();
    let sums = get_text(&agent, &format!("{FFMPEG_BASE}/checksums.sha256"))?;
    let expected = expected_hash(&sums, FFMPEG_ZIP).ok_or("no se encontró el checksum de ffmpeg")?;
    let part = bin_dir().join("ffmpeg.zip.part");
    let got = download_to(&agent, &format!("{FFMPEG_BASE}/{FFMPEG_ZIP}"), &part, 600 << 20, |pct| {
        tool_ev(emit, "ffmpeg", "working", "Descargando ffmpeg…", Some(pct));
    })?;
    if got != expected {
        let _ = std::fs::remove_file(&part);
        return Err("el checksum de ffmpeg no coincide; instalación cancelada".into());
    }

    tool_ev(emit, "ffmpeg", "working", "Extrayendo…", None);
    let result = (|| -> Result<(), String> {
        let mut zip = zip::ZipArchive::new(File::open(&part).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut count = 0;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            // enclosed_name() rejects "../" paths (zip-slip). Keep only <root>/bin/<file>.
            let Some(path) = entry.enclosed_name() else { continue };
            let in_bin = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "bin");
            let Some(name) = path.file_name().map(|n| n.to_owned()) else { continue };
            if entry.is_dir() || !in_bin || name == "ffplay.exe" {
                continue;
            }
            let mut out = File::create(bin_dir().join(&name)).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            count += 1;
        }
        if !bin_dir().join("ffmpeg.exe").is_file() || count == 0 {
            return Err("el archivo descargado no contiene ffmpeg.exe".into());
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&part);
    result
}

/// Runs `job` as a tool task (one at a time) and reports start/finish to the page.
fn tool_task(emit: Emit, tool: &'static str, job: impl FnOnce(&Emit) -> Result<String, String> + Send + 'static) {
    if TOOL_BUSY.swap(true, Ordering::SeqCst) {
        status(&emit, "Ya hay una instalación en curso");
        return;
    }
    std::thread::spawn(move || {
        let _side = SideJob::new();
        tool_ev(&emit, tool, "working", "Preparando…", None);
        match job(&emit) {
            Ok(msg) => {
                log(&format!("{tool}: {msg}"));
                tool_ev(&emit, tool, "done", &msg, None)
            }
            Err(e) => {
                log(&format!("{tool}: error: {e}"));
                tool_ev(&emit, tool, "error", &e, None)
            }
        }
        TOOL_BUSY.store(false, Ordering::SeqCst);
    });
}

pub fn install_ytdlp_task(emit: Emit) {
    tool_task(emit, "ytdlp", |e| install_ytdlp(e).map(|_| "yt-dlp instalado".into()));
}

pub fn update_ytdlp_task(emit: Emit) {
    tool_task(emit, "ytdlp", |e| {
        let Some(exe) = find_ytdlp() else {
            return install_ytdlp(e).map(|_| "yt-dlp instalado".into());
        };
        tool_ev(e, "ytdlp", "working", "Buscando actualizaciones…", None);
        let out = Command::new(&exe)
            .arg("-U")
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        if out.status.success() {
            Ok(if last.is_empty() { "yt-dlp actualizado".into() } else { last.trim().to_string() })
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no se pudo actualizar").to_string())
        }
    });
}

pub fn install_ffmpeg_task(emit: Emit) {
    tool_task(emit, "ffmpeg", |e| install_ffmpeg(e).map(|_| "ffmpeg instalado".into()));
}

// ---------- history (finished and failed downloads) ----------

fn is_media(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXT.contains(&e.to_lowercase().as_str()))
}

/// One line of the "Completadas" list: a file that was saved, or a download that failed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HistEntry {
    pub id: u64,
    pub ts: u64,         // when (ms since 1970)
    pub ok: bool,
    pub title: String,
    pub video_id: String,
    pub file: String,    // full path (ok entries)
    pub size: u64,
    pub msg: String,     // the error (failed entries)
    pub opts: Opts,      // to retry a failed one
}

fn history_path() -> PathBuf {
    config::data_dir().join(if config::debug_session() { "history-debug.json" } else { "history.json" })
}

static HISTORY: OnceLock<Mutex<Vec<HistEntry>>> = OnceLock::new();

fn history_cell() -> &'static Mutex<Vec<HistEntry>> {
    HISTORY.get_or_init(|| {
        let loaded = if cfg!(test) {
            Vec::new()
        } else {
            std::fs::read_to_string(history_path())
                .ok()
                .and_then(|t| serde_json::from_str::<Vec<HistEntry>>(&t).ok())
                .unwrap_or_default()
        };
        Mutex::new(loaded)
    })
}

fn persist_history(list: &[HistEntry]) {
    if cfg!(test) {
        return;
    }
    if let Ok(s) = serde_json::to_string(list) {
        let _ = std::fs::write(history_path(), s);
    }
}

fn history_add(mut e: HistEntry) {
    let mut h = history_cell().lock().unwrap();
    e.ts = now_ms();
    e.id = h.iter().map(|x| x.id).max().unwrap_or(0).max(e.ts) + 1;
    h.insert(0, e);
    h.truncate(300);
    persist_history(&h);
}

/// The list for the "Completadas" tab; `exists` says whether a saved file is still on disk.
pub fn history_list() -> Value {
    let h = history_cell().lock().unwrap();
    Value::Array(
        h.iter()
            .map(|e| {
                let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
                v["exists"] = json!(e.ok && Path::new(&e.file).is_file());
                v
            })
            .collect(),
    )
}

pub fn history_remove(id: u64) {
    let mut h = history_cell().lock().unwrap();
    h.retain(|e| e.id != id);
    persist_history(&h);
}

/// Empties the list (only the failed ones when `only_failed`). Files stay on disk.
pub fn history_clear(only_failed: bool) {
    let mut h = history_cell().lock().unwrap();
    h.retain(|e| only_failed && e.ok);
    persist_history(&h);
}

/// action: "open" | "show" | "delete". Only files this manager saved can be reached (the page
/// sends an entry id, never a path), and only media types are opened.
pub fn file_action(action: &str, id: u64) -> Result<(), String> {
    let path = {
        let h = history_cell().lock().unwrap();
        let e = h.iter().find(|e| e.id == id && e.ok).ok_or("ese archivo ya no está en la lista")?;
        PathBuf::from(&e.file)
    };
    if !is_media(&path.to_string_lossy()) {
        return Err("archivo no válido".into());
    }
    if !path.is_file() {
        return Err("el archivo ya no existe".into());
    }
    match action {
        "open" => {
            Command::new("explorer.exe").arg(&path).spawn().map_err(|e| e.to_string())?;
        }
        "show" => {
            Command::new("explorer.exe")
                .raw_arg(format!("/select,\"{}\"", path.display()))
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "delete" => {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            history_remove(id);
        }
        _ => return Err("acción no válida".into()),
    }
    Ok(())
}

pub fn open_folder() {
    let _ = Command::new("explorer.exe").arg(download_dir()).spawn();
}

/// Puts a failed download back in the queue and takes it off the list.
pub fn history_retry(id: u64, emit: &Emit) {
    let entry = history_cell().lock().unwrap().iter().find(|e| e.id == id && !e.ok).cloned();
    if let Some(e) = entry {
        history_remove(id);
        enqueue(e.video_id, e.opts, e.title, 0, emit.clone());
    }
}

// ---------- tools and state ----------

/// A tool's version. Slow (the exe unpacks itself on start): call from a thread.
fn tool_version(exe: &Path, arg: &str) -> Option<String> {
    let out = Command::new(exe)
        .arg(arg)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn ytdlp_version() -> Option<String> {
    tool_version(&find_ytdlp()?, "--version")
}

/// Everything the manager's window needs to draw itself (fast: no processes are started).
pub fn state() -> Value {
    let yt = find_ytdlp();
    let (ff_ok, ff_dir) = find_ffmpeg();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "settings": settings(),
        "dir": download_dir().to_string_lossy(),
        "defaultDir": default_download_dir().to_string_lossy(),
        "ytdlp": {
            "installed": yt.is_some(),
            "source": yt.as_ref().map(|p| if p.starts_with(bin_dir()) { "app" } else { "system" }),
        },
        "ffmpeg": {
            "installed": ff_ok,
            "source": if !ff_ok { Value::Null } else if ff_dir.is_some() { json!("app") } else { json!("system") },
        },
        "queue": queue_snapshot(),
        "busy": active_count() > 0,
    })
}

/// For the "Herramientas" tab and its copyable report. Starts yt-dlp and ffmpeg to ask their versions.
pub fn diagnostics() -> Value {
    let yt = find_ytdlp();
    let ff = if bin_dir().join("ffmpeg.exe").is_file() { Some(bin_dir().join("ffmpeg.exe")) } else { in_path("ffmpeg.exe") };
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "dataDir": config::data_dir().to_string_lossy(),
        "binDir": bin_dir().to_string_lossy(),
        "downloadDir": download_dir().to_string_lossy(),
        "ytdlp": yt.as_ref().map(|p| json!({ "path": p.to_string_lossy(), "version": tool_version(p, "--version") })),
        "ffmpeg": ff.as_ref().map(|p| json!({ "path": p.to_string_lossy(), "version": tool_version(p, "-version") })),
        "log": LOG.lock().unwrap().iter().cloned().collect::<Vec<_>>(),
    })
}

/// At startup: if the app's own yt-dlp has not been updated for a week, update it quietly in
/// the background (YouTube breaks old versions often). A system-wide yt-dlp is left alone.
pub fn auto_update_in_background() {
    // Test sessions (and anyone who sets PCTUBE_NO_AUTOUPDATE) never touch the network here.
    if config::debug_session() || std::env::var_os("PCTUBE_NO_AUTOUPDATE").is_some() || !settings().auto_update {
        return;
    }
    std::thread::spawn(|| {
        let _side = SideJob::new();
        let exe = bin_dir().join("yt-dlp.exe");
        let stamp = bin_dir().join("yt-dlp.updated");
        if !exe.is_file() {
            return;
        }
        let week = Duration::from_secs(7 * 24 * 3600);
        let fresh = std::fs::metadata(&stamp)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age < week);
        if fresh || active_count() > 0 {
            return;
        }
        let ok = Command::new(&exe)
            .arg("-U")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .is_ok_and(|s| s.success());
        log(if ok { "yt-dlp actualizado (revisión semanal)" } else { "no se pudo actualizar yt-dlp (revisión semanal)" });
        if ok {
            let _ = std::fs::write(&stamp, b"");
        }
    });
}

// ---------- downloads ----------

fn valid_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What the user picked in the download dialog. Every string is checked against a whitelist in
/// `Opts::sanitized` before it can reach yt-dlp's command line.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Opts {
    pub mode: String,          // "video" | "audio"
    pub max_height: u32,       // 0 = best
    pub container: String,     // mp4 | mkv | webm
    pub audio_format: String,  // mp3 | m4a | opus | flac | wav
    pub audio_quality: String, // 320K | 256K | 192K | 128K (mp3 only)
    pub duration: f64,         // seconds, to estimate conversion progress
    pub embed_meta: bool,      // write title/artist/thumbnail into the file (needs ffmpeg)
    pub remove_categories: String, // SponsorBlock categories to cut out, comma separated (needs ffmpeg)
    pub playlist_id: String,   // non-empty: download the whole playlist instead of one video
}

/// SponsorBlock categories that make sense to cut from a file (highlights are only a point).
const CUTTABLE: &[&str] = &[
    "sponsor", "intro", "outro", "selfpromo", "preview", "filler", "interaction", "music_offtopic",
];

impl Opts {
    fn sanitized(mut self) -> Self {
        // Only whitelisted names reach the command line, each at most once.
        let mut cats: Vec<&str> = Vec::new();
        for c in self.remove_categories.split(',').map(str::trim) {
            if CUTTABLE.contains(&c) && !cats.contains(&c) {
                cats.push(c);
            }
        }
        self.remove_categories = cats.join(",");
        let pick = |v: &str, ok: &[&str], default: &str| -> String {
            if ok.contains(&v) { v.to_string() } else { default.to_string() }
        };
        self.mode = pick(&self.mode, &["video", "audio"], "video");
        self.container = pick(&self.container, &["mp4", "mkv", "webm"], "mp4");
        self.audio_format = pick(&self.audio_format, &["mp3", "m4a", "opus", "flac", "wav"], "mp3");
        self.audio_quality = pick(&self.audio_quality, &["320K", "256K", "192K", "128K"], "320K");
        self.max_height = self.max_height.min(4320);
        // Playlist ids are letters, digits, '-' and '_'. Automatic mixes ("RD…") are endless.
        let pl = &self.playlist_id;
        if !(10..=64).contains(&pl.len())
            || pl.starts_with("RD")
            || !pl.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            self.playlist_id.clear();
        }
        if !self.duration.is_finite() || self.duration < 0.0 {
            self.duration = 0.0;
        }
        self
    }

    /// Constant bitrate (kbps) of the audio conversion, when the size can be predicted.
    fn cbr_kbps(&self) -> Option<u32> {
        (self.mode == "audio" && self.audio_format == "mp3")
            .then(|| self.audio_quality.trim_end_matches('K').parse().ok())
            .flatten()
    }
}

fn build_args(video_id: &str, o: &Opts, ffmpeg: (bool, Option<PathBuf>), s: &Settings) -> Vec<String> {
    let playlist = !o.playlist_id.is_empty();
    // File names come from the manager's settings (by default one folder per playlist, files
    // numbered in playlist order). They were checked when saved; the default is the safety net.
    let template = render_template(if playlist { &s.playlist_template } else { &s.name_template })
        .or_else(|_| render_template(if playlist { LIST_DEFAULT } else { NAME_DEFAULT }))
        .unwrap_or_default();
    let template = template.as_str();
    let mut a: Vec<String> = [
        "--newline",
        "--progress",
        "--no-quiet",
        if playlist { "--yes-playlist" } else { "--no-playlist" },
        "--no-warnings",
        "--windows-filenames",
        "--no-mtime",
        "--progress-template",
        // percent, bytes/s, seconds left, bytes done, bytes total, estimated total ("None" if unknown)
        "PCTUBE_PROGRESS %(progress._percent_str)s %(progress.speed)s %(progress.eta)s %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s",
        "--print",
        "before_dl:PCTUBE_TITLE %(title)s",
        "--print",
        "after_move:PCTUBE_DONE %(filepath)s",
        "--print",
        "before_dl:PCTUBE_ITEM %(playlist_index)s %(n_entries)s",
        "-o",
        template,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if playlist {
        a.push("--ignore-errors".into()); // skip private/deleted entries instead of stopping
    }
    a.push("-P".into());
    a.push(download_dir().to_string_lossy().into_owned());

    let (has_ffmpeg, ffmpeg_dir) = ffmpeg;
    if let Some(dir) = ffmpeg_dir {
        a.push("--ffmpeg-location".into());
        a.push(dir.to_string_lossy().into_owned());
    }

    // Total speed limit, shared between the downloads that run together.
    if s.speed_kbps > 0 {
        let each = (s.speed_kbps / s.parallel.max(1)).max(50);
        a.extend(["--limit-rate".into(), format!("{each}K")]);
    }
    // Extras: captions (not for audio-only) and the cover as a separate file.
    if o.mode != "audio" && s.subs != "off" {
        a.push("--write-subs".into());
        if s.subs_auto {
            a.push("--write-auto-subs".into());
        }
        a.extend(["--sub-langs".into(), s.subs_langs.clone()]);
        if has_ffmpeg {
            a.extend(["--convert-subs".into(), "srt".into()]);
            if s.subs == "embed" {
                a.push("--embed-subs".into());
            }
        }
    }
    if s.thumbnail {
        a.push("--write-thumbnail".into());
        if has_ffmpeg {
            a.extend(["--convert-thumbnails".into(), "jpg".into()]);
        }
    }

    if has_ffmpeg {
        if o.embed_meta {
            a.push("--embed-metadata".into());
            // Cover art: not every container can carry it (webm, opus, flac, wav can't, or need
            // extra tools), so only where it works. Thumbnails come as webp: convert to jpg.
            let can_embed_cover = match o.mode.as_str() {
                "audio" => matches!(o.audio_format.as_str(), "mp3" | "m4a"),
                _ => matches!(o.container.as_str(), "mp4" | "mkv"),
            };
            if can_embed_cover {
                a.extend(["--embed-thumbnail".into(), "--convert-thumbnails".into(), "jpg".into()]);
            }
        }
        if !o.remove_categories.is_empty() {
            a.extend(["--sponsorblock-remove".into(), o.remove_categories.clone()]);
        }
    }

    let h = if o.max_height > 0 { format!("[height<={}]", o.max_height) } else { String::new() };
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let extra: Vec<String> = match (o.mode.as_str(), has_ffmpeg) {
        ("audio", true) => {
            let mut v = match o.audio_format.as_str() {
                // Prefer a source that can be copied instead of re-encoded.
                "m4a" => s(&["-f", "ba[ext=m4a]/ba"]),
                "opus" => s(&["-f", "ba[ext=webm]/ba"]),
                _ => s(&["-f", "ba"]),
            };
            v.extend(s(&["-x", "--audio-format", &o.audio_format]));
            if o.audio_format == "mp3" {
                v.extend(s(&["--audio-quality", &o.audio_quality]));
            }
            v
        }
        ("audio", false) => s(&["-f", "ba[ext=m4a]/ba"]),
        (_, true) => {
            let sort = match o.container.as_str() {
                "mkv" => "res,fps",
                "webm" => "res,fps,vcodec:vp9,acodec:opus",
                _ => "res,fps,vcodec:h264,acodec:m4a",
            };
            vec![
                "-f".into(),
                format!("bv*{h}+ba/b{h}/b"),
                "--merge-output-format".into(),
                o.container.clone(),
                "-S".into(),
                sort.into(),
            ]
        }
        (_, false) => vec!["-f".into(), format!("b{h}[ext=mp4]/b{h}/b")],
    };
    a.extend(extra);
    a.push("--".into());
    if playlist {
        a.push(format!("https://www.youtube.com/playlist?list={}", o.playlist_id));
    } else {
        a.push(format!("https://www.youtube.com/watch?v={video_id}"));
    }
    a
}

/// Reports progress of a conversion step from the size of the file ffmpeg is writing (yt-dlp
/// does not forward ffmpeg's progress). `expected` unknown => indeterminate bar.
fn spawn_size_watch(label: &'static str, paths: Vec<PathBuf>, expected: Option<u64>, ctx: Ctx) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let phase = |pct: Option<i32>| ctx.send(json!({ "kind": "phase", "label": label, "pct": pct }));
        let Some(expected) = expected.filter(|e| *e > 0) else {
            phase(None);
            return;
        };
        let mut last = -1;
        while !flag.load(Ordering::SeqCst) {
            let size = paths
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .max()
                .unwrap_or(0);
            let pct = (size.saturating_mul(100) / expected).min(99) as i32;
            if pct != last {
                last = pct;
                phase(Some(pct));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    });
    stop
}

/// Total size of the separately downloaded streams ("<stem>.f137.mp4", "<stem>.f140.m4a"):
/// the merged file will end up about this big.
fn streams_size(final_path: &Path) -> u64 {
    let (Some(dir), Some(stem)) = (final_path.parent(), final_path.file_stem().and_then(|s| s.to_str())) else {
        return 0;
    };
    let prefix = format!("{stem}.f");
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

/// A file is finished (a playlist yields several per download): add it to the history.
fn record_file(ctx: &Ctx, o: &Opts, path: &str, title: Option<&str>) {
    let title = title.filter(|t| !t.is_empty()).unwrap_or(&ctx.label).to_string();
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    history_add(HistEntry {
        ok: true,
        title: title.clone(),
        video_id: ctx.video_id.clone(),
        file: path.to_string(),
        size,
        opts: o.clone(),
        ..Default::default()
    });
    ctx.send(json!({ "kind": "file", "file": path, "title": title, "size": size }));
    ctx.send(json!({ "kind": "history" }));
}

/// Runs yt-dlp and streams progress. Ok(Some(path)) on success.
fn run(exe: &Path, args: &[String], o: &Opts, has_ffmpeg: bool, ctx: &Ctx) -> Result<Option<String>, String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))?;
    ctx.pid.store(child.id(), Ordering::SeqCst);
    if ctx.is_cancelled() {
        // stopped (or paused) while yt-dlp was starting: cancel() found no process to kill yet
        let _ = child.kill();
    }

    let errs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_thread = child.stderr.take().map(|s| {
        let errs = errs.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                errs.lock().unwrap().push(line);
            }
        })
    });

    let merging = o.mode != "audio" && has_ffmpeg; // video and audio come as two streams
    let mut streams = 0;
    let mut file = None;
    let mut last_pct = -1i32;
    let mut last_sent = Instant::now();
    let mut title: Option<String> = None;
    let mut watch: Option<Arc<AtomicBool>> = None;
    let stop_watch = |w: &mut Option<Arc<AtomicBool>>| {
        if let Some(f) = w.take() {
            f.store(true, Ordering::SeqCst);
        }
    };

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(p) = line.strip_prefix("PCTUBE_PROGRESS ") {
                // "45.3% 1234567.8 83 5000000 11000000 None": percent, speed, eta, done, total, estimate
                let f: Vec<&str> = p.split_whitespace().collect();
                let num = |i: usize| f.get(i).and_then(|x| x.parse::<f64>().ok());
                if let Some(v) = f.first().and_then(|x| x.trim_end_matches('%').parse::<f32>().ok()) {
                    // when the percentage moves, and now and then so that the speed stays current
                    if v as i32 != last_pct || last_sent.elapsed() > Duration::from_millis(900) {
                        last_pct = v as i32;
                        last_sent = Instant::now();
                        ctx.send(json!({
                            "kind": "progress", "pct": last_pct, "speed": num(1), "eta": num(2),
                            "done": num(3), "total": num(4).or(num(5)),
                        }));
                    }
                }
            } else if let Some(it) = line.strip_prefix("PCTUBE_ITEM ") {
                // "3 20" inside a playlist ("NA NA" for a single video)
                let mut n = it.split_whitespace().map(|x| x.parse::<u32>().ok());
                if let (Some(Some(i)), Some(Some(total))) = (n.next(), n.next()) {
                    stop_watch(&mut watch);
                    streams = 0;
                    last_pct = -1;
                    ctx.send(json!({ "kind": "item", "index": i, "total": total }));
                }
            } else if let Some(t) = line.strip_prefix("PCTUBE_TITLE ") {
                title = Some(t.trim().to_string());
                ctx.send(json!({ "kind": "title", "title": t.trim() }));
            } else if let Some(f) = line.strip_prefix("PCTUBE_DONE ") {
                stop_watch(&mut watch);
                let f = f.trim().to_string();
                record_file(ctx, o, &f, title.as_deref());
                file = Some(f);
            } else if line.starts_with("[download] Destination:") {
                streams += 1;
                stop_watch(&mut watch);
                last_pct = -1;
                let label = match (o.mode.as_str(), merging, streams) {
                    ("audio", _, _) => "Descargando audio",
                    (_, true, 1) => "Descargando vídeo",
                    (_, true, _) => "Descargando audio",
                    _ => "Descargando",
                };
                ctx.send(json!({ "kind": "phase", "label": label, "pct": 0 }));
            } else if let Some(rest) = line.strip_prefix("[Merger] Merging formats into ") {
                stop_watch(&mut watch);
                let final_path = PathBuf::from(rest.trim().trim_matches('"'));
                let ext = final_path.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
                let stem = final_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                // ffmpeg writes "<stem>.temp.<ext>" and renames it when done.
                let temp = final_path.with_file_name(format!("{stem}.temp.{ext}"));
                let expected = streams_size(&final_path);
                watch = Some(spawn_size_watch("Uniendo vídeo y audio", vec![final_path, temp], Some(expected), ctx.clone()));
            } else if let Some(rest) = line.strip_prefix("[ExtractAudio] Destination: ") {
                stop_watch(&mut watch);
                let dest = PathBuf::from(rest.trim());
                let expected = o.cbr_kbps().map(|k| (o.duration * k as f64 * 125.0) as u64);
                watch = Some(spawn_size_watch("Convirtiendo audio", vec![dest], expected, ctx.clone()));
            } else if line.starts_with("[ModifyChapters]") {
                stop_watch(&mut watch);
                watch = Some(spawn_size_watch("Quitando patrocinios", vec![], None, ctx.clone()));
            } else if line.starts_with("[VideoRemuxer]") || line.starts_with("[VideoConvertor]") {
                stop_watch(&mut watch);
                watch = Some(spawn_size_watch("Convirtiendo vídeo", vec![], None, ctx.clone()));
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    ctx.pid.store(0, Ordering::SeqCst);
    stop_watch(&mut watch);
    if let Some(t) = err_thread {
        let _ = t.join();
    }
    // In a playlist some entries may fail (private, removed): that is fine if others came through.
    if status.success() || (!o.playlist_id.is_empty() && file.is_some() && !ctx.is_cancelled()) {
        Ok(file)
    } else {
        let errs = errs.lock().unwrap();
        let msg = errs
            .iter()
            .rev()
            .find(|l| l.contains("ERROR"))
            .or_else(|| errs.last())
            .cloned()
            .unwrap_or_else(|| "yt-dlp terminó con error".into());
        Err(msg.trim_start_matches("ERROR: ").to_string())
    }
}

fn ensure_ytdlp(emit: &Emit) -> Result<PathBuf, String> {
    if let Some(p) = find_ytdlp() {
        return Ok(p);
    }
    // Several downloads may start together: only the first one installs it.
    let _guard = INSTALL_LOCK.lock().unwrap();
    match find_ytdlp() {
        Some(p) => Ok(p),
        None => {
            status(emit, "Instalando yt-dlp (solo la primera vez)…");
            install_ytdlp(emit)
        }
    }
}

fn update_ytdlp_quiet(exe: &Path) {
    let _ = Command::new(exe)
        .arg("-U")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Tests replace the real work (yt-dlp) with a stand-in.
#[cfg(test)]
static JOB_HOOK: Mutex<Option<fn(&Ctx) -> Result<Option<String>, String>>> = Mutex::new(None);

fn job(video_id: &str, o: &Opts, ctx: &Ctx) -> Result<Option<String>, String> {
    #[cfg(test)]
    {
        let hook = *JOB_HOOK.lock().unwrap(); // copy it out: the lock must not be held while it runs
        if let Some(hook) = hook {
            return hook(ctx);
        }
    }
    let exe = ensure_ytdlp(&ctx.emit)?;
    if ctx.is_cancelled() {
        return Err("cancelada".into());
    }
    let ffmpeg = find_ffmpeg();
    let has_ffmpeg = ffmpeg.0;
    if !has_ffmpeg && o.mode != "audio" {
        ctx.status("Sin ffmpeg: calidad limitada (instálalo en la pestaña Herramientas)");
    } else {
        ctx.status("Iniciando descarga…");
    }
    let args = build_args(video_id, o, ffmpeg, &settings());
    match run(&exe, &args, o, has_ffmpeg, ctx) {
        Ok(f) => Ok(f),
        Err(_) if ctx.is_cancelled() => Err("cancelada".into()),
        // Test sessions never update the tool (they only probe the queue with made-up ids).
        Err(first) if config::debug_session() => Err(first),
        // yt-dlp.exe can't be replaced while another download is using it.
        Err(first) if active_count() > 1 => Err(first),
        Err(first) => {
            // YouTube changes break yt-dlp regularly: update it and retry once.
            ctx.status("Actualizando yt-dlp y reintentando…");
            update_ytdlp_quiet(&exe);
            run(&exe, &args, o, has_ffmpeg, ctx).map_err(|second| {
                if ctx.is_cancelled() {
                    "cancelada".into()
                } else if second == first {
                    second
                } else {
                    format!("{second} (antes: {first})")
                }
            })
        }
    }
}


// ---------- queue ----------

/// One download waiting in the queue (or running). Saved to queue.json so that what was left
/// when the manager closed can be resumed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub qid: u64,
    pub video_id: String,
    pub label: String,
    pub opts: Opts,
    /// Held back: it does not start until it is resumed.
    #[serde(default)]
    pub paused: bool,
    /// "Iniciar ahora": starts even outside the schedule.
    #[serde(default)]
    pub force: bool,
}

fn queue_path() -> PathBuf {
    config::data_dir().join(if config::debug_session() { "queue-debug.json" } else { "queue.json" })
}

/// Running downloads first, then the waiting ones.
fn all_items() -> Vec<QueueItem> {
    let st = STATE.lock().unwrap();
    st.active.iter().map(|a| a.item.clone()).chain(st.queue.iter().cloned()).collect()
}

fn persist_queue() {
    if cfg!(test) {
        return; // tests must never touch the real saved queue
    }
    let items = all_items();
    if items.is_empty() {
        let _ = std::fs::remove_file(queue_path());
    } else if let Ok(s) = serde_json::to_string(&items) {
        let _ = std::fs::write(queue_path(), s);
    }
}

/// How many downloads were left unfinished in the saved queue (PC Tube mentions them at start).
pub fn pending_count() -> usize {
    std::fs::read_to_string(queue_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<QueueItem>>(&t).ok())
        .map_or(0, |v| v.len())
}

pub fn active_count() -> usize {
    STATE.lock().unwrap().active.len()
}

/// Something is happening or about to: downloads running or waiting to start, tool work, format
/// lookups. The manager does not close itself while this is true.
pub fn busy() -> bool {
    if SIDE_JOBS.load(Ordering::SeqCst) > 0 || TOOL_BUSY.load(Ordering::SeqCst) {
        return true;
    }
    let st = STATE.lock().unwrap();
    !st.active.is_empty() || (!PAUSED_ALL.load(Ordering::SeqCst) && st.queue.iter().any(|i| !i.paused))
}

/// Queue and schedule as JSON for the pages: items [{qid, videoId, label, opts, paused, force}].
pub fn queue_snapshot() -> Value {
    let s = settings();
    let open = schedule_open();
    let st = STATE.lock().unwrap();
    let waiting: Vec<Value> = st
        .queue
        .iter()
        .map(|i| {
            json!({ "qid": i.qid, "videoId": i.video_id, "label": i.label, "opts": i.opts, "paused": i.paused, "force": i.force })
        })
        .collect();
    json!({
        "items": waiting,
        "running": !st.active.is_empty(),
        "active": st.active.iter().map(|a| a.item.qid).collect::<Vec<_>>(),
        "activeItems": st.active.iter().map(|a| json!({ "qid": a.item.qid, "videoId": a.item.video_id, "label": a.item.label, "opts": a.item.opts })).collect::<Vec<_>>(),
        "limit": s.parallel,
        "pausedAll": PAUSED_ALL.load(Ordering::SeqCst),
        "schedule": { "on": s.schedule, "open": open, "from": s.from, "to": s.to },
    })
}

fn send_queue(emit: &Emit) {
    send(emit, json!({ "kind": "queue", "queue": queue_snapshot() }));
}

/// Loads what was left in the queue last time, paused: nothing starts by itself. "Reanudar todo"
/// (or resuming one) starts it.
pub fn restore_queue() -> usize {
    if cfg!(test) {
        return 0;
    }
    let Ok(text) = std::fs::read_to_string(queue_path()) else { return 0 };
    let Ok(items) = serde_json::from_str::<Vec<QueueItem>>(&text) else { return 0 };
    let mut st = STATE.lock().unwrap();
    for mut i in items {
        if !valid_video_id(&i.video_id) || st.queue.iter().any(|q| q.qid == i.qid) {
            continue;
        }
        i.opts = i.opts.sanitized();
        i.label.truncate(200);
        i.paused = true;
        i.force = false;
        NEXT_QID.fetch_max(i.qid + 1, Ordering::SeqCst);
        st.queue.push_back(i);
    }
    st.queue.len()
}

/// Queues a download and starts as many as the limit allows. `qid` is chosen by the caller so that
/// it can follow the item (0 = pick one).
pub fn enqueue(video_id: String, opts: Opts, label: String, qid: u64, emit: Emit) {
    if !valid_video_id(&video_id) {
        status(&emit, "No se reconoció el vídeo");
        return;
    }
    let qid = if qid == 0 { NEXT_QID.fetch_add(1, Ordering::SeqCst) } else { qid };
    NEXT_QID.fetch_max(qid.saturating_add(1), Ordering::SeqCst);
    let label: String = label.chars().filter(|c| !c.is_control()).take(200).collect();
    let item = QueueItem { qid, video_id, label, opts: opts.sanitized(), paused: false, force: false };
    {
        let mut st = STATE.lock().unwrap();
        // The same thing twice would write the same files at once.
        let dup = st.queue.iter().chain(st.active.iter().map(|a| &a.item)).any(|i| {
            i.video_id == item.video_id && i.opts.playlist_id == item.opts.playlist_id && i.opts.mode == item.opts.mode
        });
        if dup {
            drop(st);
            status(&emit, "Ya está en la cola");
            return;
        }
        st.queue.push_back(item);
    }
    pump(&emit);
}

/// Starts waiting downloads while there is room under the limit (and the schedule allows it).
/// Each runs on its own thread and calls this again when it ends, so the queue drains by itself.
pub fn pump(emit: &Emit) {
    if SHUTDOWN.load(Ordering::SeqCst) {
        return;
    }
    loop {
        let limit = settings().parallel as usize;
        let open = schedule_open();
        let started = {
            let mut st = STATE.lock().unwrap();
            let next = if st.active.len() >= limit || PAUSED_ALL.load(Ordering::SeqCst) {
                None
            } else {
                st.queue.iter().position(|i| !i.paused && (open || i.force))
            };
            next.and_then(|ix| st.queue.remove(ix)).map(|item| {
                let pid = Arc::new(AtomicU32::new(0));
                let cancelled = Arc::new(AtomicBool::new(false));
                let pausing = Arc::new(AtomicBool::new(false));
                st.active.push(Active {
                    item: item.clone(),
                    pid: pid.clone(),
                    cancelled: cancelled.clone(),
                    pausing: pausing.clone(),
                });
                (item, pid, cancelled, pausing)
            })
        };
        let Some((item, pid, cancelled, pausing)) = started else { break };
        let ctx = Ctx {
            emit: emit.clone(),
            qid: item.qid,
            video_id: item.video_id.clone(),
            label: item.label.clone(),
            pid,
            cancelled,
        };
        ctx.send(json!({ "kind": "start", "label": item.label, "videoId": item.video_id, "opts": item.opts }));
        log(&format!("empieza: {} ({})", item.label, item.video_id));
        let emit = emit.clone();
        std::thread::spawn(move || {
            let result = job(&item.video_id, &item.opts, &ctx);
            if SHUTDOWN.load(Ordering::SeqCst) {
                return; // quitting: leave it saved to resume next time
            }
            let paused = pausing.load(Ordering::SeqCst);
            {
                // Out of the running list; a paused one goes back to the front of the queue.
                let mut st = STATE.lock().unwrap();
                st.active.retain(|a| a.item.qid != item.qid);
                if paused && matches!(&result, Err(e) if e == "cancelada") {
                    let mut back = item.clone();
                    back.paused = true;
                    st.queue.push_front(back);
                }
            }
            match result {
                Ok(file) => {
                    log(&format!("terminada: {}", item.label));
                    ctx.send(json!({ "kind": "done", "file": file }));
                }
                Err(e) if e == "cancelada" && paused => ctx.send(json!({ "kind": "paused" })),
                Err(e) if e == "cancelada" => {
                    log(&format!("cancelada: {}", item.label));
                    ctx.send(json!({ "kind": "cancelled" }));
                }
                Err(e) => {
                    log(&format!("error en «{}»: {e}", item.label));
                    history_add(HistEntry {
                        ok: false,
                        title: item.label.clone(),
                        video_id: item.video_id.clone(),
                        msg: e.clone(),
                        opts: item.opts.clone(),
                        ..Default::default()
                    });
                    ctx.send(json!({ "kind": "error", "msg": e }));
                    ctx.send(json!({ "kind": "history" }));
                }
            }
            pump(&emit);
        });
    }
    persist_queue();
    send_queue(emit);
}

/// Called every so often: starts what the schedule has just allowed and tells the pages when the
/// window opens or closes.
pub fn tick(emit: &Emit) {
    static LAST_OPEN: AtomicBool = AtomicBool::new(true);
    let open = schedule_open();
    if LAST_OPEN.swap(open, Ordering::SeqCst) != open {
        log(if open { "el horario permite descargar" } else { "fuera del horario: las nuevas descargas esperan" });
    }
    pump(emit);
}

/// Holds one download back. A running one is stopped but keeps its partial files, so resuming
/// continues where it left off.
pub fn pause(qid: u64, emit: &Emit) {
    let running = {
        let mut st = STATE.lock().unwrap();
        if let Some(i) = st.queue.iter_mut().find(|i| i.qid == qid) {
            i.paused = true;
            false
        } else if let Some(a) = st.active.iter().find(|a| a.item.qid == qid) {
            a.pausing.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    };
    if running {
        cancel(Some(qid));
    }
    persist_queue();
    send_queue(emit);
}

/// Lets a held-back download start (when there is room).
pub fn resume_item(qid: u64, emit: &Emit) {
    if let Some(i) = STATE.lock().unwrap().queue.iter_mut().find(|i| i.qid == qid) {
        i.paused = false;
    }
    pump(emit);
}

/// "Iniciar ahora": goes first and ignores the schedule.
pub fn start_now(qid: u64, emit: &Emit) {
    {
        let mut st = STATE.lock().unwrap();
        if let Some(ix) = st.queue.iter().position(|i| i.qid == qid) {
            if let Some(mut i) = st.queue.remove(ix) {
                i.paused = false;
                i.force = true;
                st.queue.push_front(i);
            }
        }
    }
    pump(emit);
}

/// Pauses (or lets go again) the whole queue; pausing also stops what is running.
pub fn pause_all(on: bool, emit: &Emit) {
    PAUSED_ALL.store(on, Ordering::SeqCst);
    if on {
        let running: Vec<u64> = STATE.lock().unwrap().active.iter().map(|a| a.item.qid).collect();
        for q in running {
            pause(q, emit);
        }
    } else {
        pump(emit);
    }
}

/// Starts everything that was held back (the saved queue after a restart included).
pub fn resume(emit: Emit) {
    PAUSED_ALL.store(false, Ordering::SeqCst);
    for i in STATE.lock().unwrap().queue.iter_mut() {
        i.paused = false;
    }
    pump(&emit);
}

/// Takes one waiting item out of the queue.
pub fn remove_queued(qid: u64, emit: &Emit) {
    STATE.lock().unwrap().queue.retain(|i| i.qid != qid);
    persist_queue();
    send_queue(emit);
}

/// Empties the queue and stops everything that is running.
pub fn clear_queue(emit: &Emit) {
    STATE.lock().unwrap().queue.clear();
    cancel(None);
    persist_queue();
    send_queue(emit);
}

/// The manager is closing: stop yt-dlp but keep the interrupted downloads (and the rest) saved.
pub fn shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
    persist_queue();
    cancel(None);
}

/// Stops one running download (`Some(qid)`) or all of them (`None`), with their ffmpeg children.
pub fn cancel(qid: Option<u64>) {
    let targets: Vec<(Arc<AtomicU32>, Arc<AtomicBool>)> = STATE
        .lock()
        .unwrap()
        .active
        .iter()
        .filter(|a| qid.map_or(true, |q| a.item.qid == q))
        .map(|a| (a.pid.clone(), a.cancelled.clone()))
        .collect();
    for (pid, cancelled) in targets {
        cancelled.store(true, Ordering::SeqCst);
        let pid = pid.load(Ordering::SeqCst);
        if pid != 0 {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    }
}

// ---------- available formats (for the download dialog) ----------

fn is_none(v: &Value) -> bool {
    v.as_str().map_or(true, |s| s == "none")
}

/// Asks yt-dlp what this video offers and boils it down to what the dialog needs.
/// Blocking and slow (several seconds): call from a thread.
pub fn formats(video_id: &str, emit: &Emit) -> Result<Value, String> {
    if !valid_video_id(video_id) {
        return Err("vídeo no válido".into());
    }
    let _side = SideJob::new();
    let exe = ensure_ytdlp(emit)?;
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    let query = || {
        Command::new(&exe)
            .args(["-J", "--no-playlist", "--no-warnings", "--"])
            .arg(&url)
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))
    };
    let mut out = query()?;
    if !out.status.success() {
        status(emit, "Actualizando yt-dlp y reintentando…");
        update_ytdlp_quiet(&exe);
        out = query()?;
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err
            .lines()
            .rev()
            .find(|l| l.contains("ERROR"))
            .or_else(|| err.lines().last())
            .unwrap_or("yt-dlp no pudo leer el vídeo");
        return Err(msg.trim_start_matches("ERROR: ").to_string());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("respuesta no válida de yt-dlp: {e}"))?;

    let has_ffmpeg = find_ffmpeg().0;
    let duration = v["duration"].as_f64().unwrap_or(0.0);
    let size_of = |f: &Value| -> Option<u64> {
        f["filesize"]
            .as_u64()
            .or_else(|| f["filesize_approx"].as_f64().map(|x| x as u64))
            .or_else(|| f["tbr"].as_f64().filter(|_| duration > 0.0).map(|t| (t * 125.0 * duration) as u64))
    };
    let formats = v["formats"].as_array().cloned().unwrap_or_default();

    // Best audio stream (added to the size of video-only streams when merging).
    let best_audio = formats
        .iter()
        .filter(|f| is_none(&f["vcodec"]) && !is_none(&f["acodec"]))
        .max_by(|a, b| {
            let (x, y) = (a["abr"].as_f64().unwrap_or(0.0), b["abr"].as_f64().unwrap_or(0.0));
            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
        });
    let audio_size = best_audio.and_then(size_of).unwrap_or(0);

    // height -> (fps, size). With ffmpeg: video-only streams (merged with the audio).
    // Without it: only streams that already contain audio.
    let mut by_height: BTreeMap<u64, (f64, u64)> = BTreeMap::new();
    for f in &formats {
        let Some(h) = f["height"].as_u64() else { continue };
        if is_none(&f["vcodec"]) {
            continue;
        }
        let combined = !is_none(&f["acodec"]);
        if has_ffmpeg == combined {
            continue;
        }
        let size = size_of(f).unwrap_or(0) + if has_ffmpeg { audio_size } else { 0 };
        let fps = f["fps"].as_f64().unwrap_or(30.0);
        let e = by_height.entry(h).or_insert((fps, size));
        if size > e.1 {
            e.1 = size;
        }
        if fps > e.0 {
            e.0 = fps;
        }
    }
    let video: Vec<Value> = by_height
        .iter()
        .rev()
        .map(|(h, (fps, size))| json!({ "height": h, "fps": fps, "size": size }))
        .collect();

    Ok(json!({
        "id": video_id,
        "title": v["title"],
        "channel": v["uploader"],
        "duration": duration,
        "hasFfmpeg": has_ffmpeg,
        "video": video,
        "audio": { "size": audio_size, "abr": best_audio.and_then(|f| f["abr"].as_f64()) },
    }))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn has(args: &[String], flag: &str) -> bool {
        args.iter().any(|a| a == flag)
    }
    fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
    }

    fn opts(mode: &str, container: &str, audio: &str) -> Opts {
        Opts {
            mode: mode.into(),
            container: container.into(),
            audio_format: audio.into(),
            audio_quality: "320K".into(),
            max_height: 1080,
            embed_meta: true,
            remove_categories: "sponsor,selfpromo".into(),
            duration: 100.0,
            playlist_id: String::new(),
        }
        .sanitized()
    }

    #[test]
    fn embeds_cover_only_where_the_container_can_carry_it() {
        let ff = || (true, None);
        let mp4 = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), ff(), &Settings::default());
        assert!(has(&mp4, "--embed-metadata") && has(&mp4, "--embed-thumbnail"));
        let webm = build_args("dQw4w9WgXcQ", &opts("video", "webm", "mp3"), ff(), &Settings::default());
        assert!(has(&webm, "--embed-metadata") && !has(&webm, "--embed-thumbnail"));
        let flac = build_args("dQw4w9WgXcQ", &opts("audio", "mp4", "flac"), ff(), &Settings::default());
        assert!(!has(&flac, "--embed-thumbnail"));
        let mp3 = build_args("dQw4w9WgXcQ", &opts("audio", "mp4", "mp3"), ff(), &Settings::default());
        assert!(has(&mp3, "--embed-thumbnail"));
    }

    #[test]
    fn sponsor_removal_and_metadata_need_ffmpeg() {
        let args = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), (false, None), &Settings::default());
        assert!(!has(&args, "--embed-metadata") && !has(&args, "--sponsorblock-remove"));
        let args = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), (true, None), &Settings::default());
        assert_eq!(value_of(&args, "--sponsorblock-remove"), Some("sponsor,selfpromo"));
    }

    #[test]
    fn playlist_download_uses_the_list_url_and_rejects_bad_ids() {
        let mut o = opts("video", "mp4", "mp3");
        o.playlist_id = "PLabcdefghijk-_123".into();
        let args = build_args("dQw4w9WgXcQ", &o.sanitized(), (true, None), &Settings::default());
        assert!(has(&args, "--yes-playlist") && !has(&args, "--no-playlist") && has(&args, "--ignore-errors"));
        assert_eq!(args.last().map(String::as_str), Some("https://www.youtube.com/playlist?list=PLabcdefghijk-_123"));
        // mixes and anything that is not a plain id fall back to the single video
        for bad in ["RDabcdefghijk", "PL x --exec calc", "short", ""] {
            let mut o = opts("video", "mp4", "mp3");
            o.playlist_id = bad.into();
            let args = build_args("dQw4w9WgXcQ", &o.sanitized(), (true, None), &Settings::default());
            assert!(has(&args, "--no-playlist"), "{bad}");
            assert!(args.last().unwrap().contains("watch?v=dQw4w9WgXcQ"));
        }
    }

    static QUEUE_TEST: Mutex<()> = Mutex::new(());
    static SETTINGS_TEST: Mutex<()> = Mutex::new(());
    static RUNNING: AtomicUsize = AtomicUsize::new(0);
    static MOST: AtomicUsize = AtomicUsize::new(0);
    static FINISHED: AtomicUsize = AtomicUsize::new(0);
    fn fake_job(_: &Ctx) -> Result<Option<String>, String> {
        let now = RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
        MOST.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(60));
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        FINISHED.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
    fn wait_for(n: usize) {
        let t = Instant::now();
        while FINISHED.load(Ordering::SeqCst) < n && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn runs_at_most_the_configured_number_of_downloads_at_once() {
        let _g = QUEUE_TEST.lock().unwrap();
        *JOB_HOOK.lock().unwrap() = Some(fake_job);
        let emit: Emit = Arc::new(|_: Value| {});
        let opts = || Opts { mode: "video".into(), ..Default::default() };
        // limit 2: six downloads, never more than two together; the duplicate is ignored
        set_limit(2);
        for i in 0..6 {
            enqueue(format!("aaaaaaaaaa{i}"), opts(), "x".into(), 0, emit.clone());
        }
        enqueue("aaaaaaaaaa0".into(), opts(), "dup".into(), 0, emit.clone());
        wait_for(6);
        assert_eq!(FINISHED.load(Ordering::SeqCst), 6, "all six ran (and the duplicate did not)");
        assert_eq!(MOST.load(Ordering::SeqCst), 2);
        // limit 1: strictly one after another
        MOST.store(0, Ordering::SeqCst);
        set_limit(1);
        for i in 0..3 {
            enqueue(format!("bbbbbbbbbb{i}"), opts(), "x".into(), 0, emit.clone());
        }
        wait_for(9);
        assert_eq!(MOST.load(Ordering::SeqCst), 1);
        // the limit is clamped to the allowed range
        set_limit(99);
        assert_eq!(settings().parallel as usize, MAX_PARALLEL);
        set_limit(0);
        assert_eq!(settings().parallel, 1);
        assert_eq!(active_count(), 0);
        *JOB_HOOK.lock().unwrap() = None;
    }

    #[test]
    fn queue_items_survive_a_save_and_load() {
        let item = QueueItem {
            qid: 7,
            video_id: "dQw4w9WgXcQ".into(),
            label: "Canción".into(),
            opts: Opts { mode: "audio".into(), audio_format: "mp3".into(), playlist_id: "PLabcdefghijk".into(), ..Default::default() },
            paused: true,
            force: false,
        };
        let text = serde_json::to_string(&vec![item]).unwrap();
        let back: Vec<QueueItem> = serde_json::from_str(&text).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].qid, back[0].video_id.as_str(), back[0].label.as_str()), (7, "dQw4w9WgXcQ", "Canción"));
        assert_eq!(back[0].opts.playlist_id, "PLabcdefghijk");
        assert!(back[0].paused);
        // an old file without some option fields still loads (missing ones take their defaults)
        let old: Vec<QueueItem> = serde_json::from_str(r#"[{"qid":1,"video_id":"dQw4w9WgXcQ","label":"x","opts":{"mode":"video"}}]"#).unwrap();
        assert_eq!(old[0].opts.mode, "video");
        assert_eq!(old[0].opts.max_height, 0);
        assert!(!old[0].paused && !old[0].force, "files from before pausing existed load as running items");
    }

    #[test]
    fn categories_are_whitelisted_and_deduplicated() {
        let mut o = Opts { remove_categories: "sponsor, sponsor,--exec rm -rf,poi_highlight,intro,../x".into(), ..Default::default() };
        o = o.sanitized();
        assert_eq!(o.remove_categories, "sponsor,intro");
        let none = Opts { remove_categories: "nada,--version".into(), ..Default::default() }.sanitized();
        assert_eq!(none.remove_categories, "");
    }

    #[test]
    fn video_id_goes_after_the_separator_and_options_are_whitelisted() {
        let mut o = opts("video", "mp4", "mp3");
        o.container = "mp4 --exec calc".into();
        o.audio_format = "--version".into();
        let o = o.sanitized();
        assert_eq!(o.container, "mp4");
        assert_eq!(o.audio_format, "mp3");
        let args = build_args("dQw4w9WgXcQ", &o, (true, None), &Settings::default());
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(args.len(), sep + 2, "nothing after the URL");
    }

    #[test]
    fn templates_become_ytdlp_templates_and_reject_anything_else() {
        assert_eq!(render_template("{title} [{id}]").unwrap(), "%(title).150B [%(id)s].%(ext)s");
        let t = render_template("{channel}/{playlist}/{index} - {title}").unwrap();
        assert!(t.starts_with("%(channel,uploader|Sin canal).80B/%(playlist_title|Sin lista).80B/%(playlist_index)03d - "));
        assert!(t.ends_with(".%(ext)s"));
        // everything yt-dlp could interpret, or that leaves the downloads folder, is refused
        for bad in [
            "", "   ", "{nope}", "{title", "%(title)s", "{title}%", "..\\x", "../{title}", "a/../b", "C:\\{title}", "/{title}",
            "{title}/", "{title}//{id}", "a/b/c/d/e/f/{title}", "{title}|{id}", "{title}:{id}", "{title}*", "{title}\"x", "{title}\n{id}",
            "folder./{title}",
        ] {
            assert!(render_template(bad).is_err(), "should be refused: {bad:?}");
        }
        assert!(render_template(&"a".repeat(121)).is_err());
        assert!(render_template("Música (2024) - {title} & {channel} [{date}]").is_ok());
    }

    #[test]
    fn schedule_window_handles_overnight_ranges() {
        let h = |h: u32, m: u32| h * 60 + m;
        assert!(in_window(h(2, 0), h(1, 0), h(7, 0)));
        assert!(!in_window(h(7, 0), h(1, 0), h(7, 0)), "the end is exclusive");
        assert!(!in_window(h(12, 0), h(1, 0), h(7, 0)));
        // 22:00 -> 06:00 crosses midnight
        assert!(in_window(h(23, 30), h(22, 0), h(6, 0)));
        assert!(in_window(h(3, 0), h(22, 0), h(6, 0)));
        assert!(!in_window(h(12, 0), h(22, 0), h(6, 0)));
        assert!(in_window(h(5, 0), h(8, 0), h(8, 0)), "from == to means always");
        assert_eq!(parse_hhmm("07:05"), Some(425));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("7"), None);
    }

    #[test]
    fn settings_are_clamped_and_bad_values_refused() {
        let _g = SETTINGS_TEST.lock().unwrap();
        let s = Settings {
            parallel: 99,
            speed_kbps: u32::MAX,
            subs: "x".into(),
            subs_langs: "es;rm".into(),
            from: "9".into(),
            dir: "relativa".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.parallel as usize, MAX_PARALLEL);
        assert_eq!(s.speed_kbps, 1_000_000);
        assert_eq!((s.subs.as_str(), s.subs_langs.as_str(), s.from.as_str(), s.dir.as_str()), ("off", "es,en", "01:00", ""));
        assert!(set_settings(&json!({ "nameTemplate": "{title} %(x)s" })).is_err());
        assert!(set_settings(&json!({ "from": "25:00" })).is_err());
        assert!(set_settings(&json!({ "subsLangs": "es, --exec x" })).is_err());
        assert!(set_settings(&json!({ "dir": "relativa\\carpeta" })).is_err());
        assert!(set_settings(&json!({ "parallel": "dos" })).is_err());
        assert!(set_settings(&json!(5)).is_err());
        let ok = set_settings(&json!({ "nameTemplate": "{channel} - {title}", "unknownKey": 1 })).unwrap();
        assert_eq!(ok.name_template, "{channel} - {title}");
        assert_eq!(settings().name_template, "{channel} - {title}");
        set_settings(&json!({ "nameTemplate": NAME_DEFAULT })).unwrap();
    }

    #[test]
    fn extras_and_speed_limit_reach_the_command_line() {
        let ff = (true, None);
        let o = opts("video", "mp4", "mp3");
        let plain = build_args("dQw4w9WgXcQ", &o, ff.clone(), &Settings::default());
        for flag in ["--limit-rate", "--write-subs", "--write-thumbnail"] {
            assert!(!has(&plain, flag), "{flag} must be off by default");
        }
        let s = Settings {
            speed_kbps: 1000,
            parallel: 4,
            subs: "embed".into(),
            subs_langs: "es,en".into(),
            thumbnail: true,
            ..Default::default()
        };
        let args = build_args("dQw4w9WgXcQ", &o, ff.clone(), &s);
        assert_eq!(value_of(&args, "--limit-rate"), Some("250K"), "the total is shared by the parallel downloads");
        assert_eq!(value_of(&args, "--sub-langs"), Some("es,en"));
        assert!(has(&args, "--write-subs") && has(&args, "--write-auto-subs") && has(&args, "--embed-subs") && has(&args, "--write-thumbnail"));
        // no ffmpeg: nothing that needs it
        let noff = build_args("dQw4w9WgXcQ", &o, (false, None), &s);
        assert!(has(&noff, "--write-subs") && !has(&noff, "--embed-subs") && !has(&noff, "--convert-subs") && !has(&noff, "--convert-thumbnails"));
        // audio-only downloads get no captions
        let audio = build_args("dQw4w9WgXcQ", &opts("audio", "mp4", "mp3"), ff, &s);
        assert!(!has(&audio, "--write-subs"));
        // the file name comes from the settings
        let named = build_args(
            "dQw4w9WgXcQ",
            &o,
            (true, None),
            &Settings { name_template: "{channel}/{title}".into(), ..Default::default() },
        );
        assert_eq!(value_of(&named, "-o"), Some("%(channel,uploader|Sin canal).80B/%(title).150B.%(ext)s"));
    }

    fn pause_job(ctx: &Ctx) -> Result<Option<String>, String> {
        // a "download" that runs until it is cancelled or paused, like yt-dlp would
        let t = Instant::now();
        while !ctx.is_cancelled() && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if ctx.is_cancelled() {
            Err("cancelada".into())
        } else {
            Ok(None)
        }
    }

    #[test]
    fn a_paused_download_goes_back_to_the_queue_and_resumes() {
        // The tests that use the queue take turns.
        let _g = QUEUE_TEST.lock().unwrap();
        *JOB_HOOK.lock().unwrap() = Some(pause_job);
        set_limit(1);
        let emit: Emit = Arc::new(|_: Value| {});
        let o = || Opts { mode: "video".into(), ..Default::default() };
        enqueue("pppppppppp1".into(), o(), "uno".into(), 0, emit.clone());
        enqueue("pppppppppp2".into(), o(), "dos".into(), 0, emit.clone());
        assert_eq!(active_count(), 1);
        let first = STATE.lock().unwrap().active[0].item.qid;
        pause(first, &emit);
        // the first returns to the queue, paused; the second takes its place
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) && !STATE.lock().unwrap().queue.iter().any(|i| i.qid == first && i.paused) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            STATE.lock().unwrap().queue.iter().any(|i| i.qid == first && i.paused),
            "the paused item waits in the queue"
        );
        assert!(busy());
        clear_queue(&emit);
        let t = Instant::now();
        while active_count() > 0 && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(active_count(), 0);
        assert!(STATE.lock().unwrap().queue.is_empty());
        *JOB_HOOK.lock().unwrap() = None;
        set_limit(2);
    }
}
