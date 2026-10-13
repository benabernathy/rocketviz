//! A full analysis of Fizz through the Rust core, compared with rocketviz.py's output for the same design
//! (web/public/samples/fizz.rocketviz.json). Nominal flights and wind sweeps use identical inputs and seeds, so they
//! must match; Monte Carlo uses a different random generator, so only its statistics are compared.

use std::path::PathBuf;

use rocketviz_core::analysis::flight_file::FlightFile;
use rocketviz_core::analysis::run::{analyze, AnalysisEvent, AnalysisSettings};
use rocketviz_core::analysis::stats::LandingStats;
use rocketviz_core::engine::{CancelToken, EnginePool, EngineSpec, Runtimes};
use tokio::sync::mpsc;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine_spec() -> Option<EngineSpec> {
    let dir = std::env::var_os("ROCKETVIZ_OPENROCKET_ENGINE")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo().join("engines/openrocket/build/engine"));
    if dir.join("engine.json").is_file() {
        return Some(EngineSpec::load(dir).unwrap());
    }
    assert!(std::env::var("ROCKETVIZ_REQUIRE_ENGINE").is_err(), "OpenRocket engine not built");
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn matches_rocketviz_py() {
    let Some(spec) = engine_spec() else { return };
    let python: FlightFile =
        serde_json::from_str(&std::fs::read_to_string(repo().join("web/public/samples/fizz.rocketviz.json")).unwrap())
            .unwrap();

    let pool = EnginePool::start(spec, Runtimes::default(), 2).await.unwrap();
    let design = pool.load(&repo().join("Fizz.ork")).await.unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let t0 = std::time::Instant::now();
    let rust = analyze(&pool, &design, &[], &AnalysisSettings::default(), tx, &CancelToken::new()).await.unwrap();
    let elapsed = t0.elapsed();
    let mut mc_events = 0;
    while let Ok(e) = rx.try_recv() {
        if matches!(e, AnalysisEvent::MonteCarlo { landing: Some(_), .. }) {
            mc_events += 1;
        }
    }
    eprintln!("full analysis of 2 configs (2 x 150 MC + 2 x 9 sweep) in {elapsed:?}");
    assert_eq!(mc_events, 300, "every Monte Carlo landing streamed as it finished");

    assert_eq!(rust.rocket, python.rocket);
    // Each config has its own launch site (Fizz's two simulations differ in longitude). rocketviz.py wrote the *last*
    // config's site as the file's pad, a bug; the core uses the first config's site and keeps each config's own.
    assert_eq!((rust.pad.lat, rust.pad.lon), (rust.sims[0].conditions.lat, rust.sims[0].conditions.lon));
    assert_eq!(rust.sims.len(), python.sims.len());
    for (r, p) in rust.sims.iter().zip(&python.sims) {
        assert_eq!(r.name, p.name);
        assert_eq!(r.config, p.config);
        // Nominal flight: same seed (42), same conditions.
        let land = |s: &rocketviz_core::analysis::flight_file::SimRecord| (*s.e.last().unwrap(), *s.n.last().unwrap());
        let ((re, rn), (pe, pn)) = (land(r), land(p));
        assert!(
            (re - pe).abs() < 0.05 && (rn - pn).abs() < 0.05,
            "{}: nominal landing ({re}, {rn}) vs ({pe}, {pn})",
            r.name
        );
        assert!((r.summary.max_alt - p.summary.max_alt).abs() < 0.01, "{} apogee", r.name);
        assert_eq!(r.conditions.wind_speed, p.conditions.wind_speed);
        assert_eq!(
            (r.conditions.lat, r.conditions.lon),
            (p.conditions.lat, p.conditions.lon),
            "{}: launch site",
            r.name
        );
        assert_eq!(r.events.len(), p.events.len());
        let keys = |s: &rocketviz_core::analysis::flight_file::SimRecord| {
            s.series.iter().map(|x| x.key.clone()).collect::<Vec<_>>()
        };
        assert_eq!(keys(r), keys(p), "{}: same plottable series", r.name);
        // Wind sweep: deterministic (no turbulence, seed 1).
        assert_eq!(r.sweep.len(), p.sweep.len());
        for (a, b) in r.sweep.iter().zip(&p.sweep) {
            assert_eq!(a.wind, b.wind);
            assert!(
                (a.e - b.e).abs() < 0.05 && (a.n - b.n).abs() < 0.05,
                "{} sweep {} m/s: ({}, {}) vs ({}, {})",
                r.name,
                a.wind,
                a.e,
                a.n,
                b.e,
                b.n
            );
        }
        // Monte Carlo: same dispersion model, different random draws -> same statistics within sampling error.
        let stats = |s: &rocketviz_core::analysis::flight_file::SimRecord| {
            LandingStats::from_points(&s.mc.iter().map(|m| [m.e, m.n]).collect::<Vec<_>>()).unwrap()
        };
        let (rs, ps) = (stats(r), stats(p));
        assert_eq!(rs.count, 150);
        let rel = (rs.drift.median - ps.drift.median).abs() / ps.drift.median;
        assert!(rel < 0.15, "{}: median drift {:.1} vs {:.1}", r.name, rs.drift.median, ps.drift.median);
        let rel_axis = (rs.semi_axes[0] - ps.semi_axes[0]).abs() / ps.semi_axes[0];
        assert!(rel_axis < 0.25, "{}: 1-sigma major axis {:.1} vs {:.1}", r.name, rs.semi_axes[0], ps.semi_axes[0]);
        eprintln!(
            "{}: MC median drift {:.1} m (python {:.1}), 1σ ellipse {:.1} x {:.1} m (python {:.1} x {:.1})",
            r.name,
            rs.drift.median,
            ps.drift.median,
            rs.semi_axes[0],
            rs.semi_axes[1],
            ps.semi_axes[0],
            ps.semi_axes[1]
        );
    }

    // The file the app reads round-trips.
    let json = rust.to_json().unwrap();
    let back: FlightFile = serde_json::from_str(&json).unwrap();
    assert_eq!(back.sims.len(), 2);
    eprintln!("flight file: {} KB", json.len() / 1024);
    pool.shutdown().await;
}
