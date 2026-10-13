//! A pool of engine processes for one engine.
//!
//! Engines that declare `multi_instance` (OpenRocket: it scales across processes, not threads) run as several
//! processes. The pool loads each design into every instance, splits batches across them, and restarts an instance
//! that dies, retrying its share of the batch once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::join_all;
use tokio::sync::{mpsc, RwLock};

use super::client::Engine;
use super::manifest::EngineSpec;
use super::protocol::{
    BatchResult, Case, CaseFailure, CaseResult, ConfigInfo, InitializeResult, Output, Overrides, Progress, RunResult,
};
use super::rpc::CancelToken;
use super::runtime::Runtimes;
use crate::error::{Error, Result};

/// A design loaded into the pool.
#[derive(Debug, Clone)]
pub struct Design {
    pub handle: u64,
    pub path: PathBuf,
    pub name: String,
    pub configs: Vec<ConfigInfo>,
}

struct Slot {
    engine: RwLock<Arc<Engine>>,
    /// Pool design handle -> this instance's design_id.
    design_ids: Mutex<HashMap<u64, String>>,
}

pub struct EnginePool {
    spec: EngineSpec,
    runtimes: Runtimes,
    info: InitializeResult,
    slots: Vec<Slot>,
    designs: Mutex<HashMap<u64, PathBuf>>,
    next_handle: AtomicU64,
    next_slot: AtomicUsize,
}

impl EnginePool {
    /// Starts `size` instances (1 unless the engine declares `multi_instance`) in parallel.
    pub async fn start(spec: EngineSpec, runtimes: Runtimes, size: u32) -> Result<Self> {
        let first = Engine::start(&spec, &runtimes).await?;
        let size = if first.info().capabilities.multi_instance { size.max(1) } else { 1 } as usize;
        let rest = join_all((1..size).map(|_| Engine::start(&spec, &runtimes))).await;
        let info = first.info().clone();
        let mut slots = vec![Slot::new(first)];
        for e in rest {
            slots.push(Slot::new(e?));
        }
        Ok(Self {
            spec,
            runtimes,
            info,
            slots,
            designs: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            next_slot: AtomicUsize::new(0),
        })
    }

    pub fn info(&self) -> &InitializeResult {
        &self.info
    }

    pub fn spec(&self) -> &EngineSpec {
        &self.spec
    }

    pub fn size(&self) -> usize {
        self.slots.len()
    }

    /// Loads a design into every instance.
    pub async fn load(&self, path: &Path) -> Result<Design> {
        let path = std::path::absolute(path)?;
        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let loaded = join_all(self.slots.iter().map(|s| s.load(handle, &path))).await;
        let mut first = None;
        for r in loaded {
            let r = r?;
            first.get_or_insert(r);
        }
        let first = first.expect("pool has at least one instance");
        self.designs.lock().unwrap().insert(handle, path.clone());
        Ok(Design { handle, path, name: first.name, configs: first.configs })
    }

    pub async fn unload(&self, design: &Design) {
        self.designs.lock().unwrap().remove(&design.handle);
        for s in &self.slots {
            let id = s.design_ids.lock().unwrap().remove(&design.handle);
            if let Some(id) = id {
                let _ = s.engine().await.unload(&id).await;
            }
        }
    }

    /// Runs one case on the next instance in turn.
    pub async fn run(
        &self,
        design: &Design,
        config_id: &str,
        overrides: &Overrides,
        output: Output,
    ) -> Result<RunResult> {
        let slot = &self.slots[self.next_slot.fetch_add(1, Ordering::Relaxed) % self.slots.len()];
        for attempt in 0..2 {
            let engine = slot.engine().await;
            let id = slot.design_id(self, design.handle, &engine).await?;
            match engine.run(&id, config_id, overrides, output).await {
                Err(Error::EngineExited { .. }) if attempt == 0 => self.restart(slot, &engine).await?,
                r => return r,
            }
        }
        unreachable!()
    }

