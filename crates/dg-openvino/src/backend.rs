use std::collections::HashSet;
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dg_core::{DataFormat, DataType, DeployMode, DeviceKind, Shape, Tensor};
use dg_openvino_sys::{
    Core, DeviceType, ElementType, InferRequest, Model, Node, PartialShape, PropertyKey,
    Tensor as OvTensor,
};
use dg_runtime::{
    backend_capabilities, supports_deployment, supports_device, supports_precision, BackendConfig,
    BackendDescriptor, BackendKind, BackendMetrics, BackendOptions, Error, ExecutionMode,
    InferBackend, InferPoll, ModelSource, Result, RuntimeCapabilities, RuntimeDeviceCapabilities,
    RuntimeOption, TensorInfo,
};
use serde::Deserialize;
use tracing::{trace, warn};

pub use dg_runtime::OpenVINOOptions;

pub fn backend_enabled() -> bool {
    true
}

/// Hard ceiling on the number of input/output tensors a model may declare.
/// This prevents adversarial or malformed models from forcing unbounded
/// metadata discovery loops and allocations.
const MAX_OPENVINO_IO_COUNT: usize = 1024;

/// A submitted OpenVINO request that is still in flight.
///
/// The `inputs` tensor vector must outlive the asynchronous inference because
/// `InferRequest` keeps a reference to the input tensors set on it.
struct InFlightRequest {
    request: InferRequest,
    /// Input tensors must outlive the async request reference.
    #[allow(dead_code)]
    inputs: Vec<OvTensor>,
    sequence: u64,
    submit_time: Instant,
}

pub struct OpenVINOBackend {
    option: Option<RuntimeOption>,
    core: Option<Core>,
    model: Option<Model>,
    compiled_model: Option<dg_openvino_sys::CompiledModel>,
    input_infos: Vec<TensorInfo>,
    output_infos: Vec<TensorInfo>,
    free_requests: Vec<InferRequest>,
    in_flight: Vec<InFlightRequest>,
    max_in_flight: usize,
    async_capable: bool,
    device: DeviceType<'static>,
    device_string: String,
    metrics: Option<Arc<BackendMetrics>>,
    capabilities: Option<RuntimeCapabilities>,
}

impl OpenVINOBackend {
    pub fn new() -> Self {
        Self {
            option: None,
            core: None,
            model: None,
            compiled_model: None,
            input_infos: Vec::new(),
            output_infos: Vec::new(),
            free_requests: Vec::new(),
            in_flight: Vec::new(),
            max_in_flight: 2,
            async_capable: false,
            device: DeviceType::CPU,
            device_string: "CPU".to_string(),
            metrics: None,
            capabilities: None,
        }
    }

    fn map_element_type(dtype: DataType) -> Result<ElementType> {
        if dtype == DataType::F32 {
            Ok(ElementType::F32)
        } else if dtype == DataType::F16 {
            Ok(ElementType::F16)
        } else if dtype == DataType::BF16 {
            Ok(ElementType::Bf16)
        } else if dtype == DataType::U8 {
            Ok(ElementType::U8)
        } else if dtype == DataType::I8 {
            Ok(ElementType::I8)
        } else if dtype == DataType::U16 {
            Ok(ElementType::U16)
        } else if dtype == DataType::I16 {
            Ok(ElementType::I16)
        } else if dtype == DataType::new(dg_core::TypeCode::Int, 32, 1) {
            Ok(ElementType::I32)
        } else if dtype == DataType::new(dg_core::TypeCode::Uint, 32, 1) {
            Ok(ElementType::U32)
        } else if dtype == DataType::new(dg_core::TypeCode::Int, 64, 1) {
            Ok(ElementType::I64)
        } else if dtype == DataType::new(dg_core::TypeCode::Uint, 64, 1) {
            Ok(ElementType::U64)
        } else {
            Err(Error::UnsupportedPrecision(dtype))
        }
    }

    fn map_data_type(element_type: ElementType) -> Result<DataType> {
        match element_type {
            ElementType::F32 => Ok(DataType::F32),
            ElementType::F16 => Ok(DataType::F16),
            ElementType::Bf16 => Ok(DataType::BF16),
            ElementType::U8 => Ok(DataType::U8),
            ElementType::I8 => Ok(DataType::I8),
            ElementType::U16 => Ok(DataType::U16),
            ElementType::I16 => Ok(DataType::I16),
            ElementType::I32 => Ok(DataType::new(dg_core::TypeCode::Int, 32, 1)),
            ElementType::U32 => Ok(DataType::new(dg_core::TypeCode::Uint, 32, 1)),
            ElementType::I64 => Ok(DataType::new(dg_core::TypeCode::Int, 64, 1)),
            ElementType::U64 => Ok(DataType::new(dg_core::TypeCode::Uint, 64, 1)),
            other => Err(Error::Backend(format!(
                "unsupported OpenVINO element type: {other}"
            ))),
        }
    }

