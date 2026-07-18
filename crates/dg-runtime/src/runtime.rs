use std::path::PathBuf;
use std::sync::Arc;

use dg_core::ResourcePolicy;

use crate::{
    backend::BackendKind, create_backend, supports_deployment, supports_device, supports_precision,
    BackendMetrics, CancelReport, Error, Result, RuntimeCapabilities, RuntimeOption, TensorInfo,
};

fn model_source_size(source: &crate::ModelSource) -> Result<usize> {
    match source {
        crate::ModelSource::File(path) => {
            let path: &PathBuf = path;
            Ok(std::fs::metadata(path)?.len() as usize)
        }
        crate::ModelSource::Bytes(bytes) => Ok(bytes.len()),
    }
}

/// Result of polling a submitted inference.
#[derive(Debug)]
pub enum InferPoll {
    Ready {
        outputs: Vec<dg_core::Tensor>,
        sequence: u64,
    },
    Pending,
    EndOfStream,
}

/// Validates common backend capabilities without initializing a device or model.
pub fn validate_runtime_option(option: &RuntimeOption) -> Result<()> {
    if let Some(precision) = option.precision {
        if !supports_precision(option.backend, precision) {
            return Err(Error::UnsupportedPrecision(precision));
        }
    }
    if let Some(device) = option.device {
        if !supports_device(option.backend, device) {
            return Err(Error::UnsupportedDevice(device));
        }
    }
    if let Some(deploy_mode) = option.deploy_mode {
        if !supports_deployment(option.backend, deploy_mode) {
            return Err(Error::UnsupportedDeployment(deploy_mode));
        }
    }
    Ok(())
}

fn validate_probed_capabilities(
    backend: BackendKind,
    option: &RuntimeOption,
    capabilities: &RuntimeCapabilities,
) -> Result<()> {
    let sdk_version = capabilities.sdk_version.as_deref().unwrap_or("unknown");
    let context = format!(
        "backend={backend:?}, sdk_version={sdk_version}, device_count={}, available_devices={:?}, available_precisions={:?}, available_deploy_modes={:?}, device_records={:?}",
        capabilities.device_count,
        capabilities.devices,
        capabilities.precisions,
        capabilities.deploy_modes,
        capabilities.device_records
    );
    if let Some(precision) = option.precision {
        if !capabilities.supports_precision(precision) {
            return Err(Error::CapabilityUnsupported(format!(
                "backend {backend:?} does not support requested precision {}; {context}",
                describe_precision(precision)
            )));
        }
    }
    if let Some(device) = option.device {
        if !capabilities.supports_device(device) {
            return Err(Error::CapabilityUnsupported(format!(
                "backend {backend:?} does not support requested device {device:?}; {context}"
            )));
        }
    }
    if let Some(deploy_mode) = option.deploy_mode {
        if !capabilities.supports_deployment(deploy_mode) {
            return Err(Error::CapabilityUnsupported(format!(
                "backend {backend:?} does not support requested deployment {deploy_mode:?}; {context}"
            )));
        }
    }
    Ok(())
}

fn describe_precision(precision: dg_core::DataType) -> String {
    if precision == dg_core::DataType::F4 {
        "F4".to_string()
    } else if precision == dg_core::DataType::F8 {
        "F8".to_string()
    } else if precision == dg_core::DataType::F16 {
        "F16".to_string()
    } else if precision == dg_core::DataType::BF16 {
        "BF16".to_string()
    } else if precision == dg_core::DataType::F32 {
        "F32".to_string()
    } else if precision == dg_core::DataType::F64 {
        "F64".to_string()
    } else if precision == dg_core::DataType::U8 {
        "U8".to_string()
    } else if precision == dg_core::DataType::U16 {
        "U16".to_string()
    } else if precision == dg_core::DataType::I4 {
        "I4".to_string()
    } else if precision == dg_core::DataType::I8 {
        "I8".to_string()
    } else if precision == dg_core::DataType::I16 {
        "I16".to_string()
    } else {
        format!("{precision:?}")
    }
}

