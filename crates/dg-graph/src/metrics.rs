use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dg_runtime::{BackendMetrics, BackendMetricsSnapshot};

/// Additive counters use saturating arithmetic and report wrap events through
/// `overflow_count` so scrapers can detect loss of precision.
#[derive(Debug, Default)]
pub(crate) struct ElementMetrics {
    packets_processed: AtomicU64,
    packets_received: AtomicU64,
    packets_sent: AtomicU64,
    processing_latency_ns: AtomicU64,
    processing_latency_max_ns: AtomicU64,
    queue_depth: AtomicUsize,
    max_queue_depth: AtomicUsize,
    drop_count: AtomicU64,
    backpressure_count: AtomicU64,
    /// Times this element instance was rebuilt by a hot update (state reset).
    state_reset_total: AtomicU64,
    /// True while a stream element is between disconnect and successful reconnect.
    reconnecting: AtomicBool,
    /// Successful or attempted reconnects after the initial open.
    reconnect_total: AtomicU64,
    overflow_count: AtomicU64,
    backend_metrics: Mutex<Option<Arc<BackendMetrics>>>,
}

impl ElementMetrics {
    pub(crate) fn record_received(&self) {
        self.saturating_add(&self.packets_received, 1);
        self.saturating_add(&self.packets_processed, 1);
    }

    pub(crate) fn record_source_packet(&self) {
        self.saturating_add(&self.packets_processed, 1);
    }

    pub(crate) fn record_sent(&self) {
        self.saturating_add(&self.packets_sent, 1);
    }

    pub(crate) fn record_latency(&self, duration: Duration) {
        let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
        self.saturating_add(&self.processing_latency_ns, nanos);
        self.processing_latency_max_ns
            .fetch_max(nanos, Ordering::Relaxed);
    }

    pub(crate) fn record_queue_depth(&self, depth: usize) {
        self.queue_depth.store(depth, Ordering::Relaxed);
        self.max_queue_depth.fetch_max(depth, Ordering::Relaxed);
    }

    pub(crate) fn record_drop(&self) {
        self.saturating_add(&self.drop_count, 1);
    }

    pub(crate) fn record_backpressure(&self) {
        self.saturating_add(&self.backpressure_count, 1);
    }

    pub(crate) fn record_state_reset(&self) {
        self.saturating_add(&self.state_reset_total, 1);
    }

    pub(crate) fn set_reconnecting(&self, value: bool) {
        self.reconnecting.store(value, Ordering::Relaxed);
    }

    /// Clears the reconnecting flag without counting a reconnect (first open).
    pub(crate) fn clear_reconnecting(&self) {
        self.reconnecting.store(false, Ordering::Relaxed);
    }

    pub(crate) fn record_reconnect(&self) {
        self.saturating_add(&self.reconnect_total, 1);
        self.reconnecting.store(false, Ordering::Relaxed);
    }

    pub(crate) fn attach_backend_metrics(&self, metrics: Arc<BackendMetrics>) {
        if let Ok(mut guard) = self.backend_metrics.lock() {
            *guard = Some(metrics);
        }
    }

    pub(crate) fn snapshot(&self) -> ElementMetricsSnapshot {
        let packets_processed = self.packets_processed.load(Ordering::Relaxed);
        let processing_latency_ns = self.processing_latency_ns.load(Ordering::Relaxed);
        ElementMetricsSnapshot {
            packets_processed,
            packets_received: self.packets_received.load(Ordering::Relaxed),
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            processing_latency_ns,
            processing_latency_avg_ns: processing_latency_ns
                .checked_div(packets_processed)
                .unwrap_or_default(),
            processing_latency_max_ns: self.processing_latency_max_ns.load(Ordering::Relaxed),
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            max_queue_depth: self.max_queue_depth.load(Ordering::Relaxed),
            drop_count: self.drop_count.load(Ordering::Relaxed),
            backpressure_count: self.backpressure_count.load(Ordering::Relaxed),
            state_reset_total: self.state_reset_total.load(Ordering::Relaxed),
            reconnecting: self.reconnecting.load(Ordering::Relaxed),
            reconnect_total: self.reconnect_total.load(Ordering::Relaxed),
            overflow_count: self.overflow_count.load(Ordering::Relaxed),
            backend_metrics: self
                .backend_metrics
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|m| m.snapshot())),
        }
    }

    fn saturating_add(&self, counter: &AtomicU64, value: u64) {
        let mut current = counter.load(Ordering::Relaxed);
        loop {
            let (new, overflow) = current.overflowing_add(value);
            let target = if overflow { u64::MAX } else { new };
            match counter.compare_exchange_weak(
                current,
                target,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    if overflow {
                        self.overflow_count.fetch_add(1, Ordering::Relaxed);
                    }
                    break;
                }
                Err(actual) => current = actual,
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementMetricsSnapshot {
    pub packets_processed: u64,
    pub packets_received: u64,
    pub packets_sent: u64,
    pub processing_latency_ns: u64,
    pub processing_latency_avg_ns: u64,
    pub processing_latency_max_ns: u64,
    pub queue_depth: usize,
    pub max_queue_depth: usize,
    pub drop_count: u64,
    pub backpressure_count: u64,
    /// Number of hot-update rebuilds for this node (state was reset).
    pub state_reset_total: u64,
    /// True while a stream source/sink is reconnecting.
    pub reconnecting: bool,
    /// Reconnect attempts/completions recorded by stream elements.
    pub reconnect_total: u64,
    /// Number of additive counter wrap events detected for this element.
    pub overflow_count: u64,
    pub backend_metrics: Option<BackendMetricsSnapshot>,
}

/// Receives per-node snapshots for future exporters such as Prometheus.
pub trait MetricsSink: Send + Sync {
    fn record(&self, node: &str, metrics: &ElementMetricsSnapshot);
}
