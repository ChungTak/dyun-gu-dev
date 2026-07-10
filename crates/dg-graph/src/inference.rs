use std::path::PathBuf;

use dg_core::{DataFormat, DataType, DeployMode, DeviceKind, Shape, TypeCode};
use dg_runtime::{
    BackendKind, BackendOptions, MockOptions, ModelSource, OpenVINOOptions, RknnOptions, Runtime,
    RuntimeOption, SophonOptions, TensorInfo, TensorRtOptions,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use tracing::trace;

use crate::{
    CreatedElement, Element, ElementDescriptor, ElementHandle, ElementIo, Error, NodeSpec,
    PortSchema, Result,
};

const INPUT_PORT: PortSchema = PortSchema {
    name: "in",
    dtype: None,
};
const OUTPUT_PORT: PortSchema = PortSchema {
    name: "out",
    dtype: None,
};

inventory::submit! {
    ElementDescriptor {
        kind: "inference",
        input_ports: &[INPUT_PORT],
        output_ports: &[OUTPUT_PORT],
        create: create_inference,
    }
}

struct InferenceElement {
    runtime: Runtime,
}

impl Element for InferenceElement {
    fn run(mut self: Box<Self>, io: ElementIo) -> Result<()> {
        trace!(node = %io.name, backend = ?self.runtime.backend_kind(), "running inference element");
        loop {
            let packet = match io.recv("in") {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    if io.stop.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(Error::NotRunning);
                    }
                    continue;
                }
                Err(err) => return Err(err),
            };
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }

            let input = packet
                .tensor_ref()
                .ok_or_else(|| Error::Runtime("inference expects a tensor payload".to_string()))?
                .clone();
            let meta = packet.meta.clone();
            for output in self.runtime.run(&[input])? {
                io.send("out", crate::Packet::tensor(output).with_meta(meta.clone()))?;
            }
        }
    }
}

fn create_inference(node: &NodeSpec) -> Result<CreatedElement> {
    create_inference_inner(node.params.clone()).map_err(|err| match err {
        Error::Config(message) => {
            Error::Config(format!("node {} inference params: {message}", node.name))
        }
        err => Error::Element {
            element: node.name.clone(),
            message: err.to_string(),
        },
    })
}

