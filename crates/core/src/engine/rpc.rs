//! JSON-RPC 2.0 client over newline-delimited JSON (docs/engine-protocol.md §2).
//!
//! A reader task routes each response to its waiting request and each `progress` notification to the request it
//! belongs to. If the engine exits, every pending request fails with [`Error::EngineExited`] and the tail of the
//! engine's stderr.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Notify};

use super::manifest::EngineSpec;
use super::protocol::codes;
use super::runtime::Runtimes;
use crate::error::{Error, Result};

const STDERR_TAIL_LINES: usize = 60;

/// Cooperative cancellation for long requests (batches).
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<(AtomicBool, Notify)>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0 .0.store(true, Ordering::SeqCst);
        self.0 .1.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0 .0.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        loop {
            let notified = self.0 .1.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

type Pending = oneshot::Sender<Result<Value>>;

#[derive(Default)]
struct Shared {
    pending: Mutex<HashMap<u64, Pending>>,
    progress: Mutex<HashMap<u64, mpsc::UnboundedSender<Value>>>,
    closed: AtomicBool,
    stderr: Mutex<VecDeque<String>>,
}

impl Shared {
    fn stderr_tail(&self) -> String {
        self.stderr.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n")
    }

    fn close(&self, name: &str) {
        self.closed.store(true, Ordering::SeqCst);
        let stderr = self.stderr_tail();
        for (_, tx) in self.pending.lock().unwrap().drain() {
            let _ = tx.send(Err(Error::EngineExited { engine: name.to_string(), stderr: stderr.clone() }));
        }
        self.progress.lock().unwrap().clear();
    }
}

/// A connection to one engine process.
pub struct Connection {
    name: String,
    writer: tokio::sync::Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
    child: Option<tokio::sync::Mutex<Child>>,
}

impl Connection {
    /// Starts the engine described by `spec`.
    pub fn spawn(spec: &EngineSpec, runtimes: &Runtimes) -> Result<Self> {
        let (program, args) = runtimes.command(spec)?;
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .current_dir(&spec.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // CREATE_NO_WINDOW: no console window popping up from the GUI app.
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        // Inside an AppImage the launcher points LD_LIBRARY_PATH at the AppImage's own libraries, which include copies
        // of some of the bundled Java runtime's; engines must load their runtime's own.
        if std::env::var_os("APPIMAGE").is_some() {
            cmd.env_remove("LD_LIBRARY_PATH");
        }
        let mut child = cmd.spawn().map_err(|source| Error::Spawn { engine: spec.manifest.id.clone(), source })?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let mut conn = Self::over(spec.manifest.id.clone(), stdout, stdin);
        let shared = conn.shared.clone();
        let name = conn.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(engine = %name, "{line}");
                let mut tail = shared.stderr.lock().unwrap();
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });
        conn.child = Some(tokio::sync::Mutex::new(child));
        Ok(conn)
    }

    /// A connection over arbitrary streams (tests, or engines reached some other way).
    pub fn over(
        name: String,
        reader: impl AsyncRead + Send + Unpin + 'static,
        writer: impl AsyncWrite + Send + Unpin + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let conn = Self {
            name: name.clone(),
            writer: tokio::sync::Mutex::new(Box::new(writer)),
            shared: shared.clone(),
            next_id: AtomicU64::new(1),
            child: None,
        };
        tokio::spawn(read_loop(name, reader, shared));
        conn
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// False once the engine has exited or its output closed.
    pub fn is_alive(&self) -> bool {
        !self.shared.closed.load(Ordering::SeqCst)
    }

    /// Recent stderr output from the engine, for diagnostics.
    pub fn stderr_tail(&self) -> String {
        self.shared.stderr_tail()
    }

    /// Sends a request and waits for its result.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let (_, rx) = self.start(method, params, None).await?;
        rx.await.unwrap_or_else(|_| Err(self.exited()))
    }

    /// Sends a request whose `progress` notifications go to `progress`, and cancels it (via `$/cancelRequest`) if
    /// `cancel` fires. A cancelled request returns [`Error::Cancelled`].
    pub async fn request_with_progress(
        &self,
        method: &str,
        params: Value,
        progress: mpsc::UnboundedSender<Value>,
        cancel: &CancelToken,
    ) -> Result<Value> {
        let (id, mut rx) = self.start(method, params, Some(progress)).await?;
        let result = tokio::select! {
            r = &mut rx => r.unwrap_or_else(|_| Err(self.exited())),
            _ = cancel.cancelled() => {
                self.notify("$/cancelRequest", json!({ "id": id })).await?;
                rx.await.unwrap_or_else(|_| Err(self.exited()))
            }
        };
        self.shared.progress.lock().unwrap().remove(&id);
        match result {
            Err(Error::Rpc { code: codes::REQUEST_CANCELLED, .. }) => Err(Error::Cancelled),
            r => r,
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params })).await
    }

    /// Asks the engine to shut down and waits briefly for it to exit; kills it if it doesn't.
    pub async fn shutdown(&self) {
        if self.is_alive() {
            let _ = tokio::time::timeout(Duration::from_secs(5), self.request("shutdown", Value::Null)).await;
            let _ = self.notify("exit", Value::Null).await;
        }
        if let Some(child) = &self.child {
            let mut child = child.lock().await;
            if tokio::time::timeout(Duration::from_secs(5), child.wait()).await.is_err() {
                let _ = child.kill().await;
            }
        }
    }

    /// Kills the engine process immediately.
    pub async fn kill(&self) {
        if let Some(child) = &self.child {
            let _ = child.lock().await.kill().await;
        }
    }

    async fn start(
        &self,
        method: &str,
        params: Value,
        progress: Option<mpsc::UnboundedSender<Value>>,
    ) -> Result<(u64, oneshot::Receiver<Result<Value>>)> {
        if !self.is_alive() {
            return Err(self.exited());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.shared.pending.lock().unwrap().insert(id, tx);
        if let Some(p) = progress {
            self.shared.progress.lock().unwrap().insert(id, p);
        }
        let params = if params.is_null() { json!({}) } else { params };
        if let Err(e) = self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await {
            self.shared.pending.lock().unwrap().remove(&id);
            self.shared.progress.lock().unwrap().remove(&id);
            return Err(e);
        }
        Ok((id, rx))
    }

    async fn write(&self, msg: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&msg)?;
        line.push(b'\n');
        let mut w = self.writer.lock().await;
        let r = async {
            w.write_all(&line).await?;
            w.flush().await
        }
        .await;
        r.map_err(|_| self.exited())
    }

    fn exited(&self) -> Error {
        Error::EngineExited { engine: self.name.clone(), stderr: self.stderr_tail() }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.shared.close(&self.name);
    }
}

