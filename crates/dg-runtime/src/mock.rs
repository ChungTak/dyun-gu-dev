use std::sync::Arc;
use std::time::{Duration, Instant};

use dg_core::{DataFormat, DataType, Shape, Tensor, TypeCode};
use serde::Deserialize;
use tracing::trace;

/// Maximum artificial delay the mock backend will honor. This prevents a
/// caller from forcing an unbounded `thread::sleep` or an `Instant`
/// overflow by passing a huge `delay_ms` value.
const MAX_MOCK_DELAY_MS: u64 = 3_600_000;
const MAX_MOCK_DELAY: Duration = Duration::from_millis(MAX_MOCK_DELAY_MS);

use crate::{
    backend::{BackendDescriptor, BackendKind, CancelReport, InferBackend},
    capabilities::{supports_deployment, supports_device, supports_precision},
    error::{Error, Result},
    metrics::BackendMetrics,
    option::{BackendConfig, BackendOptions, ModelSource, RuntimeOption},
    InferPoll, RuntimeCapabilities, RuntimeDeviceCapabilities, TensorInfo,
};

/// Mock backend options for CI and integration tests.
#[derive(Clone, Debug, PartialEq)]
pub struct MockOptions {
    pub input_infos: Vec<TensorInfo>,
    pub output_infos: Vec<TensorInfo>,
    pub echo_inputs: bool,
    pub fill_value: u8,
    pub delay: Option<Duration>,
    pub delays: Vec<Duration>,
    pub max_in_flight: usize,
}

impl Default for MockOptions {
    fn default() -> Self {
        let shape = Shape::new([1, 3, 224, 224]);
        let info = TensorInfo::new(shape, DataType::F32).with_layout(DataFormat::NCHW);
        Self {
            input_infos: vec![info.clone()],
            output_infos: vec![info],
            echo_inputs: true,
            fill_value: 0,
            delay: None,
            delays: Vec::new(),
            max_in_flight: 1,
        }
    }
}

/// One submitted inference that has not yet completed.
struct DelayedInference {
    sequence: u64,
    submitted: Instant,
    finish: Instant,
    inputs: Vec<Tensor>,
}

/// Pure Rust backend used in CI.
pub struct MockBackend {
    options: MockOptions,
    input_infos: Vec<TensorInfo>,
    output_infos: Vec<TensorInfo>,
    pending: Vec<DelayedInference>,
    metrics: Option<Arc<BackendMetrics>>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            options: MockOptions::default(),
            input_infos: Vec::new(),
            output_infos: Vec::new(),
            pending: Vec::new(),
            metrics: None,
        }
    }

    fn produce_outputs(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        if inputs.len() != self.input_infos.len() {
            return Err(Error::InvalidOption(
                "mock run input count must match configured inputs".to_string(),
            ));
        }

        let device = dg_core::CpuDevice::new();
        let mut outputs = Vec::with_capacity(self.output_infos.len());
        for (index, output_info) in self.output_infos.iter().enumerate() {
            let output = output_info.allocate(&device)?;
            if self.options.echo_inputs && index < inputs.len() {
                let bytes = inputs[index].buffer().read_bytes()?;
                if bytes.len() != output.buffer().len() {
                    return Err(Error::Backend(
                        "mock backend echo output size mismatch".to_string(),
                    ));
                }
                output.buffer().write_from_slice(&bytes)?;
            } else {
                let bytes = vec![self.options.fill_value; output.buffer().len()];
                output.buffer().write_from_slice(&bytes)?;
            }
            outputs.push(output);
        }
        Ok(outputs)
    }
}

