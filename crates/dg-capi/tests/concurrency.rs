use std::ffi::{c_void, CString};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::ptr;
use std::sync::{Arc, Barrier};
use std::thread;

use dg_capi::{
    dg_backend_capabilities, dg_backend_create, dg_backend_free, dg_backend_io_counts,
    dg_backend_run, dg_backend_tensor_info, dg_buffer_free, dg_buffer_import_external,
    dg_buffer_size, dg_engine_build, dg_engine_create, dg_engine_destroy, dg_engine_init,
    dg_engine_load_string, dg_engine_metrics, dg_engine_status, dg_owned_bytes_data,
    dg_owned_bytes_free, dg_owned_bytes_len, dg_tensor_create, dg_tensor_data, dg_tensor_free,
    DgBackend, DgBackendCapabilities, DgBackendKind, DgBuffer, DgByteView, DgDataFormat,
    DgDataType, DgDeviceKind, DgEngine, DgExternalMemoryV2, DgGraphFormat, DgGraphStatus,
    DgOwnedBytes, DgShapeView, DgStatus, DgStringView, DgTensor, DgTensorInfo,
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
            DgStringView {
                data: spec.as_ptr(),
                len: spec.to_bytes().len()
            },
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
    let out_ptr: *mut *mut DgTensor = Box::into_raw(Box::new(std::ptr::dangling_mut::<DgTensor>()));
    unsafe {
        assert_eq!(
            dg_tensor_create(
                DgByteView {
                    data: data.as_ptr(),
                    len: data.len()
                },
                DgShapeView {
                    dims: shape.as_ptr(),
                    rank: shape.len()
                },
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

#[test]
fn concurrent_backend_queries_and_run_do_not_race() {
    let options = CString::new(r#"{"shape":[1,4],"echo_inputs":true}"#).expect("options");
    let mut backend = ptr::null_mut();
    unsafe {
        assert_eq!(
            dg_backend_create(
                DgBackendKind::Mock as i32,
                DgByteView {
                    data: ptr::null(),
                    len: 0
                },
                DgStringView {
                    data: options.as_ptr(),
                    len: options.to_bytes().len()
                },
                &mut backend,
                ptr::null_mut(),
            ),
            DgStatus::Ok
        );
    }
    assert!(!backend.is_null());

    let input_bytes: Vec<u8> = [1.0_f32, 2.0, 3.0, 4.0]
        .into_iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    let shape = [1usize, 4];
    let mut input = ptr::null_mut();
    unsafe {
        assert_eq!(
            dg_tensor_create(
                DgByteView {
                    data: input_bytes.as_ptr(),
                    len: input_bytes.len()
                },
                DgShapeView {
                    dims: shape.as_ptr(),
                    rank: shape.len()
                },
                DgDataType::F32 as i32,
                DgDataFormat::Nc as i32,
                DgDeviceKind::Cpu as i32,
                &mut input,
                ptr::null_mut(),
            ),
            DgStatus::Ok
        );
    }

    let input_ptr = input as *const DgTensor;
    let barrier = Arc::new(Barrier::new(4));
    let input_bytes_for_run = input_bytes.clone();
    let backend_addr = backend as usize;
    let input_addr = input_ptr as usize;
    let mut handles = Vec::new();

    for _ in 0..3 {
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let backend = backend_addr as *mut DgBackend;
            barrier.wait();
            unsafe {
                let mut inputs = 0;
                let mut outputs = 0;
                assert_eq!(
                    dg_backend_io_counts(backend, &mut inputs, &mut outputs, ptr::null_mut()),
                    DgStatus::Ok
                );
                assert_eq!((inputs, outputs), (1, 1));

                let mut capabilities = DgBackendCapabilities {
                    struct_size: std::mem::size_of::<DgBackendCapabilities>() as u32,
                    struct_version: 0,
                    device_count: 0,
                    devices: [DgDeviceKind::Cpu; 8],
                    precision_count: 0,
                    precisions: [DgDataType::U8; 16],
                };
                assert_eq!(
                    dg_backend_capabilities(backend, &mut capabilities, ptr::null_mut()),
                    DgStatus::Ok
                );

                let mut info = DgTensorInfo {
                    struct_size: std::mem::size_of::<DgTensorInfo>() as u32,
                    struct_version: 0,
                    dtype: DgDataType::U8,
                    format: DgDataFormat::Auto,
                    device: DgDeviceKind::Cpu,
                    rank: 0,
                    shape: [0; 8],
                };
                assert_eq!(
                    dg_backend_tensor_info(backend, false, 0, &mut info, ptr::null_mut()),
                    DgStatus::Ok
                );
            }
        }));
    }

    let barrier = Arc::clone(&barrier);
    handles.push(thread::spawn(move || {
        let backend = backend_addr as *mut DgBackend;
        let input_ptr = input_addr as *const DgTensor;
        barrier.wait();
        for _ in 0..32 {
            let mut output = ptr::null_mut();
            let mut count = 0;
            unsafe {
                assert_eq!(
                    dg_backend_run(
                        backend,
                        &input_ptr,
                        1,
                        &mut output,
                        1,
                        &mut count,
                        ptr::null_mut(),
                    ),
                    DgStatus::Ok
                );
                assert_eq!(count, 1);

                let mut owned = ptr::null_mut();
                assert_eq!(
                    dg_tensor_data(output, &mut owned, ptr::null_mut()),
                    DgStatus::Ok
                );
                let data = dg_owned_bytes_data(owned);
                let len = dg_owned_bytes_len(owned);
                assert_eq!(
                    std::slice::from_raw_parts(data, len),
                    input_bytes_for_run.as_slice()
                );
                dg_owned_bytes_free(owned);
                dg_tensor_free(output);
            }
        }
    }));

    for handle in handles {
        handle.join().expect("thread joined");
    }

    unsafe {
        dg_tensor_free(input);
        dg_backend_free(backend);
    }
}

#[test]
fn concurrent_tensor_data_reads_are_safe() {
    let data: Vec<u8> = [1.0_f32, 2.0, 3.0, 4.0]
        .into_iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    let shape = [1usize, 4];
    let mut tensor = ptr::null_mut();
    unsafe {
        assert_eq!(
            dg_tensor_create(
                DgByteView {
                    data: data.as_ptr(),
                    len: data.len()
                },
                DgShapeView {
                    dims: shape.as_ptr(),
                    rank: shape.len()
                },
                DgDataType::F32 as i32,
                DgDataFormat::Nc as i32,
                DgDeviceKind::Cpu as i32,
                &mut tensor,
                ptr::null_mut(),
            ),
            DgStatus::Ok
        );
    }
    assert!(!tensor.is_null());

    let tensor_addr = tensor as usize;
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let expected = data.clone();
            thread::spawn(move || {
                let tensor = tensor_addr as *const DgTensor;
                barrier.wait();
                for _ in 0..100 {
                    let mut owned = ptr::null_mut();
                    assert_eq!(
                        unsafe { dg_tensor_data(tensor, &mut owned, ptr::null_mut()) },
                        DgStatus::Ok
                    );
                    assert!(!owned.is_null());
                    let bytes = unsafe {
                        std::slice::from_raw_parts(
                            dg_owned_bytes_data(owned),
                            dg_owned_bytes_len(owned),
                        )
                    };
                    assert_eq!(bytes, expected.as_slice());
                    unsafe { dg_owned_bytes_free(owned) };
                }
            })
        })
        .collect();

    barrier.wait();

    for handle in handles {
        handle.join().expect("thread joined");
    }

    unsafe { dg_tensor_free(tensor) };
}

unsafe extern "C" fn noop_release(_user_data: *mut c_void) {}

#[test]
fn concurrent_buffer_size_reads_are_safe() {
    let desc = DgExternalMemoryV2 {
        struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
        struct_version: 0,
        fd: -1,
        raw: 1,
        domain: 0, // Host
        device: 0, // Cpu
        size_bytes: 4096,
        release: Some(noop_release),
        user_data: ptr::null_mut(),
    };
    let mut buffer = ptr::null_mut();
    unsafe {
        assert_eq!(
            dg_buffer_import_external(&desc, &mut buffer, ptr::null_mut()),
            DgStatus::Ok
        );
    }
    assert!(!buffer.is_null());

    let buffer_addr = buffer as usize;
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let buffer = buffer_addr as *const DgBuffer;
                barrier.wait();
                for _ in 0..100 {
                    let mut size = 0usize;
                    assert_eq!(
                        unsafe { dg_buffer_size(buffer, &mut size, ptr::null_mut()) },
                        DgStatus::Ok
                    );
                    assert_eq!(size, 4096);
                }
            })
        })
        .collect();

    barrier.wait();

    for handle in handles {
        handle.join().expect("thread joined");
    }

    unsafe { dg_buffer_free(buffer) };
}
