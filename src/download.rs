//! Video downloads through yt-dlp (the standard tool for this; YouTube's signature and token
//! handling changes too often to reimplement), plus the tooling around it:
//! - yt-dlp is fetched from its official GitHub release and verified against SHA2-256SUMS.
//! - ffmpeg (optional, needed for best quality and MP3) comes from yt-dlp's own FFmpeg-Builds
//!   release, verified against its checksums.sha256 and unpacked into %LOCALAPPDATA%\PcTube\bin.
//! - Files land in Downloads\PC Tube and can be listed / opened / deleted from the settings panel.
//!
//! Everything reports to the page through `window.pctube._download(event)`.

use crate::{config, ipc::Out};
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const FFMPEG_BASE: &str = "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download";
const FFMPEG_ZIP: &str = "ffmpeg-master-latest-win64-gpl-shared.zip";
/// Only these file types are listed, opened or deleted from the panel.
const MEDIA_EXT: &[&str] = &[
    "mp4", "mkv", "webm", "mp3", "m4a", "opus", "ogg", "wav", "flac", "mov", "aac",
];

static TOOL_BUSY: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);
static PID: AtomicU32 = AtomicU32::new(0);
/// True while the queue worker thread is running (a download is in progress).
static WORKER: AtomicBool = AtomicBool::new(false);
/// The app is quitting: the interrupted download stays in the saved queue to resume next time.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static QUEUE: Mutex<VecDeque<QueueItem>> = Mutex::new(VecDeque::new());
static ACTIVE: Mutex<Option<QueueItem>> = Mutex::new(None);
static NEXT_QID: AtomicU64 = AtomicU64::new(1);

pub type Emit = Arc<dyn Fn(Out) + Send + Sync>;

// ---------- paths ----------

pub fn bin_dir() -> PathBuf {
    let d = config::data_dir().join("bin");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn download_dir() -> PathBuf {
    let d = dirs::download_dir()
        .unwrap_or_else(config::data_dir)
        .join("PC Tube");
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

// ---------- events to the page ----------

/// Calls `window.pctube.<func>(ev)` in the page (func is always a literal of ours).
pub(crate) fn send_raw(emit: &Emit, func: &str, ev: Value) {
    emit(Out::Eval(format!("window.pctube&&window.pctube.{func}&&window.pctube.{func}({ev})")));
}
pub(crate) fn send(emit: &Emit, ev: Value) {
    emit(Out::Eval(format!(
        "window.pctube&&window.pctube._download&&window.pctube._download({ev})"
    )));
}
fn status(emit: &Emit, msg: &str) {
    send(emit, json!({ "kind": "status", "msg": msg }));
}
fn tool_ev(emit: &Emit, tool: &str, state: &str, msg: &str, pct: Option<u32>) {
    send(emit, json!({ "kind": "tool", "tool": tool, "state": state, "msg": msg, "pct": pct }));
}

// ---------- network helpers ----------

pub(crate) fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .into()
}

pub(crate) fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

/// Streams `url` into `dest`, returns the SHA-256 (hex). `on_pct` gets 0..=100 when known.
pub(crate) fn download_to(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    limit: u64,
    mut on_pct: impl FnMut(u32),
) -> Result<String, String> {
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = resp.body().content_length();
    let mut reader = resp.into_body().into_with_config().limit(limit).reader();
    let mut out = File::create(dest).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut last_pct, mut last_t) = (0u64, u32::MAX, Instant::now());
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if let Some(t) = total.filter(|t| *t > 0) {
            let pct = (done * 100 / t).min(100) as u32;
            if pct != last_pct && last_t.elapsed() > Duration::from_millis(200) {
                last_pct = pct;
                last_t = Instant::now();
                on_pct(pct);
            }
        }
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let hash = p.next()?;
        let name = p.next()?.trim_start_matches('*');
        (name == file).then(|| hash.to_lowercase())
    })
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
        tool_ev(&emit, tool, "working", "Preparando…", None);
        match job(&emit) {
            Ok(msg) => tool_ev(&emit, tool, "done", &msg, None),
            Err(e) => tool_ev(&emit, tool, "error", &e, None),
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

// ---------- library (files) ----------

fn is_media(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXT.contains(&e.to_lowercase().as_str()))
}

/// Leftovers of an interrupted download (yt-dlp resumes them if you download again).
fn is_partial(name: &str) -> bool {
    name.ends_with(".part") || name.ends_with(".ytdl")
}