    fn device_kind(&self) -> DeviceKind {
        Self::device_kind_from_name(&self.device_string).unwrap_or(DeviceKind::Cpu)
    }

    fn tensor_info_from_port(port: &Node, device_kind: DeviceKind) -> Result<TensorInfo> {
        let dims = match port.get_shape() {
            Ok(shape) => shape
                .get_dimensions()
                .iter()
                .map(|dim| {
                    usize::try_from(*dim)
                        .map_err(|_| Error::Backend("negative OpenVINO dimension".to_string()))
                })
                .collect::<Result<Vec<_>>>()?,
            Err(_) => {
                let partial_shape = port
                    .get_partial_shape()
                    .map_err(|err| Error::Backend(err.to_string()))?;
                partial_shape
                    .get_dimensions()
                    .iter()
                    .map(|dimension| {
                        if dimension.is_dynamic() {
                            Ok(1usize)
                        } else {
                            usize::try_from(dimension.get_max()).map_err(|_| {
                                Error::Backend("negative OpenVINO dimension".to_string())
                            })
                        }
                    })
                    .collect::<Result<Vec<_>>>()?
            }
        };

        let mut info = TensorInfo::new(
            Shape::new(dims),
            Self::map_data_type(
                port.get_element_type()
                    .map_err(|err| Error::Backend(err.to_string()))?,
            )?,
        )
        .with_layout(DataFormat::Auto)
        .with_device(device_kind);

        if let Ok(name) = port.get_name() {
            info = info.with_name(name);
        }

        Ok(info)
    }

    fn read_model(core: &mut Core, source: &ModelSource, max_model_bytes: usize) -> Result<Model> {
        match source {
            ModelSource::Bytes(bytes) => {
                if bytes.len() > max_model_bytes {
                    return Err(Error::Core(dg_core::Error::Config(format!(
                        "OpenVINO model bytes {} exceeds limit {}",
                        bytes.len(),
                        max_model_bytes
                    ))));
                }
                core.read_model_from_buffer(bytes, None)
                    .map_err(|err| Error::BackendUnavailable(err.to_string()))
            }
            ModelSource::File(path) => {
                if path.extension().and_then(|ext| ext.to_str()) == Some("xml") {
                    let weights = path.with_extension("bin");
                    if !weights.exists() {
                        return Err(Error::BackendUnavailable(format!(
                            "OpenVINO IR weights file missing: {}",
                            weights.display()
                        )));
                    }
                    let xml_len = std::fs::metadata(path)?.len();
                    let bin_len = std::fs::metadata(&weights)?.len();
                    let total = xml_len.saturating_add(bin_len);
                    let total = usize::try_from(total).map_err(|_| {
                        Error::Core(dg_core::Error::Config(
                            "OpenVINO IR total size exceeds usize".to_string(),
                        ))
                    })?;
                    if total > max_model_bytes {
                        return Err(Error::Core(dg_core::Error::Config(format!(
                            "OpenVINO IR total size {} exceeds limit {}",
                            total, max_model_bytes
                        ))));
                    }
                    let model_path = path.to_str().ok_or_else(|| {
                        Error::UnsupportedModelSource("non-utf8 path".to_string())
                    })?;
                    let weights_path = weights.to_str().ok_or_else(|| {
                        Error::UnsupportedModelSource("non-utf8 path".to_string())
                    })?;
                    return core
                        .read_model_from_file(model_path, weights_path)
                        .map_err(|err| Error::BackendUnavailable(err.to_string()));
                }
                let bytes = source.load_bounded(max_model_bytes)?.into_owned();
                core.read_model_from_buffer(&bytes, None)
                    .map_err(|err| Error::BackendUnavailable(err.to_string()))
            }
        }
    }

    fn openvino_options(option: &RuntimeOption) -> Result<&dg_runtime::OpenVINOOptions> {
        let BackendOptions::OpenVINO(options) = &option.backend_options else {
            return Err(Error::InvalidOption(
                "OpenVINO backend requires OpenVINO backend options".to_string(),
            ));
        };
        Ok(options)
    }

