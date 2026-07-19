use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use anyhow::Result;
use dg_graph::{ElementMetricsSnapshot, GraphStatus};
use tiny_http::{Method, Request, Response, Server, StatusCode};
use tracing::{info, warn};

/// Prometheus/OpenMetrics exposition schema version. Bumped when field names,
/// types, or label sets change.
const METRICS_SCHEMA_VERSION: u32 = 1;

/// Shared snapshot of graph state exposed by the ops server.
#[derive(Clone, Debug)]
pub struct OpsState {
    pub status: GraphStatus,
    pub root_cause: Option<String>,
    pub element_metrics: BTreeMap<String, ElementMetricsSnapshot>,
    pub ready: bool,
    pub ready_reason: Option<String>,
    /// Supervisor-level reload counters (INT5-04/08).
    pub reload_attempts_total: u64,
    pub reload_success_total: u64,
    pub reload_rejected_total: u64,
    /// Set to false when the ops server is asked to stop or has exited its
    /// request loop. `/livez` fails while this is false.
    pub supervisor_healthy: bool,
}

/// Handle to the ops server thread.
pub struct OpsHandle {
    stop: Arc<AtomicBool>,
    server: Arc<Server>,
    state: Arc<RwLock<OpsState>>,
    thread: Option<JoinHandle<()>>,
}

impl OpsHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.write() {
            state.supervisor_healthy = false;
        }
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for OpsHandle {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.store(true, Ordering::SeqCst);
            if let Ok(mut state) = self.state.write() {
                state.supervisor_healthy = false;
            }
            self.server.unblock();
            let _ = thread.join();
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
    let state_clone = Arc::clone(&state);

    let thread = thread::spawn(move || {
        while !stop_clone.load(Ordering::SeqCst) {
            match server_clone.recv() {
                Ok(request) => handle_request(&state_clone, request),
                Err(_) => {
                    if let Ok(mut state) = state_clone.write() {
                        state.supervisor_healthy = false;
                    }
                    break;
                }
            }
        }
    });

    Ok(OpsHandle {
        stop,
        server,
        state,
        thread: Some(thread),
    })
}

