//! Statistics of Monte Carlo landing points: mean, covariance, sigma ellipses and drift distances.

use serde::{Deserialize, Serialize};

/// Summary of where a design lands under dispersed conditions. Coordinates are metres east/north of the pad.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandingStats {
    pub count: usize,
    pub mean: [f64; 2],
    /// Sample covariance (n - 1), [[ee, en], [en, nn]].
    pub cov: [[f64; 2]; 2],
    /// One-sigma semi-axes (major, minor) of the covariance ellipse.
    pub semi_axes: [f64; 2],
    /// Direction of the major axis, degrees counter-clockwise from east, in [0, 180).
    pub major_axis_deg: f64,
    pub drift: Drift,
}

/// Distances from the pad to the landing points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Drift {
    pub median: f64,
    /// Nearest-rank 90th percentile.
    pub p90: f64,
    pub max: f64,
}

impl LandingStats {
    /// Statistics of the given landing points; `None` with fewer than three.
    pub fn from_points(points: &[[f64; 2]]) -> Option<Self> {
        let n = points.len();
        if n < 3 {
            return None;
        }
        let mean =
            [points.iter().map(|p| p[0]).sum::<f64>() / n as f64, points.iter().map(|p| p[1]).sum::<f64>() / n as f64];
        let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
        for p in points {
            let (de, dn) = (p[0] - mean[0], p[1] - mean[1]);
            a += de * de;
            b += de * dn;
            c += dn * dn;
        }
        let d = (n - 1) as f64;
        let (a, b, c) = (a / d, b / d, c / d);
        // Eigenvalues of the symmetric 2x2 covariance.
        let half_trace = (a + c) / 2.0;
        let r = (((a - c) / 2.0).powi(2) + b * b).sqrt();
        let (l1, l2) = (half_trace + r, (half_trace - r).max(0.0));
        let major_axis_deg = (0.5 * (2.0 * b).atan2(a - c)).to_degrees().rem_euclid(180.0);

        let mut dist: Vec<f64> = points.iter().map(|p| p[0].hypot(p[1])).collect();
        dist.sort_by(f64::total_cmp);
        let median = if n % 2 == 1 { dist[n / 2] } else { (dist[n / 2 - 1] + dist[n / 2]) / 2.0 };
        let p90 = dist[((0.9 * n as f64) as usize).min(n - 1)];

        Some(Self {
            count: n,
            mean,
            cov: [[a, b], [b, c]],
            semi_axes: [l1.sqrt(), l2.sqrt()],
            major_axis_deg,
            drift: Drift { median, p90, max: dist[n - 1] },
        })
    }

    /// The `k`-sigma ellipse as a closed ring of `segments + 1` points (first == last).
    pub fn ellipse(&self, k: f64, segments: usize) -> Vec<[f64; 2]> {
        let (sin, cos) = self.major_axis_deg.to_radians().sin_cos();
        let (ra, rb) = (k * self.semi_axes[0], k * self.semi_axes[1]);
        (0..=segments)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / segments as f64;
                let (x, y) = (ra * t.cos(), rb * t.sin());
                [self.mean[0] + x * cos - y * sin, self.mean[1] + x * sin + y * cos]
            })
            .collect()
    }
}
