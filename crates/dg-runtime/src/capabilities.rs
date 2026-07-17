use dg_core::{DataType, DeployMode, DeviceKind};

use crate::backend::BackendKind;

/// Static capability record for a backend family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub kind: BackendKind,
    pub precisions: &'static [DataType],
    pub devices: &'static [DeviceKind],
    pub deploy_modes: &'static [DeployMode],
}

impl BackendCapabilities {
    pub fn supports_precision(&self, precision: DataType) -> bool {
        self.precisions.contains(&precision)
    }

    pub fn supports_device(&self, device: DeviceKind) -> bool {
        self.devices.contains(&device)
    }

    pub fn supports_deployment(&self, deploy_mode: DeployMode) -> bool {
        self.deploy_modes.contains(&deploy_mode)
    }
}

/// Per-device capabilities observed from a live backend probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDeviceCapabilities {
    pub kind: DeviceKind,
    pub logical_id: String,
    pub runtime_name: String,
    pub async_capable: bool,
    pub external_memory: bool,
    pub remote_tensor: bool,
    pub verified_precisions: Vec<DataType>,
}

/// Capabilities observed from a live backend or its no-hardware fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub sdk_version: Option<String>,
    pub devices: Vec<DeviceKind>,
    pub device_count: usize,
    pub precisions: Vec<DataType>,
    pub deploy_modes: Vec<DeployMode>,
    pub device_records: Vec<RuntimeDeviceCapabilities>,
}

impl RuntimeCapabilities {
    /// Converts static no-hardware capabilities into an owned runtime record.
    pub fn from_static(capabilities: &BackendCapabilities) -> Self {
        Self {
            sdk_version: None,
            devices: capabilities.devices.to_vec(),
            device_count: capabilities.devices.len(),
            precisions: capabilities.precisions.to_vec(),
            deploy_modes: capabilities.deploy_modes.to_vec(),
            device_records: Vec::new(),
        }
    }

    /// Returns whether a precision is available at runtime.
    pub fn supports_precision(&self, precision: DataType) -> bool {
        self.precisions.contains(&precision)
    }

    /// Returns whether a device is available at runtime.
    pub fn supports_device(&self, device: DeviceKind) -> bool {
        self.devices.contains(&device)
    }

    /// Returns whether a deployment mode is available at runtime.
    pub fn supports_deployment(&self, deploy_mode: DeployMode) -> bool {
        self.deploy_modes.contains(&deploy_mode)
    }

    /// Looks up live capabilities for a specific device kind and logical id.
    pub fn device_capabilities(
        &self,
        kind: DeviceKind,
        logical_id: Option<&str>,
    ) -> Option<&RuntimeDeviceCapabilities> {
        self.device_records.iter().find(|record| {
            record.kind == kind && logical_id.is_none_or(|id| record.logical_id == id)
        })
    }
}

const MOCK_PRECISIONS: &[DataType] = &[
    DataType::F32,
    DataType::F16,
    DataType::BF16,
    DataType::F8,
    DataType::F4,
    DataType::U8,
    DataType::U16,
    DataType::new(dg_core::TypeCode::Uint, 32, 1),
    DataType::new(dg_core::TypeCode::Uint, 64, 1),
    DataType::I4,
    DataType::I8,
    DataType::I16,
    DataType::new(dg_core::TypeCode::Int, 32, 1),
    DataType::new(dg_core::TypeCode::Int, 64, 1),
];

const OPENVINO_PRECISIONS: &[DataType] = &[
    DataType::F32,
    DataType::F16,
    DataType::BF16,
    DataType::U8,
    DataType::I8,
    DataType::U16,
    DataType::I16,
    DataType::new(dg_core::TypeCode::Uint, 32, 1),
    DataType::new(dg_core::TypeCode::Int, 32, 1),
    DataType::new(dg_core::TypeCode::Uint, 64, 1),
    DataType::new(dg_core::TypeCode::Int, 64, 1),
];

const RKNN_PRECISIONS: &[DataType] = &[
    DataType::F32,
    DataType::F16,
    DataType::U8,
    DataType::I8,
    DataType::U16,
    DataType::I16,
    DataType::new(dg_core::TypeCode::Uint, 32, 1),
    DataType::new(dg_core::TypeCode::Int, 32, 1),
];

