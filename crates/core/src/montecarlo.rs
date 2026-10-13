//! Monte Carlo: turning nominal launch conditions plus dispersions into engine cases.
//!
//! Case generation is engine-agnostic: it only uses the parameter keys an engine declares in `initialize`. It is
//! reproducible from `MonteCarloSpec::seed` on every platform and every version of rocketviz, because it uses its own
//! small, fixed generator (xoshiro256** seeded through SplitMix64, Box-Muller for normals) rather than a library whose
//! algorithms may change.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine::protocol::{Case, Overrides, ParameterDef};
use crate::error::{Error, Result};

/// How one parameter varies between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Dispersion {
    /// Normal around the nominal value. Directions (`circular`) wrap at 360; others clamp to the parameter's range.
    Normal { sigma: f64 },
    /// Half-normal added to the nominal value (always >= nominal), e.g. rod tilt error.
    HalfNormal { sigma: f64 },
    /// Uniform in [min, max), ignoring the nominal value, e.g. the direction of a rod tilt error.
    Uniform { min: f64, max: f64 },
}

/// One parameter's dispersion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDispersion {
    pub key: String,
    #[serde(flatten)]
    pub dispersion: Dispersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloSpec {
    pub runs: u32,
    /// Seed for the whole analysis: the same spec always produces the same cases.
    pub seed: u64,
    /// Nominal conditions (the config's defaults plus the user's overrides).
    pub nominal: Overrides,
    pub dispersions: Vec<ParamDispersion>,
}

/// Keys whose values are compass directions and wrap at 360 degrees.
const CIRCULAR: &[&str] = &["wind_direction", "rod_direction"];

/// The dispersions rocketviz has always used: wind speed and direction, rod tilt error in a random direction, and
/// a different turbulence seed per run. Wind speed sigma defaults to 30 % of the mean plus 0.3 m/s.
pub fn default_dispersions(nominal_wind_speed: f64) -> Vec<ParamDispersion> {
    let d = |key: &str, dispersion| ParamDispersion { key: key.into(), dispersion };
    vec![
        d("wind_speed", Dispersion::Normal { sigma: 0.3 * nominal_wind_speed + 0.3 }),
        d("wind_direction", Dispersion::Normal { sigma: 20.0 }),
        d("rod_angle", Dispersion::HalfNormal { sigma: 1.5 }),
        d("rod_direction", Dispersion::Uniform { min: 0.0, max: 360.0 }),
    ]
}

/// Generates the cases for a Monte Carlo analysis. Each case also gets its own turbulence `seed` (when the engine
/// accepts one). Fails if a dispersion names a parameter the engine doesn't accept or has no nominal value.
pub fn generate_cases(spec: &MonteCarloSpec, params: &[ParameterDef]) -> Result<Vec<Case>> {
    let find = |key: &str| params.iter().find(|p| p.key == key);
    for d in &spec.dispersions {
        if find(&d.key).is_none() {
            return Err(invalid(format!("the engine has no parameter {:?}", d.key)));
        }
        if !matches!(d.dispersion, Dispersion::Uniform { .. })
            && spec.nominal.get(&d.key).and_then(Value::as_f64).is_none()
        {
            return Err(invalid(format!("no nominal value for {:?}", d.key)));
        }
    }
    let seeds = find("seed").is_some();
    let mut rng = Rng::new(spec.seed);
    let mut cases = Vec::with_capacity(spec.runs as usize);
    for i in 0..spec.runs {
        let mut o = spec.nominal.clone();
        for d in &spec.dispersions {
            let p = find(&d.key).expect("checked above");
            let nominal = o.get(&d.key).and_then(Value::as_f64).unwrap_or(0.0);
            let v = match d.dispersion {
                Dispersion::Normal { sigma } => nominal + sigma * rng.normal(),
                Dispersion::HalfNormal { sigma } => nominal + (sigma * rng.normal()).abs(),
                Dispersion::Uniform { min, max } => min + (max - min) * rng.uniform(),
            };
            o.insert(d.key.clone(), json!(fit(&d.key, p, v)));
        }
        if seeds {
            // Java's int range, positive: engines may store seeds as 32-bit signed integers.
            o.insert("seed".into(), json!(rng.next_u64() >> 33));
        }
        cases.push(Case { id: format!("mc-{i}"), overrides: o });
    }
    Ok(cases)
}

/// Wraps directions to [0, 360) and clamps everything else to the parameter's declared range.
fn fit(key: &str, p: &ParameterDef, v: f64) -> f64 {
    if CIRCULAR.contains(&key) {
        return v.rem_euclid(360.0);
    }
    let v = p.min.map_or(v, |m| v.max(m));
    p.max.map_or(v, |m| v.min(m))
}

