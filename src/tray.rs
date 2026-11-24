//! System tray icon: play/pause, next, show, close. The menu text follows what is playing.

use crate::ipc::{Out, TrayAction};
use std::sync::Mutex;
use tao::event_loop::EventLoopProxy;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

const ICON_RGBA: &[u8] = include_bytes!("../assets/icon-64.rgba");
const IDLE: &str = "Nada en reproducción";

pub struct Tray {
    icon: TrayIcon,
    now: MenuItem,
    play: MenuItem,
}

impl Tray {
    pub fn new(proxy: EventLoopProxy<Out>) -> Result<Self, String> {
        let icon = Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).map_err(|e| e.to_string())?;

        let now = MenuItem::new(IDLE, false, None);
        let play = MenuItem::new("Reproducir / Pausar", true, None);
        let next = MenuItem::new("Siguiente", true, None);
        let show = MenuItem::new("Mostrar PC Tube", true, None);
        let quit = MenuItem::new("Cerrar", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &now,
            &PredefinedMenuItem::separator(),
            &play,
            &next,
            &PredefinedMenuItem::separator(),
            &show,
            &quit,
        ])
        .map_err(|e| e.to_string())?;

        // The handlers must be Send + Sync; the proxy is only Send, hence the Mutex.
        let proxy = std::sync::Arc::new(Mutex::new(proxy));
        let send = {
            let proxy = proxy.clone();
            move |a: TrayAction| {
                let _ = proxy.lock().unwrap().send_event(Out::Tray(a));
            }
        };

        let (id_play, id_next, id_show, id_quit) =
            (play.id().clone(), next.id().clone(), show.id().clone(), quit.id().clone());
        let send_menu = send.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let action = if e.id == id_play {
                TrayAction::PlayPause
            } else if e.id == id_next {
                TrayAction::Next
            } else if e.id == id_show {
                TrayAction::Show
            } else if e.id == id_quit {
                TrayAction::Quit
            } else {
                return;
            };
            send_menu(action);
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                send(TrayAction::Show);
            }
        }));

        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false) // left click shows the window, right click the menu
            .with_tooltip("PC Tube")
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self { icon, now, play })
    }

    /// Reflects what the page is playing. Empty title = nothing playing.
    pub fn update(&self, title: &str, paused: bool) {
        let short: String = if title.chars().count() > 56 {
            title.chars().take(55).collect::<String>() + "…"
        } else {
            title.to_string()
        };
        if title.is_empty() {
            self.now.set_text(IDLE);
            self.play.set_text("Reproducir / Pausar");
            let _ = self.icon.set_tooltip(Some("PC Tube"));
        } else {
            self.now.set_text(&short);
            self.play.set_text(if paused { "Reproducir" } else { "Pausar" });
            let tip = format!("PC Tube — {short}");
            let _ = self.icon.set_tooltip(Some(tip));
        }
    }
}
