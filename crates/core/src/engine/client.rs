//! A typed client for one engine process.

use std::path::Path;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::manifest::EngineSpec;
use super::protocol::{
    BatchResult, Case, InitializeResult, LoadResult, Output, Overrides, Progress, RunResult, PROTOCOL_VERSION,
};
use super::rpc::{CancelToken, Connection};
use super::runtime::Runtimes;
use crate::error::{Error, Result};

/// A running, initialized engine process.
pub struct Engine {
    conn: Connection,
    info: InitializeResult,
}

impl Engine {
    /// Starts the engine and performs the `initialize` handshake.
    pub async fn start(spec: &EngineSpec, runtimes: &Runtimes) -> Result<Self> {
        let conn = Connection::spawn(spec, runtimes)?;
        Self::initialize(conn).await
    }

    /// Initializes an engine reached over an existing connection.
    pub async fn initialize(conn: Connection) -> Result<Self> {
        let host = json!({ "name": "rocketviz", "version": env!("CARGO_PKG_VERSION") });
        let v = conn.request("initialize", json!({ "protocol_version": PROTOCOL_VERSION, "host": host })).await?;
        let info: InitializeResult = parse(&conn, v)?;
        Ok(Self { conn, info })
    }

    pub fn info(&self) -> &InitializeResult {
        &self.info
    }

    pub fn is_alive(&self) -> bool {
        self.conn.is_alive()
    }

    pub fn stderr_tail(&self) -> String {
        self.conn.stderr_tail()
    }

    pub async fn load(&self, path: &Path) -> Result<LoadResult> {
        let v = self.conn.request("load", json!({ "path": path.to_string_lossy() })).await?;
        parse(&self.conn, v)
    }

    pub async fn unload(&self, design_id: &str) -> Result<()> {
        self.conn.request("unload", json!({ "design_id": design_id })).await.map(|_| ())
    }

    pub async fn run(
        &self,
        design_id: &str,
        config_id: &str,
        overrides: &Overrides,
        output: Output,
    ) -> Result<RunResult> {
        let v = self
            .conn
            .request(
                "run",
                json!({ "design_id": design_id, "config_id": config_id, "overrides": overrides, "output": output }),
            )
            .await?;
        parse(&self.conn, v)
    }

    /// Runs a batch; each finished case is sent to `progress` as it completes.
    pub async fn run_batch(
        &self,
        design_id: &str,
        config_id: &str,
        cases: &[Case],
        output: Output,
        progress: mpsc::UnboundedSender<Progress>,
        cancel: &CancelToken,
    ) -> Result<BatchResult> {
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<Value>();
        let forward = tokio::spawn(async move {
            while let Some(v) = raw_rx.recv().await {
                match serde_json::from_value::<Progress>(v) {
                    Ok(p) => {
                        let _ = progress.send(p);
                    }
                    Err(e) => tracing::warn!("malformed progress notification: {e}"),
                }
            }
        });
        let params = json!({ "design_id": design_id, "config_id": config_id, "cases": cases, "output": output });
        let r = self.conn.request_with_progress("run_batch", params, raw_tx, cancel).await;
        let _ = forward.await;
        parse(&self.conn, r?)
    }

    pub async fn shutdown(&self) {
        self.conn.shutdown().await;
    }

    /// Kills the process without a protocol shutdown (crash-recovery tests).
    #[doc(hidden)]
    pub async fn kill(&self) {
        self.conn.kill().await;
    }
}

fn parse<T: serde::de::DeserializeOwned>(conn: &Connection, v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|e| Error::Protocol { engine: conn.name().to_string(), message: e.to_string() })
}