impl InferBackend for MockBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Mock
    }

    fn init(&mut self, option: &RuntimeOption) -> Result<()> {
        trace!("initializing mock backend");
        if let Some(precision) = option.precision {
            if !supports_precision(BackendKind::Mock, precision) {
                return Err(Error::UnsupportedPrecision(precision));
            }
        }
        if let Some(device) = option.device {
            if !supports_device(BackendKind::Mock, device) {
                return Err(Error::UnsupportedDevice(device));
            }
        }
        if let Some(deploy_mode) = option.deploy_mode {
            if !supports_deployment(BackendKind::Mock, deploy_mode) {
                return Err(Error::UnsupportedDeployment(deploy_mode));
            }
        }
        let BackendOptions::Mock(options) = &option.backend_options else {
            return Err(Error::InvalidOption(
                "mock backend requires Mock backend options".to_string(),
            ));
        };
        self.options = options.clone();
        self.input_infos = self.options.input_infos.clone();
        self.output_infos = if self.options.output_infos.is_empty() {
            self.input_infos.clone()
        } else {
            self.options.output_infos.clone()
        };
        Ok(())
    }

    fn reshape(&mut self, input_shapes: &[Shape]) -> Result<()> {
        if input_shapes.len() != self.input_infos.len() {
            return Err(Error::InvalidOption(
                "mock reshape shape count must match input count".to_string(),
            ));
        }
        for (info, shape) in self.input_infos.iter_mut().zip(input_shapes.iter()) {
            info.shape = shape.clone();
        }
        if self.output_infos.len() == self.input_infos.len() && self.options.echo_inputs {
            self.output_infos = self.input_infos.clone();
        }
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
        let start = Instant::now();
        let outputs = if self.is_async() {
            self.submit(inputs, None, 0)?;
            loop {
                match self.poll()? {
                    InferPoll::Ready { outputs, .. } => break outputs,
                    InferPoll::Pending => {
                        std::thread::sleep(self.options.delay.unwrap_or_default() / 10);
                    }
                    InferPoll::EndOfStream => {
                        return Err(Error::Backend("unexpected end of stream".to_string()));
                    }
                }
            }
        } else {
            self.produce_outputs(inputs)?
        };
        if let Some(metrics) = &self.metrics {
            let elapsed = start.elapsed().as_nanos();
            metrics.record_infer_latency_ns(u64::try_from(elapsed).unwrap_or(u64::MAX));
        }
        Ok(outputs)
    }

    fn is_async(&self) -> bool {
        self.options.delay.is_some() || !self.options.delays.is_empty()
    }

    fn max_in_flight(&self) -> usize {
        self.options.max_in_flight.clamp(1, 64)
    }

    fn in_flight(&self) -> usize {
        self.pending.len()
    }

    fn submit(
        &mut self,
        inputs: &[Tensor],
        _stream: Option<&dyn dg_core::Stream>,
        sequence: u64,
    ) -> Result<()> {
        if self.in_flight() >= self.max_in_flight() {
            return Err(Error::Backend(
                "mock backend in-flight limit reached".to_string(),
            ));
        }
        let delay = if self.options.delays.is_empty() {
            self.options.delay
        } else {
            Some(self.options.delays[self.pending.len() % self.options.delays.len()])
        };
        let Some(delay) = delay else {
            return Err(Error::Backend(
                "mock backend is not configured for async operation".to_string(),
            ));
        };
        let delay = std::cmp::min(delay, MAX_MOCK_DELAY);
        let now = Instant::now();
        let finish = now
            .checked_add(delay)
            .ok_or_else(|| Error::Backend("mock delay exceeds representable time".to_string()))?;
        self.pending.push(DelayedInference {
            sequence,
            submitted: now,
            finish,
            inputs: inputs.to_vec(),
        });
        Ok(())
    }

    fn poll(&mut self) -> Result<InferPoll> {
        let now = Instant::now();
        let mut ready: Option<(usize, Instant)> = None;
        for (index, pending) in self.pending.iter().enumerate() {
            if pending.finish <= now
                && ready
                    .as_ref()
                    .is_none_or(|(_, finish)| pending.finish < *finish)
            {
                ready = Some((index, pending.finish));
            }
        }
        if let Some((index, _)) = ready {
            let pending = self.pending.remove(index);
            let outputs = self.produce_outputs(&pending.inputs)?;
            if let Some(metrics) = &self.metrics {
                let elapsed = pending.submitted.elapsed().as_nanos();
                metrics.record_infer_latency_ns(u64::try_from(elapsed).unwrap_or(u64::MAX));
            }
            // Do not call finish_in_flight here: Runtime::poll owns the
            // in_flight accounting for both sync and async backends.
            return Ok(InferPoll::Ready {
                outputs,
                sequence: pending.sequence,
            });
        }
        Ok(InferPoll::Pending)
    }

    fn cancel(&mut self) -> Result<CancelReport> {
        let requested = u64::try_from(self.pending.len()).unwrap_or(u64::MAX);
        self.pending.clear();
        Ok(CancelReport {
            requested,
            completed: requested,
            abandoned: 0,
        })
    }

    fn attach_metrics(&mut self, metrics: Arc<BackendMetrics>) {
        self.metrics = Some(metrics);
    }

    fn probe_capabilities(&self) -> Result<RuntimeCapabilities> {
        Ok(RuntimeCapabilities {
            sdk_version: Some("mock-1".to_string()),
            devices: vec![dg_core::DeviceKind::Cpu],
            device_count: 1,
            precisions: vec![DataType::F32],
            deploy_modes: vec![dg_core::DeployMode::Host],
            device_records: vec![RuntimeDeviceCapabilities {
                kind: dg_core::DeviceKind::Cpu,
                logical_id: "mock-0".to_string(),
                runtime_name: "mock".to_string(),
                async_capable: self.is_async(),
                external_memory: false,
                remote_tensor: false,
                verified_precisions: vec![DataType::F32],
            }],
        })
    }
}

