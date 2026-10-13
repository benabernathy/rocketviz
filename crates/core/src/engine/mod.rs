//! Hosting simulation engines: separate processes speaking the rocketviz engine protocol (docs/engine-protocol.md).

pub mod client;
pub mod manifest;
pub mod pool;
pub mod protocol;
pub mod rpc;
pub mod runtime;

pub use client::Engine;
pub use manifest::EngineSpec;
pub use pool::{Design, EnginePool};
pub use rpc::CancelToken;
pub use runtime::Runtimes;