/// Runtime wrapper around a concrete backend implementation.
pub struct Runtime {
    backend: Box<dyn crate::InferBackend>,
    capabilities: RuntimeCapabilities,
    next_sequence: u64,
    sync_result: Option<(u64, Vec<dg_core::Tensor>)>,
    metrics: Arc<BackendMetrics>,
    max_in_flight: usize,
    policy: ResourcePolicy,
}

impl Runtime {
    pub fn new(option: RuntimeOption) -> Result<Self> {
        Self::new_with_policy(option, ResourcePolicy::default())
    }

    pub fn new_with_policy(option: RuntimeOption, policy: ResourcePolicy) -> Result<Self> {
        Self::new_with_policy_and_metrics(option, policy, Arc::new(BackendMetrics::default()))
    }

    pub fn new_with_metrics(option: RuntimeOption, metrics: Arc<BackendMetrics>) -> Result<Self> {
        Self::new_with_policy_and_metrics(option, ResourcePolicy::default(), metrics)
    }

    pub fn new_with_policy_and_metrics(
        option: RuntimeOption,
        policy: ResourcePolicy,
        metrics: Arc<BackendMetrics>,
    ) -> Result<Self> {
        validate_runtime_option(&option)?;
        policy.check_model_bytes(model_source_size(&option.model_source)?)?;
        let mut backend = create_backend(option.backend)?;
        backend.attach_metrics(Arc::clone(&metrics));
        backend.init(&option)?;
        let capabilities = backend.probe_capabilities()?;
        validate_probed_capabilities(option.backend, &option, &capabilities)?;
        let max_in_flight = backend.max_in_flight().max(1);
        Ok(Self {
            backend,
            capabilities,
            next_sequence: 0,
            sync_result: None,
            metrics,
            max_in_flight,
            policy,
        })
    }

    pub fn from_backend(backend: Box<dyn crate::InferBackend>) -> Self {
        Self::from_backend_with_metrics(backend, Arc::new(BackendMetrics::default()))
    }

    pub fn from_backend_with_metrics(
        mut backend: Box<dyn crate::InferBackend>,
        metrics: Arc<BackendMetrics>,
    ) -> Self {
        backend.attach_metrics(Arc::clone(&metrics));
        let max_in_flight = backend.max_in_flight().max(1);
        Self {
            backend,
            capabilities: RuntimeCapabilities {
                sdk_version: None,
                devices: Vec::new(),
                device_count: 0,
                precisions: Vec::new(),
                deploy_modes: Vec::new(),
                device_records: Vec::new(),
            },
            next_sequence: 0,
            sync_result: None,
            metrics,
            max_in_flight,
            policy: ResourcePolicy::default(),
        }
    }

    pub fn policy(&self) -> &ResourcePolicy {
        &self.policy
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.backend.kind()
    }

    /// Returns the capabilities observed during runtime initialization.
    pub fn capabilities(&self) -> &RuntimeCapabilities {
        &self.capabilities
    }

    pub fn input_infos(&self) -> &[TensorInfo] {
        self.backend.input_infos()
    }

    pub fn input_count(&self) -> usize {
        self.backend.input_count()
    }

    pub fn output_infos(&self) -> &[TensorInfo] {
        self.backend.output_infos()
    }

    pub fn output_count(&self) -> usize {
        self.backend.output_count()
    }

    pub fn reshape(&mut self, input_shapes: &[dg_core::Shape]) -> Result<()> {
        self.backend.reshape(input_shapes)
    }

    pub fn run(&mut self, inputs: &[dg_core::Tensor]) -> Result<Vec<dg_core::Tensor>> {
        self.backend.run(inputs)
    }

