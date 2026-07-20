use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

const BUCKET_COUNT: usize = 16;

/// Fixed upper bounds (in nanoseconds) for the latency histogram buckets.
///
/// Buckets: 100µs, 250µs, 500µs, 1ms, 2.5ms, 5ms, 10ms, 25ms, 50ms, 100ms,
/// 250ms, 500ms, 1s, 2.5s, 5s, +Inf.
const BUCKET_BOUNDS_NS: [u64; BUCKET_COUNT] = [
    100_000,
    250_000,
    500_000,
    1_000_000,
    2_500_000,
    5_000_000,
    10_000_000,
    25_000_000,
    50_000_000,
    100_000_000,
    250_000_000,
    500_000_000,
    1_000_000_000,
    2_500_000_000,
    5_000_000_000,
    u64::MAX,
];

/// Per-backend counters for inference submissions, in-flight requests, and
/// host/device memory copies.
///
/// Counters are updated by backends and read by [`crate::Runtime`] and graph
/// elements. All operations use `Relaxed` ordering; callers that need stricter
/// ordering should acquire/release around a snapshot.
///
/// All additive counters use checked/saturating CAS: values that exceed
/// `u64::MAX` clamp to `u64::MAX` and increment the `overflow` diagnostic.
/// `in_flight` decrements use checked subtraction; underflow clamps to zero and
/// increments the `underflow` diagnostic.
#[derive(Debug)]
pub struct BackendMetrics {
    submissions: AtomicU64,
    in_flight: AtomicU64,
    poll_pending: AtomicU64,
    backend_errors: AtomicU64,
    cancelled: AtomicU64,
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
    /// Fixed-bucket atomic histogram for inference latencies.
    infer_latencies: LatencyHistogram,
    /// Saturated/in-flight counters that overflowed.
    overflow: AtomicU64,
    /// In-flight counter that underflowed (release without matching acquire).
    underflow: AtomicU64,
}

impl Default for BackendMetrics {
    fn default() -> Self {
        Self {
            submissions: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            poll_pending: AtomicU64::new(0),
            backend_errors: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
            queue_wait_ns: AtomicU64::new(0),
            h2d_count: AtomicU64::new(0),
            h2d_bytes: AtomicU64::new(0),
            h2d_ns: AtomicU64::new(0),
            d2h_count: AtomicU64::new(0),
            d2h_bytes: AtomicU64::new(0),
            d2h_ns: AtomicU64::new(0),
            host_copy_count: AtomicU64::new(0),
            host_copy_bytes: AtomicU64::new(0),
            host_copy_ns: AtomicU64::new(0),
            infer_latencies: LatencyHistogram::default(),
            overflow: AtomicU64::new(0),
            underflow: AtomicU64::new(0),
        }
    }
}

impl BackendMetrics {
    pub fn record_submission(&self) {
        self.saturating_add(&self.submissions, 1);
        self.saturating_add(&self.in_flight, 1);
    }

    pub fn record_poll_pending(&self) {
        self.saturating_add(&self.poll_pending, 1);
    }

    pub fn record_backend_error(&self) {
        self.saturating_add(&self.backend_errors, 1);
    }

    pub fn record_cancel(&self) {
        self.saturating_add(&self.cancelled, 1);
    }

    pub fn record_queue_wait_ns(&self, ns: u64) {
        self.saturating_add(&self.queue_wait_ns, ns);
    }

    pub fn record_infer_latency_ns(&self, ns: u64) {
        self.infer_latencies.record(ns, &self.overflow);
    }

    pub fn record_h2d(&self, bytes: u64, ns: u64) {
        self.saturating_add(&self.h2d_count, 1);
        self.saturating_add(&self.h2d_bytes, bytes);
        self.saturating_add(&self.h2d_ns, ns);
    }

    pub fn record_d2h(&self, bytes: u64, ns: u64) {
        self.saturating_add(&self.d2h_count, 1);
        self.saturating_add(&self.d2h_bytes, bytes);
        self.saturating_add(&self.d2h_ns, ns);
    }

    pub fn record_host_copy(&self, bytes: u64, ns: u64) {
        self.saturating_add(&self.host_copy_count, 1);
        self.saturating_add(&self.host_copy_bytes, bytes);
        self.saturating_add(&self.host_copy_ns, ns);
    }

    pub fn finish_in_flight(&self) {
        self.checked_sub(&self.in_flight, 1);
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

    pub fn cancelled(&self) -> u64 {
        self.cancelled.load(Ordering::Relaxed)
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

    pub fn overflow_count(&self) -> u64 {
        self.overflow.load(Ordering::Relaxed)
    }

    pub fn underflow_count(&self) -> u64 {
        self.underflow.load(Ordering::Relaxed)
    }

    pub fn infer_latency_percentiles(&self) -> LatencyPercentiles {
        self.infer_latencies.snapshot()
    }

    fn saturating_add(&self, counter: &AtomicU64, value: u64) {
        saturating_fetch_add(counter, value, &self.overflow);
    }

    fn checked_sub(&self, counter: &AtomicU64, value: u64) {
        checked_fetch_sub(counter, value, &self.underflow);
    }
}

/// Fixed-bucket atomic histogram for inference latencies.
#[derive(Debug)]
struct LatencyHistogram {
    buckets: [AtomicU64; BUCKET_COUNT],
    sum: AtomicU64,
    max: AtomicU64,
    count: AtomicU64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }
}

impl LatencyHistogram {
    fn record(&self, ns: u64, overflow: &AtomicU64) {
        saturating_fetch_add(&self.count, 1, overflow);
        saturating_fetch_add(&self.sum, ns, overflow);
        self.max.fetch_max(ns, Ordering::Relaxed);
        let bucket = bucket_index(ns);
        saturating_fetch_add(&self.buckets[bucket], 1, overflow);
    }

