//! CORE7-02 ProcessRuntimePolicy validation tests.

use dg_core::{
    DeadlinePolicy, MemoryPoolConfig, ProcessRuntimePolicy, ResourcePolicy, StreamRegistryLimits,
};

#[test]
fn process_policy_default_is_valid() {
    let policy = ProcessRuntimePolicy::default();
    assert_eq!(policy.resource_policy(), &ResourcePolicy::default());
    assert_eq!(policy.memory_pool(), &MemoryPoolConfig::default());
    assert_eq!(policy.stream_registry(), &StreamRegistryLimits::default());
    assert_eq!(policy.deadlines(), &DeadlinePolicy::default());
}

#[test]
fn process_policy_rejects_zero_affinity_capacity() {
    let default = ProcessRuntimePolicy::default();
    assert!(ProcessRuntimePolicy::new(
        default.resource,
        default.memory_pool,
        default.stream_registry,
        default.deadlines,
        0,
        default.affinity_ttl_seconds,
        default.metrics_serialization_bytes,
    )
    .is_err());
}

#[test]
fn process_policy_rejects_zero_deadline() {
    let deadlines = DeadlinePolicy {
        connect_ms: 0,
        ..Default::default()
    };
    assert!(DeadlinePolicy::new(
        deadlines.connect_ms,
        deadlines.recv_poll_ms,
        deadlines.io_ms,
        deadlines.drain_ms,
        deadlines.shutdown_ms,
    )
    .is_err());
}

#[test]
fn process_policy_rejects_invalid_deadline_order() {
    assert!(DeadlinePolicy::new(
        DeadlinePolicy::DEFAULT_CONNECT_MS,
        DeadlinePolicy::DEFAULT_RECV_POLL_MS,
        DeadlinePolicy::DEFAULT_IO_MS,
        DeadlinePolicy::DEFAULT_SHUTDOWN_MS + 1,
        DeadlinePolicy::DEFAULT_SHUTDOWN_MS,
    )
    .is_err());
}

#[test]
fn process_policy_accepts_u64_ttl_above_u32_max() {
    let default = ProcessRuntimePolicy::default();
    assert!(ProcessRuntimePolicy::new(
        default.resource,
        default.memory_pool,
        default.stream_registry,
        default.deadlines,
        default.affinity_capacity,
        u64::from(u32::MAX) + 1,
        default.metrics_serialization_bytes,
    )
    .is_ok());
}

#[test]
fn process_policy_rejects_zero_affinity_ttl() {
    let default = ProcessRuntimePolicy::default();
    assert!(ProcessRuntimePolicy::new(
        default.resource,
        default.memory_pool,
        default.stream_registry,
        default.deadlines,
        default.affinity_capacity,
        0,
        default.metrics_serialization_bytes,
    )
    .is_err());
}

#[test]
fn process_policy_rejects_invalid_nested_memory_pool() {
    let default = ProcessRuntimePolicy::default();
    let invalid_pool = MemoryPoolConfig {
        max_cached_bytes: 0,
        ..default.memory_pool
    };
    let policy = ProcessRuntimePolicy {
        memory_pool: invalid_pool,
        ..default
    };
    assert!(policy.validate().is_err());
}

#[test]
fn stream_registry_accepts_u64_idle_ttl_above_u32_max() {
    assert!(StreamRegistryLimits::new(
        StreamRegistryLimits::DEFAULT_MAX_STREAMS,
        StreamRegistryLimits::DEFAULT_MAX_SUBSCRIBERS_PER_STREAM,
        StreamRegistryLimits::DEFAULT_MAX_BOOTSTRAP_BYTES,
        StreamRegistryLimits::DEFAULT_MAX_BOOTSTRAP_FRAMES,
        u64::from(u32::MAX) + 1,
    )
    .is_ok());
}

#[test]
fn stream_registry_rejects_zero_idle_ttl() {
    assert!(StreamRegistryLimits::new(
        StreamRegistryLimits::DEFAULT_MAX_STREAMS,
        StreamRegistryLimits::DEFAULT_MAX_SUBSCRIBERS_PER_STREAM,
        StreamRegistryLimits::DEFAULT_MAX_BOOTSTRAP_BYTES,
        StreamRegistryLimits::DEFAULT_MAX_BOOTSTRAP_FRAMES,
        0,
    )
    .is_err());
}
