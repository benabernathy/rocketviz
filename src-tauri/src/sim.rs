//! Simulation in the desktop app: finding engines, keeping a process pool per engine, the open design, and the
//! analysis that is running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rocketviz_core::analysis::flight_file::FlightFile;
use rocketviz_core::analysis::run::{analyze, AnalysisEvent, AnalysisSettings};
use rocketviz_core::engine::protocol::{ConfigInfo, ParameterDef};
use rocketviz_core::engine::{CancelToken, Design, EnginePool, EngineSpec, Runtimes};
use rocketviz_core::Error;
use serde::Serialize;
use tokio::sync::mpsc;

/// Returned when the user cancels; the frontend recognizes it.
pub const CANCELLED: &str = "cancelled";

#[derive(Debug, Clone, Serialize)]
pub struct EngineEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub extensions: Vec<String>,
    pub dir: String,
}

impl From<&EngineSpec> for EngineEntry {
    fn from(s: &EngineSpec) -> Self {
        let m = &s.manifest;
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            license: m.license.clone(),
            extensions: m.input_formats.iter().flat_map(|f| f.extensions.clone()).collect(),
            dir: s.dir.display().to_string(),
        }
    }
}

/// What the settings screen shows about simulation: engines found, broken manifests, and the Java runtime.
#[derive(Debug, Serialize)]
pub struct EngineReport {
    pub engines: Vec<EngineEntry>,
    pub problems: Vec<String>,
    pub java: Option<String>,
    pub java_error: Option<String>,
}

/// A design open for simulation, as the frontend needs it.
#[derive(Debug, Clone, Serialize)]
pub struct DesignInfo {
    pub name: String,
    pub path: String,
    pub engine: EngineEntry,
    pub configs: Vec<ConfigInfo>,
    pub parameters: Vec<ParameterDef>,
}

struct OpenDesign {
    pool: Arc<EnginePool>,
    design: Design,
}

pub struct Simulation {
    /// Folders searched for engines: each is an engine folder (with engine.json) or a folder of them.
    roots: Vec<PathBuf>,
    runtimes: Mutex<Runtimes>,
    pools: tokio::sync::Mutex<HashMap<String, Arc<EnginePool>>>,
    open: Mutex<Option<OpenDesign>>,
    job: Mutex<Option<CancelToken>>,
}

impl Simulation {
    pub fn new(roots: Vec<PathBuf>, runtimes: Runtimes) -> Self {
        Self {
            roots,
            runtimes: Mutex::new(runtimes),
            pools: Default::default(),
            open: Mutex::new(None),
            job: Mutex::new(None),
        }
    }

    fn discover(&self) -> (Vec<EngineSpec>, Vec<String>) {
        let (mut found, mut problems) = (Vec::new(), Vec::new());
        for root in &self.roots {
            let results = if root.join(rocketviz_core::engine::manifest::MANIFEST_FILE).is_file() {
                vec![EngineSpec::load(root)]
            } else {
                EngineSpec::discover(root)
            };
            for r in results {
                match r {
                    // The first folder providing an engine id wins (user overrides come before bundled engines).
                    Ok(spec) if !found.iter().any(|f: &EngineSpec| f.manifest.id == spec.manifest.id) => {
                        found.push(spec)
                    }
                    Ok(_) => {}
                    Err(e) => problems.push(e.to_string()),
                }
            }
        }
        (found, problems)
    }

    pub fn report(&self) -> EngineReport {
        let (engines, problems) = self.discover();
        let java = self.runtimes.lock().unwrap().java();
        EngineReport {
            engines: engines.iter().map(EngineEntry::from).collect(),
            problems,
            java: java.as_ref().ok().map(|p| p.display().to_string()),
            java_error: java.err().map(|e| e.to_string()),
        }
    }

