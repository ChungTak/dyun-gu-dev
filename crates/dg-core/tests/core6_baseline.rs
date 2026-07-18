//! CORE6-01/CORE6-03 基线回归测试（dg-core）。
//!
//! 这些测试验证 `Buffer`、`Tensor`、`Strides` 在外部内存与 stride 物理跨度上的
//! 安全行为。

use dg_core::{
    Buffer, BufferDesc, DataFormat, DataType, DeviceKind, ExternalDropGuard, ExternalHandle,
    MemoryDomain, Shape, Strides, Tensor, TensorDesc,
};

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
