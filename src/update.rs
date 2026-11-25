//! Updates. `check` asks GitHub's public API for the latest release (read only).
//! `apply` (the "Actualizar ahora" button) downloads that release's installer, verifies it against
//! the SHA256SUMS.txt published next to it, and runs it silently while the app quits; the installer
//! starts the new version when it finishes. Only the installed copy updates itself: a portable copy
//! has nothing to replace, so there the button just opens the release page.

use crate::ipc::{Emit, Out, TrayAction};
use pc_tube::http::{download_to, expected_hash, get_text, http_agent};
use serde_json::{json, Value};
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const REPO: &str = "zetacean/pc-tube";
const CACHE_FOR: Duration = Duration::from_secs(6 * 3600);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static CACHE: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
static BUSY: Mutex<bool> = Mutex::new(false);

/// "v0.10.1" / "0.2.0" -> (0, 10, 1). Anything else (pre-releases, junk) is not comparable.
fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.trim().trim_start_matches('v').split('.');
    let mut next = || it.next()?.parse::<u32>().ok();
    let v = (next()?, next()?, next()?);
    it.next().is_none().then_some(v)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The version this build reports. Test sessions can pretend to be older (PCTUBE_FAKE_VERSION).
fn current_version() -> String {
    if crate::config::debug_session() {
        if let Ok(v) = std::env::var("PCTUBE_FAKE_VERSION") {
            return v;
        }
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// True when this exe was put in place by the installer (Inno Setup leaves unins000.exe beside it).
pub fn is_installed_copy() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("unins000.exe").is_file()))
        .unwrap_or(false)
}

fn release_url_ok(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/")) && !url.contains("..")
}

/// "PC-Tube-0.4.0-setup.exe": digits and dots only between the fixed parts.
fn setup_name_ok(name: &str) -> bool {
    name.strip_prefix("PC-Tube-")
        .and_then(|r| r.strip_suffix("-setup.exe"))
        .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
}

fn fetch_latest() -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = agent
        .get(&url)
        .header("User-Agent", "PC-Tube")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err("todavía no hay versiones publicadas".into());
    }
    if status >= 400 {
        return Err(format!("GitHub respondió {status}"));
    }
    let v: Value = serde_json::from_str(&resp.body_mut().read_to_string().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("respuesta inesperada")?.to_string();
    let page = v["html_url"].as_str().unwrap_or_default().to_string();
    let notes: String = v["body"].as_str().unwrap_or_default().chars().take(1500).collect();
    let current = current_version();

    let asset = |pred: &dyn Fn(&str) -> bool| -> Option<(String, String)> {
        v["assets"].as_array()?.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            let url = a["browser_download_url"].as_str()?;
            (pred(name) && release_url_ok(url)).then(|| (name.to_string(), url.to_string()))
        })
    };
    let setup = asset(&setup_name_ok);
    let sums = asset(&|n| n == "SHA256SUMS.txt");

    Ok(json!({
        "current": current,
        "latest": tag.trim_start_matches('v'),
        "url": page,
        "notes": notes,
        "newer": is_newer(&tag, &current),
        "installed": is_installed_copy(),
        "canApply": setup.is_some() && sums.is_some(),
        "setupName": setup.as_ref().map(|s| s.0.clone()),
        "setupUrl": setup.map(|s| s.1),
        "sumsUrl": sums.map(|s| s.1),
    }))
}

/// Result is cached for a few hours unless `force` (the "check now" button).
pub fn check(force: bool) -> Result<Value, String> {
    if !force {
        if let Some((t, v)) = CACHE.lock().unwrap().as_ref() {
            if t.elapsed() < CACHE_FOR {
                return Ok(v.clone());
            }
        }
    }
    let v = fetch_latest()?;
    *CACHE.lock().unwrap() = Some((Instant::now(), v.clone()));
    Ok(v)
}