    fn device_kind_to_string(kind: DeviceKind) -> Result<String> {
        match kind {
            DeviceKind::Cpu => Ok("CPU".to_string()),
            DeviceKind::IntelGpu => Ok("GPU".to_string()),
            DeviceKind::IntelNpu => Ok("NPU".to_string()),
            other => Err(Error::UnsupportedDevice(other)),
        }
    }

    fn device_kind_from_name(name: &str) -> Option<DeviceKind> {
        if name.starts_with("CPU") {
            Some(DeviceKind::Cpu)
        } else if name.starts_with("GPU") {
            Some(DeviceKind::IntelGpu)
        } else if name.starts_with("NPU") {
            Some(DeviceKind::IntelNpu)
        } else {
            None
        }
    }

    fn precision_from_capability(cap: &str) -> Option<DataType> {
        match cap.trim() {
            "FP32" => Some(DataType::F32),
            "FP16" => Some(DataType::F16),
            "BF16" => Some(DataType::BF16),
            "U8" => Some(DataType::U8),
            "I8" => Some(DataType::I8),
            "U16" => Some(DataType::U16),
            "I16" => Some(DataType::I16),
            "I32" => Some(DataType::new(dg_core::TypeCode::Int, 32, 1)),
            "U32" => Some(DataType::new(dg_core::TypeCode::Uint, 32, 1)),
            "I64" => Some(DataType::new(dg_core::TypeCode::Int, 64, 1)),
            "U64" => Some(DataType::new(dg_core::TypeCode::Uint, 64, 1)),
            _ => None,
        }
    }

    fn parse_device_capabilities(value: &str) -> Vec<DataType> {
        let mut precisions = Vec::new();
        for token in value.split([',', ' ', ';']) {
            let token = token
                .trim()
                .trim_matches(|c| c == '[' || c == ']' || c == '"' || c == '\'');
            if token.is_empty() {
                continue;
            }
            if let Some(precision) = Self::precision_from_capability(token) {
                if !precisions.contains(&precision) {
                    precisions.push(precision);
                }
            }
        }
        precisions
    }

    #[allow(dead_code)] // retained for offline tooling / future offline probes
    fn default_precisions_for_device(kind: DeviceKind) -> Vec<DataType> {
        match kind {
            DeviceKind::Cpu => vec![
                DataType::F32,
                DataType::F16,
                DataType::BF16,
                DataType::I8,
                DataType::U8,
                DataType::I16,
                DataType::U16,
                DataType::new(dg_core::TypeCode::Int, 32, 1),
                DataType::new(dg_core::TypeCode::Int, 64, 1),
            ],
            DeviceKind::IntelGpu => vec![
                DataType::F32,
                DataType::F16,
                DataType::BF16,
                DataType::I8,
                DataType::U8,
            ],
            _ => vec![DataType::F32],
        }
    }

    fn probe_live_capabilities(&self, core: &Core) -> Result<RuntimeCapabilities> {
        let version = dg_openvino_sys::version();
        let available = core
            .available_devices()
            .map_err(|err| Error::BackendUnavailable(err.to_string()))?;

        let mut device_kinds = Vec::new();
        let mut all_precisions = Vec::new();
        let mut records = Vec::new();
        records
            .try_reserve_exact(available.len())
            .map_err(|_| Error::Backend("openvino device record allocation failed".to_string()))?;
        let mut execution_modes = Vec::new();

        for device in available {
            let device = device.to_owned();
            let name = device.as_ref().to_string();
            let Some(kind) = Self::device_kind_from_name(&name) else {
                continue;
            };
            let full_name = core
                .get_property(&device, &PropertyKey::DeviceFullName)
                .unwrap_or_else(|_| name.clone());
            let capabilities = core
                .get_property(&device, &PropertyKey::DeviceCapabilities)
                .unwrap_or_default();
            let supported_properties = core
                .get_property(&device, &PropertyKey::SupportedProperties)
                .unwrap_or_default();
            let range = core
                .get_property(&device, &PropertyKey::RangeForAsyncInferRequests)
                .unwrap_or_default();
            let async_capable = !range.is_empty()
                && (supported_properties.contains("RANGE_FOR_ASYNC_INFER_REQUESTS")
                    || supported_properties.contains("OPTIMAL_NUMBER_OF_INFER_REQUESTS"));
            // Prefer live OPTIMIZATION_CAPABILITIES only. Empty means "unknown"
            // and must not silently advertise a static precision matrix as fact.
            let verified_precisions = Self::parse_device_capabilities(&capabilities);

            all_precisions.extend(verified_precisions.iter().cloned());
            device_kinds.push(kind);
            let execution_mode = if async_capable {
                ExecutionMode::NativeAsync
            } else {
                ExecutionMode::BoundedSync
            };
            records.push(RuntimeDeviceCapabilities {
                kind,
                logical_id: name,
                runtime_name: full_name,
                async_capable,
                external_memory: false,
                remote_tensor: false,
                execution_mode,
                verified_precisions,
            });
            execution_modes.push(execution_mode);
        }

        let unique_precisions: HashSet<DataType> = all_precisions.into_iter().collect();
        let device_count = device_kinds.len();
        let execution_mode = execution_modes
            .into_iter()
            .max_by_key(|mode| match mode {
                ExecutionMode::NonInterruptible => 2,
                ExecutionMode::BoundedSync => 1,
                ExecutionMode::NativeAsync => 0,
            })
            .unwrap_or(ExecutionMode::BoundedSync);

        Ok(RuntimeCapabilities {
            sdk_version: Some(version.build_number),
            devices: device_kinds,
            device_count,
            precisions: unique_precisions.into_iter().collect(),
            deploy_modes: vec![DeployMode::Host],
            execution_mode,
            device_records: records,
        })
    }

