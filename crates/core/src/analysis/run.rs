//! Running a complete analysis of a design: the nominal flight, a Monte Carlo landing scatter and a wind sweep for
//! each configuration (what rocketviz.py did, now on the engine pool).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::flight_file::{Conditions, FlightFile, McLanding, SimRecord, SummaryRecord, SweepLanding, VERSION};
use super::geo::LatLon;
use crate::engine::protocol::{Case, ConfigInfo, Output, Overrides, Progress};
use crate::engine::{CancelToken, Design, EnginePool};
use crate::error::Result;
use crate::montecarlo::{default_dispersions, generate_cases, MonteCarloSpec, ParamDispersion};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSettings {
    /// Changes to each config's launch conditions (wind, site, ...), by parameter key.
    #[serde(default)]
    pub overrides: Overrides,
    /// Seed for the nominal flight's turbulence.
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_runs")]
    pub mc_runs: u32,
    /// Monte Carlo dispersions; `None` uses [`default_dispersions`].
    #[serde(default)]
    pub dispersions: Option<Vec<ParamDispersion>>,
    /// Monte Carlo time step (s): coarser than the nominal flight's, which landing points barely notice.
    #[serde(default = "default_mc_step")]
    pub mc_time_step: f64,
    /// Wind sweep from 0 to this many m/s in 1 m/s steps (0 disables it).
    #[serde(default = "default_sweep")]
    pub sweep_max: u32,
}

fn default_seed() -> u64 {
    42
}
fn default_runs() -> u32 {
    150
}
fn default_mc_step() -> f64 {
    0.05
}
fn default_sweep() -> u32 {
    8
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            overrides: Overrides::new(),
            seed: default_seed(),
            mc_runs: default_runs(),
            dispersions: None,
            mc_time_step: default_mc_step(),
            sweep_max: default_sweep(),
        }
    }
}

/// What an analysis reports while it runs, so the app can show progress and draw landings as they arrive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum AnalysisEvent {
    Nominal { config_id: String },
    MonteCarlo { config_id: String, done: u32, total: u32, landing: Option<[f64; 2]> },
    Sweep { config_id: String, done: u32, total: u32 },
    ConfigDone { config_id: String },
}

/// Analyzes the given configs of a design (all of them if `config_ids` is empty).
pub async fn analyze(
    pool: &EnginePool,
    design: &Design,
    config_ids: &[String],
    settings: &AnalysisSettings,
    events: mpsc::UnboundedSender<AnalysisEvent>,
    cancel: &CancelToken,
) -> Result<FlightFile> {
    let configs: Vec<&ConfigInfo> =
        design.configs.iter().filter(|c| config_ids.is_empty() || config_ids.contains(&c.id)).collect();
    let mut sims = Vec::with_capacity(configs.len());
    for config in configs {
        sims.push(analyze_config(pool, design, config, settings, &events, cancel).await?);
    }
    let first = sims.first().map(|s| s.conditions.clone()).unwrap_or_default();
    Ok(FlightFile {
        version: VERSION,
        rocket: design.name.clone(),
        pad: LatLon { lat: first.lat, lon: first.lon },
        site_alt: first.site_alt,
        sims,
    })
}

