//! Small HTTP helpers shared by the tool installer (yt-dlp, ffmpeg) and the app updater.

use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .into()
}

pub fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

/// Streams `url` into `dest`, returns the SHA-256 (hex). `on_pct` gets 0..=100 when known.
pub fn download_to(
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

/// The hash listed for `file` in a SHA256SUMS-style text.
pub fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut p = l.split_whitespace();
        let hash = p.next()?;
        let name = p.next()?.trim_start_matches('*');
        (name == file).then(|| hash.to_lowercase())
    })
}
