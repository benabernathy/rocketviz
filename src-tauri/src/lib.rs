mod settings;
mod sim;
mod tiles;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rocketviz_core::analysis::flight_file::FlightFile;
use rocketviz_core::analysis::run::{AnalysisEvent, AnalysisSettings};
use rocketviz_core::engine::Runtimes;
use rocketviz_core::export::kml::to_kml;
use rocketviz_core::montecarlo::{default_dispersions as core_default_dispersions, ParamDispersion};
use rocketviz_core::tiles::{builtin_providers, Provider, TileProxy};
use settings::SettingsFile;
use sim::{DesignInfo, EngineReport, Simulation};
use tauri::ipc::Channel;
use tauri::{Manager, State};

/// The flight shown in the app (opened, or the result of the last analysis): what Save and Export KML write.
#[derive(Default)]
struct CurrentFlight(Mutex<Option<FlightFile>>);

impl CurrentFlight {
    fn set(&self, file: FlightFile) {
        *self.0.lock().unwrap() = Some(file);
    }

    fn with<T>(&self, f: impl FnOnce(&FlightFile) -> Result<T, String>) -> Result<T, String> {
        f(self.0.lock().unwrap().as_ref().ok_or("No flight is open.")?)
    }
}

/// Reads a flight file (.rocketviz.json) chosen in the open dialog and makes it the current flight.
#[tauri::command]
fn read_flight(path: String, flight: State<'_, CurrentFlight>) -> Result<String, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    flight.set(FlightFile::from_json(&text).map_err(|e| format!("{path}: {e}"))?);
    Ok(text)
}

/// Makes a flight the frontend loaded itself (the bundled sample, a dropped file) the current flight.
#[tauri::command]
fn adopt_flight(text: String, flight: State<'_, CurrentFlight>) -> Result<(), String> {
    flight.set(FlightFile::from_json(&text)?);
    Ok(())
}

#[tauri::command]
fn save_flight(path: String, flight: State<'_, CurrentFlight>) -> Result<(), String> {
    let json = flight.with(|f| f.to_json().map_err(|e| e.to_string()))?;
    write_file(&path, json.as_bytes())
}

#[tauri::command]
fn export_kml(path: String, flight: State<'_, CurrentFlight>) -> Result<(), String> {
    let kml = flight.with(|f| Ok(to_kml(f)))?;
    write_file(&path, kml.as_bytes())
}

/// Writes through a temporary file so a failed save never leaves a half-written file behind.
fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    let path = Path::new(path);
    let tmp = path.with_extension("rocketviz-tmp");
    std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Couldn't save {}: {e}", path.display())
    })
}

#[tauri::command]
fn engine_report(sim: State<'_, Arc<Simulation>>) -> EngineReport {
    sim.report()
}

/// File extensions the installed engines open (for the open dialog).
#[tauri::command]
fn design_extensions(sim: State<'_, Arc<Simulation>>) -> Vec<String> {
    sim.extensions()
}

#[tauri::command]
async fn open_design(path: String, sim: State<'_, Arc<Simulation>>) -> Result<DesignInfo, String> {
    sim.open(Path::new(&path)).await
}

/// rocketviz's standard Monte Carlo dispersions for a nominal wind speed (the starting point for customizing them).
#[tauri::command]
fn default_dispersions(wind_speed: f64) -> Vec<ParamDispersion> {
    core_default_dispersions(wind_speed)
}

/// Analyzes the open design, streaming progress to `on_event`; returns the flight file JSON and makes it current.
#[tauri::command]
async fn run_analysis(
    config_ids: Vec<String>,
    settings: AnalysisSettings,
    on_event: Channel<AnalysisEvent>,
    sim: State<'_, Arc<Simulation>>,
    flight: State<'_, CurrentFlight>,
) -> Result<String, String> {
    let file = sim
        .analyze(&config_ids, &settings, move |e| {
            let _ = on_event.send(e);
        })
        .await?;
    let json = file.to_json().map_err(|e| e.to_string())?;
    flight.set(file);
    Ok(json)
}

#[tauri::command]
fn cancel_analysis(sim: State<'_, Arc<Simulation>>) {
    sim.cancel();
}

