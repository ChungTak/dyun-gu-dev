use std::sync::atomic::{AtomicU64, Ordering};

/// Per-backend counters for inference submissions, in-flight requests, and
/// host/device memory copies.
///
/// Counters are updated by backends and read by [`crate::Runtime`] and graph
/// elements. All operations use `Relaxed` ordering; callers that need stricter
/// ordering should acquire/release around a snapshot.
#[derive(Debug, Default)]
pub struct BackendMetrics {
    submissions: AtomicU64,
    in_flight: AtomicU64,
    poll_pending: AtomicU64,
    backend_errors: AtomicU64,
    queue_wait_ns: AtomicU64,
    h2d_count: AtomicU64,
    h2d_bytes: AtomicU64,
    h2d_ns: AtomicU64,
    d2h_count: AtomicU64,
    d2h_bytes: AtomicU64,
    d2h_ns: AtomicU64,
    host_copy_count: AtomicU64,
    host_copy_bytes: AtomicU64,
    host_copy_ns: AtomicU64,
    /// Raw inference-latency observations, guarded by a mutex for sorting.
    infer_latencies_ns: std::sync::Mutex<Vec<u64>>,
}

impl BackendMetrics {
    pub fn record_submission(&self) {
        self.submissions.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_poll_pending(&self) {
        self.poll_pending.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_backend_error(&self) {
        self.backend_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_queue_wait_ns(&self, ns: u64) {
        self.queue_wait_ns.fetch_add(ns, Ordering::Relaxed);
    }

    pub fn record_infer_latency_ns(&self, ns: u64) {
        if let Ok(mut latencies) = self.infer_latencies_ns.lock() {
            latencies.push(ns);
        }
    }

    pub fn record_h2d(&self, bytes: u64, ns: u64) {
        self.h2d_count.fetch_add(1, Ordering::Relaxed);
        self.h2d_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.h2d_ns.fetch_add(ns, Ordering::Relaxed);
    }

    pub fn record_d2h(&self, bytes: u64, ns: u64) {
        self.d2h_count.fetch_add(1, Ordering::Relaxed);
        self.d2h_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.d2h_ns.fetch_add(ns, Ordering::Relaxed);
    }

    pub fn record_host_copy(&self, bytes: u64, ns: u64) {
        self.host_copy_count.fetch_add(1, Ordering::Relaxed);
        self.host_copy_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.host_copy_ns.fetch_add(ns, Ordering::Relaxed);
    }

    pub fn finish_in_flight(&self) {
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn submissions(&self) -> u64 {
        self.submissions.load(Ordering::Relaxed)
    }

    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }

    pub fn poll_pending(&self) -> u64 {
        self.poll_pending.load(Ordering::Relaxed)
    }

    pub fn backend_errors(&self) -> u64 {
        self.backend_errors.load(Ordering::Relaxed)
    }

    pub fn queue_wait_ns(&self) -> u64 {
        self.queue_wait_ns.load(Ordering::Relaxed)
    }

    pub fn h2d_count(&self) -> u64 {
        self.h2d_count.load(Ordering::Relaxed)
    }

    pub fn h2d_bytes(&self) -> u64 {
        self.h2d_bytes.load(Ordering::Relaxed)
    }

    pub fn h2d_ns(&self) -> u64 {
        self.h2d_ns.load(Ordering::Relaxed)
    }

    pub fn d2h_count(&self) -> u64 {
        self.d2h_count.load(Ordering::Relaxed)
    }

    pub fn d2h_bytes(&self) -> u64 {
        self.d2h_bytes.load(Ordering::Relaxed)
    }

    pub fn d2h_ns(&self) -> u64 {
        self.d2h_ns.load(Ordering::Relaxed)
    }

    pub fn host_copy_count(&self) -> u64 {
        self.host_copy_count.load(Ordering::Relaxed)
    }

    pub fn host_copy_bytes(&self) -> u64 {
        self.host_copy_bytes.load(Ordering::Relaxed)
    }

    pub fn host_copy_ns(&self) -> u64 {
        self.host_copy_ns.load(Ordering::Relaxed)
    }

    pub fn infer_latency_percentiles(&self) -> LatencyPercentiles {
        let mut latencies = self
            .infer_latencies_ns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if latencies.is_empty() {
            return LatencyPercentiles {
                count: 0,
                p50_ns: 0,
                p95_ns: 0,
                p99_ns: 0,
            };
        }
        latencies.sort_unstable();
        LatencyPercentiles {
            count: latencies.len() as u64,
            p50_ns: percentile(&latencies, 0.5),
            p95_ns: percentile(&latencies, 0.95),
            p99_ns: percentile(&latencies, 0.99),
        }
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let index = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Percentile summary of inference latencies recorded by [`BackendMetrics`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LatencyPercentiles {
    pub count: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
}

/// A point-in-time snapshot of [`BackendMetrics`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendMetricsSnapshot {
    pub submissions: u64,
    pub in_flight: u64,
    pub poll_pending: u64,
    pub backend_errors: u64,
    pub queue_wait_ns: u64,
    pub h2d_count: u64,
    pub h2d_bytes: u64,
    pub h2d_ns: u64,
    pub d2h_count: u64,
    pub d2h_bytes: u64,
    pub d2h_ns: u64,
    pub host_copy_count: u64,
    pub host_copy_bytes: u64,
    pub host_copy_ns: u64,
    pub infer_latencies: LatencyPercentiles,
}

impl BackendMetrics {
    /// Returns a snapshot of all counters and latency percentiles.
    pub fn snapshot(&self) -> BackendMetricsSnapshot {
        BackendMetricsSnapshot {
            submissions: self.submissions(),
            in_flight: self.in_flight(),
            poll_pending: self.poll_pending(),
            backend_errors: self.backend_errors(),
            queue_wait_ns: self.queue_wait_ns(),
            h2d_count: self.h2d_count(),
            h2d_bytes: self.h2d_bytes(),
            h2d_ns: self.h2d_ns(),
            d2h_count: self.d2h_count(),
            d2h_bytes: self.d2h_bytes(),
            d2h_ns: self.d2h_ns(),
            host_copy_count: self.host_copy_count(),
            host_copy_bytes: self.host_copy_bytes(),
            host_copy_ns: self.host_copy_ns(),
            infer_latencies: self.infer_latency_percentiles(),
        }
    }
}
