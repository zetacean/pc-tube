use serde_json::{json, Value};
use std::{fs, path::PathBuf};

pub fn data_dir() -> PathBuf {
    let d = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PcTube");
    let _ = fs::create_dir_all(&d);
    d
}

/// Debug builds started with PCTUBE_DEBUG_PORT are test sessions: they use their own settings
/// and window files so they never touch the real ones.
pub fn debug_session() -> bool {
    cfg!(debug_assertions) && std::env::var("PCTUBE_DEBUG_PORT").is_ok()
}

fn path() -> PathBuf {
    data_dir().join(if debug_session() { "config-debug.json" } else { "config.json" })
}

/// Defaults for every setting. Keep in sync with the panel in scripts/ui.js.
pub fn defaults() -> Value {
    json!({
        "enableAdBlock": true,
        "suppressStillWatching": true,

        "enableSponsorBlock": true,
        "enableSponsorBlockToasts": true,
        // per category: "auto" | "manual" | "show" | "off"
        "sponsorBlockModes": {
            "sponsor": "auto",
            "intro": "auto",
            "outro": "auto",
            "interaction": "auto",
            "selfpromo": "auto",
            "preview": "show",
            "filler": "show",
            "music_offtopic": "show",
            "poi_highlight": "show"
        },

        "enableDeArrow": false,
        "enableDeArrowThumbnails": false,
        "deArrowFormatTitles": true,

        "enableMini": true,
        "miniCloseAction": "restore", // ✕ of the mini-player: "restore" the full window | "pause" and close

        // What the close button does: "tray" hides the window in the system tray
        // (playback continues), "exit" quits the app.
        "closeAction": "tray",
        "autostart": false,        // start with Windows, minimized to the tray
        "globalShortcuts": { "playPause": "Ctrl+Alt+Space", "next": "Ctrl+Alt+Right", "prev": "Ctrl+Alt+Left" },
        "shortcuts": {},            // page shortcuts the user changed: { action: "Alt+M" } (defaults in scripts/keys.js)
        "notifyNowPlaying": "hidden", // Windows notification when a video starts: off | hidden | always
        "captionsMode": "default",  // default (YouTube's) | off | lang
        "captionsLang": "es",
        "captionsAuto": true,       // accept auto-generated captions when there are no manual ones
        "captionsTranslate": false, // otherwise translate the first available captions into captionsLang
        "memorySaver": "auto",      // auto | hidden | off: trim memory when hidden (and now and then while in view)
        "globalHotkeys": true,     // Ctrl+Alt+Space / Right / Left work from any app
        "alwaysOnTop": false,      // keep the window above the others

        // Playback
        "preferredQuality": "auto", // auto | 2160 | 1440 | 1080 | 720 | 480 | 360
        "preferredCodec": "noav1",  // any | noav1 | h264
        "rememberSpeed": false,     // keep the playback speed between videos
        "playbackSpeed": 1.0,
        "volumeBoost": 100,         // 100 = off, up to 300 (%)

        "enableDownload": true,
        "downloadAsk": true,          // show the format/quality dialog before downloading
        "downloadMode": "video",      // "video" | "audio"
        "downloadMaxHeight": 1080,    // 0 = best available
        "downloadContainer": "mp4",   // mp4 | mkv | webm
        "downloadAudioFormat": "mp3", // mp3 | m4a | opus | flac | wav
        "downloadAudioQuality": "320K",
        "downloadEmbedMeta": true,        // title, channel and cover inside the file (needs ffmpeg)
        "downloadRemoveSponsors": false,  // cut the SponsorBlock "skip automatically" categories out

        "checkUpdates": true,       // look for a newer release on GitHub at startup
        "customCss": "",            // advanced: user CSS applied on top of youtube.com
        // Appearance (Ajustes -> Apariencia)
        "uiMode": "auto",           // auto (YouTube's own) | dark | light
        "uiTheme": "default",       // dark mode theme: default | oled | graphite | midnight | ocean | forest | violet | wine | custom
        "uiBg": "#0f0f0f",          // custom dark theme: page colour
        "uiSurface": "#1a1a1a",     // custom dark theme: cards and menus
        "uiThemeLight": "default",  // light mode theme: default | paper | sky | mint | rose | lilac | sand | custom
        "uiBgLight": "#ffffff",
        "uiSurfaceLight": "#f2f2f2",
        "uiAccent": "",             // "" = YouTube's own, or "#rrggbb"
        "uiThumbs": "default",      // default | square
        "uiGridCols": 0,            // thumbnails per row on the home page, 0 = automatic
        "uiZoom": 100,              // page zoom in %
        "uiHideComments": false,
        "uiHideRelated": false,
        "uiHideChips": false,
        "uiHideChat": false,
        "uiHidePremium": false,
        "enableFeedFilter": true,
        "filterChannels": "",       // one channel name or @handle per line
        "filterWords": "",          // one word or phrase per line, matched in the title
        "filterPages": ["home", "subscriptions", "search", "watch"],

        "hideShorts": false,
        "hideEndScreenCards": false,
        "hidePaidPromotionOverlay": false,

        "hideWatchedVideos": false,
        "hideWatchedThreshold": 80,
        "hideWatchedPages": ["home", "subscriptions", "search"]
    })
}

/// Loads the stored config merged over the defaults.
pub fn load() -> Value {
    let mut cfg = defaults();
    if let Ok(s) = fs::read_to_string(path()) {
        if let Ok(Value::Object(stored)) = serde_json::from_str::<Value>(&s) {
            for (k, v) in stored {
                cfg[k] = v;
            }
        }
    }
    cfg
}

fn same_kind(a: &Value, b: &Value) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Applies settings from an exported file. Only known keys with the right type are taken
/// (an old or hand-edited file can't inject anything else). Returns how many were applied.
pub fn merge_import(cfg: &mut Value, imported: &Value) -> usize {
    let (Some(defaults), Some(src)) = (defaults().as_object().cloned(), imported.as_object()) else {
        return 0;
    };
    let mut n = 0;
    for (k, v) in src {
        if defaults.get(k).is_some_and(|d| same_kind(d, v)) {
            cfg[k] = v.clone();
            n += 1;
        }
    }
    n
}

pub fn save(cfg: &Value) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path(), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_takes_known_keys_of_the_right_type_only() {
        let mut cfg = defaults();
        let imported = json!({
            "preferredQuality": "720",          // ok
            "volumeBoost": 200,                 // ok
            "autostart": true,                  // ok
            "alwaysOnTop": "yes",               // wrong type (string instead of bool): ignored
            "volumeBoostt": 5,                  // unknown key: ignored
            "evil": { "x": 1 },                 // unknown key: ignored
            "sponsorBlockModes": { "sponsor": "off" } // object, same kind as the default: ok
        });
        let n = merge_import(&mut cfg, &imported);
        assert_eq!(n, 4);
        assert_eq!(cfg["preferredQuality"], "720");
        assert_eq!(cfg["volumeBoost"], 200);
        assert_eq!(cfg["autostart"], true);
        assert_eq!(cfg["alwaysOnTop"], false, "wrong-typed value must not be applied");
        assert!(cfg.get("evil").is_none() && cfg.get("volumeBoostt").is_none());
    }

    #[test]
    fn import_of_a_non_object_changes_nothing() {
        let mut cfg = defaults();
        let before = cfg.clone();
        assert_eq!(merge_import(&mut cfg, &json!([1, 2, 3])), 0);
        assert_eq!(merge_import(&mut cfg, &json!("hola")), 0);
        assert_eq!(cfg, before);
    }
}
