/// Reads a flight file (.rocketviz.json) chosen by the user in the native open dialog.
#[tauri::command]
fn read_flight(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

/// True when launched with ROCKETVIZ_SMOKE=1: the frontend runs its self-test and reports back.
#[tauri::command]
fn smoke_enabled() -> bool {
    std::env::var("ROCKETVIZ_SMOKE").is_ok_and(|v| v == "1")
}

/// Prints the frontend's self-test report, saves it to the temp dir (Windows GUI builds have no console),
/// and exits non-zero if the test failed.
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, report: String, ok: bool) {
    let path = std::env::temp_dir().join("rocketviz-smoke.txt");
    let saved = match std::fs::write(&path, &report) {
        Ok(()) => format!("Report saved to {}", path.display()),
        Err(e) => format!("Could not save report to {}: {e}", path.display()),
    };
    println!("{report}\n{saved}");
    app.exit(if ok { 0 } else { 1 });
}

pub fn run() {
    // WebKitGTK's DMABUF renderer breaks WebGL on NVIDIA's proprietary drivers; opt out unless the user chose.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_flight, smoke_enabled, smoke_report])
        .run(tauri::generate_context!())
        .expect("error while running rocketviz");
}
