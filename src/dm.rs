//! PC Tube's link to PC Tube Descargas, the download manager (see `pc_tube::dmclient`). Downloads
//! are not made in this process any more: PC Tube asks the manager, which keeps working when this
//! window is closed, and its progress comes back here as events for the page.

use crate::ipc::Out;
use pc_tube::dmclient::Client;
use serde_json::Value;
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tao::event_loop::EventLoopProxy;

static CLIENT: OnceLock<Client> = OnceLock::new();

/// Must be called once at startup: events from the manager are passed to the page through `proxy`.
pub fn init(proxy: EventLoopProxy<Out>) {
    let proxy = Mutex::new(proxy); // the proxy is Send but not Sync
    let _ = CLIENT.set(Client::new(move |ev| {
        let js = format!("window.pctube&&window.pctube._download&&window.pctube._download({ev})");
        let _ = proxy.lock().unwrap().send_event(Out::Eval(js));
    }));
}

fn client() -> Result<&'static Client, String> {
    CLIENT.get().ok_or_else(|| "el gestor de descargas no está inicializado".to_string())
}

/// Runs a command in the manager, starting the manager first when it is not running.
pub fn call(cmd: &str, args: Value, timeout: Duration) -> Result<Value, String> {
    client()?.call(cmd, args, true, timeout)
}

/// Like `call`, but never starts the manager (for things that only matter while it is running).
pub fn try_call(cmd: &str, args: Value) -> Result<Value, String> {
    client()?.call(cmd, args, false, Duration::from_secs(5))
}

/// Joins a running manager, if there is one (so that its queue and progress show up in the page).
pub fn connect_if_running() {
    if let Ok(c) = client() {
        let _ = c.call("ping", Value::Null, false, Duration::from_secs(3));
    }
}

/// Shows the manager's window, starting the manager if needed.
pub fn open_window() -> Result<(), String> {
    client()?.open_window()
}

/// Asks the manager to close and waits a moment (before an update replaces its files).
pub fn quit_and_wait() {
    if let Ok(c) = client() {
        c.quit_and_wait();
    }
}
