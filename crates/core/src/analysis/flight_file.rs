//! The `.rocketviz.json` flight file (version 1), as read by the app (web/src/types.ts). Field names keep the
//! format rocketviz.py introduced, so files written by either load the same way.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::geo::LatLon;
use crate::engine::protocol::{FlightEvent, RunResult, Series};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightFile {
    pub version: u32,
    pub rocket: String,
    pub pad: LatLon,
    pub site_alt: f64,
    pub sims: Vec<SimRecord>,
}

/// One configuration's analysis: nominal flight, Monte Carlo landings and wind sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimRecord {
    pub name: String,
    pub config: String,
    pub conditions: Conditions,
    pub summary: SummaryRecord,
    pub events: Vec<FlightEvent>,
    pub t: Vec<f64>,
    pub e: Vec<f64>,
    pub n: Vec<f64>,
    pub alt: Vec<f64>,
    pub series: Vec<Series>,
    pub mc: Vec<McLanding>,
    pub sweep: Vec<SweepLanding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Conditions {
    pub wind_speed: f64,
    pub wind_dir_deg: f64,
    pub turbulence: f64,
    pub rod_angle_deg: f64,
    pub rod_dir_deg: f64,
    pub rod_length: f64,
    pub launch_into_wind: bool,
    pub lat: f64,
    pub lon: f64,
    pub site_alt: f64,
}

impl Conditions {
    /// From an engine's `conditions` (protocol parameter keys).
    pub fn from_protocol(c: &Map<String, Value>) -> Self {
        let f = |k: &str| c.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        Self {
            wind_speed: f("wind_speed"),
            wind_dir_deg: f("wind_direction"),
            turbulence: f("turbulence_intensity"),
            rod_angle_deg: f("rod_angle"),
            rod_dir_deg: f("rod_direction"),
            rod_length: f("rod_length"),
            launch_into_wind: c.get("launch_into_wind").and_then(Value::as_bool).unwrap_or(false),
            lat: f("launch_latitude"),
            lon: f("launch_longitude"),
            site_alt: f("launch_altitude"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SummaryRecord {
    pub max_alt: f64,
    pub max_v: f64,
    pub apogee_t: f64,
    pub flight_t: f64,
    pub ground_v: f64,
    pub rod_v: f64,
}

impl SummaryRecord {
    pub fn from_run(r: &RunResult) -> Self {
        let s = &r.summary;
        let v = |x: Option<f64>| x.unwrap_or(f64::NAN);
        Self {
            max_alt: v(s.max_alt),
            max_v: v(s.max_speed),
            apogee_t: v(s.apogee_time),
            flight_t: v(s.flight_time),
            ground_v: v(s.landing_speed),
            rod_v: v(s.rod_exit_speed),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct McLanding {
    pub e: f64,
    pub n: f64,
    pub wind: f64,
    pub dir: f64,
    pub apogee: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SweepLanding {
    pub wind: f64,
    pub e: f64,
    pub n: f64,
    pub apogee: f64,
}

impl FlightFile {
    /// Compact JSON. The bulky per-sample data (trajectory, series, Monte Carlo, sweep) is rounded to 6 significant
    /// digits, as rocketviz.py did: files shrink to about a third and stay far more precise than the simulation.
    /// Coordinates, conditions and summaries keep full precision (rounding the pad latitude would move it ~10 m).
    /// NaN and infinities become null.
    pub fn to_json(&self) -> serde_json::Result<String> {
        let mut v = serde_json::to_value(self)?;
        if let Some(sims) = v.get_mut("sims").and_then(Value::as_array_mut) {
            for sim in sims {
                for key in ["t", "e", "n", "alt", "series", "mc", "sweep"] {
                    if let Some(x) = sim.get_mut(key) {
                        round_numbers(x);
                    }
                }
            }
        }
        serde_json::to_string(&v)
    }
}

fn round_numbers(v: &mut Value) {
    match v {
        Value::Number(n) if n.is_f64() => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            *v = significant(x, 6)
                .and_then(|r| serde_json::Number::from_f64(r).map(Value::Number))
                .unwrap_or(Value::Null);
        }
        Value::Array(a) => a.iter_mut().for_each(round_numbers),
        Value::Object(o) => o.values_mut().for_each(round_numbers),
        _ => {}
    }
}

/// `x` rounded to `digits` significant digits; `None` for non-finite values.
pub fn significant(x: f64, digits: i32) -> Option<f64> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some(0.0);
    }
    // Format/parse round-trips exactly like Python's float(f"{x:.6g}").
    format!("{:.*e}", (digits - 1) as usize, x).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn significant_matches_python_g_format() {
        // float(f"{x:.6g}") in Python
        assert_eq!(significant(152.28539160, 6), Some(152.285));
        assert_eq!(significant(-0.0021406, 6), Some(-0.0021406));
        assert_eq!(significant(38.957760001, 6), Some(38.9578));
        assert_eq!(significant(123456789.0, 6), Some(123457000.0));
        assert_eq!(significant(f64::NAN, 6), None);
    }
}
