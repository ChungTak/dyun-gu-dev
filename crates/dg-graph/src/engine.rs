use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use dg_core::{Classification, Detection, FaceDetection, OcrText, ResourcePolicy, Tensor, Track};
use tracing::{error, info};

use crate::element::{Element, ElementHandle, ElementIo, EosState};
use crate::error::{Error, Result};
use crate::metrics::{ElementMetrics, ElementMetricsSnapshot, MetricsSink};
use crate::pipe::{DataPipe, PipeReceiver, PipeSender};
use crate::registry::create_element;
use crate::spec::{ConnectionSpec, GraphSpec, NodeSpec, ParallelType};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphDiff {
    pub added_nodes: Vec<NodeSpec>,
    pub removed_nodes: Vec<String>,
    pub updated_nodes: Vec<NodeSpec>,
    pub added_connections: Vec<String>,
    pub removed_connections: Vec<String>,
}

impl GraphDiff {
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.updated_nodes.is_empty()
            && self.added_connections.is_empty()
            && self.removed_connections.is_empty()
    }

    pub fn apply(self, graph: &mut Graph) -> Result<()> {
        let new_spec = graph.spec.clone().merge_for_diff(self)?;
        graph.reload(new_spec)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct GraphReport {
    pub sinks: BTreeMap<String, Vec<Tensor>>,
    pub detections: BTreeMap<String, Vec<Detection>>,
    pub classifications: BTreeMap<String, Vec<Classification>>,
    pub faces: BTreeMap<String, Vec<FaceDetection>>,
    pub tracks: BTreeMap<String, Vec<Track>>,
    pub ocr: BTreeMap<String, Vec<OcrText>>,
    pub element_metrics: BTreeMap<String, ElementMetricsSnapshot>,
}

impl GraphReport {
    pub fn export_metrics(&self, sink: &dyn MetricsSink) {
        for (node, metrics) in &self.element_metrics {
            sink.record(node, metrics);
        }
    }
}

type SinkMap = BTreeMap<String, Arc<Mutex<crate::element::SinkCollector>>>;

pub struct Graph {
    spec: GraphSpec,
    policy: Arc<ResourcePolicy>,
}

/// Lifecycle status of a [`RunningGraph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GraphStatus {
    Starting = 0,
    Running = 1,
    Draining = 2,
    Stopped = 3,
    Failed = 4,
    /// Transactional hot-update in progress (readiness should be false).
    Reloading = 5,
}

fn status_from_u8(value: u8) -> GraphStatus {
    match value {
        0 => GraphStatus::Starting,
        1 => GraphStatus::Running,
        2 => GraphStatus::Draining,
        3 => GraphStatus::Stopped,
        5 => GraphStatus::Reloading,
        _ => GraphStatus::Failed,
    }
}

/// A live execution of a graph. Workers and packet routes remain owned by
/// this handle until [`RunningGraph::finish`] joins them.
pub struct RunningGraph {
    spec: GraphSpec,
    policy: Arc<ResourcePolicy>,
    stop: Arc<AtomicBool>,
    workers: BTreeMap<String, LiveNode>,
    routes: RuntimeRoutes,
    sinks: SinkMap,
    metrics: BTreeMap<String, Arc<ElementMetrics>>,
    status: Arc<AtomicU8>,
    root_cause: Arc<Mutex<Option<String>>>,
}

impl Graph {
    pub fn new(spec: GraphSpec) -> Result<Self> {
        Self::new_with_policy(spec, ResourcePolicy::default())
    }

    pub fn new_with_policy(spec: GraphSpec, policy: ResourcePolicy) -> Result<Self> {
        let requested = ResourcePolicy::from(&spec.limits);
        let effective = policy
            .effective_for(&requested)
            .map_err(|err| Error::Validation {
                path: "limits".to_string(),
                message: err.to_string(),
            })?;
        spec.validate_with_policy(&effective)?;
        Ok(Self {
            spec,
            policy: Arc::new(policy),
        })
    }

    pub fn policy(&self) -> &ResourcePolicy {
        &self.policy
    }

    pub fn spec(&self) -> &GraphSpec {
        &self.spec
    }

    pub fn diff(old: &GraphSpec, new: &GraphSpec) -> GraphDiff {
        let old_nodes: BTreeMap<_, _> = old
            .nodes
            .iter()
            .map(|node| (node.name.clone(), node.clone()))
            .collect();
        let new_nodes: BTreeMap<_, _> = new
            .nodes
            .iter()
            .map(|node| (node.name.clone(), node.clone()))
            .collect();

        let mut added_nodes = Vec::new();
        let mut removed_nodes = Vec::new();
        let mut updated_nodes = Vec::new();
        for (name, node) in &new_nodes {
            match old_nodes.get(name) {
                None => added_nodes.push(node.clone()),
                Some(existing) if existing != node => updated_nodes.push(node.clone()),
                Some(_) => {}
            }
        }
        for name in old_nodes.keys() {
            if !new_nodes.contains_key(name) {
                removed_nodes.push(name.clone());
            }
        }

        let old_connections = old
            .connections
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let new_connections = new
            .connections
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let added_connections = new_connections
            .difference(&old_connections)
            .cloned()
            .collect();
        let removed_connections = old_connections
            .difference(&new_connections)
            .cloned()
            .collect();

        GraphDiff {
            added_nodes,
            removed_nodes,
            updated_nodes,
            added_connections,
            removed_connections,
        }
    }

    pub fn reload(&mut self, spec: GraphSpec) -> Result<GraphDiff> {
        let diff = Self::diff(&self.spec, &spec);
        let requested = ResourcePolicy::from(&spec.limits);
        let effective = self
            .policy
            .effective_for(&requested)
            .map_err(|err| Error::Validation {
                path: "limits".to_string(),
                message: err.to_string(),
            })?;
        spec.validate_with_policy(&effective)?;
        self.spec = spec;
        Ok(diff)
    }

    pub fn run(&self) -> Result<GraphReport> {
        self.run_with_inputs(HashMap::new())
    }

    pub fn run_with_inputs(&self, inputs: HashMap<String, Vec<Tensor>>) -> Result<GraphReport> {
        info!(
            node_count = self.spec.nodes.len(),
            parallel = ?self.spec.execution.parallel,
            queue_capacity = self.spec.execution.queue_capacity,
            "starting graph execution"
        );
        self.start(inputs)?.finish()
    }

