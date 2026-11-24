//! "Now playing" Windows notifications (toasts) with the video's thumbnail.
//! An unpackaged app needs an AppUserModelID registered under HKCU for its toasts to carry its
//! own name and icon; that is done once, at the first notification.

use std::{path::PathBuf, sync::Once};
use windows::{
    core::HSTRING,
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{ToastNotification, ToastNotificationManager},
};

const AUMID: &str = "PcTube.App";
static REGISTER: Once = Once::new();

fn register_aumid() {
    REGISTER.call_once(|| {
        let icon: PathBuf = crate::config::data_dir().join("toast-icon.png");
        let _ = std::fs::write(&icon, include_bytes!("../assets/icon.png"));
        if let Ok((key, _)) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(format!(r"Software\Classes\AppUserModelId\{AUMID}"))
        {
            let _ = key.set_value("DisplayName", &"PC Tube");
            let _ = key.set_value("IconUri", &icon.to_string_lossy().to_string());
        }
    });
}

fn xml_escape(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(160)
        .flat_map(|c| match c {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&apos;".chars().collect(),
            c => vec![c],
        })
        .collect()
}

fn valid_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The thumbnail as a local file (toasts of unpackaged apps can't use web images).
fn thumbnail(video_id: &str) -> Option<PathBuf> {
    if !valid_video_id(video_id) {
        return None;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(4)))
        .build()
        .into();
    let mut resp = agent.get(&format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")).call().ok()?;
    let bytes = resp.body_mut().with_config().limit(2 << 20).read_to_vec().ok()?;
    let path = crate::config::data_dir().join("toast-thumb.jpg");
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Builds the toast XML. `image` is a local path.
pub fn toast_xml(title: &str, channel: &str, image: Option<&str>) -> String {
    let img = image
        .map(|p| format!(r#"<image placement="appLogoOverride" src="{}"/>"#, xml_escape(p)))
        .unwrap_or_default();
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text>{}</binding></visual><audio silent="true"/></toast>"#,
        xml_escape(title),
        xml_escape(channel),
        img
    )
}

fn show(title: &str, channel: &str, video_id: &str) -> windows::core::Result<()> {
    register_aumid();
    let thumb = thumbnail(video_id);
    let xml = toast_xml(title, channel, thumb.as_ref().map(|p| p.to_string_lossy()).as_deref());
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    // Same tag: the new notification replaces the previous one instead of piling up.
    toast.SetTag(&HSTRING::from("pctube-now-playing"))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
}

/// Shows "now playing" without blocking the caller (the thumbnail is downloaded first).
pub fn now_playing(title: String, channel: String, video_id: String) {
    std::thread::spawn(move || {
        if let Err(e) = show(&title, &channel, &video_id) {
            eprintln!("notificación: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_so_a_title_cannot_inject_xml() {
        let x = toast_xml("A <b>&\"'</b>", "Canal", None);
        assert!(x.contains("A &lt;b&gt;&amp;&quot;&apos;&lt;/b&gt;"));
        assert!(!x.contains("<b>"));
        assert!(!x.contains("<image"));
    }

    /// Manual check (shows a real notification): cargo test notify_real -- --ignored --nocapture
    #[test]
    #[ignore]
    fn notify_real() {
        let r = show("Prueba de PC Tube", "Si ves esto, las notificaciones funcionan", "jNQXAC9IVRw");
        println!("show -> {r:?}");
        r.unwrap();
        std::thread::sleep(std::time::Duration::from_secs(8));
    }

    #[test]
    fn video_id_is_checked_before_building_the_thumbnail_url() {
        assert!(valid_video_id("dQw4w9WgXcQ"));
        assert!(!valid_video_id("../../etc/x"));
        assert!(!valid_video_id("short"));
    }
}
