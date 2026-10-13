//! Landing statistics against numpy (tests/fixtures/landing_stats.json, from the sample flight's Monte Carlo).

use rocketviz_core::analysis::stats::LandingStats;
use serde_json::Value;

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9 * a.abs().max(1.0), "{what}: {a} vs numpy {b}");
}

#[test]
fn matches_numpy() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/landing_stats.json")).unwrap();
    for sim in fixture["sims"].as_array().unwrap() {
        let name = sim["name"].as_str().unwrap();
        let pts: Vec<[f64; 2]> = sim["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
            .collect();
        let s = LandingStats::from_points(&pts).unwrap();
        let f = |k: &str| sim[k].as_f64().unwrap();
        close(s.mean[0], sim["mean"][0].as_f64().unwrap(), &format!("{name} mean e"));
        close(s.mean[1], sim["mean"][1].as_f64().unwrap(), &format!("{name} mean n"));
        for i in 0..2 {
            for j in 0..2 {
                close(s.cov[i][j], sim["cov"][i][j].as_f64().unwrap(), &format!("{name} cov[{i}][{j}]"));
            }
            close(s.semi_axes[i], sim["semi_axes_1sigma"][i].as_f64().unwrap(), &format!("{name} semi-axis {i}"));
        }
        close(s.major_axis_deg, f("major_axis_angle_from_east_deg"), &format!("{name} major axis angle"));
        close(s.drift.median, f("drift_median"), &format!("{name} median drift"));
        close(s.drift.p90, f("drift_p90_nearest_rank"), &format!("{name} p90 drift"));
        close(s.drift.max, f("drift_max"), &format!("{name} max drift"));
    }
}

#[test]
fn ellipse_ring_has_the_right_shape() {
    let pts: Vec<[f64; 2]> = (0..400)
        .map(|i| {
            let t = i as f64 * 0.7;
            [10.0 + 30.0 * t.cos(), -5.0 + 10.0 * t.sin()]
        })
        .collect();
    let s = LandingStats::from_points(&pts).unwrap();
    let ring = s.ellipse(2.0, 72);
    assert_eq!(ring.len(), 73);
    assert!((ring[0][0] - ring[72][0]).abs() < 1e-9 && (ring[0][1] - ring[72][1]).abs() < 1e-9, "closed");
    // Every point lies on the 2-sigma ellipse: Mahalanobis distance 2.
    let [[a, b], [_, c]] = s.cov;
    let det = a * c - b * b;
    for p in &ring {
        let (x, y) = (p[0] - s.mean[0], p[1] - s.mean[1]);
        let m2 = (c * x * x - 2.0 * b * x * y + a * y * y) / det;
        assert!((m2.sqrt() - 2.0).abs() < 1e-9, "mahalanobis {}", m2.sqrt());
    }
}

#[test]
fn too_few_points() {
    assert!(LandingStats::from_points(&[[0.0, 0.0], [1.0, 1.0]]).is_none());
}
