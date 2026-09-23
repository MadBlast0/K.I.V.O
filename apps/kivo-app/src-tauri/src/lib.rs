//! KIVO's desktop app: the Control Center window and the Island overlay. It holds no state
//! of its own: everything comes from `kivo-runtime` over IPC (ARCHITECTURE §1, §8), and closing
//! its window only hides it while KIVO keeps running (UX §1).

mod announce;
mod memory;
mod overlay;
mod runtime;

use kivo_ipc::Link;
use memory::Visibility;
use runtime::{NAVIGATE_EVENT, Runtime};
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

pub(crate) const MAIN: &str = "main";

/// How this process was started (the runtime passes these when it launches the app).
#[derive(Debug, Default, PartialEq, Eq)]
struct Launch {
    /// Start hidden (sign-in autostart, relaunch after a crash).
    background: bool,
    /// Open the Control Center on this page.
    page: Option<String>,
}

impl Launch {
    fn parse(args: &[String]) -> Self {
        let mut launch = Self::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--background" => launch.background = true,
                "--page" => launch.page = args.next().cloned(),
                _ => {}
            }
        }
        launch
    }
}

/// A page the webview should open once it is ready.
#[derive(Default)]
struct PendingPage(Mutex<Option<String>>);

/// Shows, unminimizes and focuses the Control Center, optionally on a page (tray click, a second
/// launch, "Settings" in the tray).
pub(crate) fn show_main_window(app: &AppHandle, page: Option<&str>) {
    if let Some(window) = app.get_webview_window(MAIN) {
        memory::apply(&window, Visibility::Shown, true);
        let _ = window.as_ref().show();
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(page) = page {
        let _ = app.emit(NAVIGATE_EVENT, page);
    }
}

/// What the webview needs when it starts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Boot {
    link: Link,
    page: Option<String>,
}

/// Called by a webview once its listeners are registered, so nothing is missed. Only the main
/// window gets the startup page.
#[tauri::command]
fn ui_ready(
    window: tauri::Window,
    runtime: tauri::State<'_, Runtime>,
    pending: tauri::State<'_, PendingPage>,
) -> Boot {
    Boot {
        link: runtime.link(),
        page: (window.label() == MAIN)
            .then(|| pending.0.lock().unwrap_or_else(|e| e.into_inner()).take())
            .flatten(),
    }
}

/// A request to the runtime (JSON-RPC method and params).
#[tauri::command]
async fn runtime_request(
    runtime: tauri::State<'_, Runtime>,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    runtime
        .request(&method, params.unwrap_or(Value::Null))
        .await
}

/// "Start KIVO" while the runtime isn't running.
#[tauri::command]
fn runtime_start() -> Result<(), String> {
    runtime::start_runtime()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch = Launch::parse(&args);

    tauri::Builder::default()
        // First, so a second KIVO.exe hands over and exits before anything else starts.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let again = Launch::parse(argv.get(1..).unwrap_or_default());
            show_main_window(app, again.page.as_deref());
            // KIVO was quit or crashed while this window stayed: launching KIVO starts it again.
            if !app.state::<Runtime>().connected()
                && let Err(e) = runtime::start_runtime()
            {
                eprintln!("KIVO: couldn't start the runtime: {e}");
            }
        }))
        .manage(Runtime::default())
        .manage(overlay::Overlay::default())
        .manage(PendingPage(Mutex::new(launch.page.clone())))
        .invoke_handler(tauri::generate_handler![
            ui_ready,
            runtime_request,
            runtime_start,
            overlay::overlay_fit,
            overlay::overlay_interactive,
            overlay::overlay_typing_done,
            overlay::overlay_focus,
            overlay::overlay_hover,
            overlay::overlay_drag,
            overlay::island_request,
            announce::announce
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing hides the window and KIVO keeps running (UX §1). With no runtime there
                // is no tray to bring it back, so then the app really closes.
                if window.label() == MAIN && window.app_handle().state::<Runtime>().connected() {
                    api.prevent_close();
                    // A hidden Control Center renders nothing (WebView2 invisible too).
                    if let Some(webview) = window.app_handle().get_webview_window(MAIN) {
                        let _ = webview.as_ref().hide();
                        let _ = window.hide();
                        memory::apply(&webview, Visibility::Hidden, true);
                    }
                    // The runtime decides what closing means: keep running (and, the first time,
                    // say so in a notification), or quit when "keep running" is off (UX-02).
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let runtime = app.state::<Runtime>();
                        if let Err(e) = runtime
                            .request(kivo_ipc::method::UI_WINDOW_CLOSED, Value::Null)
                            .await
                        {
                            eprintln!("KIVO: couldn't report the closed window: {e}");
                        }
                    });
                }
            }
        })
        .setup(move |app| {
            // Preloaded hidden, so the Island appears without loading anything (ARCHITECTURE §1).
            overlay::create(app.handle())?;
            if launch.background {
                // Started at sign-in: the Control Center stays hidden until opened.
                if let Some(window) = app.get_webview_window(MAIN) {
                    memory::apply(&window, Visibility::Hidden, true);
                }
            } else {
                show_main_window(app.handle(), None);
            }
            tauri::async_runtime::spawn(runtime::maintain(app.handle().clone()));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building KIVO")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<Runtime>().stop();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Launch {
        Launch::parse(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn launch_flags_from_the_runtime_are_understood() {
        assert_eq!(parse(&[]), Launch::default());
        assert!(parse(&["--background"]).background);
        assert_eq!(
            parse(&["--page", "settings"]).page.as_deref(),
            Some("settings")
        );
        assert_eq!(parse(&["--page"]).page, None);
    }
}