    /// Starts the graph without blocking the caller.
    pub fn start(&self, inputs: HashMap<String, Vec<Tensor>>) -> Result<RunningGraph> {
        let (runtime, sinks, metrics) =
            RuntimeGraph::build(self.spec.clone(), inputs, Arc::clone(&self.policy))?;
        runtime.start(sinks, metrics)
    }
}

pub struct RuntimeGraph {
    nodes: Vec<ExecNode>,
    routes: RuntimeRoutes,
    spec: GraphSpec,
    policy: Arc<ResourcePolicy>,
    stop: Arc<AtomicBool>,
}

#[derive(Clone)]
struct RuntimeRoutes {
    edges: BTreeMap<String, EdgeRoute>,
    inputs: BTreeMap<(String, String), Arc<Mutex<PipeReceiver>>>,
    outputs: BTreeMap<(String, String), Arc<Mutex<Vec<PipeSender>>>>,
}

#[derive(Clone)]
struct EdgeRoute {
    sender: PipeSender,
    receiver: Arc<Mutex<PipeReceiver>>,
}

struct LiveNode {
    control: Arc<crate::element::NodeControl>,
    workers: Vec<thread::JoinHandle<Result<()>>>,
}

impl RuntimeGraph {
    fn build(
        spec: GraphSpec,
        inputs: HashMap<String, Vec<Tensor>>,
        policy: Arc<ResourcePolicy>,
    ) -> Result<(Self, SinkMap, BTreeMap<String, Arc<ElementMetrics>>)> {
        let stop = Arc::new(AtomicBool::new(false));
        let mut nodes: BTreeMap<String, NodeRuntime> = BTreeMap::new();
        for node in &spec.nodes {
            let threads = node.threads.unwrap_or(1);
            let created = create_element(node)?;
            if threads > 1
                && (node.kind == "source" || !matches!(&created.handle, ElementHandle::None))
            {
                return Err(Error::Config(format!(
                    "node {} cannot be multi-instanced because source elements and elements with special handles are single-instance",
                    node.name,
                )));
            }
            let handle = created.handle;
            let mut elements = vec![created.element];
            for _ in 1..threads {
                let created = create_element(node)?;
                if node.kind == "source" || !matches!(&created.handle, ElementHandle::None) {
                    return Err(Error::Config(format!(
                        "node {} cannot be multi-instanced because source elements and elements with special handles are single-instance",
                        node.name,
                    )));
                }
                elements.push(created.element);
            }
            nodes.insert(
                node.name.clone(),
                NodeRuntime {
                    name: node.name.clone(),
                    elements,
                    handle,
                    inputs: HashMap::new(),
                    outputs: HashMap::new(),
                },
            );
        }

        let mut sinks = BTreeMap::new();
        let mut input_queues = BTreeMap::new();
        for (name, node) in &mut nodes {
            if let ElementHandle::Sink(collector) = &node.handle {
                sinks.insert(name.clone(), collector.clone());
            } else if let ElementHandle::Input(queue) = &node.handle {
                input_queues.insert(name.clone(), queue.clone());
            }
        }

        for (name, tensors) in inputs {
            let queue = input_queues.get(&name).ok_or_else(|| {
                Error::Config(format!("unknown input node {} for injected tensors", name))
            })?;
            let mut guard = queue
                .lock()
                .map_err(|_| Error::Runtime("input queue poisoned".to_string()))?;
            guard.extend(tensors);
        }

        let mut edge_routes = BTreeMap::new();
        let mut input_routes = BTreeMap::new();
        let mut output_routes = BTreeMap::new();
        for connection in &spec.connections {
            let parsed = ConnectionSpec::parse(connection)?;
            let pipe = match spec.execution.parallel {
                ParallelType::Pipeline => DataPipe::bounded(spec.execution.queue_capacity),
                ParallelType::Sequential | ParallelType::Task => DataPipe::unbounded(),
            };
            let (sender, receiver) = pipe.split();
            let receiver = Arc::new(Mutex::new(receiver));
            {
                let src = nodes.get_mut(&parsed.from_node).ok_or_else(|| {
                    Error::Config(format!("missing source node {}", parsed.from_node))
                })?;
                src.outputs
                    .entry(parsed.from_port.clone())
                    .or_default()
                    .push(sender.clone());
            }
            let dst = nodes.get_mut(&parsed.to_node).ok_or_else(|| {
                Error::Config(format!("missing destination node {}", parsed.to_node))
            })?;
            if dst.inputs.contains_key(&parsed.to_port) {
                return Err(Error::Config(format!(
                    "multiple inbound edges to {}.{} are not supported",
                    parsed.to_node, parsed.to_port
                )));
            }
            dst.inputs.insert(parsed.to_port.clone(), receiver.clone());
            edge_routes.insert(
                connection.clone(),
                EdgeRoute {
                    sender,
                    receiver: receiver.clone(),
                },
            );
        }

        for node in nodes.values() {
            for (port, receiver) in &node.inputs {
                input_routes.insert((node.name.clone(), port.clone()), receiver.clone());
            }
            for (port, senders) in &node.outputs {
                output_routes.insert(
                    (node.name.clone(), port.clone()),
                    Arc::new(Mutex::new(senders.clone())),
                );
            }
        }

        for node in nodes.values() {
            for port in node.inputs.keys() {
                if !spec.connections.iter().any(|conn| {
                    ConnectionSpec::parse(conn).ok().is_some_and(|parsed| {
                        parsed.to_node == node.name && parsed.to_port == *port
                    })
                }) {
                    return Err(Error::Config(format!(
                        "input port {}.{} has no upstream connection",
                        node.name, port
                    )));
                }
            }
        }

        let total_elements = nodes.values().map(|node| node.elements.len()).sum();
        let mut exec_nodes = Vec::with_capacity(total_elements);
        let mut metrics = BTreeMap::new();
        for node in nodes.into_values() {
            let node_metrics = Arc::new(ElementMetrics::default());
            metrics.insert(node.name.clone(), node_metrics.clone());
            let eos = Arc::new(Mutex::new(EosState {
                seen: false,
                broadcasts: 0,
                instances: node.elements.len(),
            }));
            let control = Arc::new(crate::element::NodeControl::default());
            for element in node.elements {
                let io = ElementIo {
                    name: node.name.clone(),
                    inputs: node
                        .inputs
                        .iter()
                        .map(|(port, receiver)| (port.clone(), receiver.clone()))
                        .collect(),
                    outputs: node
                        .outputs
                        .iter()
                        .map(|(port, senders)| {
                            (
                                port.clone(),
                                output_routes
                                    .get(&(node.name.clone(), port.clone()))
                                    .cloned()
                                    .unwrap_or_else(|| Arc::new(Mutex::new(senders.clone()))),
                            )
                        })
                        .collect(),
                    stop: stop.clone(),
                    control: control.clone(),
                    send_backoff: Duration::from_millis(1),
                    eos: eos.clone(),
                    metrics: node_metrics.clone(),
                    packet_starts: std::cell::RefCell::new(VecDeque::new()),
                    policy: Arc::clone(&policy),
                };
                exec_nodes.push(ExecNode {
                    name: node.name.clone(),
                    element,
                    io,
                });
            }
        }

        Ok((
            Self {
                nodes: exec_nodes,
                routes: RuntimeRoutes {
                    edges: edge_routes,
                    inputs: input_routes,
                    outputs: output_routes,
                },
                spec: spec.clone(),
                policy,
                stop,
            },
            sinks,
            metrics,
        ))
    }