    fn snapshot(&self) -> LatencyPercentiles {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return LatencyPercentiles::default();
        }

        let mut buckets = [0u64; BUCKET_COUNT];
        for (i, bucket) in self.buckets.iter().enumerate() {
            buckets[i] = bucket.load(Ordering::Relaxed);
        }

        let mut cumulative = [0u64; BUCKET_COUNT];
        let mut running = 0u64;
        for (i, &value) in buckets.iter().enumerate() {
            running = running.saturating_add(value);
            cumulative[i] = running;
        }

        LatencyPercentiles {
            count,
            sum_ns: self.sum.load(Ordering::Relaxed),
            max_ns: self.max.load(Ordering::Relaxed),
            p50_ns: percentile_from_histogram(&cumulative, count, 50),
            p95_ns: percentile_from_histogram(&cumulative, count, 95),
            p99_ns: percentile_from_histogram(&cumulative, count, 99),
            buckets,
            cumulative,
        }
    }
}

fn bucket_index(ns: u64) -> usize {
    BUCKET_BOUNDS_NS
        .iter()
        .position(|&bound| ns <= bound)
        .unwrap_or(BUCKET_COUNT - 1)
}

fn percentile_from_histogram(cumulative: &[u64; BUCKET_COUNT], count: u64, percent: u64) -> u64 {
    // `percent` is expected to be in 0..=100; clamp to avoid nonsensical targets.
    let percent = percent.min(100);
    let target = count.saturating_mul(percent).saturating_add(99) / 100;
    let target = target.max(1).min(count);
    for (i, &cumulative_count) in cumulative.iter().enumerate() {
        if cumulative_count >= target {
            return BUCKET_BOUNDS_NS[i];
        }
    }
    BUCKET_BOUNDS_NS[BUCKET_COUNT - 1]
}

fn saturating_fetch_add(atomic: &AtomicU64, value: u64, overflow: &AtomicU64) -> u64 {
    let mut current = atomic.load(Ordering::Relaxed);
    loop {
        let new = match current.checked_add(value) {
            Some(v) => v,
            None => {
                overflow.fetch_add(1, Ordering::Relaxed);
                u64::MAX
            }
        };
        match atomic.compare_exchange_weak(current, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return current,
            Err(actual) => current = actual,
        }
    }
}

fn checked_fetch_sub(atomic: &AtomicU64, value: u64, underflow: &AtomicU64) -> u64 {
    let mut current = atomic.load(Ordering::Relaxed);
    loop {
        let new = match current.checked_sub(value) {
            Some(v) => v,
            None => {
                underflow.fetch_add(1, Ordering::Relaxed);
                0
            }
        };
        match atomic.compare_exchange_weak(current, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return current,
            Err(actual) => current = actual,
        }
    }
}

/// Percentile summary of inference latencies recorded by [`BackendMetrics`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub count: u64,
    pub sum_ns: u64,
    pub max_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub buckets: [u64; BUCKET_COUNT],
    pub cumulative: [u64; BUCKET_COUNT],
}

impl Default for LatencyPercentiles {
    fn default() -> Self {
        Self {
            count: 0,
            sum_ns: 0,
            max_ns: 0,
            p50_ns: 0,
            p95_ns: 0,
            p99_ns: 0,
            buckets: [0; BUCKET_COUNT],
            cumulative: [0; BUCKET_COUNT],
        }
    }
}

/// A point-in-time snapshot of [`BackendMetrics`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendMetricsSnapshot {
    pub submissions: u64,
    pub in_flight: u64,
    pub poll_pending: u64,
    pub backend_errors: u64,
    pub cancelled: u64,
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
    pub overflow_count: u64,
    pub underflow_count: u64,
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
            cancelled: self.cancelled(),
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
            overflow_count: self.overflow_count(),
            underflow_count: self.underflow_count(),
            infer_latencies: self.infer_latency_percentiles(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_histogram_bucket_mapping() {
        let hist = LatencyHistogram::default();
        hist.record(50_000, &AtomicU64::new(0)); // 100µs bucket
        hist.record(100_000, &AtomicU64::new(0)); // 100µs bucket
        hist.record(100_001, &AtomicU64::new(0)); // 250µs bucket
        hist.record(6_000_000_000, &AtomicU64::new(0)); // +Inf bucket

        let snap = hist.snapshot();
        assert_eq!(snap.count, 4);
        assert_eq!(snap.buckets[0], 2);
        assert_eq!(snap.buckets[1], 1);
        assert_eq!(snap.buckets[BUCKET_COUNT - 1], 1);
    }

    #[test]
    fn percentile_approximation_from_histogram() {
        let hist = LatencyHistogram::default();
        let overflow = AtomicU64::new(0);
        for _ in 0..100 {
            hist.record(1_000_000, &overflow); // 1ms bucket
        }
        let snap = hist.snapshot();
        assert_eq!(snap.count, 100);
        assert_eq!(snap.p50_ns, 1_000_000);
        assert_eq!(snap.p95_ns, 1_000_000);
        assert_eq!(snap.p99_ns, 1_000_000);
    }
}