    fn check_host_memory_contract(option: &RuntimeOption) -> Result<()> {
        if option.zero_copy {
            return Err(Error::CapabilityUnsupported(
                "zero_copy is unsupported for OpenVINO CPU/iGPU host memory contract".to_string(),
            ));
        }
        if option.external_stream.is_some() {
            return Err(Error::CapabilityUnsupported(
                "external/remote tensor is unsupported for OpenVINO CPU/iGPU".to_string(),
            ));
        }
        Ok(())
    }

    fn fill_input_tensors(&self, inputs: &[Tensor]) -> Result<Vec<OvTensor>> {
        let mut ov_inputs = Vec::new();
        ov_inputs
            .try_reserve_exact(inputs.len())
            .map_err(|_| Error::Backend("openvino input tensor allocation failed".to_string()))?;
        for (index, input) in inputs.iter().enumerate() {
            let info = &self.input_infos[index];
            let dims = info
                .shape
                .dims()
                .iter()
                .map(|dim| {
                    i64::try_from(*dim)
                        .map_err(|_| Error::InvalidOption("shape dimension overflow".to_string()))
                })
                .collect::<Result<Vec<_>>>()?;
            let ov_shape = dg_openvino_sys::Shape::new(&dims)
                .map_err(|err| Error::Backend(err.to_string()))?;
            let element_type = Self::map_element_type(info.dtype)?;
            let mut ov_tensor = OvTensor::new(element_type, &ov_shape)
                .map_err(|err| Error::Backend(err.to_string()))?;
            let raw = ov_tensor
                .get_raw_data_mut()
                .map_err(|err| Error::Backend(err.to_string()))?;
            let bytes = input.buffer().read_bytes()?;
            if bytes.len() != raw.len() {
                return Err(Error::Backend(format!(
                    "OpenVINO input size mismatch: expected {}, got {}",
                    raw.len(),
                    bytes.len()
                )));
            }
            let start = Instant::now();
            raw.copy_from_slice(&bytes);
            if let Some(metrics) = &self.metrics {
                let bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                let ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                metrics.record_host_copy(bytes, ns);
            }
            ov_inputs.push(ov_tensor);
        }
        Ok(ov_inputs)
    }

    fn copy_output_tensors(&self, request: &InferRequest) -> Result<Vec<Tensor>> {
        let device = dg_core::CpuDevice::new();
        let mut outputs = Vec::new();
        outputs
            .try_reserve_exact(self.output_infos.len())
            .map_err(|_| Error::Backend("openvino output tensor allocation failed".to_string()))?;
        for (index, output_info) in self.output_infos.iter().enumerate() {
            let ov_tensor = request
                .get_output_tensor_by_index(index)
                .map_err(|err| Error::Backend(err.to_string()))?;
            let output = output_info.allocate(&device)?;
            let bytes = ov_tensor
                .get_raw_data()
                .map_err(|err| Error::Backend(err.to_string()))?;
            if bytes.len() != output.buffer().len() {
                return Err(Error::Backend(format!(
                    "OpenVINO output size mismatch: expected {}, got {}",
                    output.buffer().len(),
                    bytes.len()
                )));
            }
            let start = Instant::now();
            output.buffer().write_from_slice(bytes)?;
            if let Some(metrics) = &self.metrics {
                let bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                let ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                metrics.record_host_copy(bytes, ns);
            }
            outputs.push(output);
        }
        Ok(outputs)
    }

