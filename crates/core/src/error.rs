use std::path::PathBuf;

/// Errors from the rocketviz core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("engine manifest {path}: {message}")]
    Manifest { path: PathBuf, message: String },

    #[error("{runtime} runtime not found: {message}")]
    RuntimeNotFound { runtime: String, message: String },

    #[error("couldn't start engine {engine}: {source}")]
    Spawn {
        engine: String,
        #[source]
        source: std::io::Error,
    },

    /// The engine process exited or closed its output. `stderr` is the tail of what it logged.
    #[error("engine {engine} exited unexpectedly{}", stderr_suffix(.stderr))]
    EngineExited { engine: String, stderr: String },

    /// The engine answered with a JSON-RPC error.
    #[error("{message} (engine error {code})")]
    Rpc { code: i64, message: String, data: Option<serde_json::Value> },

    /// The engine sent something that isn't valid protocol.
    #[error("protocol error from engine {engine}: {message}")]
    Protocol { engine: String, message: String },

    #[error("cancelled")]
    Cancelled,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn stderr_suffix(stderr: &str) -> String {
    if stderr.trim().is_empty() {
        String::new()
    } else {
        format!(":\n{}", stderr.trim_end())
    }
}

impl Error {
    /// The JSON-RPC error code, if this is an engine-reported error.
    pub fn rpc_code(&self) -> Option<i64> {
        match self {
            Error::Rpc { code, .. } => Some(*code),
            _ => None,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
