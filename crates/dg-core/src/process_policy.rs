//! Process-wide runtime policy configured by a trusted bootstrap path.
//!
//! `ProcessRuntimePolicy` combines hard resource limits with memory-pool,
//! stream-registry, deadline, affinity and metrics-serialization budgets. It is
//! immutable for the lifetime of a process and is the single source of truth for
//! CLI, Rust and C entry points.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Error, MemoryPoolConfig, ResourcePolicy, Result};

/// Bounds for the in-process stream registry and subscriber cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StreamRegistryLimits {
    /// Maximum number of concurrent streams tracked in the registry.
    pub max_streams: usize,
    /// Maximum number of subscribers per stream.
    pub max_subscribers_per_stream: usize,
    /// Maximum bytes retained for bootstrap buffers.
    pub max_bootstrap_bytes: usize,
    /// Maximum frames retained for bootstrap buffers.
    pub max_bootstrap_frames: usize,
    /// Idle TTL for transient registry entries, in seconds.
    pub idle_ttl_seconds: u64,
}

impl StreamRegistryLimits {
    pub const DEFAULT_MAX_STREAMS: usize = 10_000;
    pub const DEFAULT_MAX_SUBSCRIBERS_PER_STREAM: usize = 256;
    pub const DEFAULT_MAX_BOOTSTRAP_BYTES: usize = 64 * 1024 * 1024;
    pub const DEFAULT_MAX_BOOTSTRAP_FRAMES: usize = 256;
    pub const DEFAULT_IDLE_TTL_SECONDS: u64 = 300;

    pub fn new(
        max_streams: usize,
        max_subscribers_per_stream: usize,
        max_bootstrap_bytes: usize,
        max_bootstrap_frames: usize,
        idle_ttl_seconds: u64,
    ) -> Result<Self> {
        let limits = Self {
            max_streams,
            max_subscribers_per_stream,
            max_bootstrap_bytes,
            max_bootstrap_frames,
            idle_ttl_seconds,
        };
        limits.validate()?;
        Ok(limits)
    }

    fn validate(&self) -> Result<()> {
        Self::check_nonzero(self.max_streams, "max_streams")?;
        Self::check_nonzero(
            self.max_subscribers_per_stream,
            "max_subscribers_per_stream",
        )?;
        Self::check_nonzero(self.max_bootstrap_bytes, "max_bootstrap_bytes")?;
        Self::check_nonzero(self.max_bootstrap_frames, "max_bootstrap_frames")?;
        Self::check_nonzero(self.idle_ttl_seconds as usize, "idle_ttl_seconds")?;
        Self::check_representable(self.max_streams, "max_streams")?;
        Self::check_representable(
            self.max_subscribers_per_stream,
            "max_subscribers_per_stream",
        )?;
        Self::check_representable(self.max_bootstrap_bytes, "max_bootstrap_bytes")?;
        Self::check_representable(self.max_bootstrap_frames, "max_bootstrap_frames")?;
        Self::check_representable(self.idle_ttl_seconds as usize, "idle_ttl_seconds")?;
        Ok(())
    }

    fn check_nonzero(value: usize, name: &str) -> Result<()> {
        if value == 0 {
            return Err(Error::Config(format!("{name} must be > 0")));
        }
        Ok(())
    }

    fn check_representable(value: usize, name: &str) -> Result<()> {
        if value > u32::MAX as usize {
            return Err(Error::Config(format!(
                "{name} {value} exceeds 32-bit representable maximum {}",
                u32::MAX
            )));
        }
        Ok(())
    }
}

impl Default for StreamRegistryLimits {
    fn default() -> Self {
        Self {
            max_streams: Self::DEFAULT_MAX_STREAMS,
            max_subscribers_per_stream: Self::DEFAULT_MAX_SUBSCRIBERS_PER_STREAM,
            max_bootstrap_bytes: Self::DEFAULT_MAX_BOOTSTRAP_BYTES,
            max_bootstrap_frames: Self::DEFAULT_MAX_BOOTSTRAP_FRAMES,
            idle_ttl_seconds: Self::DEFAULT_IDLE_TTL_SECONDS,
        }
    }
}

