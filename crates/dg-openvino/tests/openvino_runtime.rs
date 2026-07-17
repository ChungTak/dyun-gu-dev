#![cfg(feature = "backend")]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dg_core::{DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_runtime::{
    BackendKind, BackendOptions, ModelSource, OpenVINOOptions, RegressionCase, RegressionHarness,
    Runtime, RuntimeOption,
};

fn python_command() -> Command {
    for candidate in ["python", "python3"] {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return Command::new(candidate);
        }
    }
    Command::new("python")
}

fn unique_temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be monotonic")
        .as_nanos();
    std::env::temp_dir().join(format!("dg-openvino-{nanos}-{}", std::process::id()))
}

fn create_identity_model(output_dir: &Path) -> PathBuf {
    let model_path = output_dir.join("identity.xml");
    let script = r#"
import numpy as np
import openvino as ov
from openvino import opset8 as ops
import sys

target = sys.argv[1]
param = ops.parameter([1, 4], dtype=np.float32, name='input')
result = ops.result(param, name='output')
model = ov.Model([result], [param], 'identity')
ov.save_model(model, target)
"#;

    let status = python_command()
        .arg("-c")
        .arg(script)
        .arg(&model_path)
        .status()
        .expect("python should be available to build the model");
    assert!(status.success(), "python OpenVINO model generation failed");
    assert!(model_path.exists(), "XML model should exist");
    assert!(
        model_path.with_extension("bin").exists(),
        "BIN weights should exist"
    );
    model_path
}

fn create_dynamic_batch_identity_model(output_dir: &Path) -> PathBuf {
    let model_path = output_dir.join("dynamic_identity.xml");
    let script = r#"
import numpy as np
import openvino as ov
from openvino import opset8 as ops
import sys

target = sys.argv[1]
param = ops.parameter([-1, 4], dtype=np.float32, name='input')
result = ops.result(param, name='output')
model = ov.Model([result], [param], 'dynamic_identity')
ov.save_model(model, target)
"#;

    let status = python_command()
        .arg("-c")
        .arg(script)
        .arg(&model_path)
        .status()
        .expect("python should be available to build the model");
    assert!(status.success(), "python OpenVINO model generation failed");
    assert!(model_path.exists(), "XML model should exist");
    assert!(
        model_path.with_extension("bin").exists(),
        "BIN weights should exist"
    );
    model_path
}

fn openvino_lib_dir() -> PathBuf {
    let script = r#"
import pathlib
import openvino
print(pathlib.Path(openvino.__file__).resolve().parent / 'libs')
"#;

    let output = python_command()
        .arg("-c")
        .arg(script)
        .output()
        .expect("python should be available to locate OpenVINO libs");
    assert!(
        output.status.success(),
        "failed to discover OpenVINO library directory"
    );
    let path = String::from_utf8(output.stdout).expect("OpenVINO lib path should be UTF-8");
    PathBuf::from(path.trim())
}

fn prepare_loader_dir(root: &Path) -> PathBuf {
    let loader_dir = root.join("loader");
    std::fs::create_dir_all(&loader_dir).expect("create loader dir");
    let lib_dir = openvino_lib_dir();
    for (link_name, target_name) in [
        ("libopenvino.so", "libopenvino.so.2621"),
        ("libopenvino_c.so", "libopenvino_c.so.2621"),
    ] {
        let link = loader_dir.join(link_name);
        if link.exists() {
            std::fs::remove_file(&link).expect("remove stale loader symlink");
        }
        symlink(lib_dir.join(target_name), &link).expect("create OpenVINO loader symlink");
    }
    loader_dir
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

fn run_openvino_identity_model(model_path: PathBuf) {
    let option = RuntimeOption::new(
        BackendKind::OpenVINO,
        ModelSource::File(model_path),
        BackendOptions::OpenVINO(OpenVINOOptions::default()),
    )
    .with_precision(DataType::F32)
    .with_device(DeviceKind::Cpu);

    let mut runtime = Runtime::new(option).expect("construct OpenVINO runtime");
    assert_eq!(runtime.input_count(), 1);
    assert_eq!(runtime.output_count(), 1);

    let device = dg_core::CpuDevice::new();
    let input_desc = TensorDesc::new(
        Shape::new([1, 4]),
        DataType::F32,
        DataFormat::NC,
        DeviceKind::Cpu,
    )
    .with_name("input");
    let input = Tensor::allocate(&device, input_desc).expect("allocate input");
    let input_values = [1.0f32, -2.0, 3.5, 7.25];
    input
        .buffer()
        .write_from_slice(&f32_bytes(&input_values))
        .expect("seed input tensor");

    let outputs = runtime.run(&[input]).expect("run OpenVINO backend");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].buffer().read_bytes(), f32_bytes(&input_values));
}