async fn analyze_config(
    pool: &EnginePool,
    design: &Design,
    config: &ConfigInfo,
    settings: &AnalysisSettings,
    events: &mpsc::UnboundedSender<AnalysisEvent>,
    cancel: &CancelToken,
) -> Result<SimRecord> {
    let id = config.id.clone();
    let accepts = |key: &str| pool.info().parameters.iter().any(|p| p.key == key);

    // Nominal flight.
    let _ = events.send(AnalysisEvent::Nominal { config_id: id.clone() });
    let mut nominal = settings.overrides.clone();
    if accepts("seed") {
        nominal.insert("seed".into(), json!(settings.seed));
    }
    let base = pool.run(design, &id, &nominal, Output::Full).await?;
    let conditions = Conditions::from_protocol(base.conditions.as_ref().unwrap_or(&Default::default()));

    // Monte Carlo around the conditions actually flown.
    let mut mc_nominal = config.defaults.clone();
    mc_nominal.extend(settings.overrides.clone());
    mc_nominal.retain(|k, v| accepts(k) && v.is_number());
    if accepts("time_step") {
        mc_nominal.insert("time_step".into(), json!(settings.mc_time_step));
    }
    let wind = mc_nominal.get("wind_speed").and_then(Value::as_f64).unwrap_or(conditions.wind_speed);
    let spec = MonteCarloSpec {
        runs: settings.mc_runs,
        seed: settings.seed,
        nominal: mc_nominal,
        dispersions: settings.dispersions.clone().unwrap_or_else(|| default_dispersions(wind)),
    };
    let mc_cases = generate_cases(&spec, &pool.info().parameters)?;
    let mc_id = id.clone();
    let mc = run_cases(
        pool,
        design,
        &id,
        &mc_cases,
        cancel,
        move |p| {
            let landing = p.result.as_ref().and_then(|r| Some([r.landing.e?, r.landing.n?]));
            AnalysisEvent::MonteCarlo { config_id: mc_id.clone(), done: p.done, total: p.total, landing }
        },
        events,
    )
    .await?
    .into_iter()
    .filter_map(|(case, r)| {
        let o = &case.overrides;
        Some(McLanding {
            e: r.landing.e?,
            n: r.landing.n?,
            wind: o.get("wind_speed").and_then(Value::as_f64).unwrap_or(f64::NAN),
            dir: o.get("wind_direction").and_then(Value::as_f64).unwrap_or(f64::NAN),
            apogee: r.summary.max_alt.unwrap_or(f64::NAN),
        })
    })
    .collect();

    // Wind sweep: steady wind of 0..=max m/s from the nominal direction, no turbulence.
    let sweep_cases: Vec<Case> = (0..=settings.sweep_max)
        .filter(|_| settings.sweep_max > 0)
        .map(|w| {
            let mut o = settings.overrides.clone();
            o.insert("wind_speed".into(), json!(w as f64));
            o.insert("wind_direction".into(), json!(conditions.wind_dir_deg));
            o.insert("turbulence_intensity".into(), json!(0.0));
            o.insert("seed".into(), json!(1));
            o.retain(|k, _| accepts(k));
            Case { id: format!("sweep-{w}"), overrides: o }
        })
        .collect();
    let sweep_id = id.clone();
    let sweep = run_cases(
        pool,
        design,
        &id,
        &sweep_cases,
        cancel,
        move |p| AnalysisEvent::Sweep { config_id: sweep_id.clone(), done: p.done, total: p.total },
        events,
    )
    .await?
    .into_iter()
    .filter_map(|(case, r)| {
        Some(SweepLanding {
            wind: case.overrides.get("wind_speed").and_then(Value::as_f64)?,
            e: r.landing.e?,
            n: r.landing.n?,
            apogee: r.summary.max_alt.unwrap_or(f64::NAN),
        })
    })
    .collect();

    let _ = events.send(AnalysisEvent::ConfigDone { config_id: id.clone() });
    Ok(SimRecord {
        name: format!("{} — {}", config.name, config.motor),
        config: config.motor.clone(),
        summary: SummaryRecord::from_run(&base),
        conditions,
        events: base.events.clone().unwrap_or_default(),
        t: base.t.clone().unwrap_or_default(),
        e: base.e.clone().unwrap_or_default(),
        n: base.n.clone().unwrap_or_default(),
        alt: base.alt.clone().unwrap_or_default(),
        series: base.series.clone().unwrap_or_default(),
        mc,
        sweep,
    })
}

/// Runs cases as a pool batch, forwarding progress as analysis events; returns (case, result) for those that
/// succeeded, in case order.
async fn run_cases<'a>(
    pool: &EnginePool,
    design: &Design,
    config_id: &str,
    cases: &'a [Case],
    cancel: &CancelToken,
    to_event: impl Fn(&Progress) -> AnalysisEvent + Send + 'static,
    events: &mpsc::UnboundedSender<AnalysisEvent>,
) -> Result<Vec<(&'a Case, crate::engine::protocol::RunResult)>> {
    if cases.is_empty() {
        return Ok(Vec::new());
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
    let forward = {
        let events = events.clone();
        tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let _ = events.send(to_event(&p));
            }
        })
    };
    let batch = pool.run_batch(design, config_id, cases, Output::Landing, tx, cancel).await;
    let _ = forward.await;
    let batch = batch?;
    for f in &batch.failed {
        tracing::warn!(case = %f.case_id, "case failed: {}", f.message);
    }
    let by_id: std::collections::HashMap<_, _> = batch.results.into_iter().map(|c| (c.case_id, c.result)).collect();
    Ok(cases.iter().filter_map(|c| by_id.get(&c.id).map(|r| (c, r.clone()))).collect())
}
