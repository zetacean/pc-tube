//! "Is there a newer version?": asks GitHub's public API for the latest release of the project.
//! It only reads; installing stays a manual step (the page opens the release in the browser).

use serde_json::{json, Value};
use std::{
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const REPO: &str = "zetacean/pc-tube";
const CACHE_FOR: Duration = Duration::from_secs(6 * 3600);

static CACHE: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

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
    let current = env!("CARGO_PKG_VERSION");
    Ok(json!({
        "current": current,
        "latest": tag.trim_start_matches('v'),
        "url": page,
        "notes": notes,
        "newer": is_newer(&tag, current),
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
        use std::os::windows::process::CommandExt;
        let _ = Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn();
    }
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
}