/// Imagery providers for the settings screen (no keys exposed, only whether one is set).
#[tauri::command]
fn imagery_providers(proxy: State<'_, Arc<TileProxy>>) -> Vec<serde_json::Value> {
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

/// Checks a provider API key against the provider, then saves it. An empty key removes the saved one.
#[tauri::command]
async fn set_imagery_key(
    provider: String,
    key: String,
    proxy: State<'_, Arc<TileProxy>>,
    settings: State<'_, SettingsFile>,
) -> Result<(), String> {
    let key = key.trim().to_string();
    if !key.is_empty() {
        proxy.check_key(&provider, &key).await.map_err(|e| format!("The key didn't work: {e}"))?;
    }
    proxy.set_key(&provider, &key);
    let mut s = settings.load();
    if key.is_empty() {
        s.imagery_keys.remove(&provider);
    } else {
        s.imagery_keys.insert(provider, key);
    }
    settings.save(&s).map_err(|e| format!("Couldn't save settings: {e}"))
}

/// True when launched with ROCKETVIZ_SMOKE=1: the frontend runs its self-test and reports back.
#[tauri::command]
fn smoke_enabled() -> bool {
    std::env::var("ROCKETVIZ_SMOKE").is_ok_and(|v| v == "1")
}

/// A design for the self-test to simulate (ROCKETVIZ_SMOKE_DESIGN), and a folder to save its results in.
#[tauri::command]
fn smoke_design() -> Option<serde_json::Value> {
    let design = std::env::var("ROCKETVIZ_SMOKE_DESIGN").ok().filter(|p| !p.is_empty())?;
    let out = std::env::temp_dir().join("rocketviz-smoke");
    std::fs::create_dir_all(&out).ok()?;
    Some(serde_json::json!({ "design": design, "out_dir": out }))
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

/// Where engines are looked for, first match wins: ROCKETVIZ_ENGINES (a path list), the app's bundled engines,
/// and in development builds the OpenRocket engine built in this repository.
fn engine_roots(resources: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> =
        std::env::var_os("ROCKETVIZ_ENGINES").map(|v| std::env::split_paths(&v).collect()).unwrap_or_default();
    if let Some(r) = resources {
        roots.push(r.join("engines"));
    }
    if cfg!(debug_assertions) {
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../engines/openrocket/build/engine"));
    }
    roots.retain(|r| r.is_dir());
    roots
}

/// Java for Java engines: the user's choice, else the app's bundled runtime, else JAVA_HOME / PATH.
fn runtimes(java_setting: Option<PathBuf>, resources: Option<&Path>) -> Runtimes {
    let bundled = resources
        .map(|r| r.join("jre").join("bin").join(if cfg!(windows) { "java.exe" } else { "java" }))
        .filter(|p| p.is_file());
    Runtimes { java: java_setting.or(bundled), python: None }
}

pub fn run() {
    // WebKitGTK's DMABUF renderer breaks WebGL on NVIDIA's proprietary drivers; opt out unless the user chose.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let settings = SettingsFile::new(&app.path().app_config_dir()?);
            let saved = settings.load();
            let resources = app.path().resource_dir().ok();

            let proxy = Arc::new(TileProxy::new(app.path().app_cache_dir()?.join("tiles"), builtin_providers()));
            for (provider, key) in &saved.imagery_keys {
                proxy.set_key(provider, key);
            }
            // For development and testing without saving a key.
            if let Ok(key) = std::env::var("ROCKETVIZ_ESRI_KEY") {
                proxy.set_key("esri", &key);
            }

            let roots = engine_roots(resources.as_deref());
            let sim = Simulation::new(roots, runtimes(saved.java_path.clone(), resources.as_deref()));
            app.manage(proxy);
            app.manage(Arc::new(sim));
            app.manage(settings);
            app.manage(CurrentFlight::default());
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("tiles", |ctx, request, responder| {
            let proxy = ctx.app_handle().state::<Arc<TileProxy>>().inner().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(tiles::tile_response(&proxy, &path).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            read_flight,
            adopt_flight,
            save_flight,
            export_kml,
            engine_report,
            design_extensions,
            open_design,
            default_dispersions,
            run_analysis,
            cancel_analysis,
            imagery_providers,
            set_imagery_key,
            smoke_enabled,
            smoke_design,
            smoke_report,
        ])
        .build(tauri::generate_context!())
        .expect("error while building rocketviz");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            let sim = app.state::<Arc<Simulation>>().inner().clone();
            tauri::async_runtime::block_on(sim.shutdown());
        }
    });
}