    fn start(
        self,
        sinks: SinkMap,
        metrics: BTreeMap<String, Arc<ElementMetrics>>,
    ) -> Result<RunningGraph> {
        let mut workers = BTreeMap::new();
        let mut grouped: BTreeMap<String, Vec<ExecNode>> = BTreeMap::new();
        for node in self.nodes {
            grouped.entry(node.name.clone()).or_default().push(node);
        }
        let status = Arc::new(AtomicU8::new(GraphStatus::Starting as u8));
        let root_cause = Arc::new(Mutex::new(None));
        for node_spec in &self.spec.nodes {
            let exec_nodes = grouped.remove(&node_spec.name).ok_or_else(|| {
                Error::Runtime(format!("missing runtime node {}", node_spec.name))
            })?;
            let first = exec_nodes.first().ok_or_else(|| {
                Error::Runtime(format!("node {} has no executable workers", node_spec.name))
            })?;
            let control = first.io.control.clone();
            let mut handles = Vec::with_capacity(exec_nodes.len());
            for node in exec_nodes {
                let stop = self.stop.clone();
                handles.push(thread::spawn(move || {
                    run_element(node.element, node.io, &stop)
                }));
            }
            workers.insert(
                node_spec.name.clone(),
                LiveNode {
                    control,
                    workers: handles,
                },
            );
        }
        if !grouped.is_empty() {
            return Err(Error::Runtime("runtime contains unknown nodes".to_string()));
        }
        let running = RunningGraph {
            spec: self.spec,
            policy: self.policy,
            stop: self.stop,
            workers,
            routes: self.routes,
            sinks,
            metrics,
            status,
            root_cause,
        };
        running.set_status(GraphStatus::Running);
        Ok(running)
    }
}

impl RunningGraph {
    fn set_status(&self, status: GraphStatus) {
        self.status.store(status as u8, Ordering::SeqCst);
    }

    fn set_root_cause(&self, error: &Error) {
        if let Ok(mut guard) = self.root_cause.lock() {
            if guard.is_none() {
                *guard = Some(error.to_string());
            }
        }
    }

    /// Returns the current lifecycle status and, if any, the first root cause
    /// that moved the graph into [`GraphStatus::Failed`].
    pub fn status(&self) -> (GraphStatus, Option<String>) {
        let status = status_from_u8(self.status.load(Ordering::SeqCst));
        let root_cause = self.root_cause.lock().ok().and_then(|guard| guard.clone());
        (status, root_cause)
    }

