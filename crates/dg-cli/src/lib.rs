use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{mpsc, Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use dg_core::{DataFormat, DataType, DeviceKind, MemoryDomain, ProcessRuntimePolicy};
use dg_graph::{ElementMetricsSnapshot, Graph, GraphDiff, GraphReport, GraphSpec, GraphStatus};
use dg_media::{
    FrameLayout, FrameTransferRequest, HandleKind, MediaFrame, MemoryDtype, MemoryFormat,
    TransferReport, ZeroCopyPlanner,
};
#[cfg(feature = "stream")]
use dg_stream::{
    CodecId, MediaKind, MemoryStreamHub, PublisherSink, Rational32, TrackInfo, TrackReadiness,
};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

mod ops;
use ops::OpsState;

use dg_elements as _;
#[cfg(feature = "media")]
use dg_media as _;
#[cfg(feature = "openvino")]
use dg_openvino as _;
#[cfg(feature = "rknn")]
use dg_rknn as _;
#[cfg(feature = "sophon")]
use dg_sophon as _;
#[cfg(feature = "stream")]
use dg_stream as _;
#[cfg(feature = "tensorrt")]
use dg_tensorrt as _;

#[derive(Debug, Parser)]
#[command(
    name = "dg",
    version,
    about = "Run and inspect dg graph specifications"
)]
pub struct Cli {
    #[arg(long, global = true, short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        watch: bool,
        #[arg(long, default_value = "127.0.0.1:9090")]
        ops_bind: String,
        #[arg(long)]
        ops_disable: bool,
        #[arg(long)]
        runtime_limits: Option<PathBuf>,
    },
    #[cfg(feature = "stream")]
    Demo {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        runtime_limits: Option<PathBuf>,
    },
    Validate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        runtime_limits: Option<PathBuf>,
    },
    ListElements,
    Schema {
        #[arg(long)]
        kind: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
}