fn is_loopback(bind: &str) -> bool {
    if let Ok(addr) = bind.parse::<std::net::SocketAddr>() {
        return addr.ip().is_loopback();
    }

    let (host, _) = bind.rsplit_once(':').unwrap_or((bind, ""));
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    if host == "localhost" {
        return true;
    }

    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

fn handle_request(state: &Arc<RwLock<OpsState>>, request: Request) {
    let response = match (request.method(), request.url()) {
        (&Method::Get, "/livez") => livez_response(state),
        (&Method::Get, "/readyz") => readyz_response(state),
        (&Method::Get, "/metrics") => metrics_response(state),
        (&Method::Head, path) => {
            let status = match path {
                "/livez" => {
                    if is_live(state) {
                        StatusCode(200)
                    } else {
                        StatusCode(503)
                    }
                }
                "/readyz" => {
                    if is_ready(state) {
                        StatusCode(200)
                    } else {
                        StatusCode(503)
                    }
                }
                "/metrics" => StatusCode(200),
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

fn livez_response(state: &Arc<RwLock<OpsState>>) -> Response<std::io::Cursor<Vec<u8>>> {
    if is_live(state) {
        Response::from_string("ok\n")
    } else {
        Response::from_string("not live\n").with_status_code(StatusCode(503))
    }
}

fn is_live(state: &Arc<RwLock<OpsState>>) -> bool {
    state.read().map(|s| s.supervisor_healthy).unwrap_or(false)
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
    state
        .read()
        .map(|s| {
            s.ready
                && s.status == GraphStatus::Running
                && s.root_cause.is_none()
                && s.supervisor_healthy
        })
        .unwrap_or(false)
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
    let snapshot = match state.read() {
        Ok(guard) => guard.clone(),
        Err(_) => {
            return Response::from_string("ops state poisoned").with_status_code(StatusCode(500))
        }
    };
    let body = match render_metrics(&snapshot) {
        Ok(text) => text,
        Err(err) => {
            return Response::from_string(format!("failed to render metrics: {err}"))
                .with_status_code(StatusCode(500))
        }
    };
    Response::from_string(body)
}

fn render_metrics(snapshot: &OpsState) -> Result<String> {
    let mut out = String::new();
    writeln!(
        out,
        "# HELP dg_cli_metrics_schema_version Metric exposition schema version"
    )?;
    writeln!(out, "# TYPE dg_cli_metrics_schema_version gauge")?;
    writeln!(
        out,
        "dg_cli_metrics_schema_version {}",
        METRICS_SCHEMA_VERSION
    )?;
    writeln!(out)?;
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
    writeln!(
        out,
        "# HELP dg_graph_reload_attempts_total Graph configuration reload attempts"
    )?;
    writeln!(out, "# TYPE dg_graph_reload_attempts_total counter")?;
    writeln!(
        out,
        "dg_graph_reload_attempts_total {}",
        snapshot.reload_attempts_total
    )?;
    writeln!(
        out,
        "# HELP dg_graph_reload_success_total Successful graph configuration reloads"
    )?;
    writeln!(out, "# TYPE dg_graph_reload_success_total counter")?;
    writeln!(
        out,
        "dg_graph_reload_success_total {}",
        snapshot.reload_success_total
    )?;
    writeln!(
        out,
        "# HELP dg_graph_reload_rejected_total Rejected graph configuration reloads"
    )?;
    writeln!(out, "# TYPE dg_graph_reload_rejected_total counter")?;
    writeln!(
        out,
        "dg_graph_reload_rejected_total {}",
        snapshot.reload_rejected_total
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
        render_counter(
            &mut out,
            "dg_element_state_resets_total",
            &node_label,
            metrics.state_reset_total,
        )?;
        render_counter(
            &mut out,
            "dg_element_overflow_count_total",
            &node_label,
            metrics.overflow_count,
        )?;
        render_counter(
            &mut out,
            "dg_element_reconnects_total",
            &node_label,
            metrics.reconnect_total,
        )?;
        writeln!(
            out,
            "# HELP dg_element_reconnecting 1 if the element is mid-reconnect"
        )?;
        writeln!(out, "# TYPE dg_element_reconnecting gauge")?;
        writeln!(
            out,
            "dg_element_reconnecting{{node=\"{node_label}\"}} {}",
            u8::from(metrics.reconnecting)
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

        if let Some(backend) = &metrics.backend_metrics {
            render_counter(
                &mut out,
                "dg_backend_submissions_total",
                &node_label,
                backend.submissions,
            )?;
            writeln!(
                out,
                "# HELP dg_backend_in_flight Current backend in-flight submissions"
            )?;
            writeln!(out, "# TYPE dg_backend_in_flight gauge")?;
            writeln!(
                out,
                "dg_backend_in_flight{{node=\"{node_label}\"}} {}",
                backend.in_flight
            )?;
            render_counter(
                &mut out,
                "dg_backend_poll_pending_total",
                &node_label,
                backend.poll_pending,
            )?;
            render_counter(
                &mut out,
                "dg_backend_errors_total",
                &node_label,
                backend.backend_errors,
            )?;
            render_counter(
                &mut out,
                "dg_backend_h2d_copies_total",
                &node_label,
                backend.h2d_count,
            )?;
            render_counter(
                &mut out,
                "dg_backend_h2d_bytes_total",
                &node_label,
                backend.h2d_bytes,
            )?;
            render_counter(
                &mut out,
                "dg_backend_d2h_copies_total",
                &node_label,
                backend.d2h_count,
            )?;
            render_counter(
                &mut out,
                "dg_backend_d2h_bytes_total",
                &node_label,
                backend.d2h_bytes,
            )?;
            render_counter(
                &mut out,
                "dg_backend_host_copies_total",
                &node_label,
                backend.host_copy_count,
            )?;
            render_counter(
                &mut out,
                "dg_backend_host_copy_bytes_total",
                &node_label,
                backend.host_copy_bytes,
            )?;
            writeln!(
                out,
                "# HELP dg_backend_infer_latency_p50_nanoseconds Inference latency p50"
            )?;
            writeln!(out, "# TYPE dg_backend_infer_latency_p50_nanoseconds gauge")?;
            writeln!(
                out,
                "dg_backend_infer_latency_p50_nanoseconds{{node=\"{node_label}\"}} {}",
                backend.infer_latencies.p50_ns
            )?;
            writeln!(
                out,
                "# HELP dg_backend_infer_latency_p95_nanoseconds Inference latency p95"
            )?;
            writeln!(out, "# TYPE dg_backend_infer_latency_p95_nanoseconds gauge")?;
            writeln!(
                out,
                "dg_backend_infer_latency_p95_nanoseconds{{node=\"{node_label}\"}} {}",
                backend.infer_latencies.p95_ns
            )?;
            writeln!(
                out,
                "# HELP dg_backend_infer_latency_p99_nanoseconds Inference latency p99"
            )?;
            writeln!(out, "# TYPE dg_backend_infer_latency_p99_nanoseconds gauge")?;
            writeln!(
                out,
                "dg_backend_infer_latency_p99_nanoseconds{{node=\"{node_label}\"}} {}",
                backend.infer_latencies.p99_ns
            )?;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn sample_state() -> Arc<RwLock<OpsState>> {
        Arc::new(RwLock::new(OpsState {
            status: GraphStatus::Running,
            root_cause: None,
            element_metrics: BTreeMap::new(),
            ready: true,
            ready_reason: None,
            reload_attempts_total: 1,
            reload_success_total: 1,
            reload_rejected_total: 0,
            supervisor_healthy: true,
        }))
    }

    #[test]
    fn livez_returns_200_while_supervisor_healthy() {
        let state = sample_state();
        let response = livez_response(&state);
        assert_eq!(response.status_code(), StatusCode(200));
    }

    #[test]
    fn livez_returns_503_when_supervisor_unhealthy() {
        let state = sample_state();
        state.write().unwrap().supervisor_healthy = false;
        let response = livez_response(&state);
        assert_eq!(response.status_code(), StatusCode(503));
        let mut body = String::new();
        response.into_reader().read_to_string(&mut body).unwrap();
        assert!(body.contains("not live"));
    }

    #[test]
    fn readyz_returns_200_when_graph_ready() {
        let state = sample_state();
        let response = readyz_response(&state);
        assert_eq!(response.status_code(), StatusCode(200));
    }

    #[test]
    fn readyz_returns_503_when_graph_not_running() {
        let state = sample_state();
        state.write().unwrap().status = GraphStatus::Stopped;
        let response = readyz_response(&state);
        assert_eq!(response.status_code(), StatusCode(503));
    }

    #[test]
    fn readyz_returns_503_with_reason_when_not_ready() {
        let state = sample_state();
        {
            let mut s = state.write().unwrap();
            s.ready = false;
            s.ready_reason = Some("stream element reconnecting".to_string());
        }
        let response = readyz_response(&state);
        assert_eq!(response.status_code(), StatusCode(503));
        let mut body = String::new();
        response.into_reader().read_to_string(&mut body).unwrap();
        assert!(body.contains("stream element reconnecting"));
    }

    #[test]
    fn metrics_expose_schema_version_and_bounded_labels() {
        let snapshot = OpsState {
            status: GraphStatus::Running,
            root_cause: None,
            element_metrics: BTreeMap::new(),
            ready: true,
            ready_reason: None,
            reload_attempts_total: 0,
            reload_success_total: 0,
            reload_rejected_total: 0,
            supervisor_healthy: true,
        };
        let text = render_metrics(&snapshot).expect("render_metrics succeeds");
        assert!(text.contains("dg_cli_metrics_schema_version 1"));
        // No raw user-provided strings (e.g. a URL) should appear as metric names.
        assert!(!text.contains("http://"));
    }
}
