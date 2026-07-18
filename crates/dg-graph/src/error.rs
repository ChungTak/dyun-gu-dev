use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("invalid graph spec at {path}: {message}")]
    Validation { path: String, message: String },
    #[error("unknown graph format for path: {0}")]
    UnknownFormat(PathBuf),
    #[error("unknown node kind: {0}")]
    UnknownNodeKind(String),
    #[error("unknown port {port} on node {node}")]
    UnknownPort { node: String, port: String },
    #[error("connection type mismatch from {from_node}.{from_port} to {to_node}.{to_port}")]
    PortTypeMismatch {
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
    },
    #[error("duplicate node name: {0}")]
    DuplicateNode(String),
    #[error("graph contains a cycle")]
    CycleDetected,
    #[error("element error in {element}: {message}")]
    Element { element: String, message: String },
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("graph is not running")]
    NotRunning,
    #[error("graph is not built: {0}")]
    NotBuilt(String),
    #[error("resource limit exceeded for {resource}: requested {requested}, limit {limit}")]
    ResourceLimit {
        resource: String,
        requested: usize,
        limit: usize,
    },
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("busy: {0}")]
    Busy(String),
    #[error("cancelled")]
    Cancelled,
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("invariant violation: {0}")]
    Invariant(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    RuntimeBackend(#[from] dg_runtime::Error),
    #[error(transparent)]
    Core(#[from] dg_core::Error),
}