const TENSORRT_PRECISIONS: &[DataType] = &[
    DataType::F32,
    DataType::F16,
    DataType::U8,
    DataType::I8,
    DataType::new(dg_core::TypeCode::Uint, 32, 1),
    DataType::new(dg_core::TypeCode::Int, 32, 1),
];

const SOPHON_PRECISIONS: &[DataType] = &[
    DataType::F32,
    DataType::F16,
    DataType::U8,
    DataType::I8,
    DataType::new(dg_core::TypeCode::Uint, 32, 1),
    DataType::new(dg_core::TypeCode::Int, 32, 1),
];

const MOCK_DEVICES: &[DeviceKind] = &[
    DeviceKind::Cpu,
    DeviceKind::IntelGpu,
    DeviceKind::IntelNpu,
    DeviceKind::CudaGpu,
    DeviceKind::RknnNpu,
    DeviceKind::SophonTpu,
];
const OPENVINO_DEVICES: &[DeviceKind] =
    &[DeviceKind::Cpu, DeviceKind::IntelGpu, DeviceKind::IntelNpu];
const RKNN_DEVICES: &[DeviceKind] = &[DeviceKind::RknnNpu];
const TENSORRT_DEVICES: &[DeviceKind] = &[DeviceKind::CudaGpu];
const SOPHON_DEVICES: &[DeviceKind] = &[DeviceKind::SophonTpu];

const MOCK_DEPLOYS: &[DeployMode] = &[DeployMode::SoC, DeployMode::Host];
const OPENVINO_DEPLOYS: &[DeployMode] = &[DeployMode::Host];
const RKNN_DEPLOYS: &[DeployMode] = &[DeployMode::SoC];
const TENSORRT_DEPLOYS: &[DeployMode] = &[DeployMode::Host];
const SOPHON_DEPLOYS: &[DeployMode] = &[DeployMode::SoC, DeployMode::Host];

const MOCK_CAPS: BackendCapabilities = BackendCapabilities {
    kind: BackendKind::Mock,
    precisions: MOCK_PRECISIONS,
    devices: MOCK_DEVICES,
    deploy_modes: MOCK_DEPLOYS,
};
const OPENVINO_CAPS: BackendCapabilities = BackendCapabilities {
    kind: BackendKind::OpenVINO,
    precisions: OPENVINO_PRECISIONS,
    devices: OPENVINO_DEVICES,
    deploy_modes: OPENVINO_DEPLOYS,
};
const RKNN_CAPS: BackendCapabilities = BackendCapabilities {
    kind: BackendKind::Rknn,
    precisions: RKNN_PRECISIONS,
    devices: RKNN_DEVICES,
    deploy_modes: RKNN_DEPLOYS,
};
const TENSORRT_CAPS: BackendCapabilities = BackendCapabilities {
    kind: BackendKind::TensorRt,
    precisions: TENSORRT_PRECISIONS,
    devices: TENSORRT_DEVICES,
    deploy_modes: TENSORRT_DEPLOYS,
};
const SOPHON_CAPS: BackendCapabilities = BackendCapabilities {
    kind: BackendKind::Sophon,
    precisions: SOPHON_PRECISIONS,
    devices: SOPHON_DEVICES,
    deploy_modes: SOPHON_DEPLOYS,
};

/// Returns the static capability record for a backend kind.
pub fn backend_capabilities(kind: BackendKind) -> Option<&'static BackendCapabilities> {
    match kind {
        BackendKind::Mock => Some(&MOCK_CAPS),
        BackendKind::OpenVINO => Some(&OPENVINO_CAPS),
        BackendKind::Rknn => Some(&RKNN_CAPS),
        BackendKind::TensorRt => Some(&TENSORRT_CAPS),
        BackendKind::Sophon => Some(&SOPHON_CAPS),
    }
}

pub fn supports_precision(kind: BackendKind, precision: DataType) -> bool {
    backend_capabilities(kind).is_some_and(|caps| caps.supports_precision(precision))
}

pub fn supports_device(kind: BackendKind, device: DeviceKind) -> bool {
    backend_capabilities(kind).is_some_and(|caps| caps.supports_device(device))
}

pub fn supports_deployment(kind: BackendKind, deploy_mode: DeployMode) -> bool {
    backend_capabilities(kind).is_some_and(|caps| caps.supports_deployment(deploy_mode))
}
