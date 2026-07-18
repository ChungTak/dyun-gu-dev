//! Process-level resource policy and effective limit computation.
//!
//! `ResourcePolicy` is created by a trusted bootstrap path and is immutable for
//! the lifetime of a process.  Graphs and runtimes can only tighten these hard
//! limits; requesting a larger value fails with the field, requested value and
//! hard limit in the error.

use std::cmp;

use crate::{Error, Result};

/// Process-level hard limits for configuration, model, tensor and frame
/// resources.
///
/// The default values mirror the published `dg/v1` GraphSpec defaults:
///
/// * `max_config_bytes` - 8 MiB
/// * `max_include_depth` - 16
/// * `max_include_count` - 64
/// * `max_nodes` - 1024
/// * `max_connections` - 8192
/// * `max_tensor_bytes` - 512 MiB
/// * `max_frame_bytes` - 512 MiB
/// * `max_model_bytes` - 2 GiB
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePolicy {
    pub max_config_bytes: usize,
    pub max_include_depth: usize,
    pub max_include_count: usize,
    pub max_nodes: usize,
    pub max_connections: usize,
    pub max_tensor_bytes: usize,
    pub max_frame_bytes: usize,
    pub max_model_bytes: usize,
    /// Maximum number of packets held by input queues, sinks and report collectors.
    pub max_buffer_packets: usize,
    /// Maximum bytes held by input queues, sinks and report collectors.
    pub max_buffer_bytes: usize,
}

impl ResourcePolicy {
    pub const DEFAULT_MAX_CONFIG_BYTES: usize = 8 * 1024 * 1024;
    pub const DEFAULT_MAX_INCLUDE_DEPTH: usize = 16;
    pub const DEFAULT_MAX_INCLUDE_COUNT: usize = 64;
    pub const DEFAULT_MAX_NODES: usize = 1024;
    pub const DEFAULT_MAX_CONNECTIONS: usize = 8192;
    pub const DEFAULT_MAX_TENSOR_BYTES: usize = 512 * 1024 * 1024;
    pub const DEFAULT_MAX_FRAME_BYTES: usize = 512 * 1024 * 1024;
    pub const DEFAULT_MAX_MODEL_BYTES: usize = 2 * 1024 * 1024 * 1024;
    pub const DEFAULT_MAX_BUFFER_PACKETS: usize = 10_000;
    pub const DEFAULT_MAX_BUFFER_BYTES: usize = 1024 * 1024 * 1024;

    /// Creates a new policy after validating that all limits are non-zero,
    /// representable on the current platform and satisfy their relationships.
    pub fn new(limits: ResourcePolicy) -> Result<Self> {
        Self::validate(&limits)?;
        Ok(limits)
    }

    /// Returns a policy where every limit is the minimum of the hard limit and
    /// the graph-requested limit.
    ///
    /// If any requested limit is larger than the hard limit, this returns an
    /// error identifying the field, requested value and hard limit.
    pub fn effective_for(&self, requested: &ResourcePolicy) -> Result<Self> {
        let effective = ResourcePolicy {
            max_config_bytes: cmp::min(self.max_config_bytes, requested.max_config_bytes),
            max_include_depth: cmp::min(self.max_include_depth, requested.max_include_depth),
            max_include_count: cmp::min(self.max_include_count, requested.max_include_count),
            max_nodes: cmp::min(self.max_nodes, requested.max_nodes),
            max_connections: cmp::min(self.max_connections, requested.max_connections),
            max_tensor_bytes: cmp::min(self.max_tensor_bytes, requested.max_tensor_bytes),
            max_frame_bytes: cmp::min(self.max_frame_bytes, requested.max_frame_bytes),
            max_model_bytes: cmp::min(self.max_model_bytes, requested.max_model_bytes),
            max_buffer_packets: cmp::min(self.max_buffer_packets, requested.max_buffer_packets),
            max_buffer_bytes: cmp::min(self.max_buffer_bytes, requested.max_buffer_bytes),
        };

        let mut exceeded = Vec::new();
        Self::check_exceeded(
            &mut exceeded,
            "max_config_bytes",
            requested.max_config_bytes,
            self.max_config_bytes,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_include_depth",
            requested.max_include_depth,
            self.max_include_depth,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_include_count",
            requested.max_include_count,
            self.max_include_count,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_nodes",
            requested.max_nodes,
            self.max_nodes,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_connections",
            requested.max_connections,
            self.max_connections,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_tensor_bytes",
            requested.max_tensor_bytes,
            self.max_tensor_bytes,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_frame_bytes",
            requested.max_frame_bytes,
            self.max_frame_bytes,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_model_bytes",
            requested.max_model_bytes,
            self.max_model_bytes,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_buffer_packets",
            requested.max_buffer_packets,
            self.max_buffer_packets,
        );
        Self::check_exceeded(
            &mut exceeded,
            "max_buffer_bytes",
            requested.max_buffer_bytes,
            self.max_buffer_bytes,
        );

        if !exceeded.is_empty() {
            return Err(Error::Config(exceeded.join("; ")));
        }

        Self::validate(&effective)?;
        Ok(effective)
    }