    pub fn run_with_stream(
        &mut self,
        inputs: &[dg_core::Tensor],
        stream: Option<&dyn dg_core::Stream>,
    ) -> Result<Vec<dg_core::Tensor>> {
        self.backend.run_with_stream(inputs, stream)
    }

    /// Returns the live metrics owned by this runtime (and usually shared with
    /// the backend).
    pub fn metrics(&self) -> &BackendMetrics {
        &self.metrics
    }

    /// Returns the shared metrics handle so callers can attach it to graph
    /// element metrics.
    pub fn metrics_arc(&self) -> &Arc<BackendMetrics> {
        &self.metrics
    }

    /// Current number of in-flight submissions.
    pub fn in_flight(&self) -> usize {
        self.metrics.in_flight() as usize
    }

    /// Maximum number of in-flight submissions this runtime allows.
    pub fn max_in_flight(&self) -> usize {
        self.max_in_flight
    }

    /// Returns true when the wrapped backend is truly asynchronous.
    pub fn is_async(&self) -> bool {
        self.backend.is_async()
    }

    /// Submit one inference without blocking for its result.
    ///
    /// Returns a sequence number that will be echoed by [`Runtime::poll`] so
    /// that out-of-order completion preserves per-submission metadata.
    pub fn submit(
        &mut self,
        inputs: &[dg_core::Tensor],
        stream: Option<&dyn dg_core::Stream>,
    ) -> Result<u64> {
        let current_in_flight = if self.backend.is_async() {
            self.backend.in_flight()
        } else {
            self.sync_result.is_some() as usize
        };
        if current_in_flight >= self.max_in_flight {
            return Err(Error::Backend(
                "inference in-flight limit reached".to_string(),
            ));
        }
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(Error::SequenceExhausted)?;
        if self.backend.is_async() {
            self.backend.submit(inputs, stream, sequence)?;
        } else {
            let outputs = self.backend.run_with_stream(inputs, stream)?;
            self.sync_result = Some((sequence, outputs));
        }
        self.metrics.record_submission();
        Ok(sequence)
    }

    pub fn poll(&mut self) -> Result<InferPoll> {
        if self.backend.is_async() {
            match self.backend.poll() {
                Ok(InferPoll::Pending) => {
                    self.metrics.record_poll_pending();
                    Ok(InferPoll::Pending)
                }
                Ok(InferPoll::Ready { outputs, sequence }) => {
                    self.metrics.finish_in_flight();
                    Ok(InferPoll::Ready { outputs, sequence })
                }
                Ok(InferPoll::EndOfStream) => Ok(InferPoll::EndOfStream),
                Err(err) => {
                    self.metrics.finish_in_flight();
                    self.metrics.record_backend_error();
                    Err(err)
                }
            }
        } else if let Some((sequence, outputs)) = self.sync_result.take() {
            self.metrics.finish_in_flight();
            Ok(InferPoll::Ready { outputs, sequence })
        } else {
            Ok(InferPoll::Pending)
        }
    }

    pub fn cancel(&mut self) -> Result<CancelReport> {
        if self.backend.is_async() {
            let report = self.backend.cancel()?;
            let released = report.completed.saturating_add(report.abandoned);
            for _ in 0..released {
                self.metrics.finish_in_flight();
            }
            let failed = report.requested.saturating_sub(released);
            for _ in 0..failed {
                self.metrics.record_backend_error();
            }
            self.metrics.record_cancel();
            Ok(report)
        } else if self.sync_result.take().is_some() {
            self.metrics.finish_in_flight();
            self.metrics.record_cancel();
            Ok(CancelReport {
                requested: 1,
                completed: 1,
                abandoned: 0,
            })
        } else {
            Ok(CancelReport::default())
        }
    }

    pub fn backend_mut(&mut self) -> &mut dyn crate::InferBackend {
        self.backend.as_mut()
    }

    pub fn backend(&self) -> &dyn crate::InferBackend {
        self.backend.as_ref()
    }
}