    /// File extensions some engine can open (for the open dialog's filter).
    pub fn extensions(&self) -> Vec<String> {
        let mut v: Vec<String> =
            self.discover().0.iter().flat_map(|s| EngineEntry::from(s).extensions).map(|e| e.to_lowercase()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Opens a design with the engine that reads its file type (starting that engine's processes the first time),
    /// replacing the design that was open.
    pub async fn open(&self, path: &Path) -> Result<DesignInfo, String> {
        if self.job.lock().unwrap().is_some() {
            return Err("A simulation is running. Cancel it or let it finish first.".into());
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let (engines, problems) = self.discover();
        let Some(spec) = engines.into_iter().find(|s| s.accepts(ext)) else {
            let mut msg = if self.roots.is_empty() {
                "No simulation engines are installed.".to_string()
            } else {
                format!("No simulation engine opens .{ext} files.")
            };
            if !problems.is_empty() {
                msg += &format!(" Engine problems: {}", problems.join("; "));
            }
            if cfg!(debug_assertions) {
                msg += " (Development: build the OpenRocket engine with `./gradlew assembleEngine` in engines/openrocket.)";
            }
            return Err(msg);
        };
        let entry = EngineEntry::from(&spec);
        let pool = self.pool(spec).await?;

        let previous = self.open.lock().unwrap().take();
        if let Some(prev) = previous {
            prev.pool.unload(&prev.design).await;
        }
        let design = pool.load(path).await.map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
        let info = DesignInfo {
            name: design.name.clone(),
            path: design.path.display().to_string(),
            engine: entry,
            configs: design.configs.clone(),
            parameters: pool.info().parameters.clone(),
        };
        *self.open.lock().unwrap() = Some(OpenDesign { pool, design });
        Ok(info)
    }

    async fn pool(&self, spec: EngineSpec) -> Result<Arc<EnginePool>, String> {
        let mut pools = self.pools.lock().await;
        if let Some(p) = pools.get(&spec.manifest.id) {
            return Ok(p.clone());
        }
        let (id, name, size) = (spec.manifest.id.clone(), spec.manifest.name.clone(), spec.recommended_instances());
        let runtimes = self.runtimes.lock().unwrap().clone();
        let pool = EnginePool::start(spec, runtimes, size).await.map_err(|e| match e {
            Error::RuntimeNotFound { ref runtime, .. } if runtime == "java" => format!(
                "{name} needs Java 21 or newer, which wasn't found. Install it (for example Eclipse Temurin 21) or \
                 set JAVA_HOME. ({e})"
            ),
            e => format!("Couldn't start the {name} engine: {e}"),
        })?;
        let pool = Arc::new(pool);
        pools.insert(id, pool.clone());
        Ok(pool)
    }

    /// Runs an analysis of the open design, sending events as it goes. Only one analysis runs at a time.
    pub async fn analyze(
        &self,
        config_ids: &[String],
        settings: &AnalysisSettings,
        events: impl Fn(AnalysisEvent) + Send + 'static,
    ) -> Result<FlightFile, String> {
        let (pool, design) = {
            let open = self.open.lock().unwrap();
            let o = open.as_ref().ok_or("Open a design first.")?;
            (o.pool.clone(), o.design.clone())
        };
        let cancel = {
            let mut job = self.job.lock().unwrap();
            if job.is_some() {
                return Err("A simulation is already running.".into());
            }
            job.insert(CancelToken::new()).clone()
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let forward = tokio::spawn(async move {
            while let Some(e) = rx.recv().await {
                events(e);
            }
        });
        let result = analyze(&pool, &design, config_ids, settings, tx, &cancel).await;
        let _ = forward.await;
        *self.job.lock().unwrap() = None;
        match result {
            Ok(file) => Ok(file),
            Err(_) if cancel.is_cancelled() => Err(CANCELLED.into()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn cancel(&self) {
        if let Some(job) = self.job.lock().unwrap().as_ref() {
            job.cancel();
        }
    }

    /// Stops every engine process (app exit). Processes are also killed when dropped, but asking first lets them
    /// exit cleanly.
    pub async fn shutdown(&self) {
        self.cancel();
        let pools: Vec<_> = self.pools.lock().await.drain().map(|(_, p)| p).collect();
        for p in pools {
            p.shutdown().await;
        }
    }
}