fn invalid(message: String) -> Error {
    Error::Rpc { code: crate::engine::protocol::codes::INVALID_PARAMS, message, data: None }
}

/// xoshiro256** (Blackman & Vigna), seeded with SplitMix64. Fixed forever so Monte Carlo cases stay reproducible.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
    spare_normal: Option<f64>,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut splitmix = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self { s: [splitmix(), splitmix(), splitmix(), splitmix()], spare_normal: None }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1), 53 bits of precision.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal (Box-Muller; the second value of each pair is kept for the next call).
    pub fn normal(&mut self) -> f64 {
        if let Some(z) = self.spare_normal.take() {
            return z;
        }
        let u1 = 1.0 - self.uniform(); // (0, 1]: avoids ln(0)
        let u2 = self.uniform();
        let r = (-2.0 * u1.ln()).sqrt();
        let (s, c) = (std::f64::consts::TAU * u2).sin_cos();
        self.spare_normal = Some(r * s);
        r * c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::protocol::ParameterKind;

    fn param(key: &str, min: Option<f64>, max: Option<f64>) -> ParameterDef {
        ParameterDef {
            key: key.into(),
            name: key.into(),
            unit: String::new(),
            kind: ParameterKind::Standard,
            min,
            max,
            description: None,
        }
    }

    fn params() -> Vec<ParameterDef> {
        vec![
            param("wind_speed", Some(0.0), None),
            param("wind_direction", None, None),
            param("rod_angle", Some(0.0), Some(90.0)),
            param("rod_direction", None, None),
            param("seed", None, None),
        ]
    }

    fn spec(runs: u32, seed: u64) -> MonteCarloSpec {
        MonteCarloSpec {
            runs,
            seed,
            nominal: json!({"wind_speed": 2.0, "wind_direction": 350.0, "rod_angle": 0.0}).as_object().unwrap().clone(),
            dispersions: default_dispersions(2.0),
        }
    }

    #[test]
    fn generator_is_fixed() {
        // Reference values from an independent implementation of SplitMix64 + xoshiro256** (checked against
        // SplitMix64's published test vector). If these change, saved analyses would no longer reproduce.
        let mut r = Rng::new(42);
        assert_eq!(
            [r.next_u64(), r.next_u64(), r.next_u64()],
            [0x1578_0B2E_0C2E_C716, 0x6104_D986_6D11_3A7E, 0xAE17_5332_39E4_99A1]
        );
    }

    #[test]
    fn normal_has_the_right_moments() {
        let mut r = Rng::new(7);
        let xs: Vec<f64> = (0..200_000).map(|_| r.normal()).collect();
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / xs.len() as f64;
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((var - 1.0).abs() < 0.01, "variance {var}");
    }

    #[test]
    fn same_spec_same_cases_and_seed_changes_them() {
        let a = generate_cases(&spec(50, 1), &params()).unwrap();
        let b = generate_cases(&spec(50, 1), &params()).unwrap();
        let c = generate_cases(&spec(50, 2), &params()).unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
        assert_ne!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&c).unwrap());
    }

    #[test]
    fn values_respect_ranges_and_wrap_directions() {
        let cases = generate_cases(&spec(2000, 3), &params()).unwrap();
        for c in &cases {
            let o = &c.overrides;
            let f = |k: &str| o[k].as_f64().unwrap();
            assert!(f("wind_speed") >= 0.0);
            assert!((0.0..360.0).contains(&f("wind_direction")));
            assert!((0.0..360.0).contains(&f("rod_direction")));
            assert!(f("rod_angle") >= 0.0);
            assert!(o["seed"].as_u64().unwrap() < (1 << 31));
        }
        // Wind direction 350 +/- 20 wraps through north: some cases land just past 0.
        assert!(cases.iter().any(|c| c.overrides["wind_direction"].as_f64().unwrap() < 30.0));
        let ids: std::collections::HashSet<_> = cases.iter().map(|c| c.overrides["seed"].as_u64().unwrap()).collect();
        assert!(ids.len() > 1990, "per-case seeds are distinct");
    }

    #[test]
    fn rejects_parameters_the_engine_lacks() {
        let mut s = spec(10, 1);
        s.dispersions.push(ParamDispersion {
            key: "motor_impulse_scale".into(),
            dispersion: Dispersion::Normal { sigma: 0.05 },
        });
        assert!(generate_cases(&s, &params()).is_err());
    }
}
