use std::ffi::CString;
use std::os::fd::{FromRawFd, IntoRawFd};
use std::ptr;
use std::sync::{Arc, Barrier};
use std::thread;

use dg_capi::{
    dg_buffer_import_external, dg_engine_build, dg_engine_create, dg_engine_destroy,
    dg_engine_init, dg_engine_load_string, dg_engine_metrics, dg_engine_status,
    dg_owned_bytes_free, dg_tensor_create, DgEngine, DgExternalMemoryV2, DgGraphFormat,
    DgGraphStatus, DgOwnedBytes, DgStatus, DgTensor,
};

const BASE_SPEC: &str = r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1000
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

unsafe fn create_engine() -> *mut DgEngine {
    let mut engine = ptr::null_mut();
    assert_eq!(dg_engine_create(&mut engine, ptr::null_mut()), DgStatus::Ok);
    assert!(!engine.is_null());
    engine
}

unsafe fn load_and_build(engine: *mut DgEngine) {
    let spec = CString::new(BASE_SPEC).unwrap();
    assert_eq!(
        dg_engine_load_string(
            engine,
            DgGraphFormat::Yaml as i32,
            spec.as_ptr(),
            ptr::null_mut(),
        ),
        DgStatus::Ok
    );
    assert_eq!(dg_engine_build(engine, ptr::null_mut()), DgStatus::Ok);
    assert_eq!(dg_engine_init(engine, ptr::null_mut()), DgStatus::Ok);
}

#[test]
fn concurrent_create_and_destroy_isolated() {
    let threads: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..10 {
                    let engine = unsafe { create_engine() };
                    unsafe {
                        assert_eq!(dg_engine_destroy(engine, 0, ptr::null_mut()), DgStatus::Ok);
                    }
                }
            })
        })
        .collect();
    for handle in threads {
        handle.join().unwrap();
    }
}

#[test]
fn concurrent_status_and_metrics_do_not_deadlock() {
    let engine = unsafe { create_engine() };
    unsafe { load_and_build(engine) };

    let engine_addr = engine as usize;
    let barrier = Arc::new(Barrier::new(5));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let engine = engine_addr as *mut DgEngine;
                barrier.wait();
                for _ in 0..50 {
                    let mut status = DgGraphStatus::NotRunning;
                    let status_result = unsafe {
                        dg_engine_status(engine, &mut status, ptr::null_mut(), ptr::null_mut())
                    };
                    assert_eq!(status_result, DgStatus::Ok);

                    let mut owned: *mut DgOwnedBytes = ptr::null_mut();
                    let metrics_result =
                        unsafe { dg_engine_metrics(engine, &mut owned, ptr::null_mut()) };
                    assert_eq!(metrics_result, DgStatus::Ok);
                    if !owned.is_null() {
                        unsafe { dg_owned_bytes_free(owned) };
                    }
                }
            })
        })
        .collect();

    barrier.wait();

    for handle in threads {
        handle.join().unwrap();
    }

    unsafe {
        let mut status = DgGraphStatus::NotRunning;
        let _ = dg_engine_status(engine, &mut status, ptr::null_mut(), ptr::null_mut());
        assert_eq!(
            dg_engine_destroy(engine, 5_000, ptr::null_mut()),
            DgStatus::Ok
        );
    }
}

#[test]
fn invalid_external_fd_is_rejected_safely() {
    // Open a file, take its fd, close it, then pass the now-stale fd to the C API.
    let file = std::fs::File::open("/dev/null").expect("open /dev/null");
    let stale_fd = file.into_raw_fd();
    // Close the fd so it is no longer valid.
    let _ = unsafe { std::fs::File::from_raw_fd(stale_fd) };

    let desc = DgExternalMemoryV2 {
        struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
        struct_version: 0,
        fd: stale_fd,
        raw: 0,
        domain: 0, // Host
        device: 0, // Cpu
        size_bytes: 1,
        release: None,
        user_data: ptr::null_mut(),
    };
    let mut buffer = ptr::null_mut();
    unsafe {
        assert_eq!(
            dg_buffer_import_external(&desc, &mut buffer, ptr::null_mut()),
            DgStatus::InvalidArgument
        );
    }
    assert!(buffer.is_null());
}

#[test]
fn tensor_create_invalid_dtype_zeroes_output_pointer() {
    let data = [1u8];
    let shape = [1usize];
    let out_ptr: *mut *mut DgTensor =
        Box::into_raw(Box::new(std::ptr::dangling_mut::<DgTensor>()));
    unsafe {
        assert_eq!(
            dg_tensor_create(
                data.as_ptr(),
                data.len(),
                shape.as_ptr(),
                shape.len(),
                -1,
                0,
                0,
                out_ptr,
                ptr::null_mut(),
            ),
            DgStatus::InvalidArgument
        );
        assert!((*out_ptr).is_null());
        let _ = Box::from_raw(out_ptr);
    }
}
