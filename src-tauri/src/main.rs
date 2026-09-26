// The desktop app: a window around vidcrop-core. The web page and the HTTP API both
// send commands to the same `Core`; state changes are pushed to the page as "state" events.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use vidcrop_core::session::{Core, Host};

struct TauriHost {
    app: AppHandle,
    recording: AtomicBool,
}

impl Host for TauriHost {
    fn changed(&self, state: &Value) {
        if let Some(path) = state["file"]["path"].as_str() {
            // Let the <video> element load this one file through the asset protocol.
            let _ = self.app.asset_protocol_scope().allow_file(path);
        }
        let recording = !state["recording"].is_null();
        if recording != self.recording.swap(recording, Ordering::SeqCst) {
            recording_changed(&self.app, recording);
        }
        let _ = self.app.emit("state", state);
    }

    fn ui(&self, cmd: &str, args: &Value) -> Result<(), String> {
        let _ = cmd;
        self.app.emit("ui", args).map_err(|e| e.to_string())
    }
}

/// Hides the window while recording, shows the tray's Stop item, and brings the window back after.
fn recording_changed(app: &AppHandle, recording: bool) {
    let win = app.get_webview_window("main");
    if let Some(tray) = app.tray_by_id("rec") {
        let _ = tray.set_visible(recording);
    }
    if let Some(w) = win {
        if recording {
            let _ = w.minimize();
        } else {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
    if recording {
        let app = app.clone();
        std::thread::spawn(move || loop {
            let Some(core) = app.try_state::<Arc<Core>>() else { return };
            let secs = core.state()["recording"]["seconds"].as_f64();
            let Some(tray) = app.tray_by_id("rec") else { return };
            match secs {
                Some(s) => {
                    let _ = tray.set_title(Some(format!("REC {}:{:02}", s as u64 / 60, s as u64 % 60)));
                }
                None => return,
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        });
    }
}

/// The page's only way into the core: the same commands the HTTP API takes.
#[tauri::command]
async fn cmd(core: State<'_, Arc<Core>>, command: Value) -> Result<Value, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.exec_json(command)).await.map_err(|e| e.to_string())?
}

/// Where the API is listening, so the page can show it.
#[tauri::command]
fn api_info(info: State<'_, ApiState>) -> Value {
    serde_json::json!({ "url": info.0, "file": info.1 })
}

struct ApiState(String, String);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![cmd, api_info])
        .setup(|app| {
            let core = Core::new(Box::new(TauriHost { app: app.handle().clone(), recording: AtomicBool::new(false) }));
            app.manage(core.clone());

            let (url, file) = match vidcrop_core::api::start(core.clone(), 0, &vidcrop_core::api::default_api_file()) {
                Ok(i) => (i.url, i.file.to_string_lossy().into_owned()),
                Err(e) => (String::new(), e),
            };
            app.manage(ApiState(url, file));

            let stop = MenuItem::with_id(app, "stop", "Stop recording", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&stop])?;
            let tray = TrayIconBuilder::with_id("rec")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("VidCrop is recording")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| {
                    if event.id == "stop" {
                        let core = app.state::<Arc<Core>>().inner().clone();
                        let app = app.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = core.exec_json(serde_json::json!({ "cmd": "record_stop" })) {
                                let _ = app.emit("error", e);
                            }
                        });
                    }
                })
                .build(app)?;
            tray.set_visible(false)?;

            // `vidcrop-app somefile.mp4` opens it right away.
            if let Some(path) = std::env::args().nth(1).filter(|a| !a.starts_with('-')) {
                let _ = core.exec_json(serde_json::json!({ "cmd": "open", "path": path }));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running VidCrop");
}
