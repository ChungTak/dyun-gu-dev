#![no_main]

use std::ffi::CString;

use dg_capi::{
    dg_engine_create, dg_engine_destroy, dg_engine_load_string, DgEngine, DgGraphFormat,
    DgStringView,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = data
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    let Ok(content) = CString::new(input) else {
        return;
    };
    let mut engine: *mut DgEngine = std::ptr::null_mut();
    // SAFETY: the output pointer is valid local storage; the returned handle is
    // released exactly once below when creation succeeds.
    let status = unsafe { dg_engine_create(&mut engine, std::ptr::null_mut()) };
    if status != dg_capi::DgStatus::Ok {
        return;
    }
    for format in [
        DgGraphFormat::Yaml as i32,
        DgGraphFormat::Json as i32,
        DgGraphFormat::Toml as i32,
    ] {
        // SAFETY: `engine` is a handle returned by `dg_engine_create`, and
        // `content` is a valid NUL-terminated C string.
        let _ = unsafe { dg_engine_load_string(engine, format, DgStringView { data: content.as_ptr(), len: content.to_bytes().len() }, std::ptr::null_mut()) };
    }
    // SAFETY: `engine` was returned by `dg_engine_create` and has not been freed.
    unsafe { dg_engine_destroy(engine, 0, std::ptr::null_mut()) };
});