async fn read_loop(name: String, reader: impl AsyncRead + Unpin, shared: Arc<Shared>) {
    let mut lines = BufReader::new(reader).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) | Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(engine = %name, "non-protocol output on stdout ({e}): {line:.200}");
                continue;
            }
        };
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            let Some(tx) = shared.pending.lock().unwrap().remove(&id) else {
                tracing::warn!(engine = %name, "response to unknown request {id}");
                continue;
            };
            let result = match (msg.get("result"), msg.get("error")) {
                (_, Some(err)) => Err(Error::Rpc {
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(codes::INTERNAL_ERROR),
                    message: err.get("message").and_then(Value::as_str).unwrap_or("engine error").to_string(),
                    data: err.get("data").cloned(),
                }),
                (Some(result), None) => Ok(result.clone()),
                (None, None) => Err(Error::Protocol {
                    engine: name.clone(),
                    message: format!("response {id} has neither result nor error"),
                }),
            };
            let _ = tx.send(result);
        } else if msg.get("method").and_then(Value::as_str) == Some("progress") {
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            if let Some(id) = params.get("request_id").and_then(Value::as_u64) {
                if let Some(tx) = shared.progress.lock().unwrap().get(&id) {
                    let _ = tx.send(params);
                }
            }
        }
    }
    shared.close(&name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, split, DuplexStream};

    /// A scripted fake engine: replies to each request with `respond(method, params)`.
    fn fake_engine(respond: impl Fn(&str, &Value) -> Value + Send + 'static) -> Connection {
        let (client, server) = duplex(1 << 16);
        let (cr, cw) = split(client);
        tokio::spawn(async move {
            let (sr, mut sw) = split::<DuplexStream>(server);
            let mut lines = BufReader::new(sr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg: Value = serde_json::from_str(&line).unwrap();
                let Some(id) = msg.get("id").cloned() else { continue };
                let method = msg["method"].as_str().unwrap().to_string();
                let reply = respond(&method, &msg["params"]);
                let out = if let Some(err) = reply.get("error") {
                    json!({"jsonrpc": "2.0", "id": id, "error": err})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "result": reply})
                };
                sw.write_all(format!("{out}\n").as_bytes()).await.unwrap();
            }
        });
        Connection::over("fake".into(), cr, cw)
    }

    #[tokio::test]
    async fn request_response_and_errors() {
        let c = fake_engine(|m, p| match m {
            "echo" => p.clone(),
            _ => json!({"error": {"code": -32601, "message": "Method not found"}}),
        });
        assert_eq!(c.request("echo", json!({"x": 1})).await.unwrap(), json!({"x": 1}));
        let err = c.request("nope", Value::Null).await.unwrap_err();
        assert_eq!(err.rpc_code(), Some(codes::METHOD_NOT_FOUND));
    }

    #[tokio::test]
    async fn concurrent_requests_get_their_own_responses() {
        let c = Arc::new(fake_engine(|_, p| p.clone()));
        let tasks: Vec<_> = (0..50)
            .map(|i| {
                let c = c.clone();
                tokio::spawn(async move { c.request("echo", json!({"i": i})).await.unwrap()["i"].as_i64().unwrap() })
            })
            .collect();
        for (i, t) in tasks.into_iter().enumerate() {
            assert_eq!(t.await.unwrap(), i as i64);
        }
    }

    #[tokio::test]
    async fn engine_exit_fails_pending_requests() {
        let (client, server) = duplex(1024);
        let (cr, cw) = split(client);
        let c = Connection::over("dying".into(), cr, cw);
        let pending = tokio::spawn(async move { c.request("slow", Value::Null).await });
        tokio::task::yield_now().await;
        drop(server);
        assert!(matches!(pending.await.unwrap(), Err(Error::EngineExited { .. })));
    }

    #[tokio::test]
    async fn cancel_token_wakes_waiters() {
        let t = CancelToken::new();
        let t2 = t.clone();
        let w = tokio::spawn(async move { t2.cancelled().await });
        tokio::task::yield_now().await;
        t.cancel();
        tokio::time::timeout(Duration::from_secs(1), w).await.unwrap().unwrap();
        assert!(t.is_cancelled());
    }
}
