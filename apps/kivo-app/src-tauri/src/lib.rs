//! KIVO's settings and control window. The always-on work (audio, wake word, tools) lives in
//! `kivo-runtime`; this process is only the UI and talks to the runtime over IPC.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running KIVO");
}
