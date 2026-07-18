use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    DeviceKind, Error, ExternalDropGuard, ExternalHandle, MemoryDomain, MemoryType, Result,
};

/// Buffer descriptor used for allocations and validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferDesc {
    pub size_bytes: usize,
    pub align: usize,
}

impl BufferDesc {
    pub fn new(size_bytes: usize, align: usize) -> Self {
        Self { size_bytes, align }
    }
}

#[derive(Clone, Debug)]
enum BufferStorage {
    Host(Arc<RwLock<Vec<u8>>>),
    External {
        bytes: Option<Arc<RwLock<Vec<u8>>>>,
        _guard: Arc<ExternalDropGuard>,
    },
}

impl BufferStorage {
    fn try_read_bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Host(bytes) => Ok(read_guard(bytes).clone()),
            Self::External { bytes: None, .. } => Err(Error::Buffer(
                "external buffer is not host-mapped; call map or stage explicitly".to_string(),
            )),
            Self::External {
                bytes: Some(bytes), ..
            } => Ok(read_guard(bytes).clone()),
        }
    }

    fn write_from_slice(&self, src: &[u8]) -> Result<()> {
        match self {
            Self::Host(bytes) => {
                let mut guard = write_guard(bytes);
                if guard.len() != src.len() {
                    return Err(Error::Buffer(
                        "source and destination size differ".to_string(),
                    ));
                }
                guard.copy_from_slice(src);
                Ok(())
            }
            Self::External {
                bytes: Some(bytes), ..
            } => {
                let mut guard = write_guard(bytes);
                if guard.len() != src.len() {
                    return Err(Error::Buffer(
                        "source and destination size differ".to_string(),
                    ));
                }
                guard.copy_from_slice(src);
                Ok(())
            }
            Self::External { bytes: None, .. } => Err(Error::Buffer(
                "external buffer is not host-mapped; call map or stage explicitly".to_string(),
            )),
        }
    }
}

fn read_guard(lock: &RwLock<Vec<u8>>) -> RwLockReadGuard<'_, Vec<u8>> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_guard(lock: &RwLock<Vec<u8>>) -> RwLockWriteGuard<'_, Vec<u8>> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Shared byte storage with RAII semantics.
#[derive(Clone, Debug)]
pub struct Buffer {
    device: DeviceKind,
    domain: MemoryDomain,
    desc: BufferDesc,
    external: ExternalHandle,
    storage: Arc<BufferStorage>,
}

