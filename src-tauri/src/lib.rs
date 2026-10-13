use std::sync::Arc;

use rocketviz_core::tiles::{builtin_providers, Provider, TileError, TileProxy};
use tauri::http::{header, Response, StatusCode};
use tauri::Manager;

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

/// Imagery providers for the map settings (no keys exposed, only whether one is set).
#[tauri::command]
fn imagery_providers(proxy: tauri::State<'_, Arc<TileProxy>>) -> Vec<serde_json::Value> {
    proxy
        .providers()
        .into_iter()
        .map(|p: &Provider| {
            serde_json::json!({
                "id": p.id, "name": p.name, "attribution": p.attribution, "max_zoom": p.max_zoom,
                "coverage": p.coverage, "keyed": p.keyed_url.is_some(), "has_key": proxy.has_key(&p.id),
            })
        })
        .collect()
}

/// Serves `tiles://localhost/<provider>/<z>/<x>/<y>` (on Windows `http://tiles.localhost/...`) from the tile proxy.
async fn tile_response(proxy: &TileProxy, path: &str) -> Response<Vec<u8>> {
    let respond = |status: StatusCode, content_type: &str, max_age: u64, body: Vec<u8>| {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, format!("max-age={max_age}"))
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body)
            .expect("valid response")
    };
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let parsed = match parts.as_slice() {
        [provider, z, x, y] => {
            match (z.parse(), x.parse(), y.trim_end_matches(|c: char| !c.is_ascii_digit()).parse()) {
                (Ok(z), Ok(x), Ok(y)) => Some((*provider, z, x, y)),
                _ => None,
            }
        }
        _ => None,
    };
    let Some((provider, z, x, y)) = parsed else {
        return respond(StatusCode::BAD_REQUEST, "text/plain", 0, format!("bad tile path {path}").into_bytes());
    };
    match proxy.tile(provider, z, x, y).await {
        Ok(t) => respond(StatusCode::OK, &t.content_type, t.max_age, t.bytes),
        Err(e) => {
            let status = match e {
                TileError::UnknownProvider(_) | TileError::OutOfRange { .. } => StatusCode::NOT_FOUND,
                TileError::Upstream { status: 404, .. } => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_GATEWAY,
            };
            log_tile_error(&e);
            respond(status, "text/plain", 0, e.to_string().into_bytes())
        }
    }
}

fn log_tile_error(e: &TileError) {
    eprintln!("rocketviz tiles: {e}");
}

pub fn run() {
    // WebKitGTK's DMABUF renderer breaks WebGL on NVIDIA's proprietary drivers; opt out unless the user chose.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache = app.path().app_cache_dir()?.join("tiles");
            let proxy = Arc::new(TileProxy::new(cache, builtin_providers()));
            // Until the settings UI exists: an ArcGIS Location Platform key from the environment.
            if let Ok(key) = std::env::var("ROCKETVIZ_ESRI_KEY") {
                proxy.set_key("esri", &key);
            }
            app.manage(proxy);
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("tiles", |ctx, request, responder| {
            let proxy = ctx.app_handle().state::<Arc<TileProxy>>().inner().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(tile_response(&proxy, &path).await);
            });
        })
        .invoke_handler(tauri::generate_handler![read_flight, smoke_enabled, smoke_report, imagery_providers])
        .run(tauri::generate_context!())
        .expect("error while running rocketviz");
}