    fn check_exceeded(out: &mut Vec<String>, name: &str, requested: usize, hard: usize) {
        if requested > hard {
            out.push(format!(
                "{name} requested {requested} exceeds hard limit {hard}"
            ));
        }
    }

    fn validate(policy: &ResourcePolicy) -> Result<()> {
        Self::check_nonzero(policy.max_config_bytes, "max_config_bytes")?;
        Self::check_nonzero(policy.max_include_depth, "max_include_depth")?;
        Self::check_nonzero(policy.max_include_count, "max_include_count")?;
        Self::check_nonzero(policy.max_nodes, "max_nodes")?;
        Self::check_nonzero(policy.max_connections, "max_connections")?;
        Self::check_nonzero(policy.max_tensor_bytes, "max_tensor_bytes")?;
        Self::check_nonzero(policy.max_frame_bytes, "max_frame_bytes")?;
        Self::check_nonzero(policy.max_model_bytes, "max_model_bytes")?;
        Self::check_representable(policy.max_config_bytes, "max_config_bytes")?;
        Self::check_representable(policy.max_include_depth, "max_include_depth")?;
        Self::check_representable(policy.max_include_count, "max_include_count")?;
        Self::check_representable(policy.max_nodes, "max_nodes")?;
        Self::check_representable(policy.max_connections, "max_connections")?;
        Self::check_representable(policy.max_tensor_bytes, "max_tensor_bytes")?;
        Self::check_representable(policy.max_frame_bytes, "max_frame_bytes")?;
        Self::check_representable(policy.max_model_bytes, "max_model_bytes")?;
        Self::check_nonzero(policy.max_buffer_packets, "max_buffer_packets")?;
        Self::check_representable(policy.max_buffer_packets, "max_buffer_packets")?;
        Self::check_nonzero(policy.max_buffer_bytes, "max_buffer_bytes")?;
        Self::check_representable(policy.max_buffer_bytes, "max_buffer_bytes")?;
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

    pub fn check_config_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_config_bytes {
            return Err(Error::Config(format!(
                "config bytes {bytes} exceeds limit {}",
                self.max_config_bytes
            )));
        }
        Ok(())
    }

    pub fn check_include_depth(&self, depth: usize) -> Result<()> {
        if depth > self.max_include_depth {
            return Err(Error::Config(format!(
                "include depth {depth} exceeds limit {}",
                self.max_include_depth
            )));
        }
        Ok(())
    }

    pub fn check_include_count(&self, count: usize) -> Result<()> {
        if count > self.max_include_count {
            return Err(Error::Config(format!(
                "include count {count} exceeds limit {}",
                self.max_include_count
            )));
        }
        Ok(())
    }

    pub fn check_nodes(&self, nodes: usize) -> Result<()> {
        if nodes > self.max_nodes {
            return Err(Error::Config(format!(
                "node count {nodes} exceeds limit {}",
                self.max_nodes
            )));
        }
        Ok(())
    }

    pub fn check_connections(&self, connections: usize) -> Result<()> {
        if connections > self.max_connections {
            return Err(Error::Config(format!(
                "connection count {connections} exceeds limit {}",
                self.max_connections
            )));
        }
        Ok(())
    }

    pub fn check_tensor_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_tensor_bytes {
            return Err(Error::Config(format!(
                "tensor bytes {bytes} exceeds limit {}",
                self.max_tensor_bytes
            )));
        }
        Ok(())
    }

    pub fn check_frame_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_frame_bytes {
            return Err(Error::Config(format!(
                "frame bytes {bytes} exceeds limit {}",
                self.max_frame_bytes
            )));
        }
        Ok(())
    }

    pub fn check_model_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_model_bytes {
            return Err(Error::Config(format!(
                "model bytes {bytes} exceeds limit {}",
                self.max_model_bytes
            )));
        }
        Ok(())
    }

    pub fn check_buffer_packets(&self, packets: usize) -> Result<()> {
        if packets > self.max_buffer_packets {
            return Err(Error::Config(format!(
                "buffer packet count {packets} exceeds limit {}",
                self.max_buffer_packets
            )));
        }
        Ok(())
    }

    pub fn check_buffer_bytes(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_buffer_bytes {
            return Err(Error::Config(format!(
                "buffer bytes {bytes} exceeds limit {}",
                self.max_buffer_bytes
            )));
        }
        Ok(())
    }
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_config_bytes: Self::DEFAULT_MAX_CONFIG_BYTES,
            max_include_depth: Self::DEFAULT_MAX_INCLUDE_DEPTH,
            max_include_count: Self::DEFAULT_MAX_INCLUDE_COUNT,
            max_nodes: Self::DEFAULT_MAX_NODES,
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            max_tensor_bytes: Self::DEFAULT_MAX_TENSOR_BYTES,
            max_frame_bytes: Self::DEFAULT_MAX_FRAME_BYTES,
            max_model_bytes: Self::DEFAULT_MAX_MODEL_BYTES,
            max_buffer_packets: Self::DEFAULT_MAX_BUFFER_PACKETS,
            max_buffer_bytes: Self::DEFAULT_MAX_BUFFER_BYTES,
        }
    }
}
