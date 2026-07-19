//! CORE6-01/CORE6-03 基线回归测试（dg-core）。
//!
//! 这些测试验证 `Buffer`、`Tensor`、`Strides` 在外部内存与 stride 物理跨度上的
//! 安全行为。

use dg_core::{
    Allocator, Buffer, BufferDesc, DataFormat, DataType, DeviceKind, ExternalDropGuard,
    ExternalHandle, MemoryDomain, MemoryPool, MemoryPoolConfig, ResourcePolicy, Shape, Strides,
    Tensor, TensorDesc,
};
use std::sync::Arc;

#[test]
fn external_only_buffer_read_bytes_is_not_silent_empty() {
    let guard = ExternalDropGuard::new(|| {});
    let buffer = Buffer::from_external(
        DeviceKind::Cpu,
        MemoryDomain::DmaBuf,
        BufferDesc::new(8, 1),
        ExternalHandle::from_raw(0),
        guard,
    )
    .expect("create external-only buffer");

    // read_bytes and try_read_bytes must fail explicitly instead of returning
    // an empty Vec that could be fed into a backend as valid input.
    assert!(buffer.read_bytes().is_err());
    assert!(buffer.try_read_bytes().is_err());
    assert!(buffer.try_into_host_bytes().is_err());
}

#[test]
fn tensor_from_buffer_accepts_physical_stride_span() {
    // Shape [2, 1000] with row-major contiguous strides uses 2000 f32 elements
    // (8000 bytes). With strides [2000, 1] the physical span is
    // (2-1)*2000 + (1000-1)*1 + 1 = 3000 elements, i.e. 12000 bytes.
    let desc = TensorDesc::new(
        Shape::new([2, 1000]),
        DataType::F32,
        DataFormat::NCHW,
        DeviceKind::Cpu,
    )
    .with_strides(Strides::new([2000, 1]));

    let buffer = Buffer::allocate_host(DeviceKind::Cpu, 12000).expect("allocate 12000 bytes");
    Tensor::from_buffer(desc, buffer)
        .expect("physical stride span (12000 bytes) must be accepted over logical bytes (8000)");
}

#[test]
fn resource_policy_frame_bytes_boundary() {
    let policy = ResourcePolicy {
        max_frame_bytes: 8,
        ..ResourcePolicy::default()
    };
    policy.check_frame_bytes(7).expect("limit-1");
    policy.check_frame_bytes(8).expect("limit");
    assert!(policy.check_frame_bytes(9).is_err(), "limit+1");
}

#[test]
fn tensor_allocate_with_policy_rejects_before_allocation() {
    let policy = ResourcePolicy {
        max_tensor_bytes: 16,
        ..ResourcePolicy::default()
    };
    let device = dg_core::CpuDevice::new();
    let ok_desc = TensorDesc::new(
        Shape::new([4]),
        DataType::U8,
        DataFormat::Auto,
        DeviceKind::Cpu,
    );
    Tensor::allocate_with_policy(&device, ok_desc, &policy).expect("16 bytes at limit");

    let over = TensorDesc::new(
        Shape::new([17]),
        DataType::U8,
        DataFormat::Auto,
        DeviceKind::Cpu,
    );
    assert!(
        Tensor::allocate_with_policy(&device, over, &policy).is_err(),
        "limit+1 must fail before allocate"
    );
}

#[test]
fn memory_pool_cache_capacity_is_bounded() {
    let config = MemoryPoolConfig::new(128, 3, 2).expect("config");
    let pool = MemoryPool::with_config(Arc::new(dg_core::CpuAllocator), config);
    for size in [16usize, 32, 48, 64] {
        let buffer = pool
            .allocate(dg_core::BufferDesc::new(size, 1))
            .expect("allocate");
        pool.deallocate(buffer).expect("return");
    }
    assert!(pool.cached_buffer_count() <= 3);
    assert!(pool.cached_bytes() <= 128);
    let metrics = pool.metrics_snapshot();
    assert!(metrics.evictions >= 1 || metrics.rejected_returns >= 1);
}