    /// Runs a batch split across all instances. Progress reports carry pool-wide `done`/`total`; results come back in
    /// case order. Results are collected from progress notifications as they arrive, so if an instance dies only its
    /// unfinished cases are lost; the instance is restarted and those cases are retried once.
    pub async fn run_batch(
        &self,
        design: &Design,
        config_id: &str,
        cases: &[Case],
        output: Output,
        progress: mpsc::UnboundedSender<Progress>,
        cancel: &CancelToken,
    ) -> Result<BatchResult> {
        let total = cases.len() as u32;
        let results = Arc::new(Mutex::new(HashMap::<String, RunResult>::new()));
        let failed = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let chunks: Vec<Vec<Case>> =
            (0..self.slots.len()).map(|i| cases.iter().skip(i).step_by(self.slots.len()).cloned().collect()).collect();

        let runs = self.slots.iter().zip(chunks).filter(|(_, c)| !c.is_empty()).map(|(slot, mut remaining)| {
            let (progress, results, failed) = (progress.clone(), results.clone(), failed.clone());
            async move {
                for attempt in 0..2 {
                    let engine = slot.engine().await;
                    let id = slot.design_id(self, design.handle, &engine).await?;
                    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
                    let relay = {
                        let (progress, results, failed) = (progress.clone(), results.clone(), failed.clone());
                        tokio::spawn(async move {
                            while let Some(mut p) = rx.recv().await {
                                let done = {
                                    let mut r = results.lock().unwrap();
                                    let mut f = failed.lock().unwrap();
                                    match (&p.result, &p.error) {
                                        (Some(res), _) => {
                                            r.insert(p.case_id.clone(), res.clone());
                                        }
                                        (None, Some(err)) => {
                                            f.insert(p.case_id.clone(), err.clone());
                                        }
                                        (None, None) => {}
                                    }
                                    (r.len() + f.len()) as u32
                                };
                                p.done = done;
                                p.total = total;
                                let _ = progress.send(p);
                            }
                        })
                    };
                    let r = engine.run_batch(&id, config_id, &remaining, output, tx, cancel).await;
                    let _ = relay.await;
                    match r {
                        Ok(b) => {
                            // Normally already collected from progress; the final result is authoritative.
                            let mut res = results.lock().unwrap();
                            res.extend(b.results.into_iter().map(|c| (c.case_id, c.result)));
                            failed.lock().unwrap().extend(b.failed.into_iter().map(|f| (f.case_id, f.message)));
                            return Ok(());
                        }
                        Err(Error::EngineExited { .. }) if attempt == 0 && !cancel.is_cancelled() => {
                            self.restart(slot, &engine).await?;
                            let res = results.lock().unwrap();
                            remaining.retain(|c| !res.contains_key(&c.id));
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
        });
        for r in join_all(runs).await {
            r?;
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let mut results = std::mem::take(&mut *results.lock().unwrap());
        let mut failed = std::mem::take(&mut *failed.lock().unwrap());
        let mut out = BatchResult { results: Vec::with_capacity(cases.len()), failed: Vec::new() };
        for c in cases {
            if let Some(r) = results.remove(&c.id) {
                out.results.push(CaseResult { case_id: c.id.clone(), result: r });
            } else {
                let message =
                    failed.remove(&c.id).unwrap_or_else(|| "engine instance crashed twice running this case".into());
                out.failed.push(CaseFailure { case_id: c.id.clone(), message });
            }
        }
        Ok(out)
    }

    /// Kills one instance's process, as a crash would (crash-recovery tests).
    #[doc(hidden)]
    pub async fn kill_instance(&self, index: usize) {
        self.slots[index].engine().await.kill().await;
    }

    pub async fn shutdown(&self) {
        join_all(self.slots.iter().map(|s| async move { s.engine().await.shutdown().await })).await;
    }

    /// Replaces a dead instance (unless another task already did) and reloads its designs lazily.
    async fn restart(&self, slot: &Slot, dead: &Arc<Engine>) -> Result<()> {
        let mut guard = slot.engine.write().await;
        if !Arc::ptr_eq(&guard, dead) {
            return Ok(());
        }
        tracing::warn!(engine = %self.spec.manifest.id, stderr = %dead.stderr_tail(), "restarting engine instance");
        *guard = Arc::new(Engine::start(&self.spec, &self.runtimes).await?);
        slot.design_ids.lock().unwrap().clear();
        Ok(())
    }

    fn design_path(&self, handle: u64) -> Option<PathBuf> {
        self.designs.lock().unwrap().get(&handle).cloned()
    }
}

impl Slot {
    fn new(engine: Engine) -> Self {
        Self { engine: RwLock::new(Arc::new(engine)), design_ids: Mutex::new(HashMap::new()) }
    }

    async fn engine(&self) -> Arc<Engine> {
        self.engine.read().await.clone()
    }

    async fn load(&self, handle: u64, path: &Path) -> Result<super::protocol::LoadResult> {
        let r = self.engine().await.load(path).await?;
        self.design_ids.lock().unwrap().insert(handle, r.design_id.clone());
        Ok(r)
    }

    /// This instance's id for a pool design, reloading it if the instance was restarted.
    async fn design_id(&self, pool: &EnginePool, handle: u64, engine: &Engine) -> Result<String> {
        if let Some(id) = self.design_ids.lock().unwrap().get(&handle) {
            return Ok(id.clone());
        }
        let path = pool.design_path(handle).ok_or_else(|| Error::Rpc {
            code: super::protocol::codes::UNKNOWN_DESIGN,
            message: format!("design {handle} isn't loaded"),
            data: None,
        })?;
        let r = engine.load(&path).await?;
        self.design_ids.lock().unwrap().insert(handle, r.design_id.clone());
        Ok(r.design_id)
    }
}
