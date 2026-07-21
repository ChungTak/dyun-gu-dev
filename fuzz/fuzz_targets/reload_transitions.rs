#![no_main]

use std::ffi::CString;

use dg_capi::{
    dg_engine_build, dg_engine_create, dg_engine_destroy, dg_engine_init, dg_engine_load_string,
    dg_engine_reload_string, dg_engine_stop, DgEngine, DgGraphFormat, DgStatus, DgStringView,
};
use libfuzzer_sys::fuzz_target;

const BASE_SPEC: &str = r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1000000
      shape: [1, 4]
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#;

fuzz_target!(|data: &[u8]| {
    let content: Vec<u8> = data.iter().copied().take_while(|b| *b != 0).collect();
    let Ok(reload) = CString::new(content) else {
        return;
    };

    let mut engine: *mut DgEngine = std::ptr::null_mut();
    if unsafe { dg_engine_create(&mut engine, std::ptr::null_mut()) } != dg_capi::DgStatus::Ok {
        return;
    }

    let base = CString::new(BASE_SPEC).unwrap();
    let _ = unsafe {
        dg_engine_load_string(engine, DgGraphFormat::Yaml as i32, DgStringView { data: base.as_ptr(), len: base.to_bytes().len() },
            std::ptr::null_mut(),
        )
    };
    let _ = unsafe { dg_engine_build(engine, std::ptr::null_mut()) };
    let _ = unsafe { dg_engine_init(engine, std::ptr::null_mut()) };

    for format in [
        DgGraphFormat::Yaml as i32,
        DgGraphFormat::Json as i32,
        DgGraphFormat::Toml as i32,
    ] {
        let _ = unsafe {
            dg_engine_reload_string(engine, format, DgStringView { data: reload.as_ptr(), len: reload.to_bytes().len() }, std::ptr::null_mut())
        };
    }

    // Cooperative stop followed by a finite destroy deadline with retry on Busy.
    // This prevents leaked workers/engines across fuzz iterations.
    let _ = unsafe { dg_engine_stop(engine, std::ptr::null_mut()) };
    std::thread::sleep(std::time::Duration::from_millis(10));
    for _ in 0..4 {
        let status = unsafe { dg_engine_destroy(engine, 5000, std::ptr::null_mut()) };
        if status == DgStatus::Busy {
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        break;
    }
});
