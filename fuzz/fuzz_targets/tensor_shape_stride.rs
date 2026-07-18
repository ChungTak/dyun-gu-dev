#![no_main]

use dg_core::{DataFormat, DataType, DeviceKind, Shape, TensorDesc, TypeCode};
use libfuzzer_sys::fuzz_target;

fn take_u64(bytes: &mut &[u8]) -> Option<u64> {
    if bytes.len() < 8 {
        return None;
    }
    let (head, tail) = bytes.split_at(8);
    *bytes = tail;
    Some(u64::from_le_bytes(head.try_into().unwrap()))
}

fn take_u8(bytes: &mut &[u8]) -> Option<u8> {
    if bytes.is_empty() {
        return None;
    }
    let v = bytes[0];
    *bytes = &bytes[1..];
    Some(v)
}

fn type_code(code: u8) -> TypeCode {
    match code % 7 {
        0 => TypeCode::Uint,
        1 => TypeCode::Int,
        2 => TypeCode::Float,
        3 => TypeCode::Bfloat,
        4 => TypeCode::Float8,
        5 => TypeCode::Float4,
        _ => TypeCode::OpaqueHandle,
    }
}

fn data_format(fmt: u8) -> DataFormat {
    match fmt % 9 {
        0 => DataFormat::Auto,
        1 => DataFormat::N,
        2 => DataFormat::NC,
        3 => DataFormat::NCHW,
        4 => DataFormat::NHWC,
        5 => DataFormat::NC4HW,
        6 => DataFormat::NC8HW,
        7 => DataFormat::NCDHW,
        _ => DataFormat::OIHW,
    }
}

fuzz_target!(|data: &[u8]| {
    let mut input = data;

    let rank = match take_u8(&mut input) {
        Some(v) => (v % 8) as usize,
        None => return,
    };

    let mut dims = Vec::with_capacity(rank);
    for _ in 0..rank {
        if let Some(v) = take_u64(&mut input) {
            dims.push((v % 16 + 1) as usize);
        } else {
            break;
        }
    }
    if dims.len() != rank {
        return;
    }

    let bits = take_u8(&mut input).unwrap_or(8);
    let lanes = (take_u8(&mut input).unwrap_or(1) % 4).max(1);
    let code = take_u8(&mut input).unwrap_or(0);
    let format_code = take_u8(&mut input).unwrap_or(0);

    let dtype = DataType::new(type_code(code), bits, lanes);
    let format = data_format(format_code);
    let shape = Shape::new(dims);

    let desc = TensorDesc::new(shape.clone(), dtype, format, DeviceKind::Cpu);
    let _ = desc.storage_bytes();

    let mut stride_values = Vec::with_capacity(shape.rank());
    for _ in 0..shape.rank() {
        if let Some(v) = take_u64(&mut input) {
            stride_values.push((v % 256 + 1) as usize);
        } else {
            break;
        }
    }
    if stride_values.len() == shape.rank() {
        let strides = dg_core::Strides::new(stride_values);
        let _ = strides.is_contiguous_for(&shape);
        let explicit = desc.with_strides(strides.clone());
        let _ = explicit.storage_bytes();
    }

    if let Ok(contiguous) = shape.contiguous_strides() {
        let _ = contiguous.is_contiguous_for(&shape);
    }
});