pub fn run(cli: Cli) -> Result<ExitCode> {
    init_logging(cli.verbose);
    match cli.command {
        Command::Run {
            config,
            format,
            watch,
            ops_bind,
            ops_disable,
            runtime_limits,
        } => run_graph_with_watch(
            &config,
            format,
            watch,
            if ops_disable { None } else { Some(ops_bind) },
            load_runtime_limits(runtime_limits.as_deref())?,
        ),
        #[cfg(feature = "stream")]
        Command::Demo {
            config,
            runtime_limits,
        } => {
            let summary = run_demo(&config, load_runtime_limits(runtime_limits.as_deref())?)?;
            println!(
                "demo completed: {} mock streams, {} frames, planned copy count: {}",
                summary.streams, summary.frames, summary.planned_copy_count
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate {
            config,
            runtime_limits,
        } => {
            validate_graph(&config, load_runtime_limits(runtime_limits.as_deref())?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::ListElements => {
            list_elements()?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Schema { kind } => {
            schema(kind.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

pub fn run_graph(
    path: &Path,
    format: OutputFormat,
    process_policy: ProcessRuntimePolicy,
) -> Result<()> {
    let exit_code = run_graph_with_watch(path, format, false, None, process_policy)?;
    if exit_code == ExitCode::SUCCESS {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "graph run failed with exit code {exit_code:?}"
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoSummary {
    pub streams: usize,
    pub frames: usize,
    pub planned_copy_count: usize,
}

const DEMO_INPUTS: [&str; 2] = ["mock://demo/input-a", "mock://demo/input-b"];
const DEMO_FRAME_COUNT: usize = 3;

#[cfg(feature = "stream")]
pub fn run_demo(path: &Path, process_policy: ProcessRuntimePolicy) -> Result<DemoSummary> {
    let spec = load_spec(path)?;
    let publishers = DEMO_INPUTS
        .iter()
        .map(|url| seed_demo_stream(url))
        .collect::<Result<Vec<_>>>()?;
    let graph = Graph::new_with_process_policy(spec, process_policy).context("build demo graph")?;
    let report = graph.run().context("run demo graph")?;
    for publisher in publishers {
        publisher
            .join()
            .map_err(|_| anyhow::anyhow!("demo publisher thread panicked"))??;
    }
    let planned_copy_count = planned_demo_copy_count()?;
    let frames = ["input_a", "input_b"]
        .into_iter()
        .filter_map(|name| report.element_metrics.get(name))
        .map(|metrics| metrics.packets_processed)
        .sum::<u64>();
    let frames = usize::try_from(frames).context("demo frame count exceeds usize")?;
    Ok(DemoSummary {
        streams: DEMO_INPUTS.len(),
        frames,
        planned_copy_count,
    })
}

#[cfg(feature = "stream")]
fn seed_demo_stream(url: &str) -> Result<JoinHandle<Result<()>>> {
    let publisher = MemoryStreamHub::global().publish(url, Default::default())?;
    let mut track = TrackInfo::new(1, MediaKind::Video, CodecId::MJPEG, 90_000);
    track.readiness = TrackReadiness::Ready;
    track.width = Some(2);
    track.height = Some(2);
    track.fps = Some(Rational32::new(30, 1));
    publisher.update_tracks(vec![track])?;
    let url = url.to_string();
    Ok(std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while MemoryStreamHub::global().subscriber_count(&url) == 0 {
            if Instant::now() >= deadline {
                return Err(anyhow::anyhow!("demo subscriber did not connect: {url}"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        for index in 0..DEMO_FRAME_COUNT {
            let mut frame = MediaFrame::from_host_bytes(
                dg_media::MediaFrameKind::Tensor,
                DataType::U8,
                DataFormat::N,
                vec![12],
                DeviceKind::Cpu,
                vec![u8::try_from(index)?; 12],
            )?;
            frame
                .meta
                .tags
                .insert("media".to_string(), "video".to_string());
            frame
                .meta
                .tags
                .insert("keyframe".to_string(), (index == 0).to_string());
            publisher.push_frame(Arc::new(frame))?;
        }
        publisher.close()?;
        Ok(())
    }))
}

fn planned_demo_copy_count() -> Result<usize> {
    let layout = FrameLayout {
        dims: vec![2, 2, 3],
        format: MemoryFormat::Rgb24,
        dtype: MemoryDtype::U8,
        plane_count: 1,
        strides: vec![6],
        subsampling: None,
        packed: true,
    };
    let request = FrameTransferRequest {
        source_domain: MemoryDomain::Host,
        target_domain: MemoryDomain::Host,
        source_handle: HandleKind::HostBytes,
        target_handle: HandleKind::HostBytes,
        source_layout: layout.clone(),
        target_layout: layout,
        has_lifetime_guard: true,
        staging_supported: true,
        operation: "demo mock input to media pipeline".to_string(),
    };
    let report: TransferReport = ZeroCopyPlanner::new().plan_frame(&request)?;
    tracing::info!(
        source_domain = ?report.source_domain,
        target_domain = ?report.target_domain,
        path = ?report.path.domains,
        copy_count = report.copy_count,
        "demo planned frame transfer"
    );
    Ok(report.copy_count)
}

fn run_graph_with_watch(
    path: &Path,
    format: OutputFormat,
    watch: bool,
    ops_bind: Option<String>,
    process_policy: ProcessRuntimePolicy,
) -> Result<ExitCode> {
    const POLL_INTERVAL: Duration = Duration::from_millis(100);
    let shutdown_timeout = process_policy.deadlines().shutdown();

    #[allow(clippy::large_enum_variant)]
    enum SupervisorEvent {
        Signal(i32),
        Reload(std::result::Result<(GraphSpec, GraphDiff), dg_graph::Error>),
    }

    // Install real-protocol connectors before build/start so failures are not
    // deferred until the first pull/push open (INT5-05).
    if let Err(error) = ensure_runtime_connectors() {
        error!(error = %error, "failed to initialize runtime connectors");
        return Ok(ExitCode::from(3));
    }

    let spec = match load_spec(path) {
        Ok(spec) => spec,
        Err(error) => {
            error!(error = %error, path = %path.display(), "configuration error");
            return Ok(ExitCode::from(2));
        }
    };
    let graph = match Graph::new_with_process_policy(spec, process_policy.clone()) {
        Ok(graph) => graph,
        Err(error) => {
            error!(error = %error, "failed to build graph");
            return Ok(ExitCode::from(2));
        }
    };
    let mut running = match graph.start(std::collections::HashMap::new()) {
        Ok(running) => Some(running),
        Err(error) => {
            error!(error = %error, "failed to start graph");
            return Ok(ExitCode::from(3));
        }
    };

    let ops_state: Arc<RwLock<OpsState>> = Arc::new(RwLock::new(OpsState {
        status: GraphStatus::Starting,
        root_cause: None,
        element_metrics: BTreeMap::new(),
        ready: false,
        ready_reason: Some("graph is starting".to_string()),
        reload_attempts_total: 0,
        reload_success_total: 0,
        reload_rejected_total: 0,
        supervisor_healthy: true,
    }));
    let mut ops_handle: Option<ops::OpsHandle> = ops_bind
        .as_deref()
        .map(|bind| ops::start(Arc::clone(&ops_state), bind))
        .transpose()?;

    let (event_tx, event_rx) = mpsc::channel::<SupervisorEvent>();

    // Signal handler thread.
    let signal_tx = event_tx.clone();
    std::thread::spawn(move || {
        let mut signals = match Signals::new([SIGTERM, SIGINT]) {
            Ok(signals) => signals,
            Err(_) => return,
        };
        for signal in signals.forever() {
            if signal_tx.send(SupervisorEvent::Signal(signal)).is_err() {
                break;
            }
        }
    });

    // Keep the watch handle alive for the entire supervisor loop. Dropping it
    // early shuts down the watcher thread (WatchHandle::Drop) and silently
    // disables live reload.
    let _watch_handle = if watch {
        let watch_tx = event_tx;
        Some(dg_graph::watch(path, move |result| {
            let _ = watch_tx.send(SupervisorEvent::Reload(result));
        })?)
    } else {
        drop(event_tx);
        None
    };

    let mut status: Option<ExitCode> = None;
    while status.is_none() {
        match event_rx.recv_timeout(POLL_INTERVAL) {
            Ok(SupervisorEvent::Signal(signal)) => {
                info!(signal, "received termination signal");
                update_ops_state(&ops_state, GraphStatus::Draining, None, BTreeMap::new());
                let Some(mut running) = running.take() else {
                    break;
                };
                running.request_stop();
                match running.shutdown(shutdown_timeout) {
                    Ok(()) => {
                        let report = running.finish()?;
                        print_report(&report, format)?;
                        status = Some(ExitCode::SUCCESS);
                    }
                    Err(dg_graph::Error::Timeout(_)) => {
                        status = Some(ExitCode::from(4));
                    }
                    Err(error) => {
                        error!(error = %error, "shutdown error");
                        status = Some(ExitCode::from(3));
                    }
                }
            }
            Ok(SupervisorEvent::Reload(Ok((spec, diff)))) => {
                let Some(running) = running.as_mut() else {
                    break;
                };
                record_reload_attempt(&ops_state);
                // Apply the full candidate spec so limits/execution updates land
                // even when the topology diff is empty.
                match running.apply_hot_update_spec(spec) {
                    Ok(applied) => {
                        record_reload_success(&ops_state);
                        info!(
                            added_nodes = applied.added_nodes.len(),
                            removed_nodes = applied.removed_nodes.len(),
                            updated_nodes = applied.updated_nodes.len(),
                            "graph configuration reload applied"
                        );
                        if !applied.is_empty() || !diff.is_empty() {
                            let output = render_diff(&applied, format)?;
                            println!("{output}");
                        }
                    }
                    Err(error) => {
                        record_reload_rejected(&ops_state);
                        warn!(error = %error, "graph configuration reload rejected");
                        println!("{}", render_reload_rejected(&error.to_string()));
                    }
                }
            }
            Ok(SupervisorEvent::Reload(Err(error))) => {
                record_reload_attempt(&ops_state);
                record_reload_rejected(&ops_state);
                warn!(error = %error, "graph configuration reload parse/validate failed");
                println!("{}", render_reload_rejected(&error.to_string()));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(running_ref) = running.as_mut() else {
                    break;
                };
                match running_ref.poll() {
                    Ok(()) => {
                        let (graph_status, root_cause) = running_ref.status();
                        let metrics = running_ref.metrics_snapshot();
                        update_ops_state(&ops_state, graph_status, root_cause.as_deref(), metrics);
                        if graph_status == GraphStatus::Stopped {
                            if let Some(running) = running.take() {
                                let report = running.finish()?;
                                print_report(&report, format)?;
                                status = Some(ExitCode::SUCCESS);
                            } else {
                                error!("graph reported Stopped but handle was already taken");
                                update_ops_state(
                                    &ops_state,
                                    GraphStatus::Failed,
                                    Some("graph state inconsistent"),
                                    BTreeMap::new(),
                                );
                                status = Some(ExitCode::from(3));
                            }
                        }
                    }
                    Err(error) => {
                        error!(error = %error, "graph worker failed");
                        update_ops_state(
                            &ops_state,
                            GraphStatus::Failed,
                            Some(&error.to_string()),
                            BTreeMap::new(),
                        );
                        status = Some(ExitCode::from(3));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                status = Some(ExitCode::from(3));
            }
        }
    }

    if let Some(handle) = ops_handle.take() {
        handle.stop();
    }

    Ok(status.unwrap_or(ExitCode::SUCCESS))
}

fn update_ops_state(
    ops_state: &Arc<RwLock<OpsState>>,
    status: GraphStatus,
    root_cause: Option<&str>,
    metrics: BTreeMap<String, ElementMetricsSnapshot>,
) {
    let reconnecting = metrics.values().any(|m| m.reconnecting);
    let ready = status == GraphStatus::Running && !reconnecting;
    let ready_reason = if ready {
        None
    } else if reconnecting {
        Some("stream element is connecting or reconnecting".to_string())
    } else if status == GraphStatus::Reloading {
        Some("graph configuration is reloading".to_string())
    } else {
        Some(
            root_cause
                .map(|cause| cause.to_string())
                .unwrap_or_else(|| format!("graph status is {status:?}")),
        )
    };
    if let Ok(mut state) = ops_state.write() {
        state.status = status;
        state.root_cause = root_cause.map(str::to_string);
        state.element_metrics = metrics;
        state.ready = ready;
        state.ready_reason = ready_reason;
    }
}

fn record_reload_attempt(ops_state: &Arc<RwLock<OpsState>>) {
    if let Ok(mut state) = ops_state.write() {
        state.reload_attempts_total = state.reload_attempts_total.saturating_add(1);
    }
}

fn record_reload_success(ops_state: &Arc<RwLock<OpsState>>) {
    if let Ok(mut state) = ops_state.write() {
        state.reload_success_total = state.reload_success_total.saturating_add(1);
    }
}

fn record_reload_rejected(ops_state: &Arc<RwLock<OpsState>>) {
    if let Ok(mut state) = ops_state.write() {
        state.reload_rejected_total = state.reload_rejected_total.saturating_add(1);
    }
}

fn ensure_runtime_connectors() -> Result<()> {
    #[cfg(feature = "cheetah")]
    {
        dg_stream::install_embedded_cheetah_connector().map_err(|error| {
            anyhow::anyhow!("failed to install cheetah stream connector: {error}")
        })?;
        info!("cheetah stream connector installed");
    }
    Ok(())
}

pub fn validate_graph(path: &Path, process_policy: ProcessRuntimePolicy) -> Result<()> {
    let spec = load_spec(path)?;
    let _ = Graph::new_with_process_policy(spec, process_policy)
        .with_context(|| format!("validate graph config {}", path.display()))?;
    println!("valid: {}", path.display());
    Ok(())
}

pub fn list_elements() -> Result<()> {
    let mut kinds = dg_graph::registered_elements()
        .into_iter()
        .map(|descriptor| descriptor.kind)
        .collect::<Vec<_>>();
    kinds.sort_unstable();
    kinds.dedup();
    for kind in kinds {
        println!("{kind}");
    }
    Ok(())
}

pub fn schema(kind: Option<&str>) -> Result<()> {
    let value = match kind {
        Some(kind) => dg_graph::element_params_schema(kind)
            .ok_or_else(|| anyhow::anyhow!("unknown element kind: {kind}"))?,
        None => serde_json::to_value(dg_graph::all_element_schemas())?,
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn load_spec(path: &Path) -> Result<GraphSpec> {
    GraphSpec::load_from_path(path).with_context(|| format!("load graph config {}", path.display()))
}

fn load_runtime_limits(path: Option<&Path>) -> Result<ProcessRuntimePolicy> {
    let Some(path) = path else {
        return Ok(ProcessRuntimePolicy::default());
    };
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read runtime limits {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let policy: ProcessRuntimePolicy = match ext.as_str() {
        "json" => serde_json::from_str(&content)
            .with_context(|| format!("parse runtime limits as JSON {}", path.display()))?,
        "yaml" | "yml" => serde_yaml_ng::from_str(&content)
            .with_context(|| format!("parse runtime limits as YAML {}", path.display()))?,
        "toml" => toml::from_str(&content)
            .with_context(|| format!("parse runtime limits as TOML {}", path.display()))?,
        _ => {
            return Err(anyhow::anyhow!(
                "unsupported runtime limits format for {}; expected json/yaml/toml",
                path.display()
            ))
        }
    };
    Ok(policy)
}

fn print_report(report: &GraphReport, format: OutputFormat) -> Result<()> {
    let summary = ReportSummary::from(report);
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summary)?),
        OutputFormat::Text => {
            println!("graph run completed");
            println!("sinks: {}", summary.sinks.len());
            for sink in &summary.sinks {
                println!(
                    "  {}: {} tensor(s), {} detection(s), {} classification(s), \
                     {} face(s), {} track(s), {} OCR result(s)",
                    sink.name,
                    sink.tensors,
                    sink.detections,
                    sink.classifications,
                    sink.faces,
                    sink.tracks,
                    sink.ocr
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct DiffSummary {
    added_nodes: Vec<String>,
    removed_nodes: Vec<String>,
    updated_nodes: Vec<String>,
    added_connections: Vec<String>,
    removed_connections: Vec<String>,
}

fn diff_summary(diff: &GraphDiff) -> DiffSummary {
    DiffSummary {
        added_nodes: diff
            .added_nodes
            .iter()
            .map(|node| node.name.clone())
            .collect(),
        removed_nodes: diff.removed_nodes.clone(),
        updated_nodes: diff
            .updated_nodes
            .iter()
            .map(|node| node.name.clone())
            .collect(),
        added_connections: diff.added_connections.clone(),
        removed_connections: diff.removed_connections.clone(),
    }
}

fn render_diff(diff: &GraphDiff, format: OutputFormat) -> Result<String> {
    let summary = diff_summary(diff);
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(&summary)?),
        OutputFormat::Text => {
            let mut lines = vec!["graph configuration reloaded".to_string()];
            if !summary.added_nodes.is_empty() {
                lines.push(format!("added nodes: {}", summary.added_nodes.join(", ")));
            }
            if !summary.removed_nodes.is_empty() {
                lines.push(format!(
                    "removed nodes: {}",
                    summary.removed_nodes.join(", ")
                ));
            }
            if !summary.updated_nodes.is_empty() {
                lines.push(format!(
                    "updated nodes: {}",
                    summary.updated_nodes.join(", ")
                ));
            }
            if !summary.added_connections.is_empty() {
                lines.push(format!(
                    "added connections: {}",
                    summary.added_connections.join(", ")
                ));
            }
            if !summary.removed_connections.is_empty() {
                lines.push(format!(
                    "removed connections: {}",
                    summary.removed_connections.join(", ")
                ));
            }
            Ok(lines.join("\n"))
        }
    }
}

fn render_reload_rejected(error: &str) -> String {
    // Hot-update failures after quiesce may leave the graph Failed rather than
    // the previous topology still running; do not claim rollback succeeded.
    format!(
        "graph configuration reload REJECTED: {error}; check graph status — \
         a failed mid-update may mark the graph Failed rather than keep the previous topology"
    )
}

#[derive(Debug, serde::Serialize)]
struct ReportSummary {
    sinks: Vec<SinkSummary>,
}

#[derive(Debug, serde::Serialize)]
struct SinkSummary {
    name: String,
    tensors: usize,
    detections: usize,
    classifications: usize,
    faces: usize,
    tracks: usize,
    ocr: usize,
}

impl From<&GraphReport> for ReportSummary {
    fn from(report: &GraphReport) -> Self {
        let mut names = report
            .sinks
            .keys()
            .chain(report.detections.keys())
            .chain(report.classifications.keys())
            .chain(report.faces.keys())
            .chain(report.tracks.keys())
            .chain(report.ocr.keys())
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let sinks = names
            .into_iter()
            .map(|name| SinkSummary {
                tensors: report.sinks.get(&name).map_or(0, Vec::len),
                detections: report.detections.get(&name).map_or(0, Vec::len),
                classifications: report.classifications.get(&name).map_or(0, Vec::len),
                faces: report.faces.get(&name).map_or(0, Vec::len),
                tracks: report.tracks.get(&name).map_or(0, Vec::len),
                ocr: report.ocr.get(&name).map_or(0, Vec::len),
                name,
            })
            .collect();
        Self { sinks }
    }
}

fn init_logging(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    use std::fs;

    use dg_core::ProcessRuntimePolicy;
    use dg_graph::{GraphDiff, NodeSpec};

    #[cfg(feature = "stream")]
    use super::Command;
    use super::{
        list_elements, render_diff, render_reload_rejected, run_graph, schema, validate_graph,
        OutputFormat,
    };

    fn temp_config() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dg-cli-{}-{}.yaml",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let content = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1, 2]
  - name: infer
    kind: inference
    params:
      backend: mock
      options:
        shape: [1, 2]
        echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#;
        fs::write(&path, content).expect("write config");
        path
    }

    #[test]
    fn commands_run_validate_and_list_elements() {
        let path = temp_config();
        let policy = ProcessRuntimePolicy::default();
        validate_graph(&path, policy.clone()).expect("validate config");
        run_graph(&path, OutputFormat::Json, policy).expect("run config");
        list_elements().expect("list elements");
        #[cfg(feature = "stream")]
        {
            let kinds = dg_graph::registered_elements()
                .into_iter()
                .map(|descriptor| descriptor.kind)
                .collect::<std::collections::BTreeSet<_>>();
            for kind in [
                "media_decode",
                "media_encode",
                "media_resize",
                "media_osd",
                "rtsp_src",
                "httpflv_src",
                "rtmp_sink",
                "webrtc_sink",
            ] {
                assert!(kinds.contains(kind), "missing registered element {kind}");
            }
        }
        fs::remove_file(path).expect("remove config");
    }

    #[test]
    fn documented_multi_algorithm_example_runs() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/mock-multi-algorithm.yaml");
        let policy = ProcessRuntimePolicy::default();
        validate_graph(&path, policy.clone()).expect("validate documented example");
        run_graph(&path, OutputFormat::Json, policy).expect("run documented example");
    }

    #[test]
    fn multi_stream_demo_runs_and_reports_planned_copy_count() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/mock-multi-stream-demo.yaml");
        let summary =
            super::run_demo(&path, ProcessRuntimePolicy::default()).expect("run multi-stream demo");
        assert_eq!(summary.streams, 2);
        assert_eq!(summary.frames, 6);
        assert_eq!(summary.planned_copy_count, 0);
    }

    #[test]
    fn schema_command_exports_all_and_one_element() {
        schema(None).expect("export all element schemas");
        #[cfg(feature = "stream")]
        {
            schema(Some("media_osd")).expect("export media OSD schema");
            let command = Command::Schema {
                kind: Some("media_osd".to_string()),
            };
            assert!(matches!(command, Command::Schema { .. }));
            let schema = dg_graph::element_params_schema("media_osd").expect("media OSD schema");
            assert_eq!(schema["properties"]["boxes"]["type"], "array");
        }
    }

    #[test]
    fn diff_rendering_supports_text_and_json() {
        let diff = GraphDiff {
            added_nodes: vec![NodeSpec {
                name: "added".to_string(),
                kind: "source".to_string(),
                template: None,
                params: serde_json::json!({}),
                ..NodeSpec::default()
            }],
            removed_nodes: vec!["removed".to_string()],
            updated_nodes: vec![NodeSpec {
                name: "updated".to_string(),
                kind: "sink".to_string(),
                template: None,
                params: serde_json::json!({}),
                ..NodeSpec::default()
            }],
            added_connections: vec!["added.out -> updated.in".to_string()],
            removed_connections: vec!["old.out -> removed.in".to_string()],
        };

        let text = render_diff(&diff, OutputFormat::Text).expect("render text diff");
        assert!(text.contains("added nodes: added"));
        assert!(text.contains("removed nodes: removed"));
        assert!(text.contains("updated nodes: updated"));
        assert!(text.contains("added connections: added.out -> updated.in"));
        assert!(text.contains("removed connections: old.out -> removed.in"));

        let json = render_diff(&diff, OutputFormat::Json).expect("render JSON diff");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse JSON diff");
        assert_eq!(value["added_nodes"], serde_json::json!(["added"]));
        assert_eq!(value["removed_nodes"], serde_json::json!(["removed"]));
        assert_eq!(value["updated_nodes"], serde_json::json!(["updated"]));
        assert_eq!(
            value["added_connections"],
            serde_json::json!(["added.out -> updated.in"])
        );
        assert_eq!(
            value["removed_connections"],
            serde_json::json!(["old.out -> removed.in"])
        );
    }

    #[test]
    fn invalid_reload_message_keeps_previous_configuration() {
        let message = render_reload_rejected("invalid node parameters");
        assert!(message.contains("REJECTED"));
        assert!(message.contains("invalid node parameters"));
        assert!(message.contains("graph configuration reload REJECTED"));
        assert!(message.contains("check graph status"));
    }

    #[cfg(feature = "openvino")]
    #[test]
    fn openvino_feature_registers_configuration() {
        let config = dg_runtime::BackendConfig::new(
            Some(std::path::PathBuf::from("model.xml")),
            serde_json::json!({"device": "GPU"}),
        );
        let option = dg_runtime::configure_backend("openvino", config).expect("configure OpenVINO");
        assert_eq!(option.backend, dg_runtime::BackendKind::OpenVINO);
        assert_eq!(
            option
                .backend_options
                .as_openvino()
                .expect("OpenVINO options")
                .device,
            "GPU"
        );
    }

    #[cfg(feature = "openvino")]
    #[test]
    fn validate_rejects_openvino_capability_mismatch_without_initializing_model() {
        let path = std::env::temp_dir().join(format!(
            "dg-cli-openvino-preflight-{}.yaml",
            std::process::id()
        ));
        let content = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: infer
    kind: inference
    params:
      backend: openvino
      model: missing.xml
      device: cuda_gpu
"#;
        fs::write(&path, content).expect("write config");
        let err = validate_graph(&path, ProcessRuntimePolicy::default())
            .expect_err("device should fail preflight");
        fs::remove_file(path).expect("remove config");
        assert!(format!("{err:#}").contains("unsupported device: CudaGpu"));
    }
}