fn create_inference_inner(value: Value) -> Result<CreatedElement> {
    let params: InferenceParams = serde_json::from_value(value)
        .map_err(|err| Error::Config(format!("invalid parameters: {err}")))?;
    let backend = parse_backend(&params.backend)?;
    let deploy_mode = params
        .deploy_mode
        .as_deref()
        .map(parse_deploy_mode)
        .transpose()?;
    let backend_options =
        build_backend_options(backend, &params.options, params.core_mask, deploy_mode)?;
    let model_source = model_source(backend, params.model)?;
    let mut option = RuntimeOption::new(backend, model_source, backend_options);
    if let Some(precision) = params.precision.as_deref() {
        option = option.with_precision(parse_dtype(precision)?);
    }
    if let Some(device) = params.device.as_deref() {
        option = option.with_device(parse_device(device)?);
    }
    if let Some(deploy_mode) = deploy_mode {
        option = option.with_deploy_mode(deploy_mode);
    }
    if let Some(core_mask) = params.core_mask {
        option = option.with_core_mask(core_mask);
    }

    let mut runtime = Runtime::new(option)?;
    if runtime.input_count() != 1 {
        return Err(Error::Config(format!(
            "inference element requires a single-input model, got {} inputs",
            runtime.input_count()
        )));
    }
    if runtime.output_count() == 0 {
        return Err(Error::Config("inference model has no outputs".to_string()));
    }
    if let Some(shape) = params.reshape {
        runtime.reshape(&[Shape::new(shape)])?;
    }

    Ok(CreatedElement {
        element: Box::new(InferenceElement { runtime }),
        handle: ElementHandle::None,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InferenceParams {
    backend: String,
    #[serde(default)]
    model: Option<PathBuf>,
    #[serde(default)]
    precision: Option<String>,
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    deploy_mode: Option<String>,
    #[serde(default)]
    core_mask: Option<u32>,
    #[serde(default)]
    reshape: Option<Vec<usize>>,
    #[serde(default)]
    options: Value,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MockParams {
    shape: Option<Vec<usize>>,
    output_shape: Option<Vec<usize>>,
    dtype: Option<String>,
    output_dtype: Option<String>,
    layout: Option<String>,
    echo_inputs: Option<bool>,
    fill_value: Option<u8>,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct OpenVinoParams {
    device: String,
}

impl Default for OpenVinoParams {
    fn default() -> Self {
        Self {
            device: "CPU".to_string(),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RknnParams {
    enable_zero_copy: bool,
    dynamic_shape: bool,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TensorRtParams {
    device_id: Option<u32>,
    workspace_size_mb: usize,
    enable_fp16: bool,
    enable_int8: bool,
}

impl Default for TensorRtParams {
    fn default() -> Self {
        Self {
            device_id: None,
            workspace_size_mb: 1024,
            enable_fp16: false,
            enable_int8: false,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SophonParams {
    device_id: Option<u32>,
}

fn build_backend_options(
    backend: BackendKind,
    value: &Value,
    core_mask: Option<u32>,
    deploy_mode: Option<DeployMode>,
) -> Result<BackendOptions> {
    match backend {
        BackendKind::Mock => {
            let params: MockParams = parse_options(backend, value)?;
            let shape = Shape::new(params.shape.unwrap_or_else(|| vec![1, 4]));
            let output_shape =
                Shape::new(params.output_shape.unwrap_or_else(|| shape.dims().to_vec()));
            let dtype = params
                .dtype
                .as_deref()
                .map(parse_dtype)
                .transpose()?
                .unwrap_or(DataType::F32);
            let output_dtype = params
                .output_dtype
                .as_deref()
                .map(parse_dtype)
                .transpose()?
                .unwrap_or(dtype);
            let layout = params
                .layout
                .as_deref()
                .map(parse_layout)
                .transpose()?
                .unwrap_or(DataFormat::NC);
            Ok(BackendOptions::Mock(MockOptions {
                input_infos: vec![TensorInfo::new(shape, dtype).with_layout(layout)],
                output_infos: vec![TensorInfo::new(output_shape, output_dtype).with_layout(layout)],
                echo_inputs: params.echo_inputs.unwrap_or(true),
                fill_value: params.fill_value.unwrap_or(0),
            }))
        }
        BackendKind::OpenVINO => {
            let params: OpenVinoParams = parse_options(backend, value)?;
            Ok(BackendOptions::OpenVINO(OpenVINOOptions {
                device: params.device,
            }))
        }
        BackendKind::Rknn => {
            let params: RknnParams = parse_options(backend, value)?;
            Ok(BackendOptions::Rknn(RknnOptions {
                core_mask,
                enable_zero_copy: params.enable_zero_copy,
                dynamic_shape: params.dynamic_shape,
            }))
        }
        BackendKind::TensorRt => {
            let params: TensorRtParams = parse_options(backend, value)?;
            Ok(BackendOptions::TensorRt(TensorRtOptions {
                device_id: params.device_id,
                workspace_size_mb: params.workspace_size_mb,
                enable_fp16: params.enable_fp16,
                enable_int8: params.enable_int8,
            }))
        }
        BackendKind::Sophon => {
            let params: SophonParams = parse_options(backend, value)?;
            Ok(BackendOptions::Sophon(SophonOptions {
                deploy_mode: deploy_mode.unwrap_or(DeployMode::Host),
                device_id: params.device_id,
                core_mask,
            }))
        }
    }
}

fn parse_options<T: DeserializeOwned>(backend: BackendKind, value: &Value) -> Result<T> {
    let value = if value.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        value.clone()
    };
    serde_json::from_value(value)
        .map_err(|err| Error::Config(format!("{backend:?} inference options: {err}")))
}

fn model_source(backend: BackendKind, model: Option<PathBuf>) -> Result<ModelSource> {
    if backend == BackendKind::Mock {
        return Ok(ModelSource::Bytes(Vec::new()));
    }
    model
        .map(ModelSource::File)
        .ok_or_else(|| Error::Config(format!("{backend:?} inference requires a model file path")))
}

fn parse_backend(value: &str) -> Result<BackendKind> {
    match value {
        "mock" => Ok(BackendKind::Mock),
        "openvino" => Ok(BackendKind::OpenVINO),
        "rknn" => Ok(BackendKind::Rknn),
        "tensorrt" => Ok(BackendKind::TensorRt),
        "sophon" => Ok(BackendKind::Sophon),
        _ => Err(Error::Config(format!("unknown inference backend: {value}"))),
    }
}

fn parse_dtype(value: &str) -> Result<DataType> {
    match value {
        "f4" => Ok(DataType::F4),
        "f8" => Ok(DataType::F8),
        "f16" => Ok(DataType::F16),
        "f32" => Ok(DataType::F32),
        "f64" => Ok(DataType::F64),
        "bf16" => Ok(DataType::BF16),
        "u8" => Ok(DataType::U8),
        "u16" => Ok(DataType::U16),
        "u32" => Ok(DataType::new(TypeCode::Uint, 32, 1)),
        "u64" => Ok(DataType::new(TypeCode::Uint, 64, 1)),
        "i4" => Ok(DataType::I4),
        "i8" => Ok(DataType::I8),
        "i16" => Ok(DataType::I16),
        "i32" => Ok(DataType::new(TypeCode::Int, 32, 1)),
        "i64" => Ok(DataType::new(TypeCode::Int, 64, 1)),
        _ => Err(Error::Config(format!(
            "unsupported inference precision: {value}"
        ))),
    }
}

fn parse_layout(value: &str) -> Result<DataFormat> {
    match value {
        "auto" => Ok(DataFormat::Auto),
        "n" => Ok(DataFormat::N),
        "nc" => Ok(DataFormat::NC),
        "nchw" => Ok(DataFormat::NCHW),
        "nhwc" => Ok(DataFormat::NHWC),
        "nc4hw" => Ok(DataFormat::NC4HW),
        "nc8hw" => Ok(DataFormat::NC8HW),
        "ncdhw" => Ok(DataFormat::NCDHW),
        "oihw" => Ok(DataFormat::OIHW),
        _ => Err(Error::Config(format!(
            "unsupported inference layout: {value}"
        ))),
    }
}

fn parse_device(value: &str) -> Result<DeviceKind> {
    match value {
        "cpu" => Ok(DeviceKind::Cpu),
        "intel_gpu" => Ok(DeviceKind::IntelGpu),
        "intel_npu" => Ok(DeviceKind::IntelNpu),
        "cuda" | "cuda_gpu" => Ok(DeviceKind::CudaGpu),
        "rknn" | "rknn_npu" => Ok(DeviceKind::RknnNpu),
        "sophon" | "sophon_tpu" => Ok(DeviceKind::SophonTpu),
        _ => Err(Error::Config(format!(
            "unsupported inference device: {value}"
        ))),
    }
}

fn parse_deploy_mode(value: &str) -> Result<DeployMode> {
    match value {
        "host" => Ok(DeployMode::Host),
        "soc" => Ok(DeployMode::SoC),
        _ => Err(Error::Config(format!(
            "unsupported inference deploy_mode: {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use crate::{Graph, GraphFormat, GraphSpec};

    #[test]
    fn generic_mock_inference_runs_in_graph() {
        let yaml = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 2
      shape: [1, 2]
  - name: infer
    kind: inference
    params:
      backend: mock
      reshape: [1, 2]
      options:
        shape: [1, 2]
        echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#;
        let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml)
            .expect("parse")
            .normalize_with_base_dir(None)
            .expect("normalize");
        let report = Graph::new(spec).expect("build").run().expect("run");
        let outputs = report.sinks.get("sink").expect("sink outputs");
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].desc().shape().dims(), &[1, 2]);
    }

    #[test]
    fn real_backend_requires_model_path_before_initialization() {
        let yaml = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: infer
    kind: inference
    params:
      backend: tensorrt
"#;
        let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml)
            .expect("parse")
            .normalize_with_base_dir(None)
            .expect("normalize");
        let graph = Graph::new(spec).expect("build");
        let err = graph.run().expect_err("model is required");
        assert!(err.to_string().contains("requires a model file path"));
    }

    #[test]
    fn inference_options_reject_unknown_fields_with_node_context() {
        let yaml = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: infer
    kind: inference
    params:
      backend: mock
      options:
        unknown: true
"#;
        let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml)
            .expect("parse")
            .normalize_with_base_dir(None)
            .expect("normalize");
        let graph = Graph::new(spec).expect("build");
        let err = graph.run().expect_err("unknown option is rejected");
        let message = err.to_string();
        assert!(message.contains("node infer"));
        assert!(message.contains("unknown field"));
    }
}