fn create_mock_backend() -> Box<dyn InferBackend> {
    Box::new(MockBackend::new())
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MockConfig {
    shape: Option<Vec<usize>>,
    output_shape: Option<Vec<usize>>,
    dtype: Option<String>,
    output_dtype: Option<String>,
    layout: Option<String>,
    echo_inputs: Option<bool>,
    fill_value: Option<u8>,
    delay_ms: Option<u64>,
    delays_ms: Option<Vec<u64>>,
    max_in_flight: Option<usize>,
}

fn configure_mock(config: BackendConfig) -> Result<RuntimeOption> {
    let params: MockConfig = config.parse_options("mock")?;
    let shape = Shape::new(params.shape.unwrap_or_else(|| vec![1, 4]));
    let output_shape = Shape::new(params.output_shape.unwrap_or_else(|| shape.dims().to_vec()));
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
    let options = BackendOptions::Mock(MockOptions {
        input_infos: vec![TensorInfo::new(shape, dtype).with_layout(layout)],
        output_infos: vec![TensorInfo::new(output_shape, output_dtype).with_layout(layout)],
        echo_inputs: params.echo_inputs.unwrap_or(true),
        fill_value: params.fill_value.unwrap_or(0),
        delay: params.delay_ms.map(parse_delay_ms).transpose()?,
        delays: params
            .delays_ms
            .unwrap_or_default()
            .into_iter()
            .map(parse_delay_ms)
            .collect::<Result<Vec<_>>>()?,
        max_in_flight: params.max_in_flight.unwrap_or(1).clamp(1, 64),
    });
    let model_source = config.model().map_or_else(
        || ModelSource::Bytes(Vec::new()),
        |path| ModelSource::File(path.to_path_buf()),
    );
    Ok(config.into_runtime_option(BackendKind::Mock, model_source, options))
}

fn parse_delay_ms(ms: u64) -> Result<Duration> {
    if ms > MAX_MOCK_DELAY_MS {
        return Err(Error::InvalidOption(format!(
            "mock delay_ms {ms} exceeds maximum {MAX_MOCK_DELAY_MS}"
        )));
    }
    Ok(Duration::from_millis(ms))
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
        _ => Err(Error::InvalidOption(format!(
            "unsupported mock precision: {value}"
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
        _ => Err(Error::InvalidOption(format!(
            "unsupported mock layout: {value}"
        ))),
    }
}

inventory::submit! {
    BackendDescriptor {
        kind: BackendKind::Mock,
        name: "mock",
        create: create_mock_backend,
        configure: configure_mock,
    }
}
