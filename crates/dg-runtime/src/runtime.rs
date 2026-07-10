use crate::{
    backend::BackendKind, create_backend, supports_deployment, supports_device, supports_precision,
    Error, Result, RuntimeOption, TensorInfo,
};

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

/// Runtime wrapper around a concrete backend implementation.
pub struct Runtime {
    backend: Box<dyn crate::InferBackend>,
}

impl Runtime {
    pub fn new(option: RuntimeOption) -> Result<Self> {
        validate_runtime_option(&option)?;
        let mut backend = create_backend(option.backend)?;
        backend.init(&option)?;
        Ok(Self { backend })
    }

    pub fn from_backend(backend: Box<dyn crate::InferBackend>) -> Self {
        Self { backend }
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.backend.kind()
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

    pub fn backend_mut(&mut self) -> &mut dyn crate::InferBackend {
        self.backend.as_mut()
    }

    pub fn backend(&self) -> &dyn crate::InferBackend {
        self.backend.as_ref()
    }
}
