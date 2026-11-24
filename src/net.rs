use std::time::Duration;

/// Only these hosts may be fetched through the Rust proxy (avoids CORS in the page,
/// and keeps the bridge from being an open proxy).
const ALLOWED_HOSTS: &[&str] = &["sponsor.ajay.app", "dearrow-thumb.ajay.app"];

pub fn fetch(url: &str) -> Result<String, String> {
    let host = url
        .strip_prefix("https://")
        .and_then(|r| r.split(['/', '?']).next())
        .ok_or("only https urls are allowed")?;
    if !ALLOWED_HOSTS.contains(&host) {
        return Err(format!("host not allowed: {host}"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if status >= 400 {
        return Err(format!("{status}"));
    }
    Ok(body)
}
