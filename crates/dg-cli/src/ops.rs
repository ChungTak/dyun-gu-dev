use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use anyhow::Result;
use dg_graph::{ElementMetricsSnapshot, GraphStatus};
use tiny_http::{Method, Request, Response, Server, StatusCode};
use tracing::{info, warn};

/// Shared snapshot of graph state exposed by the ops server.
#[derive(Clone, Debug)]
pub struct OpsState {
    pub status: GraphStatus,
    pub root_cause: Option<String>,
    pub element_metrics: BTreeMap<String, ElementMetricsSnapshot>,
    pub ready: bool,
    pub ready_reason: Option<String>,
}

/// Handle to the ops server thread.
pub struct OpsHandle {
    stop: Arc<AtomicBool>,
    server: Arc<Server>,
    thread: Option<JoinHandle<()>>,
}

impl OpsHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for OpsHandle {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop.store(true, Ordering::SeqCst);
            self.server.unblock();
        }
    }
}

/// Starts a local HTTP ops server on `bind` and returns a handle.
///
/// Binds to loopback by default. If `bind` resolves to a non-loopback
/// interface, a security warning is logged because the ops endpoint is
/// intended for local observability only and does not authenticate.
pub fn start(state: Arc<RwLock<OpsState>>, bind: &str) -> Result<OpsHandle> {
    let server = Arc::new(
        Server::http(bind)
            .map_err(|err| anyhow::anyhow!("failed to bind ops server on {bind}: {err}"))?,
    );
    let actual_addr = server.server_addr().to_string();
    if !is_loopback(bind) {
        warn!(
            ops_bind = %actual_addr,
            "ops server bound to a non-loopback address; it exposes health and metrics without authentication"
        );
    }
    info!(ops_bind = %actual_addr, "ops server listening");

    let stop = Arc::new(AtomicBool::new(false));
    let server_clone: Arc<Server> = Arc::clone(&server);
    let stop_clone = Arc::clone(&stop);

    let thread = thread::spawn(move || {
        while !stop_clone.load(Ordering::SeqCst) {
            match server_clone.recv() {
                Ok(request) => handle_request(&state, request),
                Err(_) => break,
            }
        }
    });

    Ok(OpsHandle {
        stop,
        server,
        thread: Some(thread),
    })
}

fn is_loopback(bind: &str) -> bool {
    bind.starts_with("127.")
        || bind.starts_with("[::1]")
        || bind.starts_with("::1")
        || bind.starts_with("localhost:")
        || bind == "localhost"
}

fn handle_request(state: &Arc<RwLock<OpsState>>, request: Request) {
    let response = match (request.method(), request.url()) {
        (&Method::Get, "/livez") => livez_response(),
        (&Method::Get, "/readyz") => readyz_response(state),
        (&Method::Get, "/metrics") => metrics_response(state),
        (&Method::Head, path) => {
            let status = match path {
                "/livez" | "/readyz" | "/metrics" => {
                    if path == "/readyz" && !is_ready(state) {
                        StatusCode(503)
                    } else {
                        StatusCode(200)
                    }
                }
                _ => StatusCode(404),
            };
            Response::from_string("").with_status_code(status)
        }
        _ => Response::from_string("").with_status_code(StatusCode(404)),
    };

    if let Err(err) = request.respond(response) {
        warn!(error = %err, "ops response failed");
    }
}

fn livez_response() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string("ok\n")
}

fn readyz_response(state: &Arc<RwLock<OpsState>>) -> Response<std::io::Cursor<Vec<u8>>> {
    if is_ready(state) {
        Response::from_string("ready\n")
    } else {
        let reason = read_ready_reason(state);
        Response::from_string(reason).with_status_code(StatusCode(503))
    }
}

fn is_ready(state: &Arc<RwLock<OpsState>>) -> bool {
    state.read().map(|s| s.ready).unwrap_or(false)
}

fn read_ready_reason(state: &Arc<RwLock<OpsState>>) -> String {
    state
        .read()
        .map(|s| {
            s.ready_reason
                .clone()
                .or_else(|| s.root_cause.clone())
                .unwrap_or_else(|| format!("graph status is {:?}", s.status))
        })
        .unwrap_or_else(|_| "ops state locked".to_string())
}

