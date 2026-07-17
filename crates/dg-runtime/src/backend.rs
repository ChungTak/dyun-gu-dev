use std::sync::Arc;

use crate::{
    backend_capabilities, BackendConfig, BackendMetrics, Error, InferPoll, Result,
    RuntimeCapabilities, RuntimeOption, TensorInfo,
};

/// Backend families available to the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackendKind {
    Mock,
    OpenVINO,
    Rknn,
    TensorRt,
    Sophon,
}

/// A backend implementation.
pub trait InferBackend: Send {
    fn kind(&self) -> BackendKind;
    fn init(&mut self, option: &RuntimeOption) -> Result<()>;
    fn reshape(&mut self, input_shapes: &[dg_core::Shape]) -> Result<()>;
    fn input_count(&self) -> usize;
    fn output_count(&self) -> usize;
    fn input_info(&self, index: usize) -> Result<&TensorInfo>;
    fn output_info(&self, index: usize) -> Result<&TensorInfo>;
    fn input_infos(&self) -> &[TensorInfo];
    fn output_infos(&self) -> &[TensorInfo];
    fn run(&mut self, inputs: &[dg_core::Tensor]) -> Result<Vec<dg_core::Tensor>>;

    fn run_with_stream(
        &mut self,
        inputs: &[dg_core::Tensor],
        _stream: Option<&dyn dg_core::Stream>,
    ) -> Result<Vec<dg_core::Tensor>> {
        self.run(inputs)
    }

    /// Returns true when this backend uses a real submit/poll contract.
    ///
    /// Synchronous backends may omit [`InferBackend::submit`] and
    /// [`InferBackend::poll`].
    fn is_async(&self) -> bool {
        false
    }

    /// Maximum number of in-flight submissions this backend allows.
    fn max_in_flight(&self) -> usize {
        1
    }

    /// Number of in-flight submissions currently held by the backend.
    fn in_flight(&self) -> usize {
        0
    }

    /// Submits one inference without waiting for completion.
    ///
    /// `sequence` is a caller-assigned identifier returned by [`poll`] so that
    /// out-of-order completion can be matched back to the original submission.
    fn submit(
        &mut self,
        _inputs: &[dg_core::Tensor],
        _stream: Option<&dyn dg_core::Stream>,
        _sequence: u64,
    ) -> Result<()> {
        Err(Error::Backend(
            "backend does not support async submit".to_string(),
        ))
    }

    /// Polls the backend for any completed submission.
    fn poll(&mut self) -> Result<InferPoll> {
        Ok(InferPoll::Pending)
    }

    /// Attaches the runtime-owned metrics handle to this backend.
    fn attach_metrics(&mut self, _metrics: Arc<BackendMetrics>) {}

    /// Cancels all in-flight submissions.
    fn cancel(&mut self) {}

    /// Probes backend capabilities after initialization.
    fn probe_capabilities(&self) -> Result<RuntimeCapabilities> {
        Ok(backend_capabilities(self.kind())
            .map(RuntimeCapabilities::from_static)
            .unwrap_or_else(|| RuntimeCapabilities {
                sdk_version: None,
                devices: Vec::new(),
                device_count: 0,
                precisions: Vec::new(),
                deploy_modes: Vec::new(),
                device_records: Vec::new(),
            }))
    }
}

/// Static backend descriptor used by the registry.
pub struct BackendDescriptor {
    pub kind: BackendKind,
    pub name: &'static str,
    pub create: fn() -> Box<dyn InferBackend>,
    pub configure: fn(BackendConfig) -> Result<RuntimeOption>,
}

/// Discover registered backends.
pub fn registered_backends() -> Vec<&'static BackendDescriptor> {
    inventory::iter::<BackendDescriptor>.into_iter().collect()
}

/// Construct a backend by kind.
pub fn create_backend(kind: BackendKind) -> Result<Box<dyn InferBackend>> {
    registered_backends()
        .into_iter()
        .find(|descriptor| descriptor.kind == kind)
        .map(|descriptor| (descriptor.create)())
        .ok_or(Error::UnsupportedBackend(kind))
}

/// Build runtime options through the backend registered under `name`.
pub fn configure_backend(name: &str, config: BackendConfig) -> Result<RuntimeOption> {
    registered_backends()
        .into_iter()
        .find(|descriptor| descriptor.name == name)
        .ok_or_else(|| Error::UnsupportedBackendName(name.to_string()))
        .and_then(|descriptor| (descriptor.configure)(config))
}
