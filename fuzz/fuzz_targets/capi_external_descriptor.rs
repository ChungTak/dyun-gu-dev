#![no_main]

use std::os::raw::{c_int, c_void};

use dg_capi::{
    dg_buffer_free, dg_buffer_import_external, dg_buffer_size, dg_tensor_create_external,
    dg_tensor_free, DgBuffer, DgDataFormat, DgDataType, DgDeviceKind, DgExternalMemoryV2,
    DgMemoryDomain, DgReleaseCallback, DgShapeView, DgTensor,
};
use libfuzzer_sys::fuzz_target;

extern "C" fn noop_release(_: *mut c_void) {}

fn take_u64(bytes: &mut &[u8]) -> Option<u64> {
    if bytes.len() < 8 {
        return None;
    }
    let (head, tail) = bytes.split_at(8);
    *bytes = tail;
    Some(u64::from_le_bytes(head.try_into().unwrap()))
}

fn take_i32(bytes: &mut &[u8]) -> Option<i32> {
    if bytes.len() < 4 {
        return None;
    }
    let (head, tail) = bytes.split_at(4);
    *bytes = tail;
    Some(i32::from_le_bytes(head.try_into().unwrap()))
}

fn take_i64(bytes: &mut &[u8]) -> Option<i64> {
    if bytes.len() < 8 {
        return None;
    }
    let (head, tail) = bytes.split_at(8);
    *bytes = tail;
    Some(i64::from_le_bytes(head.try_into().unwrap()))
}

fuzz_target!(|data: &[u8]| {
    let mut input = data;

    let fd = take_i64(&mut input).unwrap_or(-1) as c_int;
    let raw = take_u64(&mut input).unwrap_or(0);
    let domain = take_i32(&mut input).unwrap_or(DgMemoryDomain::Host as i32);
    let device = take_i32(&mut input).unwrap_or(DgDeviceKind::Cpu as i32);
    let size_bytes = take_u64(&mut input).unwrap_or(0) as usize;

    // If raw is requested, use a fixed no-op release callback so the fuzzer
    // never passes an arbitrary function pointer across the FFI boundary.
    let release: DgReleaseCallback = if raw != 0 {
        Some(noop_release)
    } else {
        None
    };

    let desc = DgExternalMemoryV2 {
        struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
        struct_version: 0,
        fd,
        raw,
        domain,
        device,
        size_bytes,
        release,
        user_data: std::ptr::null_mut(),
    };

    let mut buffer: *mut DgBuffer = std::ptr::null_mut();
    let status = unsafe { dg_buffer_import_external(&desc, &mut buffer, std::ptr::null_mut()) };
    if status == dg_capi::DgStatus::Ok && !buffer.is_null() {
        let mut size = 0usize;
        let _ = unsafe { dg_buffer_size(buffer, &mut size, std::ptr::null_mut()) };
        unsafe { dg_buffer_free(buffer) };
    }

    // Try to create a tensor backed by the same descriptor. Pick a small rank
    // and dimensions from the remaining bytes; the size is unlikely to match,
    // which exercises the size-mismatch path.
    let rank = input.first().copied().unwrap_or(0) as usize % 8;
    let mut input = &input[1.min(input.len())..];
    let mut shape = [0usize; 8];
    for dim in shape.iter_mut().take(rank) {
        if let Some(v) = take_u64(&mut input) {
            *dim = (v % 16 + 1) as usize;
        } else {
            break;
        }
    }
    let dtype = take_i32(&mut input).unwrap_or(DgDataType::U8 as i32);
    let format = take_i32(&mut input).unwrap_or(DgDataFormat::Nc as i32);

    let mut tensor: *mut DgTensor = std::ptr::null_mut();
    let status = unsafe {
        dg_tensor_create_external(
                    &desc,
                    DgShapeView { dims: shape.as_ptr(), rank: rank },
                    dtype,
                    format,
                    &mut tensor,
                    std::ptr::null_mut(),
        )
    };
    if status == dg_capi::DgStatus::Ok && !tensor.is_null() {
        unsafe { dg_tensor_free(tensor) };
    }
});
