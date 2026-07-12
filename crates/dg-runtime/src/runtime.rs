use crate::{
    backend::BackendKind, create_backend, supports_deployment, supports_device, supports_precision,
    Error, Result, RuntimeCapabilities, RuntimeOption, TensorInfo,
};

/// Result of polling a submitted inference.
#[derive(Debug)]
pub enum InferPoll {
    Ready(Vec<dg_core::Tensor>),
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
        "backend={backend:?}, sdk_version={sdk_version}, device_count={}, available_devices={:?}, available_precisions={:?}, available_deploy_modes={:?}",
        capabilities.device_count,
        capabilities.devices,
        capabilities.precisions,
        capabilities.deploy_modes
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
    in_flight: Option<Vec<dg_core::Tensor>>,
}

impl Runtime {
    pub fn new(option: RuntimeOption) -> Result<Self> {
        validate_runtime_option(&option)?;
        let mut backend = create_backend(option.backend)?;
        backend.init(&option)?;
        let capabilities = backend.probe_capabilities()?;
        validate_probed_capabilities(option.backend, &option, &capabilities)?;
        Ok(Self {
            backend,
            capabilities,
            in_flight: None,
        })
    }

    pub fn from_backend(backend: Box<dyn crate::InferBackend>) -> Self {
        Self {
            backend,
            capabilities: RuntimeCapabilities {
                sdk_version: None,
                devices: Vec::new(),
                device_count: 0,
                precisions: Vec::new(),
                deploy_modes: Vec::new(),
            },
            in_flight: None,
        }
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

    /// Submit one inference and buffer its result for [`Runtime::poll`].
    ///
    /// Only one submission may be in flight at a time. Call `poll` to consume
    /// the buffered result before submitting another inference.
    pub fn submit(
        &mut self,
        inputs: &[dg_core::Tensor],
        stream: Option<&dyn dg_core::Stream>,
    ) -> Result<()> {
        if self.in_flight.is_some() {
            return Err(Error::Backend(
                "inference submission already in flight".to_string(),
            ));
        }
        self.in_flight = Some(self.backend.run_with_stream(inputs, stream)?);
        Ok(())
    }

    pub fn poll(&mut self) -> Result<InferPoll> {
        Ok(self
            .in_flight
            .take()
            .map_or(InferPoll::Pending, InferPoll::Ready))
    }

    pub fn backend_mut(&mut self) -> &mut dyn crate::InferBackend {
        self.backend.as_mut()
    }

    pub fn backend(&self) -> &dyn crate::InferBackend {
        self.backend.as_ref()
    }
}