    /// Idempotently requests a cooperative stop of all workers.
    pub fn request_stop(&self) {
        // Reloading is numeric 5; do not use integer comparison against Draining.
        let status = status_from_u8(self.status.load(Ordering::SeqCst));
        if matches!(
            status,
            GraphStatus::Starting | GraphStatus::Running | GraphStatus::Reloading
        ) {
            self.set_status(GraphStatus::Draining);
        }
        self.stop.store(true, Ordering::Relaxed);
        for node in self.workers.values() {
            node.control.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Shuts the graph down, waiting up to `timeout` for workers to join.
    /// A timeout keeps the graph in [`GraphStatus::Draining`] so the caller
    /// can retry.
    pub fn shutdown(&mut self, timeout: Duration) -> Result<()> {
        self.request_stop();
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| Error::Runtime("shutdown timeout overflowed".to_string()))?;
        let mut first_error = None;
        for node in self.workers.values_mut() {
            while let Some(worker) = node.workers.pop() {
                while !worker.is_finished() {
                    if Instant::now() >= deadline {
                        node.workers.push(worker);
                        return Err(Error::Runtime("shutdown timed out".to_string()));
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                match worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) if is_cancellation(&error) => {}
                    Ok(Err(error)) => first_error = select_error(first_error, error),
                    Err(_) => {
                        first_error = select_error(
                            first_error,
                            Error::Runtime("element worker panicked".to_string()),
                        );
                    }
                }
            }
        }
        if let Some(error) = first_error {
            self.set_root_cause(&error);
            self.set_status(GraphStatus::Failed);
            Err(error)
        } else {
            self.set_status(GraphStatus::Stopped);
            Ok(())
        }
    }

    /// Polls finished workers without blocking. If all workers have finished
    /// the status moves to [`GraphStatus::Stopped`]; a worker failure moves it
    /// to [`GraphStatus::Failed`] and returns the root cause.
    pub fn poll(&mut self) -> Result<()> {
        let mut first_error = None;
        let mut all_finished = true;
        for node in self.workers.values_mut() {
            let mut still_running = Vec::new();
            for worker in node.workers.drain(..) {
                if worker.is_finished() {
                    match worker.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) if is_cancellation(&error) => {}
                        Ok(Err(error)) => first_error = select_error(first_error, error),
                        Err(_) => {
                            first_error = select_error(
                                first_error,
                                Error::Runtime("element worker panicked".to_string()),
                            );
                        }
                    }
                } else {
                    all_finished = false;
                    still_running.push(worker);
                }
            }
            node.workers = still_running;
        }
        if let Some(error) = first_error {
            self.set_root_cause(&error);
            self.set_status(GraphStatus::Failed);
            return Err(error);
        }
        if all_finished && self.workers.values().all(|node| node.workers.is_empty()) {
            self.set_status(GraphStatus::Stopped);
        }
        Ok(())
    }

    /// Returns a snapshot of per-element metrics.
    pub fn metrics_snapshot(&self) -> BTreeMap<String, ElementMetricsSnapshot> {
        self.metrics
            .iter()
            .map(|(name, metrics)| (name.clone(), metrics.snapshot()))
            .collect()
    }

    /// Applies a validated graph diff while workers are running.
    ///
    /// The update follows the transaction boundary: validate, prepare, quiesce
    /// affected workers, switch routes/spec, spawn replacement workers, and
    /// drain in-flight packets.
    ///
    /// Prefer [`Self::apply_hot_update_spec`] when the full candidate
    /// configuration is available so top-level fields (`limits`, `execution`,
    /// `defaults`, …) are applied rather than only topology diffs.
    pub fn apply_hot_update(&mut self, diff: GraphDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }
        let candidate = self.spec.clone().merge_for_diff(diff.clone())?;
        self.set_status(GraphStatus::Reloading);
        match self.apply_hot_update_candidate(diff, candidate) {
            Ok(()) => {
                self.set_status(GraphStatus::Running);
                Ok(())
            }
            Err(error) => {
                if !matches!(self.status().0, GraphStatus::Failed) {
                    self.set_status(GraphStatus::Running);
                }
                Err(error)
            }
        }
    }

    /// Applies a full candidate graph specification while workers are running.
    ///
    /// Topology-only changes rebuild affected nodes; configuration-only changes
    /// (limits / execution / defaults / variables) update `self.spec` without
    /// restarting workers.
    pub fn apply_hot_update_spec(&mut self, new_spec: GraphSpec) -> Result<GraphDiff> {
        let diff = Graph::diff(&self.spec, &new_spec);
        let topology_changed = !diff.is_empty();
        let config_changed = self.spec.limits != new_spec.limits
            || self.spec.execution != new_spec.execution
            || self.spec.defaults != new_spec.defaults
            || self.spec.variables != new_spec.variables
            || self.spec.allow_cycles != new_spec.allow_cycles
            || self.spec.templates != new_spec.templates;

        if !topology_changed && !config_changed {
            return Ok(diff);
        }

        let requested = ResourcePolicy::from(&new_spec.limits);
        let effective = self
            .policy
            .effective_for(&requested)
            .map_err(|err| Error::Validation {
                path: "limits".to_string(),
                message: err.to_string(),
            })?;
        new_spec.validate_with_policy(&effective)?;

        if !topology_changed {
            // Config-only: no worker restart. New queue capacities apply to
            // edges created by a later topology update.
            self.spec = new_spec;
            return Ok(diff);
        }

        self.set_status(GraphStatus::Reloading);
        match self.apply_hot_update_candidate(diff.clone(), new_spec) {
            Ok(()) => {
                self.set_status(GraphStatus::Running);
                Ok(diff)
            }
            Err(error) => {
                // Fail-closed: leave Failed if the candidate apply already set it;
                // otherwise restore Running when only pre-quiesce validation failed.
                if !matches!(self.status().0, GraphStatus::Failed) {
                    self.set_status(GraphStatus::Running);
                }
                Err(error)
            }
        }
    }

    fn apply_hot_update_candidate(&mut self, diff: GraphDiff, candidate: GraphSpec) -> Result<()> {
        let requested = ResourcePolicy::from(&candidate.limits);
        let effective = self
            .policy
            .effective_for(&requested)
            .map_err(|err| Error::Validation {
                path: "limits".to_string(),
                message: err.to_string(),
            })?;
        candidate.validate_with_policy(&effective)?;
        let previous_spec = self.spec.clone();

        let mut affected = BTreeSet::new();
        for name in diff
            .removed_nodes
            .iter()
            .chain(diff.updated_nodes.iter().map(|node| &node.name))
        {
            affected.insert(name.clone());
        }
        for node in &diff.added_nodes {
            affected.insert(node.name.clone());
        }
        for connection in diff
            .added_connections
            .iter()
            .chain(diff.removed_connections.iter())
        {
            let parsed = ConnectionSpec::parse(connection)?;
            affected.insert(parsed.from_node);
            affected.insert(parsed.to_node);
        }

        // Prepare replacements first so create failures never touch live workers.
        let mut prepared = BTreeMap::new();
        for node in &candidate.nodes {
            if affected.contains(&node.name) {
                prepared.insert(node.name.clone(), PreparedNode::new(node)?);
            }
        }

        // Quiesce and join affected workers *before* switching routes so a join
        // failure can re-spawn previous workers on the still-intact routes.
        let affected_names = affected.iter().cloned().collect::<Vec<_>>();
        for name in &affected_names {
            if let Some(node) = self.workers.get(name) {
                node.control.stop.store(true, Ordering::Relaxed);
            }
        }
        let mut joined = Vec::new();
        for name in &affected_names {
            if let Some(mut node) = self.workers.remove(name) {
                if let Err(error) = join_workers(&mut node.workers, true) {
                    // Routes are still the previous topology; restore workers
                    // already joined so the graph can keep running.
                    if let Err(restore_error) =
                        self.respawn_nodes_from_spec(&previous_spec, &joined)
                    {
                        self.set_root_cause(&restore_error);
                        self.set_status(GraphStatus::Failed);
                        return Err(error);
                    }
                    return Err(error);
                }
                joined.push(name.clone());
            }
            if diff.removed_nodes.iter().any(|removed| removed == name) {
                self.metrics.remove(name);
                self.sinks.remove(name);
            }
        }

        let mut next_edges = BTreeMap::new();
        let mut drain_routes = Vec::new();
        for connection in &candidate.connections {
            let parsed = ConnectionSpec::parse(connection)?;
            let old_route = self.routes.edges.remove(connection);
            let route = if !affected.contains(&parsed.to_node) {
                old_route.ok_or_else(|| {
                    Error::Runtime(format!("missing route for connection {connection}"))
                })?
            } else {
                let pipe = match candidate.execution.parallel {
                    ParallelType::Pipeline => DataPipe::bounded(candidate.execution.queue_capacity),
                    ParallelType::Sequential | ParallelType::Task => DataPipe::unbounded(),
                };
                let (sender, receiver) = pipe.split();
                if let Some(old_route) = old_route {
                    drain_routes.push((
                        old_route.receiver,
                        sender.clone(),
                        !affected.contains(&parsed.from_node),
                    ));
                }
                EdgeRoute {
                    sender,
                    receiver: Arc::new(Mutex::new(receiver)),
                }
            };
            next_edges.insert(connection.clone(), route);
        }

        let mut next_inputs = BTreeMap::new();
        let mut output_senders = BTreeMap::<(String, String), Vec<PipeSender>>::new();
        for connection in &candidate.connections {
            let parsed = ConnectionSpec::parse(connection)?;
            let route = next_edges.get(connection).ok_or_else(|| {
                Error::Runtime(format!("missing route for connection {connection}"))
            })?;
            next_inputs.insert(
                (parsed.to_node.clone(), parsed.to_port.clone()),
                route.receiver.clone(),
            );
            let output_key = (parsed.from_node.clone(), parsed.from_port.clone());
            output_senders
                .entry(output_key)
                .or_default()
                .push(route.sender.clone());
        }

        let mut next_outputs = BTreeMap::new();
        for (output_key, senders) in output_senders {
            let route = self
                .routes
                .outputs
                .get(&output_key)
                .cloned()
                .unwrap_or_else(|| Arc::new(Mutex::new(Vec::new())));
            *route
                .lock()
                .map_err(|_| Error::Runtime("output route lock poisoned".to_string()))? = senders;
            next_outputs.insert(output_key, route);
        }

        self.routes.edges = next_edges;
        self.routes.inputs = next_inputs.clone();
        self.routes.outputs = next_outputs;

        // Spawn replacement workers using the switched routes (inline path
        // matches the pre-restore implementation for success-path stability).
        for node in &candidate.nodes {
            if !affected.contains(&node.name) {
                continue;
            }
            let Some(prepared_node) = prepared.remove(&node.name) else {
                continue;
            };
            let control = Arc::new(crate::element::NodeControl::default());
            let eos = Arc::new(Mutex::new(EosState {
                seen: false,
                broadcasts: 0,
                instances: prepared_node.elements.len(),
            }));
            let node_metrics = Arc::new(ElementMetrics::default());
            node_metrics.record_state_reset();
            let mut routes_in = HashMap::new();
            let mut routes_out = HashMap::new();
            for connection in &candidate.connections {
                let parsed = ConnectionSpec::parse(connection)?;
                if parsed.to_node == node.name {
                    let key = (node.name.clone(), parsed.to_port.clone());
                    let route = next_inputs
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| Error::Runtime("missing input route".to_string()))?;
                    routes_in.insert(parsed.to_port, route);
                }
                if parsed.from_node == node.name {
                    let key = (node.name.clone(), parsed.from_port.clone());
                    let route = self
                        .routes
                        .outputs
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| Error::Runtime("missing output route".to_string()))?;
                    routes_out.insert(parsed.from_port, route);
                }
            }
            let mut handles = Vec::with_capacity(prepared_node.elements.len());
            for element in prepared_node.elements {
                let io = ElementIo {
                    name: node.name.clone(),
                    inputs: routes_in.clone(),
                    outputs: routes_out.clone(),
                    stop: self.stop.clone(),
                    control: control.clone(),
                    send_backoff: Duration::from_millis(1),
                    eos: eos.clone(),
                    metrics: node_metrics.clone(),
                    packet_starts: std::cell::RefCell::new(VecDeque::new()),
                    policy: Arc::clone(&self.policy),
                };
                let stop = self.stop.clone();
                handles.push(thread::spawn(move || run_element(element, io, &stop)));
            }
            self.workers.insert(
                node.name.clone(),
                LiveNode {
                    control,
                    workers: handles,
                },
            );
            self.metrics.insert(node.name.clone(), node_metrics.clone());
            if let ElementHandle::Sink(collector) = prepared_node.handle {
                self.sinks.insert(node.name.clone(), collector);
            }
        }

        if let Err(error) = self.drain_routes(drain_routes) {
            // Post-switch failure: fail-closed. Full route rollback is unsafe
            // while Arc-shared output tables may already advertise new senders.
            self.set_root_cause(&error);
            self.set_status(GraphStatus::Failed);
            return Err(error);
        }

        self.spec = candidate;
        Ok(())
    }

    /// Re-spawns nodes from `spec` using the current route tables (must match `spec`).
    /// Used after a join failure before routes were switched.
    fn respawn_nodes_from_spec(&mut self, spec: &GraphSpec, names: &[String]) -> Result<()> {
        for name in names {
            let Some(node) = spec.nodes.iter().find(|node| node.name == *name) else {
                continue;
            };
            if self.workers.contains_key(name) {
                continue;
            }
            let prepared_node = PreparedNode::new(node)?;
            let control = Arc::new(crate::element::NodeControl::default());
            let eos = Arc::new(Mutex::new(EosState {
                seen: false,
                broadcasts: 0,
                instances: prepared_node.elements.len(),
            }));
            let node_metrics = Arc::new(ElementMetrics::default());
            let mut routes_in = HashMap::new();
            let mut routes_out = HashMap::new();
            for connection in &spec.connections {
                let parsed = ConnectionSpec::parse(connection)?;
                if parsed.to_node == node.name {
                    let key = (node.name.clone(), parsed.to_port.clone());
                    let route = self
                        .routes
                        .inputs
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| Error::Runtime("missing input route".to_string()))?;
                    routes_in.insert(parsed.to_port, route);
                }
                if parsed.from_node == node.name {
                    let key = (node.name.clone(), parsed.from_port.clone());
                    let route = self
                        .routes
                        .outputs
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| Error::Runtime("missing output route".to_string()))?;
                    routes_out.insert(parsed.from_port, route);
                }
            }
            let mut handles = Vec::with_capacity(prepared_node.elements.len());
            for element in prepared_node.elements {
                let io = ElementIo {
                    name: node.name.clone(),
                    inputs: routes_in.clone(),
                    outputs: routes_out.clone(),
                    stop: self.stop.clone(),
                    control: control.clone(),
                    send_backoff: Duration::from_millis(1),
                    eos: eos.clone(),
                    metrics: node_metrics.clone(),
                    packet_starts: std::cell::RefCell::new(VecDeque::new()),
                    policy: Arc::clone(&self.policy),
                };
                let stop = self.stop.clone();
                handles.push(thread::spawn(move || run_element(element, io, &stop)));
            }
            self.workers.insert(
                node.name.clone(),
                LiveNode {
                    control,
                    workers: handles,
                },
            );
            self.metrics.insert(node.name.clone(), node_metrics);
            if let ElementHandle::Sink(collector) = prepared_node.handle {
                self.sinks.insert(node.name.clone(), collector);
            }
        }
        Ok(())
    }

    fn drain_routes(
        &self,
        drain_routes: Vec<(Arc<Mutex<PipeReceiver>>, PipeSender, bool)>,
    ) -> Result<()> {
        for (old_receiver, sender, upstream_stays_live) in drain_routes {
            loop {
                if self.stop.load(Ordering::Relaxed) {
                    return Err(Error::NotRunning);
                }
                let packet = {
                    let receiver = old_receiver
                        .lock()
                        .map_err(|_| Error::Runtime("drain route lock poisoned".to_string()))?;
                    if upstream_stays_live {
                        receiver.recv_timeout(Duration::from_millis(1))
                    } else {
                        receiver.try_recv().map_err(|error| match error {
                            std::sync::mpsc::TryRecvError::Empty => {
                                std::sync::mpsc::RecvTimeoutError::Timeout
                            }
                            std::sync::mpsc::TryRecvError::Disconnected => {
                                std::sync::mpsc::RecvTimeoutError::Disconnected
                            }
                        })
                    }
                };
                match packet {
                    Ok(packet) => {
                        let mut pending = packet;
                        let is_eos = pending.is_eos();
                        loop {
                            match sender.try_send(pending) {
                                Ok(()) => break,
                                Err(std::sync::mpsc::TrySendError::Full(p)) => {
                                    pending = p;
                                    thread::sleep(Duration::from_millis(1));
                                    if self.stop.load(Ordering::Relaxed) {
                                        return Err(Error::NotRunning);
                                    }
                                }
                                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                    return Err(Error::Runtime(
                                        "drain route disconnected".to_string(),
                                    ));
                                }
                            }
                        }
                        if is_eos {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) if !upstream_stays_live => {
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if self.stop.load(Ordering::Relaxed) {
                            return Err(Error::NotRunning);
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        Ok(())
    }

    /// Joins all workers and returns the collected sink report.
    pub fn finish(mut self) -> Result<GraphReport> {
        let workers = std::mem::take(&mut self.workers);
        let mut first_error = None;
        for mut node in workers.into_values() {
            if let Err(error) = join_workers(&mut node.workers, false) {
                first_error = select_error(first_error, error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        collect_report(&self.sinks, &self.metrics)
    }

    /// Alias for [`RunningGraph::finish`].
    pub fn join(self) -> Result<GraphReport> {
        self.finish()
    }
}

impl Drop for RunningGraph {
    fn drop(&mut self) {
        if matches!(self.status().0, GraphStatus::Stopped | GraphStatus::Failed) {
            return;
        }
        self.stop.store(true, Ordering::Relaxed);
        for node in self.workers.values() {
            node.control.stop.store(true, Ordering::Relaxed);
        }
        let mut workers = std::mem::take(&mut self.workers);
        let deadline = Instant::now() + Duration::from_secs(5);
        for node in workers.values_mut() {
            while let Some(worker) = node.workers.pop() {
                while !worker.is_finished() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                if worker.is_finished() {
                    let _ = worker.join();
                }
            }
        }
    }
}

struct PreparedNode {
    handle: ElementHandle,
    elements: Vec<Box<dyn Element>>,
}

impl PreparedNode {
    fn new(node: &NodeSpec) -> Result<Self> {
        let threads = node.threads.unwrap_or(1);
        let created = create_element(node)?;
        if threads > 1 && (node.kind == "source" || !matches!(&created.handle, ElementHandle::None))
        {
            return Err(Error::Config(format!(
                "node {} cannot be multi-instanced because source elements and elements with special handles are single-instance",
                node.name,
            )));
        }
        let handle = created.handle;
        let mut elements = vec![created.element];
        for _ in 1..threads {
            let created = create_element(node)?;
            if node.kind == "source" || !matches!(&created.handle, ElementHandle::None) {
                return Err(Error::Config(format!(
                    "node {} cannot be multi-instanced because source elements and elements with special handles are single-instance",
                    node.name,
                )));
            }
            elements.push(created.element);
        }
        Ok(Self { handle, elements })
    }
}

fn join_workers(workers: &mut Vec<thread::JoinHandle<Result<()>>>, cancelled: bool) -> Result<()> {
    let mut first_error = None;
    while let Some(worker) = workers.pop() {
        match worker.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) if cancelled && is_cancellation(&error) => {}
            Ok(Err(error)) => first_error = select_error(first_error, error),
            Err(_) => {
                first_error = select_error(
                    first_error,
                    Error::Runtime("element worker panicked".to_string()),
                )
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn collect_report(
    sinks: &SinkMap,
    metrics: &BTreeMap<String, Arc<ElementMetrics>>,
) -> Result<GraphReport> {
    let mut report = GraphReport::default();
    for (name, sink) in sinks {
        let guard = sink
            .lock()
            .map_err(|_| Error::Runtime("sink lock poisoned".to_string()))?;
        report.sinks.insert(name.clone(), guard.tensors.clone());
        report.detections.insert(
            name.clone(),
            guard
                .detections
                .iter()
                .flat_map(|batch| batch.iter().cloned())
                .collect(),
        );
        report.classifications.insert(
            name.clone(),
            guard
                .classifications
                .iter()
                .flat_map(|batch| batch.iter().cloned())
                .collect(),
        );
        report.faces.insert(
            name.clone(),
            guard
                .faces
                .iter()
                .flat_map(|batch| batch.iter().cloned())
                .collect(),
        );
        report.tracks.insert(
            name.clone(),
            guard
                .tracks
                .iter()
                .flat_map(|batch| batch.iter().cloned())
                .collect(),
        );
        report.ocr.insert(
            name.clone(),
            guard
                .ocr
                .iter()
                .flat_map(|batch| batch.iter().cloned())
                .collect(),
        );
    }
    for (node, metrics) in metrics {
        report
            .element_metrics
            .insert(node.clone(), metrics.snapshot());
    }
    Ok(report)
}

fn is_cancellation(error: &Error) -> bool {
    matches!(error, Error::NotRunning)
}

fn select_error(current: Option<Error>, candidate: Error) -> Option<Error> {
    match current {
        Some(existing) if !is_cancellation(&existing) || is_cancellation(&candidate) => {
            Some(existing)
        }
        _ => Some(candidate),
    }
}

fn run_element(element: Box<dyn Element>, io: ElementIo, stop: &Arc<AtomicBool>) -> Result<()> {
    match catch_unwind(AssertUnwindSafe(|| element.run(io))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => {
            if !is_cancellation(&err) {
                stop.store(true, Ordering::Relaxed);
            }
            Err(err)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err(Error::Runtime("element panicked".to_string()))
        }
    }
}

struct NodeRuntime {
    name: String,
    elements: Vec<Box<dyn Element>>,
    handle: ElementHandle,
    inputs: HashMap<String, Arc<Mutex<PipeReceiver>>>,
    outputs: HashMap<String, Vec<PipeSender>>,
}

struct ExecNode {
    name: String,
    element: Box<dyn Element>,
    io: ElementIo,
}

impl GraphSpec {
    fn merge_for_diff(self, diff: GraphDiff) -> Result<Self> {
        if diff.is_empty() {
            return Ok(self);
        }
        let mut spec = self;
        for node in diff.removed_nodes {
            spec.nodes.retain(|existing| existing.name != node);
        }
        for node in diff.added_nodes {
            spec.nodes.push(node);
        }
        for node in diff.updated_nodes {
            spec.nodes.retain(|existing| existing.name != node.name);
            spec.nodes.push(node);
        }
        for conn in diff.removed_connections {
            spec.connections.retain(|existing| existing != &conn);
        }
        spec.connections.extend(diff.added_connections);
        Ok(spec)
    }
}

/// Controls a background graph specification file watcher.
pub struct WatchHandle {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl WatchHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Watches a graph specification file and all included files, debounces
/// changes for 100 ms, and reports validated (spec, diff) pairs.
pub fn watch(
    path: impl AsRef<Path>,
    mut callback: impl FnMut(Result<(GraphSpec, GraphDiff)>) + Send + 'static,
) -> Result<WatchHandle> {
    let path = path.as_ref().to_path_buf();
    let (stop, stop_receiver) = mpsc::channel();

    let thread = thread::spawn(move || {
        let (mut previous, mut tracked_paths) = match load_spec_and_track(&path) {
            Ok(result) => result,
            Err(error) => {
                notify_watch(&mut callback, Err(error));
                return;
            }
        };

        let mut states: BTreeMap<PathBuf, Option<FileState>> = BTreeMap::new();
        for p in &tracked_paths {
            states.insert(p.clone(), read_file_state(p));
        }

        const POLL_INTERVAL: Duration = Duration::from_millis(50);
        const DEBOUNCE: Duration = Duration::from_millis(100);
        let mut pending_change: Option<Instant> = None;

        loop {
            let wait = if let Some(pending) = pending_change {
                let elapsed = pending.elapsed();
                if elapsed >= DEBOUNCE {
                    pending_change = None;
                    match reload_and_retrack(&path, &previous) {
                        Ok((spec, diff, paths)) => {
                            previous = spec.clone();
                            tracked_paths = paths;
                            states.clear();
                            for p in &tracked_paths {
                                states.insert(p.clone(), read_file_state(p));
                            }
                            notify_watch(&mut callback, Ok((spec, diff)));
                        }
                        Err(error) => {
                            error!(path = %path.display(), error = %error, "graph watch reload failed");
                            notify_watch(&mut callback, Err(error));
                        }
                    }
                    continue;
                }
                DEBOUNCE - elapsed
            } else {
                POLL_INTERVAL
            };

            match stop_receiver.recv_timeout(POLL_INTERVAL.min(wait)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            if pending_change.is_some() {
                continue;
            }

            let mut changed = false;
            let mut next_states = BTreeMap::new();
            // Re-read the current set of tracked paths and detect any change.
            for p in &tracked_paths {
                let new_state = read_file_state(p);
                if states.get(p) != Some(&new_state) {
                    changed = true;
                }
                next_states.insert(p.clone(), new_state);
            }
            states = next_states;
            if changed {
                pending_change = Some(Instant::now());
            }
        }
    });

    Ok(WatchHandle {
        stop: Some(stop),
        thread: Some(thread),
    })
}

fn load_spec_and_track(path: &Path) -> Result<(GraphSpec, Vec<PathBuf>)> {
    GraphSpec::load_from_path_with_includes(path)
}

fn reload_and_retrack(
    path: &Path,
    previous: &GraphSpec,
) -> Result<(GraphSpec, GraphDiff, Vec<PathBuf>)> {
    let (spec, includes) = GraphSpec::load_from_path_with_includes(path)?;
    let diff = Graph::diff(previous, &spec);
    Ok((spec, diff, includes))
}

#[derive(Clone, Debug, PartialEq)]
struct FileState {
    modified: SystemTime,
    len: u64,
    ino: u64,
}

fn read_file_state(path: &Path) -> Option<FileState> {
    fs::metadata(path).ok().map(|metadata| {
        #[cfg(unix)]
        let ino = metadata.ino();
        #[cfg(not(unix))]
        let ino = 0;
        FileState {
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            len: metadata.len(),
            ino,
        }
    })
}

fn notify_watch(
    callback: &mut impl FnMut(Result<(GraphSpec, GraphDiff)>),
    result: Result<(GraphSpec, GraphDiff)>,
) {
    if catch_unwind(AssertUnwindSafe(|| callback(result))).is_err() {
        error!("graph watch callback panicked");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dg_core::DataType;
    use serde_json::json;

    use super::*;
    use crate::element::{CreatedElement, PortSchema};
    use crate::registry::ElementDescriptor;
    use crate::spec::{GraphSpecBuilder, NodeSpec};

    static THREADED_INSTANCE_COUNT: AtomicUsize = AtomicUsize::new(0);

    const TEST_INPUT: PortSchema = PortSchema {
        name: "in",
        dtype: Some(DataType::F32),
        required: true,
    };
    const TEST_OUTPUT: PortSchema = PortSchema {
        name: "out",
        dtype: Some(DataType::F32),
        required: false,
    };

    struct ThreadedPassthrough;

    impl Element for ThreadedPassthrough {
        fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
            loop {
                let packet = match io.recv("in")? {
                    Some(packet) => packet,
                    None => continue,
                };
                if packet.is_eos() {
                    io.broadcast_eos()?;
                    return Ok(());
                }
                io.send("out", packet)?;
            }
        }
    }

    fn create_threaded_passthrough(_: &NodeSpec) -> Result<CreatedElement> {
        THREADED_INSTANCE_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(CreatedElement {
            element: Box::new(ThreadedPassthrough),
            handle: ElementHandle::None,
        })
    }

    inventory::submit! {
        ElementDescriptor {
            kind: "threaded_test_passthrough",
            input_ports: &[TEST_INPUT],
            output_ports: &[TEST_OUTPUT],
            params: &[],
            validate: None,
            create: create_threaded_passthrough,
        }
    }

    struct InfiniteSource;

    impl Element for InfiniteSource {
        fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
            while !io.should_stop() {
                thread::sleep(Duration::from_millis(1));
            }
            Err(Error::NotRunning)
        }
    }

    fn create_infinite_source(_: &NodeSpec) -> Result<CreatedElement> {
        Ok(CreatedElement {
            element: Box::new(InfiniteSource),
            handle: ElementHandle::None,
        })
    }

    inventory::submit! {
        ElementDescriptor {
            kind: "infinite_source",
            input_ports: &[],
            output_ports: &[TEST_OUTPUT],
            params: &[],
            validate: None,
            create: create_infinite_source,
        }
    }

    fn infinite_graph_spec() -> GraphSpec {
        GraphSpecBuilder::new()
            .add_node(NodeSpec {
                name: "source".to_string(),
                kind: "infinite_source".to_string(),
                params: json!({}),
                ..NodeSpec::default()
            })
            .add_node(NodeSpec {
                name: "sink".to_string(),
                kind: "sink".to_string(),
                params: json!({}),
                ..NodeSpec::default()
            })
            .connect("source.out -> sink.in")
            .build()
            .expect("build infinite spec")
    }

    #[test]
    fn pipeline_creates_and_runs_each_requested_instance() {
        THREADED_INSTANCE_COUNT.store(0, Ordering::SeqCst);
        let spec = GraphSpecBuilder::new()
            .add_node(NodeSpec {
                name: "source".to_string(),
                kind: "source".to_string(),
                params: json!({"count": 8, "shape": [1, 4]}),
                ..NodeSpec::default()
            })
            .add_node(NodeSpec {
                name: "threaded".to_string(),
                kind: "threaded_test_passthrough".to_string(),
                threads: Some(2),
                params: json!({}),
                ..NodeSpec::default()
            })
            .add_node(NodeSpec {
                name: "sink".to_string(),
                kind: "sink".to_string(),
                params: json!({}),
                ..NodeSpec::default()
            })
            .connect("source.out -> threaded.in")
            .connect("threaded.out -> sink.in")
            .build()
            .expect("build threaded test graph");

        let report = Graph::new(spec)
            .expect("construct threaded test graph")
            .run()
            .expect("run threaded test graph");
        assert_eq!(
            THREADED_INSTANCE_COUNT.load(Ordering::SeqCst),
            2,
            "requested instances should each be created"
        );
        assert_eq!(report.sinks["sink"].len(), 8);
    }

    #[test]
    fn request_stop_is_idempotent() {
        let graph = Graph::new(infinite_graph_spec()).expect("build infinite graph");
        let running = graph.start(HashMap::new()).expect("start infinite graph");
        assert_eq!(running.status().0, GraphStatus::Running);
        running.request_stop();
        assert_eq!(running.status().0, GraphStatus::Draining);
        running.request_stop();
        assert_eq!(running.status().0, GraphStatus::Draining);
    }

    #[test]
    fn infinite_source_stops_within_deadline() {
        let graph = Graph::new(infinite_graph_spec()).expect("build infinite graph");
        let mut running = graph.start(HashMap::new()).expect("start infinite graph");
        running.request_stop();
        running
            .shutdown(Duration::from_secs(2))
            .expect("shutdown infinite graph within deadline");
        assert_eq!(running.status().0, GraphStatus::Stopped);
    }

    #[test]
    fn metrics_snapshot_reports_nodes() {
        let graph = Graph::new(infinite_graph_spec()).expect("build infinite graph");
        let running = graph.start(HashMap::new()).expect("start infinite graph");
        let snapshot = running.metrics_snapshot();
        assert!(snapshot.contains_key("source"));
        assert!(snapshot.contains_key("sink"));
        running.request_stop();
    }

    fn root_cause() -> Error {
        Error::Element {
            element: "decode".to_string(),
            message: "recorded frame has an invalid payload size".to_string(),
        }
    }

    #[test]
    fn error_selection_prefers_root_cause_over_cancellation() {
        let selected = select_error(Some(Error::NotRunning), root_cause());
        assert!(matches!(selected, Some(Error::Element { .. })));
    }

    #[test]
    fn error_selection_keeps_root_cause_when_cancellation_arrives_later() {
        let selected = select_error(Some(root_cause()), Error::NotRunning);
        assert!(matches!(selected, Some(Error::Element { .. })));
    }

    #[test]
    fn cancellation_is_only_the_not_running_error() {
        assert!(is_cancellation(&Error::NotRunning));
        assert!(!is_cancellation(&root_cause()));
    }
}
