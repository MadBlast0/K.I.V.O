fn main() {
    // Only these commands exist for the webview, and each window's capability file must grant
    // them one by one (SECURITY §9).
    let manifest = tauri_build::AppManifest::new().commands(&[
        "ui_ready",
        "runtime_request",
        "runtime_start",
        "overlay_fit",
        "overlay_interactive",
        "overlay_typing_done",
        "overlay_focus",
        "overlay_hover",
        "overlay_drag",
        "island_request",
        "announce",
        "window_effects",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to run the Tauri build script");
}