impl Buffer {
    pub(crate) fn try_new_host(device: DeviceKind, desc: BufferDesc) -> Result<Self> {
        if desc.align == 0 {
            return Err(Error::InvalidArgument(
                "buffer alignment must be non-zero".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(desc.size_bytes)
            .map_err(|_| Error::OutOfMemory)?;
        bytes.resize(desc.size_bytes, 0);
        Ok(Self {
            device,
            domain: MemoryDomain::Host,
            desc,
            external: ExternalHandle::none(),
            storage: Arc::new(BufferStorage::Host(Arc::new(RwLock::new(bytes)))),
        })
    }

    pub fn allocate_host(device: DeviceKind, size_bytes: usize) -> Result<Self> {
        Self::try_new_host(device, BufferDesc::new(size_bytes, 1))
    }

    pub fn from_host_bytes(device: DeviceKind, desc: BufferDesc, bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != desc.size_bytes {
            return Err(Error::Buffer(
                "host bytes do not match descriptor size".to_string(),
            ));
        }
        Ok(Self {
            device,
            domain: MemoryDomain::Host,
            desc,
            external: ExternalHandle::none(),
            storage: Arc::new(BufferStorage::Host(Arc::new(RwLock::new(bytes)))),
        })
    }

    pub fn from_external_with_host_bytes(
        device: DeviceKind,
        domain: MemoryDomain,
        desc: BufferDesc,
        external: ExternalHandle,
        bytes: Vec<u8>,
        guard: ExternalDropGuard,
    ) -> Result<Self> {
        if bytes.len() != desc.size_bytes {
            return Err(Error::Buffer(
                "external bytes do not match descriptor size".to_string(),
            ));
        }
        Ok(Self {
            device,
            domain,
            desc,
            external,
            storage: Arc::new(BufferStorage::External {
                bytes: Some(Arc::new(RwLock::new(bytes))),
                _guard: Arc::new(guard),
            }),
        })
    }

    /// Imports an external handle without allocating host storage.
    pub fn from_external(
        device: DeviceKind,
        domain: MemoryDomain,
        desc: BufferDesc,
        external: ExternalHandle,
        guard: ExternalDropGuard,
    ) -> Result<Self> {
        Ok(Self {
            device,
            domain,
            desc,
            external,
            storage: Arc::new(BufferStorage::External {
                bytes: None,
                _guard: Arc::new(guard),
            }),
        })
    }

    pub fn device(&self) -> DeviceKind {
        self.device
    }

    pub fn domain(&self) -> MemoryDomain {
        self.domain
    }

    pub fn memory_type(&self) -> MemoryType {
        match self.domain {
            MemoryDomain::Host => MemoryType::Host,
            MemoryDomain::DmaBuf
            | MemoryDomain::DrmPrime
            | MemoryDomain::VaapiSurface
            | MemoryDomain::CudaDevice
            | MemoryDomain::MppBuffer
            | MemoryDomain::SophonDevice
            | MemoryDomain::Opaque => MemoryType::Device,
        }
    }

    pub fn desc(&self) -> BufferDesc {
        self.desc
    }

    pub fn len(&self) -> usize {
        self.desc.size_bytes
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.storage)
    }

    /// Returns whether host bytes can be read without staging.
    pub fn is_host_readable(&self) -> bool {
        !matches!(
            self.storage.as_ref(),
            BufferStorage::External { bytes: None, .. }
        )
    }

    /// Reads host bytes, cloning shared storage when necessary.
    ///
    /// Device-only external buffers return [`Error::Buffer`].
    pub fn read_bytes(&self) -> Result<Vec<u8>> {
        self.storage.try_read_bytes()
    }

    /// Reads bytes only when host storage is already available.
    pub fn try_read_bytes(&self) -> Result<Vec<u8>> {
        self.read_bytes()
    }

    /// Explicitly maps host-backed storage. External-only buffers require
    /// [`Self::map_with`] because dg-core cannot dereference vendor handles.
    pub fn map(&self) -> Result<Vec<u8>> {
        self.read_bytes()
    }

    /// Explicitly stages an external buffer using a caller-provided mapper.
    pub fn map_with(
        &self,
        mapper: impl FnOnce(ExternalHandle, MemoryDomain, usize) -> Result<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        if !matches!(
            self.storage.as_ref(),
            BufferStorage::External { bytes: None, .. }
        ) {
            return self.read_bytes();
        }
        let bytes = mapper(self.external, self.domain, self.desc.size_bytes)?;
        if bytes.len() != self.desc.size_bytes {
            return Err(Error::Buffer(
                "mapped external bytes do not match descriptor size".to_string(),
            ));
        }
        Ok(bytes)
    }

    /// Consumes the buffer and returns host bytes when readable.
    ///
    /// Unique host storage is moved without copying. Shared host storage clones
    /// once. Device-only external storage returns [`Error::Buffer`].
    pub fn try_into_host_bytes(self) -> Result<Vec<u8>> {
        if !self.is_host_readable() {
            return Err(Error::Buffer(
                "buffer is not host-readable; staging is required".to_string(),
            ));
        }
        let Self { storage, .. } = self;
        match Arc::try_unwrap(storage) {
            Ok(BufferStorage::Host(bytes)) => match Arc::try_unwrap(bytes) {
                Ok(lock) => match lock.into_inner() {
                    Ok(bytes) => Ok(bytes),
                    Err(poisoned) => Ok(poisoned.into_inner()),
                },
                Err(bytes) => Ok(read_guard(&bytes).clone()),
            },
            Ok(BufferStorage::External {
                bytes: Some(bytes), ..
            }) => match Arc::try_unwrap(bytes) {
                Ok(lock) => match lock.into_inner() {
                    Ok(bytes) => Ok(bytes),
                    Err(poisoned) => Ok(poisoned.into_inner()),
                },
                Err(bytes) => Ok(read_guard(&bytes).clone()),
            },
            Ok(BufferStorage::External { bytes: None, .. }) => {
                unreachable!("is_host_readable checked above")
            }
            Err(storage) => match &*storage {
                BufferStorage::Host(bytes) => Ok(read_guard(bytes).clone()),
                BufferStorage::External {
                    bytes: Some(bytes), ..
                } => Ok(read_guard(bytes).clone()),
                BufferStorage::External { bytes: None, .. } => {
                    unreachable!("is_host_readable checked above")
                }
            },
        }
    }

    /// Alias for [`Self::try_into_host_bytes`].
    pub fn into_host_bytes(self) -> Result<Vec<u8>> {
        self.try_into_host_bytes()
    }

    pub fn write_from_slice(&self, src: &[u8]) -> Result<()> {
        if src.len() != self.len() {
            return Err(Error::Buffer(
                "source and destination size differ".to_string(),
            ));
        }
        self.storage.write_from_slice(src)
    }

    pub fn copy_into(&self, dst: &mut [u8]) -> Result<()> {
        if dst.len() != self.len() {
            return Err(Error::Buffer(
                "source and destination size differ".to_string(),
            ));
        }
        let bytes = self.read_bytes()?;
        dst.copy_from_slice(&bytes);
        Ok(())
    }

    pub fn copy_to(&self, dst: &Buffer) -> Result<()> {
        let bytes = self.read_bytes()?;
        dst.write_from_slice(&bytes)
    }

    pub fn external(&self) -> ExternalHandle {
        self.external
    }
}

#[cfg(test)]
mod host_bytes_tests {
    use super::{Buffer, BufferDesc, DeviceKind, Error, ExternalDropGuard, ExternalHandle};
    use crate::MemoryDomain;

    #[test]
    fn is_host_readable_false_for_device_only_external() {
        let buffer = Buffer::from_external(
            DeviceKind::Cpu,
            MemoryDomain::DmaBuf,
            BufferDesc::new(4, 1),
            ExternalHandle::from_raw(1),
            ExternalDropGuard::new(|| {}),
        )
        .expect("external buffer");
        assert!(!buffer.is_host_readable());
        assert!(buffer.try_into_host_bytes().is_err());
    }

    #[test]
    fn try_into_host_bytes_moves_unique_host_storage() {
        let buffer = Buffer::from_host_bytes(DeviceKind::Cpu, BufferDesc::new(3, 1), vec![1, 2, 3])
            .expect("host buffer");
        assert!(buffer.is_host_readable());
        let bytes = buffer.try_into_host_bytes().expect("into host bytes");
        assert_eq!(bytes, vec![1, 2, 3]);
    }

    #[test]
    fn try_into_host_bytes_clones_shared_host_storage() {
        let buffer = Buffer::from_host_bytes(DeviceKind::Cpu, BufferDesc::new(2, 1), vec![9, 8])
            .expect("host buffer");
        let shared = buffer.clone();
        let bytes = buffer.try_into_host_bytes().expect("into host bytes");
        assert_eq!(bytes, vec![9, 8]);
        assert_eq!(shared.read_bytes().unwrap(), vec![9, 8]);
    }

    #[test]
    fn try_into_host_bytes_rejects_device_external() {
        let buffer = Buffer::from_external(
            DeviceKind::Cpu,
            MemoryDomain::CudaDevice,
            BufferDesc::new(8, 1),
            ExternalHandle::from_raw(99),
            ExternalDropGuard::new(|| {}),
        )
        .expect("cuda external");
        let err = buffer.try_into_host_bytes().expect_err("expected error");
        assert!(matches!(err, Error::Buffer(_)));
    }
}