fn run_openvino_dynamic_batch_model(model_path: PathBuf) {
    let option = RuntimeOption::new(
        BackendKind::OpenVINO,
        ModelSource::File(model_path),
        BackendOptions::OpenVINO(OpenVINOOptions::default()),
    )
    .with_precision(DataType::F32)
    .with_device(DeviceKind::Cpu);

    let mut runtime = Runtime::new(option).expect("construct OpenVINO runtime");
    runtime
        .reshape(&[Shape::new([2, 4])])
        .expect("reshape runtime");
    assert_eq!(runtime.input_infos()[0].shape.dims(), &[2, 4]);
    assert_eq!(runtime.output_infos()[0].shape.dims(), &[2, 4]);

    let device = dg_core::CpuDevice::new();
    let input_desc = TensorDesc::new(
        Shape::new([2, 4]),
        DataType::F32,
        DataFormat::NC,
        DeviceKind::Cpu,
    )
    .with_name("input");
    let input = Tensor::allocate(&device, input_desc).expect("allocate input");
    let input_values = [1.0f32, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0];
    input
        .buffer()
        .write_from_slice(&f32_bytes(&input_values))
        .expect("seed input tensor");

    let outputs = runtime.run(&[input]).expect("run OpenVINO backend");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].buffer().read_bytes(), f32_bytes(&input_values));
}

fn run_openvino_regression_model(model_path: PathBuf) {
    let option = RuntimeOption::new(
        BackendKind::OpenVINO,
        ModelSource::File(model_path),
        BackendOptions::OpenVINO(OpenVINOOptions::default()),
    )
    .with_precision(DataType::F32)
    .with_device(DeviceKind::Cpu);
    let mut runtime = Runtime::new(option).expect("construct OpenVINO regression runtime");
    let case = RegressionCase::from_json(include_str!("fixtures/openvino_identity_f32.json"))
        .expect("load OpenVINO regression fixture");
    let report = RegressionHarness::run(&mut runtime, &case).expect("run OpenVINO regression");
    assert_eq!(report.case, "openvino_identity_f32");
    assert!(report.max_absolute_error <= 0.000001);
    assert!(report.max_relative_error <= 0.000001);
    assert!(report.minimum_cosine_similarity >= 0.999999);
}

