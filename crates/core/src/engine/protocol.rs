//! Message types of the rocketviz engine protocol, version 1 (docs/engine-protocol.md).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL_VERSION: u32 = 1;

/// `initialize` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: u32,
    pub engine: EngineIdentity,
    pub capabilities: Capabilities,
    pub parameters: Vec<ParameterDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineIdentity {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub batch: bool,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default = "one")]
    pub max_workers: u32,
    #[serde(default)]
    pub cancel: bool,
    #[serde(default)]
    pub multi_instance: bool,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterKind {
    Standard,
    Engine,
}

/// An override parameter the engine accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub unit: String,
    pub kind: ParameterKind,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// `load` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResult {
    pub design_id: String,
    pub name: String,
    pub configs: Vec<ConfigInfo>,
}

/// One runnable setup in a design (in OpenRocket, a simulation and its motor configuration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub motor: String,
    /// Launch conditions keyed by parameter key (numbers, plus engine extras such as booleans).
    #[serde(default)]
    pub defaults: Map<String, Value>,
}

/// Detail level of a run result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Landing,
    Summary,
    Track,
    #[default]
    Full,
}

/// Parameter overrides for one case, keyed by parameter key.
pub type Overrides = Map<String, Value>;

/// Result of one simulated case. Fields beyond `summary` and `landing` depend on the requested [`Output`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub summary: Summary,
    pub landing: Landing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<FlightEvent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Vec<Series>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub max_alt: Option<f64>,
    pub max_speed: Option<f64>,
    pub apogee_time: Option<f64>,
    pub flight_time: Option<f64>,
    pub landing_speed: Option<f64>,
    pub rod_exit_speed: Option<f64>,
}

/// Landing point, metres east and north of the pad.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Landing {
    pub e: Option<f64>,
    pub n: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlightEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub t: f64,
}

/// One recorded quantity, aligned with the run's `t`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Series {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub group: String,
    pub data: Vec<Option<f64>>,
}

/// One case of a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    pub id: String,
    #[serde(default)]
    pub overrides: Overrides,
}

/// `run_batch` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub results: Vec<CaseResult>,
    #[serde(default)]
    pub failed: Vec<CaseFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub case_id: String,
    pub result: RunResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseFailure {
    pub case_id: String,
    pub message: String,
}

/// `progress` notification: one case of a batch finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub request_id: Value,
    pub case_id: String,
    pub done: u32,
    pub total: u32,
    #[serde(default)]
    pub result: Option<RunResult>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Error codes of docs/engine-protocol.md §6.
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const UNSUPPORTED_PROTOCOL: i64 = -32010;
    pub const LOAD_FAILED: i64 = -32011;
    pub const UNKNOWN_DESIGN: i64 = -32012;
    pub const SIMULATION_FAILED: i64 = -32013;
    pub const BAD_OVERRIDE: i64 = -32014;
    pub const REQUEST_CANCELLED: i64 = -32800;
}
