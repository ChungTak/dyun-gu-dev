//! CORE6-01 基线失败测试（dg-core）。
//!
//! 这些测试在 CORE6-03/04/08 修复前被标记为 `#[ignore]`；它们验证当前
//! `Buffer`、`Tensor`、`Strides` 在资源/stride/外部内存路径上的已知缺陷。
//! 修复对应风险后应取消 ignore 并改为通过。

use dg_core::{
    Buffer, BufferDesc, DataFormat, DataType, DeviceKind, ExternalDropGuard, ExternalHandle,
    MemoryDomain, Shape, Strides, Tensor, TensorDesc,
};

#[test]
#[ignore = "R6-009: CORE6-03 will make external-only buffer reads fallible instead of silent empty"]
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

    // The current implementation returns an empty Vec for device-only external
    // memory because it cannot dereference the handle. Product code must error
    // explicitly instead of feeding empty inputs to backends.
    assert!(
        !buffer.read_bytes().is_empty(),
        "read_bytes must not silently return empty for external-only buffers"
    );
}

#[test]
#[ignore = "R6-010: CORE6-03 will account for physical stride span in storage_bytes"]
fn tensor_from_buffer_accepts_physical_stride_span() {
    // Shape [2, 1000] with row-major contiguous strides uses 2000 f32 elements
    // (8000 bytes). With strides [2000, 1] the physical span is
    // max((2-1)*2000, (1000-1)*1) + 1 = 2001 elements, i.e. 8004 bytes.
    // TensorDesc::storage_bytes currently returns only logical bytes (8000),
    // so a correctly-sized 8004-byte buffer is rejected.
    let desc = TensorDesc::new(
        Shape::new([2, 1000]),
        DataType::F32,
        DataFormat::NCHW,
        DeviceKind::Cpu,
    )
    .with_strides(Strides::new([2000, 1]));

    let buffer = Buffer::allocate_host(DeviceKind::Cpu, 8004);
    Tensor::from_buffer(desc, buffer)
        .expect("physical stride span (8004 bytes) must be accepted over logical bytes (8000)");
}
