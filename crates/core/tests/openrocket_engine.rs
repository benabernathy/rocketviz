//! Integration tests against the real OpenRocket engine (engines/openrocket/build/engine, built with
//! `./gradlew assembleEngine`). Skipped when it hasn't been built, unless ROCKETVIZ_REQUIRE_ENGINE=1 (CI).

use std::path::PathBuf;
use std::time::Duration;

use rocketviz_core::engine::protocol::{codes, Case, Output, Overrides};
use rocketviz_core::engine::{CancelToken, Engine, EnginePool, EngineSpec, Runtimes};
use rocketviz_core::Error;
use serde_json::{json, Value};
use tokio::sync::mpsc;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine_spec() -> Option<EngineSpec> {
    let dir = std::env::var_os("ROCKETVIZ_OPENROCKET_ENGINE")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo().join("engines/openrocket/build/engine"));
    if dir.join("engine.json").is_file() {
        return Some(EngineSpec::load(dir).expect("valid engine.json"));
    }
    assert!(std::env::var("ROCKETVIZ_REQUIRE_ENGINE").is_err(), "OpenRocket engine not built at {}", dir.display());
    eprintln!("skipping: build the engine with `cd engines/openrocket && ./gradlew assembleEngine`");
    None
}

fn fizz() -> PathBuf {
    repo().join("Fizz.ork")
}

fn overrides(v: Value) -> Overrides {
    v.as_object().unwrap().clone()
}

fn cases(n: usize) -> Vec<Case> {
    (0..n)
        .map(|i| Case {
            id: format!("mc-{i}"),
            overrides: overrides(json!({
                "wind_speed": 1.0 + (i % 7) as f64 * 0.4, "wind_direction": 60.0 + (i * 37 % 60) as f64,
                "rod_angle": (i % 4) as f64 * 0.75, "rod_direction": (i * 73 % 360) as f64,
                "seed": 1000 + i, "time_step": 0.05
            })),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn single_engine_matches_golden() {
    let Some(spec) = engine_spec() else { return };
    let engine = Engine::start(&spec, &Runtimes::default()).await.unwrap();
    assert_eq!(engine.info().engine.id, "openrocket");
    assert!(engine.info().capabilities.multi_instance);

    let design = engine.load(&fizz()).await.unwrap();
    assert_eq!(design.name, "Fizz");
    assert_eq!(design.configs.len(), 2);

    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("engines/openrocket/src/test/resources/golden/fizz.json")).unwrap(),
    )
    .unwrap();
    for cfg in golden["configs"].as_array().unwrap() {
        for case in cfg["cases"].as_array().unwrap() {
            let r = engine
                .run(
                    &design.design_id,
                    cfg["id"].as_str().unwrap(),
                    &overrides(case["overrides"].clone()),
                    Output::Full,
                )
                .await
                .unwrap();
            let (ge, gn) = (case["landing"]["e"].as_f64().unwrap(), case["landing"]["n"].as_f64().unwrap());
            let (re, rn) = (r.landing.e.unwrap(), r.landing.n.unwrap());
            assert!(
                (ge - re).abs() < 0.05 && (gn - rn).abs() < 0.05,
                "{} {}: landing ({re}, {rn}) vs golden ({ge}, {gn})",
                cfg["motor"],
                case["id"]
            );
            let series = r.series.as_ref().unwrap();
            assert!(series.iter().any(|s| s.key == "total_velocity"));
            assert!(series.iter().all(|s| s.data.len() == r.t.as_ref().unwrap().len()));
        }
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_errors_come_back_as_rpc_codes() {
    let Some(spec) = engine_spec() else { return };
    let engine = Engine::start(&spec, &Runtimes::default()).await.unwrap();
    let err = engine.load(&repo().join("no-such.ork")).await.unwrap_err();
    assert_eq!(err.rpc_code(), Some(codes::LOAD_FAILED));
    let d = engine.load(&fizz()).await.unwrap();
    let err = engine.run(&d.design_id, "0", &overrides(json!({"wind_speed": -3})), Output::Landing).await.unwrap_err();
    assert_eq!(err.rpc_code(), Some(codes::BAD_OVERRIDE));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_batch_matches_single_runs_and_reports_progress() {
    let Some(spec) = engine_spec() else { return };
    let pool = EnginePool::start(spec, Runtimes::default(), 2).await.unwrap();
    assert_eq!(pool.size(), 2);
    let design = pool.load(&fizz()).await.unwrap();
    let cases = cases(40);

    let (tx, mut rx) = mpsc::unbounded_channel();
    let batch = pool.run_batch(&design, "1", &cases, Output::Landing, tx, &CancelToken::new()).await.unwrap();
    let mut progress = Vec::new();
    while let Ok(p) = rx.try_recv() {
        progress.push(p);
    }
    assert_eq!(batch.results.len(), 40);
    assert!(batch.failed.is_empty());
    assert_eq!(progress.len(), 40, "one progress report per case");
    let mut done: Vec<u32> = progress.iter().map(|p| p.done).collect();
    done.sort();
    assert_eq!(done, (1..=40).collect::<Vec<_>>(), "pool-wide done counter");
    assert!(progress.iter().all(|p| p.total == 40));

    // Each result is in case order and identical to running that case on its own (any instance, any order).
    for (c, r) in cases.iter().zip(&batch.results) {
        assert_eq!(c.id, r.case_id);
        let single = pool.run(&design, "1", &c.overrides, Output::Landing).await.unwrap();
        assert_eq!(single.landing, r.result.landing, "{}", c.id);
    }
    pool.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_batch_can_be_cancelled() {
    let Some(spec) = engine_spec() else { return };
    let pool = EnginePool::start(spec, Runtimes::default(), 2).await.unwrap();
    let design = pool.load(&fizz()).await.unwrap();
    let cases = cases(600);
    let cancel = CancelToken::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        rx.recv().await;
        c2.cancel();
    });
    let t0 = std::time::Instant::now();
    let r = pool.run_batch(&design, "1", &cases, Output::Landing, tx, &cancel).await;
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    assert!(t0.elapsed() < Duration::from_secs(10), "stopped promptly");
    // The pool is still usable afterwards.
    pool.run(&design, "0", &Overrides::new(), Output::Landing).await.unwrap();
    pool.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pool_recovers_when_an_instance_crashes_mid_batch() {
    let Some(spec) = engine_spec() else { return };
    let pool = std::sync::Arc::new(EnginePool::start(spec, Runtimes::default(), 2).await.unwrap());
    let design = pool.load(&fizz()).await.unwrap();
    let cases = cases(120);

    let (tx, mut rx) = mpsc::unbounded_channel::<rocketviz_core::engine::protocol::Progress>();
    let killer = {
        let pool = pool.clone();
        tokio::spawn(async move {
            // Kill instance 1 once the batch is under way.
            while let Some(p) = rx.recv().await {
                if p.done == 10 {
                    pool.kill_instance(1).await;
                }
            }
        })
    };
    let batch = pool.run_batch(&design, "1", &cases, Output::Landing, tx, &CancelToken::new()).await.unwrap();
    killer.await.unwrap();
    assert_eq!(batch.results.len(), 120, "every case completed despite the crash: failed {:?}", batch.failed);
    for (c, r) in cases.iter().zip(&batch.results) {
        assert_eq!(c.id, r.case_id);
    }
    // Results after recovery are still the deterministic ones.
    let check = pool.run(&design, "1", &cases[119].overrides, Output::Landing).await.unwrap();
    assert_eq!(check.landing, batch.results[119].result.landing);
    pool.shutdown().await;
}
