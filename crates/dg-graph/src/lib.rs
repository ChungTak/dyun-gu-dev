#![forbid(unsafe_code)]

//! Graph execution, configuration, and pipeline orchestration.
//!
//! `dg-graph` owns the stream-oriented execution model for composing registered
//! elements into a runnable DAG. It also defines the versioned, format-agnostic
//! `GraphSpec` configuration model used to load and build graphs.

mod builtin;
mod element;
mod engine;
mod error;
mod fault;
mod inference;
mod metrics;
mod packet;
mod pipe;
mod pool;
mod registry;
mod schema;
mod spec;

pub use element::{CreatedElement, Element, ElementHandle, ElementIo, PortSchema, SinkCollector};
pub use engine::{watch, Graph, GraphDiff, GraphReport, GraphStatus, RunningGraph, WatchHandle};
pub use error::{Error, ErrorScope, Result};
pub use fault::{
    arm as arm_hot_update_fault, clear as clear_hot_update_fault,
    exclusive as exclusive_hot_update_fault, HotUpdateFaultGuard, HotUpdateFaultPoint,
};
pub use inventory;
pub use metrics::{ElementMetricsSnapshot, MetricsSink};
pub use packet::{Packet, PacketMeta, PacketPayload};
pub use pipe::{DataPipe, PipeReceiver, PipeSender, DEFAULT_QUEUE_CAPACITY};
pub use pool::ThreadPool;
pub use registry::{
    create_element, element_ports, find_element, registered_elements, validate_element,
    ElementDescriptor,
};
pub use schema::{
    all_element_schemas, element_params_schema, params_json_schema, ParamField, ParamType,
};
pub use spec::{
    ConnectionSpec, DefaultsSpec, DeviceDefault, DeviceDefaultDetails, ExecutionSpec, GraphFormat,
    GraphSpec, GraphSpecBuilder, NodeSpec, NodeTemplate, ParallelType,
};

// Bring built-in registrations into the inventory at link time.
// The module is intentionally private; the submit! calls are the important side effect.