fn run_openvino_cpu_async_inflight(model_path: &Path, max_in_flight: usize) {
    let option = RuntimeOption::new(
        BackendKind::OpenVINO,
        ModelSource::File(model_path.to_path_buf()),
        BackendOptions::OpenVINO(OpenVINOOptions {
            device: "CPU".to_string(),
            max_in_flight,
        }),
    )
    .with_precision(DataType::F32)
    .with_device(DeviceKind::Cpu);

    let mut runtime = Runtime::new(option).unwrap_or_else(|err| {
        panic!("construct OpenVINO runtime for in-flight {max_in_flight}: {err}")
    });
    assert!(runtime.is_async(), "OpenVINO backend should be async");

    let device = dg_core::CpuDevice::new();
    let input_values: Vec<[f32; 4]> = (0..8)
        .map(|index| {
            [
                index as f32,
                index as f32 + 0.25,
                index as f32 + 0.5,
                index as f32 + 0.75,
            ]
        })
        .collect();

    let start = Instant::now();
    let mut sequences = Vec::new();
    for value in &input_values {
        let input_desc = TensorDesc::new(
            Shape::new([1, 4]),
            DataType::F32,
            DataFormat::NC,
            DeviceKind::Cpu,
        )
        .with_name("input");
        let input = Tensor::allocate(&device, input_desc).expect("allocate input");
        input
            .buffer()
            .write_from_slice(&f32_bytes(value))
            .expect("seed input");
        let sequence = runtime
            .submit(std::slice::from_ref(&input), None)
            .expect("submit should succeed");
        sequences.push(sequence);
        assert!(runtime.in_flight() <= max_in_flight);
    }

    let mut received = 0u64;
    let deadline = Instant::now() + Duration::from_secs(30);
    while received < sequences.len() as u64 && Instant::now() < deadline {
        if let dg_runtime::InferPoll::Ready { outputs, sequence } = runtime.poll().expect("poll") {
            let index = sequences
                .iter()
                .position(|s| *s == sequence)
                .expect("unknown sequence");
            assert_eq!(
                outputs[0].buffer().read_bytes(),
                f32_bytes(&input_values[index])
            );
            received += 1;
        }
    }
    assert_eq!(
        received,
        sequences.len() as u64,
        "all submissions should complete"
    );
    let elapsed = start.elapsed();

    let snapshot = runtime.metrics().snapshot();
    assert_eq!(snapshot.submissions, 8);
    assert_eq!(snapshot.in_flight, 0);
    assert_eq!(snapshot.backend_errors, 0);
    assert!(
        snapshot.host_copy_bytes > 0,
        "host tensor copies should be recorded"
    );
    assert_eq!(snapshot.infer_latencies.count, 8);

    let throughput = 8.0 / elapsed.as_secs_f64();
    println!(
        "OpenVINO CPU max_in_flight={max_in_flight}: elapsed={elapsed:?}, throughput={throughput:.2} inf/s, \
         host_copy_bytes={}, p50={}us, p95={}us, p99={}us",
        snapshot.host_copy_bytes,
        snapshot.infer_latencies.p50_ns / 1000,
        snapshot.infer_latencies.p95_ns / 1000,
        snapshot.infer_latencies.p99_ns / 1000,
    );
}

#[test]
#[ignore]
fn openvino_identity_model_runs_end_to_end() {
    if std::env::var_os("DG_OPENVINO_E2E_CHILD").is_some() {
        let model_path = std::env::var_os("DG_OPENVINO_E2E_MODEL_PATH")
            .map(PathBuf::from)
            .expect("model path should be provided");
        run_openvino_identity_model(model_path);
        return;
    }

    assert!(dg_openvino::backend_enabled());

    let temp_dir = unique_temp_dir();
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let loader_dir = prepare_loader_dir(&temp_dir);
    let lib_dir = openvino_lib_dir();
    let model_path = create_dynamic_batch_identity_model(&temp_dir);

    let current_exe = std::env::current_exe().expect("locate current test binary");
    let current_ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let ld_library_path = format!(
        "{}:{}:{}",
        loader_dir.display(),
        lib_dir.display(),
        current_ld_library_path
    );

    let status = Command::new(current_exe)
        .arg("--exact")
        .arg("openvino_identity_model_runs_end_to_end")
        .arg("--ignored")
        .arg("--nocapture")
        .env("DG_OPENVINO_E2E_CHILD", "1")
        .env("DG_OPENVINO_E2E_MODEL_PATH", &model_path)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .status()
        .expect("spawn OpenVINO child test process");
    assert!(status.success(), "child OpenVINO process should succeed");
}