    fn take_free_request(&mut self) -> Result<InferRequest> {
        if let Some(request) = self.free_requests.pop() {
            return Ok(request);
        }
        let total = self.free_requests.len() + self.in_flight.len();
        if total >= self.max_in_flight {
            return Err(Error::Backend(
                "OpenVINO request pool exhausted".to_string(),
            ));
        }
        let compiled_model = self
            .compiled_model
            .as_mut()
            .ok_or_else(|| Error::Backend("compiled model missing".to_string()))?;
        compiled_model
            .create_infer_request()
            .map_err(|err| Error::BackendUnavailable(err.to_string()))
    }

    fn return_request(&mut self, request: InferRequest) {
        self.free_requests.push(request);
    }
}

impl Default for OpenVINOBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InferBackend for OpenVINOBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::OpenVINO
    }

    fn is_async(&self) -> bool {
        self.async_capable
    }

    fn max_in_flight(&self) -> usize {
        self.max_in_flight
    }

    fn in_flight(&self) -> usize {
        self.in_flight.len()
    }

    fn attach_metrics(&mut self, metrics: Arc<BackendMetrics>) {
        self.metrics = Some(metrics);
    }

    fn probe_capabilities(&self) -> Result<RuntimeCapabilities> {
        if let Some(capabilities) = &self.capabilities {
            return Ok(capabilities.clone());
        }
        if let Some(core) = &self.core {
            // SAFETY: `core` is only accessed through `self` and this is a
            // private helper that borrows it immutably.
            return self.probe_live_capabilities(core);
        }
        backend_capabilities(BackendKind::OpenVINO)
            .map(RuntimeCapabilities::from_static)
            .ok_or_else(|| {
                Error::BackendUnavailable("OpenVINO capabilities unavailable".to_string())
            })
    }

    fn init(&mut self, option: &RuntimeOption) -> Result<()> {
        trace!("initializing OpenVINO backend");
        Self::check_host_memory_contract(option)?;
        let openvino_options = Self::openvino_options(option)?;
        self.max_in_flight = openvino_options.max_in_flight.clamp(1, 64);

        if let Some(precision) = option.precision {
            if !supports_precision(BackendKind::OpenVINO, precision) {
                return Err(Error::UnsupportedPrecision(precision));
            }
        }
        if let Some(device) = option.device {
            if !supports_device(BackendKind::OpenVINO, device) {
                return Err(Error::UnsupportedDevice(device));
            }
        }
        if let Some(deploy_mode) = option.deploy_mode {
            if !supports_deployment(BackendKind::OpenVINO, deploy_mode) {
                return Err(Error::UnsupportedDeployment(deploy_mode));
            }
        }

        let mut core = Core::new().map_err(|err| Error::BackendUnavailable(err.to_string()))?;
        let device_string = if let Some(kind) = option.device {
            Self::device_kind_to_string(kind)?
        } else {
            openvino_options.device.clone()
        };
        self.device = DeviceType::from(device_string.as_str()).to_owned();
        self.device_string = device_string;

        let capabilities = self.probe_live_capabilities(&core)?;
        let Some(target_kind) = Self::device_kind_from_name(&self.device_string) else {
            return Err(Error::CapabilityUnsupported(format!(
                "OpenVINO device {} has an unknown device kind",
                self.device_string
            )));
        };
        let Some(target_record) = capabilities
            .device_records
            .iter()
            .find(|record| record.kind == target_kind)
        else {
            return Err(Error::CapabilityUnsupported(format!(
                "OpenVINO device {} is not available; live devices: {:?}; sdk_version={}",
                self.device_string,
                capabilities.devices,
                capabilities.sdk_version.as_deref().unwrap_or("unknown")
            )));
        };

        self.async_capable = target_record.async_capable;

        if let Some(precision) = option.precision {
            let record = target_record;
            if record.verified_precisions.is_empty() {
                // Live property missing: do not invent device-verified precisions.
                // Fall back to the static backend matrix with an explicit warning.
                if !supports_precision(BackendKind::OpenVINO, precision) {
                    return Err(Error::UnsupportedPrecision(precision));
                }
                warn!(
                    device = %self.device_string,
                    ?precision,
                    "OpenVINO OPTIMIZATION_CAPABILITIES empty; precision accepted via static backend matrix only"
                );
            } else if !record.verified_precisions.contains(&precision) {
                return Err(Error::CapabilityUnsupported(format!(
                    "OpenVINO device {} does not support precision {:?}; verified={:?}; sdk_version={}",
                    self.device_string,
                    precision,
                    record.verified_precisions,
                    capabilities.sdk_version.as_deref().unwrap_or("unknown")
                )));
            }
        }
        self.capabilities = Some(capabilities);

        let model = Self::read_model(
            &mut core,
            &option.model_source,
            option.process_policy.resource_policy().max_model_bytes,
        )?;
        let compiled_model = core
            .compile_model(
                &model,
                DeviceType::from(self.device_string.as_str()).to_owned(),
            )
            .map_err(|err| {
                Error::BackendUnavailable(format!(
                    "OpenVINO failed to compile model for {}: {err}",
                    self.device_string
                ))
            })?;

        let input_count = compiled_model
            .get_input_size()
            .map_err(|err| Error::Backend(err.to_string()))?;
        let output_count = compiled_model
            .get_output_size()
            .map_err(|err| Error::Backend(err.to_string()))?;
        if input_count > MAX_OPENVINO_IO_COUNT {
            return Err(Error::Backend(format!(
                "openvino model declares {input_count} inputs, limit is {MAX_OPENVINO_IO_COUNT}"
            )));
        }
        if output_count > MAX_OPENVINO_IO_COUNT {
            return Err(Error::Backend(format!(
                "openvino model declares {output_count} outputs, limit is {MAX_OPENVINO_IO_COUNT}"
            )));
        }

        let mut input_infos = Vec::new();
        input_infos
            .try_reserve_exact(input_count)
            .map_err(|_| Error::Backend("openvino input info allocation failed".to_string()))?;
        for index in 0..input_count {
            let port = compiled_model
                .get_input_by_index(index)
                .map_err(|err| Error::Backend(err.to_string()))?;
            input_infos.push(Self::tensor_info_from_port(&port, self.device_kind())?);
        }

        let mut output_infos = Vec::new();
        output_infos
            .try_reserve_exact(output_count)
            .map_err(|_| Error::Backend("openvino output info allocation failed".to_string()))?;
        for index in 0..output_count {
            let port = compiled_model
                .get_output_by_index(index)
                .map_err(|err| Error::Backend(err.to_string()))?;
            output_infos.push(Self::tensor_info_from_port(&port, self.device_kind())?);
        }

        let mut compiled_model = compiled_model;
        for _ in 0..self.max_in_flight {
            self.free_requests.push(
                compiled_model
                    .create_infer_request()
                    .map_err(|err| Error::BackendUnavailable(err.to_string()))?,
            );
        }

        self.option = Some(option.clone());
        self.core = Some(core);
        self.model = Some(model);
        self.compiled_model = Some(compiled_model);
        self.input_infos = input_infos;
        self.output_infos = output_infos;
        Ok(())
    }

    fn reshape(&mut self, input_shapes: &[Shape]) -> Result<()> {
        if !self.in_flight.is_empty() {
            return Err(Error::Backend(
                "cannot reshape OpenVINO model while requests are in flight".to_string(),
            ));
        }
        if self.option.is_none() {
            return Err(Error::InvalidOption("backend not initialized".to_string()));
        }
        let Some(model) = self.model.as_ref() else {
            return Err(Error::InvalidOption("model not initialized".to_string()));
        };
        if input_shapes.len() != self.input_infos.len() {
            return Err(Error::InvalidOption(
                "reshape shape count must match model inputs".to_string(),
            ));
        }

        let mut partial_shapes = Vec::new();
        let mut input_ports = Vec::new();
        partial_shapes
            .try_reserve_exact(input_shapes.len())
            .map_err(|_| Error::Backend("openvino partial shape allocation failed".to_string()))?;
        input_ports
            .try_reserve_exact(input_shapes.len())
            .map_err(|_| Error::Backend("openvino input port allocation failed".to_string()))?;
        for (index, shape) in input_shapes.iter().enumerate() {
            let dims = shape
                .dims()
                .iter()
                .map(|dim| {
                    i64::try_from(*dim)
                        .map_err(|_| Error::InvalidOption("shape dimension overflow".to_string()))
                })
                .collect::<Result<Vec<_>>>()?;
            let partial_shape = PartialShape::new_static(
                i64::try_from(dims.len())
                    .map_err(|_| Error::InvalidOption("rank overflow".to_string()))?,
                &dims,
            )
            .map_err(|err| Error::Backend(err.to_string()))?;
            let port = model
                .get_input_by_index(index)
                .map_err(|err| Error::Backend(err.to_string()))?;
            input_ports.push(port);
            partial_shapes.push(partial_shape);
        }

        let pairs: Vec<(&Node, &PartialShape)> =
            input_ports.iter().zip(partial_shapes.iter()).collect();
        let Some(model) = self.model.as_mut() else {
            return Err(Error::InvalidOption("model not initialized".to_string()));
        };
        model
            .reshape_by_ports(&pairs)
            .map_err(|err| Error::Backend(err.to_string()))?;
        self.free_requests.clear();
        let compiled_model = self
            .core
            .as_mut()
            .ok_or_else(|| Error::InvalidOption("core missing".to_string()))?
            .compile_model(
                model,
                DeviceType::from(self.device_string.as_str()).to_owned(),
            )
            .map_err(|err| Error::BackendUnavailable(err.to_string()))?;

        let mut compiled_model = compiled_model;
        for _ in 0..self.max_in_flight {
            self.free_requests.push(
                compiled_model
                    .create_infer_request()
                    .map_err(|err| Error::BackendUnavailable(err.to_string()))?,
            );
        }

        self.compiled_model = Some(compiled_model);
        let existing_infos = self.input_infos.clone();
        let mut new_input_infos = Vec::new();
        new_input_infos
            .try_reserve_exact(input_shapes.len())
            .map_err(|_| {
                Error::Backend("openvino reshape input info allocation failed".to_string())
            })?;
        for (index, shape) in input_shapes.iter().enumerate() {
            let mut info = TensorInfo::new(shape.clone(), existing_infos[index].dtype)
                .with_layout(existing_infos[index].layout.unwrap_or(DataFormat::Auto));
            if let Some(name) = existing_infos[index].name.clone() {
                info = info.with_name(name);
            }
            new_input_infos.push(info);
        }
        self.input_infos = new_input_infos;

        let output_count = self
            .compiled_model
            .as_ref()
            .ok_or_else(|| Error::InvalidOption("compiled model missing".to_string()))?
            .get_output_size()
            .map_err(|err| Error::Backend(err.to_string()))?;
        if output_count > MAX_OPENVINO_IO_COUNT {
            return Err(Error::Backend(format!(
                "openvino reshaped model declares {output_count} outputs, limit is {MAX_OPENVINO_IO_COUNT}"
            )));
        }
        let mut output_infos = Vec::new();
        output_infos.try_reserve_exact(output_count).map_err(|_| {
            Error::Backend("openvino reshape output info allocation failed".to_string())
        })?;
        for index in 0..output_count {
            let port = self
                .compiled_model
                .as_ref()
                .ok_or_else(|| Error::InvalidOption("compiled model missing".to_string()))?
                .get_output_by_index(index)
                .map_err(|err| Error::Backend(err.to_string()))?;
            output_infos.push(Self::tensor_info_from_port(&port, self.device_kind())?);
        }
        self.output_infos = output_infos;
        Ok(())
    }

    fn input_count(&self) -> usize {
        self.input_infos.len()
    }

    fn output_count(&self) -> usize {
        self.output_infos.len()
    }

    fn input_info(&self, index: usize) -> Result<&TensorInfo> {
        self.input_infos
            .get(index)
            .ok_or_else(|| Error::InvalidOption(format!("input index out of range: {index}")))
    }

    fn output_info(&self, index: usize) -> Result<&TensorInfo> {
        self.output_infos
            .get(index)
            .ok_or_else(|| Error::InvalidOption(format!("output index out of range: {index}")))
    }

    fn input_infos(&self) -> &[TensorInfo] {
        &self.input_infos
    }

    fn output_infos(&self) -> &[TensorInfo] {
        &self.output_infos
    }

    fn run(&mut self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        self.submit(inputs, None, 0)?;
        let mut spin_count = 0;
        loop {
            match self.poll()? {
                InferPoll::Ready { outputs, .. } => return Ok(outputs),
                InferPoll::Pending => {
                    spin_count += 1;
                    if spin_count > 1_000_000 {
                        std::thread::sleep(Duration::from_micros(10));
                    } else {
                        std::thread::yield_now();
                    }
                }
                InferPoll::EndOfStream => {
                    return Err(Error::Backend("unexpected end of stream".to_string()))
                }
            }
        }
    }

    fn submit(
        &mut self,
        inputs: &[Tensor],
        _stream: Option<&dyn dg_core::Stream>,
        sequence: u64,
    ) -> Result<()> {
        if inputs.len() != self.input_infos.len() {
            return Err(Error::InvalidOption(
                "input count must match model inputs".to_string(),
            ));
        }
        let ov_inputs = self.fill_input_tensors(inputs)?;
        let mut request = self.take_free_request()?;
        for (index, ov_tensor) in ov_inputs.iter().enumerate() {
            request
                .set_input_tensor_by_index(index, ov_tensor)
                .map_err(|err| Error::Backend(err.to_string()))?;
        }
        request
            .infer_async()
            .map_err(|err| Error::Backend(err.to_string()))?;
        self.in_flight.push(InFlightRequest {
            request,
            inputs: ov_inputs,
            sequence,
            submit_time: Instant::now(),
        });
        Ok(())
    }

    fn poll(&mut self) -> Result<InferPoll> {
        let mut ready_index = None;
        for (index, in_flight) in self.in_flight.iter_mut().enumerate() {
            match in_flight.request.wait(0) {
                Ok(()) => {
                    ready_index = Some(index);
                    break;
                }
                Err(err) => {
                    use dg_openvino_sys::InferenceErrorKind;
                    if err.kind == InferenceErrorKind::ResultNotReady
                        || err.kind == InferenceErrorKind::RequestBusy
                    {
                        continue;
                    }
                    ready_index = Some(index);
                    break;
                }
            }
        }

        let Some(index) = ready_index else {
            return Ok(InferPoll::Pending);
        };

        let in_flight = self.in_flight.remove(index);
        if let Some(metrics) = &self.metrics {
            let ns = u64::try_from(in_flight.submit_time.elapsed().as_nanos()).unwrap_or(u64::MAX);
            metrics.record_infer_latency_ns(ns);
        }

        match in_flight.request.get_output_tensor_by_index(0) {
            Ok(_) => {
                let outputs = self.copy_output_tensors(&in_flight.request)?;
                self.return_request(in_flight.request);
                Ok(InferPoll::Ready {
                    outputs,
                    sequence: in_flight.sequence,
                })
            }
            Err(err) => {
                self.return_request(in_flight.request);
                Err(Error::Backend(format!(
                    "OpenVINO async inference failed for sequence {}: {err}",
                    in_flight.sequence
                )))
            }
        }
    }

    fn cancel(&mut self) -> dg_runtime::Result<dg_runtime::CancelReport> {
        use dg_runtime::CancelReport;
        let requested = u64::try_from(self.in_flight.len()).unwrap_or(u64::MAX);
        let mut completed = 0u64;
        let mut failed = 0u64;
        for mut in_flight in self.in_flight.drain(..) {
            match in_flight.request.cancel() {
                Ok(_) => completed += 1,
                Err(_) => failed += 1,
            }
        }
        Ok(CancelReport {
            requested,
            completed,
            abandoned: 0,
            failed,
            unsupported: 0,
        })
    }
}