/// Resolves a bare file name inside the downloads folder (no paths, media types only;
/// partial files only when `allow_partial`).
fn media_path(name: &str, allow_partial: bool) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.file_name().map(|n| n != p.as_os_str()).unwrap_or(true) {
        return None;
    }
    if !is_media(name) && !(allow_partial && is_partial(name)) {
        return None;
    }
    let full = download_dir().join(name);
    full.is_file().then_some(full)
}

/// yt-dlp's version. Slow (the exe unpacks itself on start): call from a thread.
pub fn ytdlp_version() -> Option<String> {
    let exe = find_ytdlp()?;
    let out = Command::new(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Snapshot for the "Descargas" panel (fast: no processes are started).
pub fn info() -> Value {
    let yt = find_ytdlp();
    let (ff_ok, ff_dir) = find_ffmpeg();

    let mut files: Vec<Value> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(download_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let partial = is_partial(&name);
            if !partial && !is_media(&name) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            if !m.is_file() {
                continue;
            }
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            files.push(json!({ "name": name, "size": m.len(), "modified": modified, "partial": partial }));
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f["modified"].as_u64().unwrap_or(0)));
    files.truncate(300);

    json!({
        "dir": download_dir().to_string_lossy(),
        "ytdlp": {
            "installed": yt.is_some(),
            "source": yt.as_ref().map(|p| if p.starts_with(bin_dir()) { "app" } else { "system" }),
        },
        "ffmpeg": {
            "installed": ff_ok,
            "source": if !ff_ok { Value::Null } else if ff_dir.is_some() { json!("app") } else { json!("system") },
        },
        "files": files,
        "busy": WORKER.load(Ordering::SeqCst),
        "queue": queue_snapshot(),
    })
}

/// At startup: if the app's own yt-dlp has not been updated for a week, update it quietly in
/// the background (YouTube breaks old versions often). A system-wide yt-dlp is left alone.
pub fn auto_update_in_background() {
    // Test sessions (and anyone who sets PCTUBE_NO_AUTOUPDATE) never touch the network here.
    if config::debug_session() || std::env::var_os("PCTUBE_NO_AUTOUPDATE").is_some() {
        return;
    }
    std::thread::spawn(|| {
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
        if fresh || WORKER.load(Ordering::SeqCst) {
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
        if ok {
            let _ = std::fs::write(&stamp, b"");
        }
    });
}

pub fn open_folder() {
    let _ = Command::new("explorer.exe").arg(download_dir()).spawn();
}

/// action: "open" | "show" | "delete"
pub fn file_action(action: &str, name: &str) -> Result<(), String> {
    // Incomplete files can be shown or deleted, but never opened.
    let path = media_path(name, action != "open").ok_or("archivo no válido")?;
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
        "delete" => std::fs::remove_file(&path).map_err(|e| e.to_string())?,
        _ => return Err("acción no válida".into()),
    }
    Ok(())
}

// ---------- downloads ----------

use std::collections::BTreeMap;

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

fn build_args(video_id: &str, o: &Opts, ffmpeg: (bool, Option<PathBuf>)) -> Vec<String> {
    let playlist = !o.playlist_id.is_empty();
    let template = if playlist {
        // one folder per playlist, files numbered in playlist order
        "%(playlist_title).80B/%(playlist_index)03d - %(title).150B [%(id)s].%(ext)s"
    } else {
        "%(title).150B [%(id)s].%(ext)s"
    };
    let mut a: Vec<String> = [
        "--newline",
        "--progress",
        "--no-quiet",
        if playlist { "--yes-playlist" } else { "--no-playlist" },
        "--no-warnings",
        "--windows-filenames",
        "--no-mtime",
        "--progress-template",
        "PCTUBE_PROGRESS %(progress._percent_str)s",
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
fn spawn_size_watch(label: &'static str, paths: Vec<PathBuf>, expected: Option<u64>, emit: Emit) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let phase = |pct: Option<i32>| send(&emit, json!({ "kind": "phase", "label": label, "pct": pct }));
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

/// Runs yt-dlp and streams progress. Ok(Some(path)) on success.
fn run(exe: &Path, args: &[String], o: &Opts, has_ffmpeg: bool, emit: &Emit) -> Result<Option<String>, String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))?;
    PID.store(child.id(), Ordering::SeqCst);

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
    let mut watch: Option<Arc<AtomicBool>> = None;
    let stop_watch = |w: &mut Option<Arc<AtomicBool>>| {
        if let Some(f) = w.take() {
            f.store(true, Ordering::SeqCst);
        }
    };

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(p) = line.strip_prefix("PCTUBE_PROGRESS ") {
                if let Ok(v) = p.trim().trim_end_matches('%').trim().parse::<f32>() {
                    if v as i32 != last_pct {
                        last_pct = v as i32;
                        send(emit, json!({ "kind": "progress", "pct": last_pct }));
                    }
                }
            } else if let Some(it) = line.strip_prefix("PCTUBE_ITEM ") {
                // "3 20" inside a playlist ("NA NA" for a single video)
                let mut n = it.split_whitespace().map(|x| x.parse::<u32>().ok());
                if let (Some(Some(i)), Some(Some(total))) = (n.next(), n.next()) {
                    stop_watch(&mut watch);
                    streams = 0;
                    last_pct = -1;
                    send(emit, json!({ "kind": "item", "index": i, "total": total }));
                }
            } else if let Some(t) = line.strip_prefix("PCTUBE_TITLE ") {
                send(emit, json!({ "kind": "title", "title": t.trim() }));
            } else if let Some(f) = line.strip_prefix("PCTUBE_DONE ") {
                stop_watch(&mut watch);
                file = Some(f.trim().to_string());
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
                send(emit, json!({ "kind": "phase", "label": label, "pct": 0 }));
            } else if let Some(rest) = line.strip_prefix("[Merger] Merging formats into ") {
                stop_watch(&mut watch);
                let final_path = PathBuf::from(rest.trim().trim_matches('"'));
                let ext = final_path.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
                let stem = final_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                // ffmpeg writes "<stem>.temp.<ext>" and renames it when done.
                let temp = final_path.with_file_name(format!("{stem}.temp.{ext}"));
                let expected = streams_size(&final_path);
                watch = Some(spawn_size_watch("Uniendo vídeo y audio", vec![final_path, temp], Some(expected), emit.clone()));
            } else if let Some(rest) = line.strip_prefix("[ExtractAudio] Destination: ") {
                stop_watch(&mut watch);
                let dest = PathBuf::from(rest.trim());
                let expected = o.cbr_kbps().map(|k| (o.duration * k as f64 * 125.0) as u64);
                watch = Some(spawn_size_watch("Convirtiendo audio", vec![dest], expected, emit.clone()));
            } else if line.starts_with("[ModifyChapters]") {
                stop_watch(&mut watch);
                watch = Some(spawn_size_watch("Quitando patrocinios", vec![], None, emit.clone()));
            } else if line.starts_with("[VideoRemuxer]") || line.starts_with("[VideoConvertor]") {
                stop_watch(&mut watch);
                watch = Some(spawn_size_watch("Convirtiendo vídeo", vec![], None, emit.clone()));
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    PID.store(0, Ordering::SeqCst);
    stop_watch(&mut watch);
    if let Some(t) = err_thread {
        let _ = t.join();
    }
    // In a playlist some entries may fail (private, removed): that is fine if others came through.
    if status.success() || (!o.playlist_id.is_empty() && file.is_some() && !CANCELLED.load(Ordering::SeqCst)) {
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

fn job(video_id: &str, o: &Opts, emit: &Emit) -> Result<Option<String>, String> {
    let exe = ensure_ytdlp(emit)?;
    let ffmpeg = find_ffmpeg();
    let has_ffmpeg = ffmpeg.0;
    if !has_ffmpeg && o.mode != "audio" {
        status(emit, "Sin ffmpeg: calidad limitada (instálalo en Ajustes → Descargas)");
    } else {
        status(emit, "Iniciando descarga…");
    }
    let args = build_args(video_id, o, ffmpeg);
    match run(&exe, &args, o, has_ffmpeg, emit) {
        Ok(f) => Ok(f),
        Err(_) if CANCELLED.load(Ordering::SeqCst) => Err("cancelada".into()),
        // Test sessions never update the tool (they only probe the queue with made-up ids).
        Err(first) if config::debug_session() => Err(first),
        Err(first) => {
            // YouTube changes break yt-dlp regularly: update it and retry once.
            status(emit, "Actualizando yt-dlp y reintentando…");
            update_ytdlp_quiet(&exe);
            run(&exe, &args, o, has_ffmpeg, emit).map_err(|second| {
                if CANCELLED.load(Ordering::SeqCst) {
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
/// when the app closed can be resumed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub qid: u64,
    pub video_id: String,
    pub label: String,
    pub opts: Opts,
}

fn queue_path() -> PathBuf {
    config::data_dir().join(if config::debug_session() { "queue-debug.json" } else { "queue.json" })
}

/// Active download first, then the waiting ones.
fn all_items() -> Vec<QueueItem> {
    let mut v: Vec<QueueItem> = ACTIVE.lock().unwrap().iter().cloned().collect();
    v.extend(QUEUE.lock().unwrap().iter().cloned());
    v
}

fn persist_queue() {
    let items = all_items();
    if items.is_empty() {
        let _ = std::fs::remove_file(queue_path());
    } else if let Ok(s) = serde_json::to_string(&items) {
        let _ = std::fs::write(queue_path(), s);
    }
}

/// Waiting items as JSON for the page: [{qid, videoId, label, opts}].
pub fn queue_snapshot() -> Value {
    let waiting: Vec<Value> = QUEUE
        .lock()
        .unwrap()
        .iter()
        .map(|i| json!({ "qid": i.qid, "videoId": i.video_id, "label": i.label, "opts": i.opts }))
        .collect();
    json!({
        "items": waiting,
        "running": WORKER.load(Ordering::SeqCst),
        "active": ACTIVE.lock().unwrap().as_ref().map(|i| i.qid),
    })
}

fn send_queue(emit: &Emit) {
    send(emit, json!({ "kind": "queue", "queue": queue_snapshot() }));
}

/// Loads what was left in the queue last time. Nothing starts by itself: the page shows it and
/// "Reanudar" (or the next download) starts it.
pub fn restore_queue() {
    let Ok(text) = std::fs::read_to_string(queue_path()) else { return };
    let Ok(items) = serde_json::from_str::<Vec<QueueItem>>(&text) else { return };
    let mut q = QUEUE.lock().unwrap();
    for mut i in items {
        if !valid_video_id(&i.video_id) {
            continue;
        }
        i.opts = i.opts.sanitized();
        i.label.truncate(200);
        NEXT_QID.fetch_max(i.qid + 1, Ordering::SeqCst);
        q.push_back(i);
    }
}

/// Queues a download and starts the worker if it is idle. `qid` is chosen by the page so that
/// it can follow the item (0 = pick one).
pub fn enqueue(video_id: String, opts: Opts, label: String, qid: u64, emit: Emit) {
    if !valid_video_id(&video_id) {
        status(&emit, "No se reconoció el vídeo");
        return;
    }
    let qid = if qid == 0 { NEXT_QID.fetch_add(1, Ordering::SeqCst) } else { qid };
    NEXT_QID.fetch_max(qid + 1, Ordering::SeqCst);
    let label: String = label.chars().filter(|c| !c.is_control()).take(200).collect();
    QUEUE.lock().unwrap().push_back(QueueItem { qid, video_id, label, opts: opts.sanitized() });
    persist_queue();
    send_queue(&emit);
    ensure_worker(emit);
}

/// Starts the worker unless it is already running.
pub fn ensure_worker(emit: Emit) {
    if WORKER.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        loop {
            // Decide "empty" and clear WORKER under the queue lock, so an item pushed right now
            // is either seen here or finds WORKER false and starts a new worker.
            let item = {
                let mut q = QUEUE.lock().unwrap();
                match q.pop_front() {
                    Some(i) => i,
                    None => {
                        WORKER.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            };
            *ACTIVE.lock().unwrap() = Some(item.clone());
            CANCELLED.store(false, Ordering::SeqCst);
            send(&emit, json!({ "kind": "start", "qid": item.qid, "label": item.label, "videoId": item.video_id, "opts": item.opts }));
            send_queue(&emit);
            let result = job(&item.video_id, &item.opts, &emit);
            if SHUTDOWN.load(Ordering::SeqCst) {
                return; // quitting: leave it saved to resume next time
            }
            match result {
                Ok(file) => send(&emit, json!({ "kind": "done", "qid": item.qid, "file": file })),
                Err(e) if e == "cancelada" => send(&emit, json!({ "kind": "cancelled", "qid": item.qid })),
                Err(e) => send(&emit, json!({ "kind": "error", "qid": item.qid, "msg": e })),
            }
            *ACTIVE.lock().unwrap() = None;
            persist_queue();
            send_queue(&emit);
        }
        persist_queue();
        send_queue(&emit);
    });
}

/// Starts the saved queue ("Reanudar").
pub fn resume(emit: Emit) {
    if QUEUE.lock().unwrap().is_empty() {
        send_queue(&emit);
    } else {
        ensure_worker(emit);
    }
}

/// Takes one waiting item out of the queue.
pub fn remove_queued(qid: u64, emit: &Emit) {
    QUEUE.lock().unwrap().retain(|i| i.qid != qid);
    persist_queue();
    send_queue(emit);
}

/// Empties the queue and stops the running download.
pub fn clear_queue(emit: &Emit) {
    QUEUE.lock().unwrap().clear();
    persist_queue();
    send_queue(emit);
    cancel();
}

/// The app is closing: stop yt-dlp but keep the interrupted download (and the rest) saved.
pub fn shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
    persist_queue();
    cancel();
}

/// Stops the running yt-dlp (and its ffmpeg child) if any.
pub fn cancel() {
    let pid = PID.load(Ordering::SeqCst);
    if pid != 0 {
        CANCELLED.store(true, Ordering::SeqCst);
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
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
        let mp4 = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), ff());
        assert!(has(&mp4, "--embed-metadata") && has(&mp4, "--embed-thumbnail"));
        let webm = build_args("dQw4w9WgXcQ", &opts("video", "webm", "mp3"), ff());
        assert!(has(&webm, "--embed-metadata") && !has(&webm, "--embed-thumbnail"));
        let flac = build_args("dQw4w9WgXcQ", &opts("audio", "mp4", "flac"), ff());
        assert!(!has(&flac, "--embed-thumbnail"));
        let mp3 = build_args("dQw4w9WgXcQ", &opts("audio", "mp4", "mp3"), ff());
        assert!(has(&mp3, "--embed-thumbnail"));
    }

    #[test]
    fn sponsor_removal_and_metadata_need_ffmpeg() {
        let args = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), (false, None));
        assert!(!has(&args, "--embed-metadata") && !has(&args, "--sponsorblock-remove"));
        let args = build_args("dQw4w9WgXcQ", &opts("video", "mp4", "mp3"), (true, None));
        assert_eq!(value_of(&args, "--sponsorblock-remove"), Some("sponsor,selfpromo"));
    }

    #[test]
    fn playlist_download_uses_the_list_url_and_rejects_bad_ids() {
        let mut o = opts("video", "mp4", "mp3");
        o.playlist_id = "PLabcdefghijk-_123".into();
        let args = build_args("dQw4w9WgXcQ", &o.sanitized(), (true, None));
        assert!(has(&args, "--yes-playlist") && !has(&args, "--no-playlist") && has(&args, "--ignore-errors"));
        assert_eq!(args.last().map(String::as_str), Some("https://www.youtube.com/playlist?list=PLabcdefghijk-_123"));
        // mixes and anything that is not a plain id fall back to the single video
        for bad in ["RDabcdefghijk", "PL x --exec calc", "short", ""] {
            let mut o = opts("video", "mp4", "mp3");
            o.playlist_id = bad.into();
            let args = build_args("dQw4w9WgXcQ", &o.sanitized(), (true, None));
            assert!(has(&args, "--no-playlist"), "{bad}");
            assert!(args.last().unwrap().contains("watch?v=dQw4w9WgXcQ"));
        }
    }

    #[test]
    fn queue_items_survive_a_save_and_load() {
        let item = QueueItem {
            qid: 7,
            video_id: "dQw4w9WgXcQ".into(),
            label: "Canción".into(),
            opts: Opts { mode: "audio".into(), audio_format: "mp3".into(), playlist_id: "PLabcdefghijk".into(), ..Default::default() },
        };
        let text = serde_json::to_string(&vec![item]).unwrap();
        let back: Vec<QueueItem> = serde_json::from_str(&text).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].qid, back[0].video_id.as_str(), back[0].label.as_str()), (7, "dQw4w9WgXcQ", "Canción"));
        assert_eq!(back[0].opts.playlist_id, "PLabcdefghijk");
        // an old file without some option fields still loads (missing ones take their defaults)
        let old: Vec<QueueItem> = serde_json::from_str(r#"[{"qid":1,"video_id":"dQw4w9WgXcQ","label":"x","opts":{"mode":"video"}}]"#).unwrap();
        assert_eq!(old[0].opts.mode, "video");
        assert_eq!(old[0].opts.max_height, 0);
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
        let args = build_args("dQw4w9WgXcQ", &o, (true, None));
        let sep = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[sep + 1], "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(args.len(), sep + 2, "nothing after the URL");
    }
}
