use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Scope of an error within a graph execution.
///
/// The scope determines whether the graph should drop a single frame, retry
/// or reconnect a stream, fail a single node, or fail the entire graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorScope {
    /// Drop the current frame and continue the same stream.
    FrameLocal,
    /// Close the old endpoint and reconnect within budget.
    StreamLocal,
    /// Stop the failing node and fail-closed the graph.
    NodeFatal,
    /// Save the first root cause and stop all workers.
    GraphFatal,
}

impl ErrorScope {
    /// Returns true if this scope should move the graph to Failed.
    pub fn is_fatal(&self) -> bool {
        matches!(self, ErrorScope::NodeFatal | ErrorScope::GraphFatal)
    }
}

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
    /// Single-frame data-plane error: drop the frame and continue the stream.
    #[error("bad frame in {element}: {message}")]
    BadFrame { element: String, message: String },
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

impl Error {
    /// Returns the error scope used to decide whether the graph should drop a
    /// frame, reconnect a stream, fail the node, or fail the entire graph.
    pub fn scope(&self) -> ErrorScope {
        match self {
            // Cancellation / not-running are frame-local control signals; they do
            // not imply any persistent failure.
            Error::Cancelled | Error::NotRunning => ErrorScope::FrameLocal,
            // Single-frame data errors (NaN, shape, decode corruption) drop and
            // continue without failing the graph or sibling streams.
            Error::BadFrame { .. } => ErrorScope::FrameLocal,
            // Invariant violations corrupt shared graph state and are always
            // graph-fatal. Worker panics are reported as Invariant.
            Error::Invariant(_) => ErrorScope::GraphFatal,
            // All other errors stop the failing node and fail-closed the graph.
            // This includes config/model/capability/resource errors and
            // element-level runtime failures.
            _ => ErrorScope::NodeFatal,
        }
    }

    /// Returns true for cooperative stop / not-running signals (not data drops).
    pub fn is_cancellation(&self) -> bool {
        matches!(self, Error::Cancelled | Error::NotRunning)
    }
}