/// Deadline budgets used by connectors, shutdown and drain paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeadlinePolicy {
    /// Maximum time to wait for a connection to be established, in milliseconds.
    pub connect_ms: u64,
    /// Maximum interval between recv polls, in milliseconds.
    pub recv_poll_ms: u64,
    /// Maximum time for a single I/O operation, in milliseconds.
    pub io_ms: u64,
    /// Maximum time to drain in-flight work on shutdown, in milliseconds.
    pub drain_ms: u64,
    /// Maximum time to wait for full shutdown, in milliseconds.
    pub shutdown_ms: u64,
}

impl DeadlinePolicy {
    pub const DEFAULT_CONNECT_MS: u64 = 10_000;
    pub const DEFAULT_RECV_POLL_MS: u64 = 100;
    pub const DEFAULT_IO_MS: u64 = 30_000;
    pub const DEFAULT_DRAIN_MS: u64 = 10_000;
    pub const DEFAULT_SHUTDOWN_MS: u64 = 30_000;

    pub fn new(
        connect_ms: u64,
        recv_poll_ms: u64,
        io_ms: u64,
        drain_ms: u64,
        shutdown_ms: u64,
    ) -> Result<Self> {
        let policy = Self {
            connect_ms,
            recv_poll_ms,
            io_ms,
            drain_ms,
            shutdown_ms,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<()> {
        Self::check_nonzero(self.connect_ms, "connect_ms")?;
        Self::check_nonzero(self.recv_poll_ms, "recv_poll_ms")?;
        Self::check_nonzero(self.io_ms, "io_ms")?;
        Self::check_nonzero(self.drain_ms, "drain_ms")?;
        Self::check_nonzero(self.shutdown_ms, "shutdown_ms")?;
        Self::check_order(self.connect_ms, self.io_ms, "connect_ms", "io_ms")?;
        Self::check_order(self.drain_ms, self.shutdown_ms, "drain_ms", "shutdown_ms")?;
        Ok(())
    }

    fn check_nonzero(value: u64, name: &str) -> Result<()> {
        if value == 0 {
            return Err(Error::Config(format!("{name} must be > 0")));
        }
        Ok(())
    }

    fn check_order(left: u64, right: u64, left_name: &str, right_name: &str) -> Result<()> {
        if left > right {
            return Err(Error::Config(format!(
                "{left_name} ({left}) cannot exceed {right_name} ({right})"
            )));
        }
        Ok(())
    }

    pub fn connect(&self) -> Duration {
        Duration::from_millis(self.connect_ms)
    }

    pub fn recv_poll(&self) -> Duration {
        Duration::from_millis(self.recv_poll_ms)
    }

    pub fn io(&self) -> Duration {
        Duration::from_millis(self.io_ms)
    }

    pub fn drain(&self) -> Duration {
        Duration::from_millis(self.drain_ms)
    }

    pub fn shutdown(&self) -> Duration {
        Duration::from_millis(self.shutdown_ms)
    }
}

impl Default for DeadlinePolicy {
    fn default() -> Self {
        Self {
            connect_ms: Self::DEFAULT_CONNECT_MS,
            recv_poll_ms: Self::DEFAULT_RECV_POLL_MS,
            io_ms: Self::DEFAULT_IO_MS,
            drain_ms: Self::DEFAULT_DRAIN_MS,
            shutdown_ms: Self::DEFAULT_SHUTDOWN_MS,
        }
    }
}

/// Trusted process-wide policy configured at bootstrap.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProcessRuntimePolicy {
    /// Hard resource limits that graphs may only tighten.
    #[serde(flatten)]
    pub resource: ResourcePolicy,
    /// Host-side memory pool cache configuration.
    pub memory_pool: MemoryPoolConfig,
    /// Stream/subscriber/bootstrap cache limits.
    pub stream_registry: StreamRegistryLimits,
    /// Connector, drain and shutdown deadlines.
    pub deadlines: DeadlinePolicy,
    /// Maximum affinity entries retained by the scheduler.
    pub affinity_capacity: usize,
    /// Affinity entry TTL, in seconds.
    pub affinity_ttl_seconds: u64,
    /// Maximum bytes emitted in a single metrics serialization response.
    pub metrics_serialization_bytes: usize,
}

impl ProcessRuntimePolicy {
    pub const DEFAULT_AFFINITY_CAPACITY: usize = 1_000;
    pub const DEFAULT_AFFINITY_TTL_SECONDS: u64 = 600;
    pub const DEFAULT_METRICS_SERIALIZATION_BYTES: usize = 8 * 1024 * 1024;