/// Opens the release page found by the last check in the default browser. The address comes from
/// our own cache (never from the page) and must be this project's releases page.
pub fn open_release_page() {
    let url = CACHE
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|(_, v)| v["url"].as_str().map(str::to_string))
        .filter(|u| u.starts_with(&format!("https://github.com/{REPO}/releases")));
    if let Some(url) = url {
        let _ = Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

fn ev(emit: &Emit, state: &str, msg: &str, pct: Option<u32>) {
    let payload = json!({ "state": state, "msg": msg, "pct": pct });
    emit(Out::Eval(format!("window.pctube&&window.pctube._update&&window.pctube._update({payload})")));
}

/// Downloads the installer of the cached "newer" release, verifies it and (unless `dry`) starts it
/// and quits the app. Runs on its own thread.
pub fn apply(emit: Emit, dry: bool) {
    {
        let mut busy = BUSY.lock().unwrap();
        if *busy {
            return;
        }
        *busy = true;
    }
    std::thread::spawn(move || {
        let r = apply_inner(&emit, dry);
        *BUSY.lock().unwrap() = false;
        if let Err(e) = r {
            ev(&emit, "error", &e, None);
        }
    });
}

fn apply_inner(emit: &Emit, dry: bool) -> Result<(), String> {
    let info = CACHE.lock().unwrap().as_ref().map(|(_, v)| v.clone()).ok_or("busca primero si hay una versión nueva")?;
    if !info["newer"].as_bool().unwrap_or(false) {
        return Err("ya tienes la última versión".into());
    }
    if !dry && !is_installed_copy() {
        return Err("esta copia es portátil: instala la versión desde la página de descargas".into());
    }
    let (Some(name), Some(setup_url), Some(sums_url)) = (
        info["setupName"].as_str(),
        info["setupUrl"].as_str(),
        info["sumsUrl"].as_str(),
    ) else {
        return Err("la versión nueva no trae instalador".into());
    };
    if !setup_name_ok(name) || !release_url_ok(setup_url) || !release_url_ok(sums_url) {
        return Err("dirección de descarga no válida".into());
    }

    let agent = http_agent();
    ev(emit, "working", "Comprobando la versión…", Some(0));
    let expected = expected_hash(&get_text(&agent, sums_url)?, name).ok_or("no se encontró la suma de verificación del instalador")?;

    let dir: PathBuf = std::env::temp_dir().join("PcTube-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let setup = dir.join(name);
    let got = download_to(&agent, setup_url, &setup, 200 << 20, |pct| ev(emit, "working", "Descargando la actualización…", Some(pct)))?;
    if got != expected {
        let _ = std::fs::remove_file(&setup);
        return Err("el instalador descargado no coincide con su suma de verificación: no se instala".into());
    }
    if dry {
        ev(emit, "verified", "Descarga verificada (prueba: no se instala)", Some(100));
        return Ok(());
    }

    // The download manager is part of the installation: it must close before its files are replaced
    // (what was running stays in its saved queue).
    ev(emit, "installing", "Cerrando el gestor de descargas…", Some(100));
    crate::dm::quit_and_wait();

    // The installer is started after a pause so that this process (and its single-instance
    // mutex) is gone first. It reopens the app when it finishes (see installer/pctube.iss).
    let script = dir.join("update.cmd");
    let body = format!(
        "@echo off\r\nping -n 4 127.0.0.1 >nul\r\n\"{}\" /SILENT /SUPPRESSMSGBOXES /NORESTART\r\n",
        setup.display()
    );
    std::fs::write(&script, body).map_err(|e| e.to_string())?;
    Command::new("cmd")
        .arg("/C")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | 0x0000_0200)
        .spawn()
        .map_err(|e| format!("no se pudo iniciar el instalador: {e}"))?;
    ev(emit, "installing", "Instalando… PC Tube se reiniciará solo", Some(100));
    std::thread::sleep(Duration::from_millis(800));
    emit(Out::Tray(TrayAction::Quit));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(!is_newer("0.3.0-beta.1", "0.2.0"), "pre-releases are ignored");
        assert!(!is_newer("nightly", "0.2.0"));
    }

    #[test]
    fn only_expected_installer_names_and_urls_pass() {
        assert!(setup_name_ok("PC-Tube-0.4.0-setup.exe"));
        assert!(!setup_name_ok("PC-Tube-0.4.0-win64.zip"));
        assert!(!setup_name_ok("PC-Tube--setup.exe"));
        assert!(!setup_name_ok("PC-Tube-0.4.0 & calc-setup.exe"));
        assert!(!setup_name_ok("..\\PC-Tube-0.4.0-setup.exe"));
        assert!(release_url_ok("https://github.com/zetacean/pc-tube/releases/download/v0.4.0/PC-Tube-0.4.0-setup.exe"));
        assert!(!release_url_ok("https://github.com/evil/pc-tube/releases/download/v0.4.0/x.exe"));
        assert!(!release_url_ok("https://github.com/zetacean/pc-tube/releases/download/../../x"));
        assert!(!release_url_ok("http://github.com/zetacean/pc-tube/releases/download/v1/x.exe"));
    }
}