#[test]
#[ignore]
fn openvino_dynamic_batch_reshape_updates_outputs() {
    if std::env::var_os("DG_OPENVINO_E2E_CHILD").is_some() {
        let model_path = std::env::var_os("DG_OPENVINO_E2E_MODEL_PATH")
            .map(PathBuf::from)
            .expect("model path should be provided");
        run_openvino_dynamic_batch_model(model_path);
        return;
    }

    let temp_dir = unique_temp_dir();
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let loader_dir = prepare_loader_dir(&temp_dir);
    let lib_dir = openvino_lib_dir();
    let model_path = create_identity_model(&temp_dir);

    let current_exe = std::env::current_exe().expect("locate current test binary");
    let current_ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let ld_library_path = format!(
        "{}:{}:{}",
        loader_dir.display(),
        lib_dir.display(),
        current_ld_library_path
    );

    let status = Command::new(current_exe)
        .arg("--exact")
        .arg("openvino_dynamic_batch_reshape_updates_outputs")
        .arg("--ignored")
        .arg("--nocapture")
        .env("DG_OPENVINO_E2E_CHILD", "1")
        .env("DG_OPENVINO_E2E_MODEL_PATH", &model_path)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .status()
        .expect("spawn OpenVINO child test process");
    assert!(status.success(), "child OpenVINO process should succeed");
}

#[test]
#[ignore]
fn openvino_cpu_regression_runs_through_test01_harness() {
    if std::env::var_os("DG_OPENVINO_E2E_CHILD").is_some() {
        let model_path = std::env::var_os("DG_OPENVINO_E2E_MODEL_PATH")
            .map(PathBuf::from)
            .expect("model path should be provided");
        run_openvino_regression_model(model_path);
        return;
    }

    let temp_dir = unique_temp_dir();
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let loader_dir = prepare_loader_dir(&temp_dir);
    let lib_dir = openvino_lib_dir();
    let model_path = create_identity_model(&temp_dir);

    let current_exe = std::env::current_exe().expect("locate current test binary");
    let current_ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let ld_library_path = format!(
        "{}:{}:{}",
        loader_dir.display(),
        lib_dir.display(),
        current_ld_library_path
    );

    let status = Command::new(current_exe)
        .arg("--exact")
        .arg("openvino_cpu_regression_runs_through_test01_harness")
        .arg("--ignored")
        .arg("--nocapture")
        .env("DG_OPENVINO_E2E_CHILD", "1")
        .env("DG_OPENVINO_E2E_MODEL_PATH", &model_path)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .status()
        .expect("spawn OpenVINO regression child test process");
    assert!(
        status.success(),
        "child OpenVINO regression test should succeed"
    );
}

#[test]
#[ignore]
fn openvino_cpu_async_inflight_1_2_4_records_baseline_metrics() {
    if std::env::var_os("DG_OPENVINO_E2E_CHILD").is_some() {
        let model_path = std::env::var_os("DG_OPENVINO_E2E_MODEL_PATH")
            .map(PathBuf::from)
            .expect("model path should be provided");
        for max_in_flight in [1usize, 2, 4] {
            run_openvino_cpu_async_inflight(&model_path, max_in_flight);
        }
        return;
    }

    let temp_dir = unique_temp_dir();
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let loader_dir = prepare_loader_dir(&temp_dir);
    let lib_dir = openvino_lib_dir();
    let model_path = create_identity_model(&temp_dir);

    let current_exe = std::env::current_exe().expect("locate current test binary");
    let current_ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let ld_library_path = format!(
        "{}:{}:{}",
        loader_dir.display(),
        lib_dir.display(),
        current_ld_library_path
    );

    let status = Command::new(current_exe)
        .arg("--exact")
        .arg("openvino_cpu_async_inflight_1_2_4_records_baseline_metrics")
        .arg("--ignored")
        .arg("--nocapture")
        .env("DG_OPENVINO_E2E_CHILD", "1")
        .env("DG_OPENVINO_E2E_MODEL_PATH", &model_path)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .status()
        .expect("spawn OpenVINO child test process");
    assert!(
        status.success(),
        "child OpenVINO in-flight baseline test should succeed"
    );
}