fn create_openvino_backend() -> Box<dyn InferBackend> {
    Box::new(OpenVINOBackend::new())
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct OpenVinoConfig {
    device: Option<String>,
    max_in_flight: Option<usize>,
}

fn configure_openvino(config: BackendConfig) -> Result<RuntimeOption> {
    let params: OpenVinoConfig = config.parse_options("openvino")?;
    let top_level = config
        .device()
        .map(OpenVINOBackend::device_kind_to_string)
        .transpose()?;
    let device = if let Some(top) = top_level {
        if params.device.is_some() {
            warn!("OpenVINO options.device is deprecated; use top-level graph device instead");
        }
        if let Some(options_device) = &params.device {
            if *options_device != top {
                return Err(Error::InvalidOption(format!(
                    "graph device `{top}` conflicts with options.openvino.device `{options_device}`"
                )));
            }
        }
        top
    } else if let Some(device) = params.device {
        warn!("OpenVINO options.device is deprecated; use top-level graph device instead");
        device
    } else {
        "CPU".to_string()
    };
    let max_in_flight = params.max_in_flight.unwrap_or(2).clamp(1, 64);
    let model_source = config.require_model_file("OpenVINO")?;
    Ok(config.into_runtime_option(
        BackendKind::OpenVINO,
        model_source,
        BackendOptions::OpenVINO(OpenVINOOptions {
            device,
            max_in_flight,
        }),
    ))
}

inventory::submit! {
    BackendDescriptor {
        kind: BackendKind::OpenVINO,
        name: "openvino",
        create: create_openvino_backend,
        configure: configure_openvino,
    }
}