fn metrics_response(state: &Arc<RwLock<OpsState>>) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = match render_metrics(state) {
        Ok(text) => text,
        Err(err) => {
            return Response::from_string(format!("failed to render metrics: {err}"))
                .with_status_code(StatusCode(500))
        }
    };
    Response::from_string(body)
}

fn render_metrics(state: &Arc<RwLock<OpsState>>) -> Result<String> {
    let snapshot = state
        .read()
        .map_err(|_| anyhow::anyhow!("ops state poisoned"))?;

    let mut out = String::new();
    writeln!(
        out,
        "# HELP dg_graph_status 1 if the graph is running, 0 otherwise"
    )?;
    writeln!(out, "# TYPE dg_graph_status gauge")?;
    writeln!(
        out,
        "dg_graph_status {{status=\"{:?}\"}} {}",
        snapshot.status,
        if snapshot.status == GraphStatus::Running {
            1
        } else {
            0
        }
    )?;
    writeln!(out)?;

    for (node, metrics) in &snapshot.element_metrics {
        let node_label = sanitize_label(node);
        render_counter(
            &mut out,
            "dg_element_packets_received_total",
            &node_label,
            metrics.packets_received,
        )?;
        render_counter(
            &mut out,
            "dg_element_packets_processed_total",
            &node_label,
            metrics.packets_processed,
        )?;
        render_counter(
            &mut out,
            "dg_element_packets_sent_total",
            &node_label,
            metrics.packets_sent,
        )?;
        render_counter(
            &mut out,
            "dg_element_drops_total",
            &node_label,
            metrics.drop_count,
        )?;
        render_counter(
            &mut out,
            "dg_element_backpressure_events_total",
            &node_label,
            metrics.backpressure_count,
        )?;

        writeln!(
            out,
            "# HELP dg_element_queue_depth Current element input queue depth"
        )?;
        writeln!(out, "# TYPE dg_element_queue_depth gauge")?;
        writeln!(
            out,
            "dg_element_queue_depth{{node=\"{node_label}\"}} {}",
            metrics.queue_depth
        )?;

        writeln!(
            out,
            "# HELP dg_element_queue_depth_max Maximum element input queue depth observed"
        )?;
        writeln!(out, "# TYPE dg_element_queue_depth_max gauge")?;
        writeln!(
            out,
            "dg_element_queue_depth_max{{node=\"{node_label}\"}} {}",
            metrics.max_queue_depth
        )?;

        writeln!(
            out,
            "# HELP dg_element_processing_latency_nanoseconds_total Cumulative processing latency"
        )?;
        writeln!(
            out,
            "# TYPE dg_element_processing_latency_nanoseconds_total counter"
        )?;
        writeln!(
            out,
            "dg_element_processing_latency_nanoseconds_total{{node=\"{node_label}\"}} {}",
            metrics.processing_latency_ns
        )?;

        writeln!(
            out,
            "# HELP dg_element_processing_latency_max_nanoseconds Maximum processing latency observed"
        )?;
        writeln!(
            out,
            "# TYPE dg_element_processing_latency_max_nanoseconds gauge"
        )?;
        writeln!(
            out,
            "dg_element_processing_latency_max_nanoseconds{{node=\"{node_label}\"}} {}",
            metrics.processing_latency_max_ns
        )?;

        writeln!(
            out,
            "# HELP dg_element_processing_latency_avg_nanoseconds Average processing latency observed"
        )?;
        writeln!(
            out,
            "# TYPE dg_element_processing_latency_avg_nanoseconds gauge"
        )?;
        writeln!(
            out,
            "dg_element_processing_latency_avg_nanoseconds{{node=\"{node_label}\"}} {}",
            metrics.processing_latency_avg_ns
        )?;

        writeln!(out)?;
    }

    Ok(out)
}

fn render_counter(out: &mut String, name: &str, node_label: &str, value: u64) -> std::fmt::Result {
    writeln!(out, "# HELP {name} {name} counter")?;
    writeln!(out, "# TYPE {name} counter")?;
    writeln!(out, "{name}{{node=\"{node_label}\"}} {value}")
}

fn sanitize_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "_")
}