    pub fn new(
        resource: ResourcePolicy,
        memory_pool: MemoryPoolConfig,
        stream_registry: StreamRegistryLimits,
        deadlines: DeadlinePolicy,
        affinity_capacity: usize,
        affinity_ttl_seconds: u64,
        metrics_serialization_bytes: usize,
    ) -> Result<Self> {
        let policy = Self {
            resource,
            memory_pool,
            stream_registry,
            deadlines,
            affinity_capacity,
            affinity_ttl_seconds,
            metrics_serialization_bytes,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<()> {
        Self::check_nonzero(self.affinity_capacity, "affinity_capacity")?;
        Self::check_nonzero(self.affinity_ttl_seconds as usize, "affinity_ttl_seconds")?;
        Self::check_nonzero(
            self.metrics_serialization_bytes,
            "metrics_serialization_bytes",
        )?;
        Self::check_representable(self.affinity_capacity, "affinity_capacity")?;
        Self::check_representable(self.affinity_ttl_seconds as usize, "affinity_ttl_seconds")?;
        Self::check_representable(
            self.metrics_serialization_bytes,
            "metrics_serialization_bytes",
        )?;
        Ok(())
    }

    fn check_nonzero(value: usize, name: &str) -> Result<()> {
        if value == 0 {
            return Err(Error::Config(format!("{name} must be > 0")));
        }
        Ok(())
    }

    fn check_representable(value: usize, name: &str) -> Result<()> {
        if value > u32::MAX as usize {
            return Err(Error::Config(format!(
                "{name} {value} exceeds 32-bit representable maximum {}",
                u32::MAX
            )));
        }
        Ok(())
    }

    pub fn resource_policy(&self) -> &ResourcePolicy {
        &self.resource
    }

    pub fn memory_pool(&self) -> &MemoryPoolConfig {
        &self.memory_pool
    }

    pub fn stream_registry(&self) -> &StreamRegistryLimits {
        &self.stream_registry
    }

    pub fn deadlines(&self) -> &DeadlinePolicy {
        &self.deadlines
    }

    pub fn affinity_capacity(&self) -> usize {
        self.affinity_capacity
    }

    pub fn affinity_ttl(&self) -> Duration {
        Duration::from_secs(self.affinity_ttl_seconds)
    }

    pub fn metrics_serialization_bytes(&self) -> usize {
        self.metrics_serialization_bytes
    }
}

impl Default for ProcessRuntimePolicy {
    fn default() -> Self {
        Self {
            resource: ResourcePolicy::default(),
            memory_pool: MemoryPoolConfig::default(),
            stream_registry: StreamRegistryLimits::default(),
            deadlines: DeadlinePolicy::default(),
            affinity_capacity: Self::DEFAULT_AFFINITY_CAPACITY,
            affinity_ttl_seconds: Self::DEFAULT_AFFINITY_TTL_SECONDS,
            metrics_serialization_bytes: Self::DEFAULT_METRICS_SERIALIZATION_BYTES,
        }
    }
}
