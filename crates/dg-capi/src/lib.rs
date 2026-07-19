//! Stable C ABI for graph construction, tensor exchange, and external buffers.

#![allow(clippy::missing_safety_doc)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::fd::{AsRawFd, FromRawFd};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use dg_core::{
    Buffer, BufferDesc, CpuDevice, DataFormat, DataType, DeviceKind, ExternalDropGuard,
    ExternalHandle, MemoryDomain, ResourcePolicy, Shape, Tensor, TensorDesc, TypeCode,
};
use dg_graph::{
    ElementMetricsSnapshot, Graph, GraphDiff, GraphFormat, GraphSpec, GraphStatus, NodeSpec,
    RunningGraph,
};
#[cfg(feature = "media")]
use dg_media as _;
use dg_runtime::{
    configure_backend, create_backend, BackendConfig, BackendKind, InferBackend, ModelSource,
};
#[cfg(feature = "stream")]
use dg_stream as _;
use serde_json::{Map, Value};

#[cfg(unix)]
mod unix_dup {
    use std::ffi::c_int;

    extern "C" {
        pub fn dup(fd: c_int) -> c_int;
    }
}

/// ABI status returned by every fallible C entry point.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgStatus {
    Ok = 0,
    Again = 1,
    EndOfStream = 2,
    /// Engine handle is busy with another exclusive operation.
    Busy = 3,
    InvalidArgument = -1,
    NullPointer = -2,
    InvalidHandle = -3,
    ParseError = -4,
    NotBuilt = -5,
    RuntimeError = -6,
    Unsupported = -7,
    Panic = -8,
    InternalError = -9,
}

/// Lifecycle status of a running graph engine.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgGraphStatus {
    Starting = 0,
    Running = 1,
    Draining = 2,
    Stopped = 3,
    Failed = 4,
    Reloading = 5,
    NotRunning = -1,
}

/// Graph serialization format.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgGraphFormat {
    Yaml = 0,
    Json = 1,
    Toml = 2,
}

/// Supported tensor element types.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgDataType {
    U8 = 0,
    U16 = 1,
    I4 = 2,
    I8 = 3,
    I16 = 4,
    F4 = 5,
    F8 = 6,
    F16 = 7,
    Bf16 = 8,
    F32 = 9,
    F64 = 10,
    U32 = 11,
    I32 = 12,
    U64 = 13,
    I64 = 14,
}

/// Tensor layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgDataFormat {
    Auto = 0,
    N = 1,
    Nc = 2,
    Nchw = 3,
    Nhwc = 4,
    Nc4hw = 5,
    Nc8hw = 6,
    Ncdhw = 7,
    Oihw = 8,
}

/// Logical device family.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgDeviceKind {
    Cpu = 0,
    IntelGpu = 1,
    IntelNpu = 2,
    CudaGpu = 3,
    RknnNpu = 4,
    SophonTpu = 5,
}

/// Imported external memory domain.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgMemoryDomain {
    Host = 0,
    DmaBuf = 1,
    DrmPrime = 2,
    VaapiSurface = 3,
    CudaDevice = 4,
    MppBuffer = 5,
    SophonDevice = 6,
    Opaque = 7,
}

/// Backend family for direct backend lifecycle operations.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DgBackendKind {
    Mock = 0,
    OpenVino = 1,
    Rknn = 2,
    TensorRt = 3,
    Sophon = 4,
}

/// Borrowed UTF-8 string view. The caller keeps the underlying memory valid for
/// the duration of the ABI call.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DgStringView {
    pub data: *const c_char,
    pub len: usize,
}

/// Borrowed byte view. The caller keeps the underlying memory valid for the
/// duration of the ABI call.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DgByteView {
    pub data: *const u8,
    pub len: usize,
}

/// Borrowed shape dimensions view. The caller keeps the underlying memory valid
/// for the duration of the ABI call.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DgShapeView {
    pub dims: *const usize,
    pub rank: usize,
}

/// Release callback for imported raw external memory. It is invoked exactly once
/// when the library no longer references the external handle.
pub type DgReleaseCallback = Option<unsafe extern "C" fn(*mut c_void)>;

/// External memory descriptor for v2 ABI imports.
///
/// Exactly one of `fd` or `raw` must be valid. FD imports are duplicated; the
/// library closes the duplicate. Raw imports require a non-null release callback.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DgExternalMemoryV2 {
    pub struct_size: u32,
    pub struct_version: u32,
    pub fd: c_int,
    pub raw: u64,
    pub domain: i32,
    pub device: i32,
    pub size_bytes: usize,
    pub release: DgReleaseCallback,
    pub user_data: *mut c_void,
}

/// Owned byte buffer returned by v2 ABI calls. The library allocates and frees
/// it; callers must use `dg_owned_bytes_free`.
pub struct DgOwnedBytes {
    bytes: Vec<u8>,
}

/// Opaque error handle returned by fallible v2 ABI calls. The library allocates
/// and frees it; callers must use `dg_error_free`.
pub struct DgError {
    status: DgStatus,
    category: CString,
    operation: CString,
    message: CString,
}

impl DgOwnedBytes {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Returns a non-null, aligned pointer to the byte buffer.
    ///
    /// When `len()` is zero the pointer is a dangling alignment pointer; callers
    /// must not dereference it and should use `dg_owned_bytes_len` to determine
    /// the number of valid bytes.
    fn data(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl DgError {
    fn new(status: DgStatus, message: impl Into<String>) -> Self {
        let message = format_c_error_message(message);
        let message_c = CString::new(message.clone()).unwrap_or_else(|_| {
            c"kind=Other operation=Unknown detail=invalid error message".to_owned()
        });
        let operation = parse_operation(&message);
        let operation_c = CString::new(operation).unwrap_or_else(|_| c"Unknown".to_owned());
        let category_c =
            CString::new(format!("{status:?}")).unwrap_or_else(|_| c"Unknown".to_owned());
        Self {
            status,
            category: category_c,
            operation: operation_c,
            message: message_c,
        }
    }
}

fn parse_operation(message: &str) -> String {
    message
        .split_whitespace()
        .find_map(|token| token.strip_prefix("operation="))
        .map(String::from)
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Fixed-size tensor metadata returned by direct backend queries.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DgTensorInfo {
    /// Size of this struct in bytes (`sizeof(DgTensorInfo)`).
    pub struct_size: u32,
    /// ABI struct version; must be 0 for the current definition.
    pub struct_version: u32,
    pub dtype: DgDataType,
    pub format: DgDataFormat,
    pub device: DgDeviceKind,
    pub rank: usize,
    pub shape: [usize; 8],
}

/// Runtime capabilities returned by a direct backend.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DgBackendCapabilities {
    /// Size of this struct in bytes (`sizeof(DgBackendCapabilities)`).
    pub struct_size: u32,
    /// ABI struct version; must be 0 for the current definition.
    pub struct_version: u32,
    pub device_count: usize,
    pub devices: [DgDeviceKind; 8],
    pub precision_count: usize,
    pub precisions: [DgDataType; 16],
}

/// Opaque graph engine handle.
pub struct DgEngine {
    /// Shared lock: writers for load/build/reload/run/shutdown; readers for
    /// stop/status/metrics (INT5-09 concurrent observability).
    inner: RwLock<Engine>,
}

/// Clones the `Arc` backing a `DgEngine` pointer without consuming the pointer.
///
/// SAFETY: `ptr` must have been returned by `dg_engine_create` and not yet freed.
unsafe fn clone_engine_arc(ptr: *const DgEngine) -> Arc<DgEngine> {
    let arc = unsafe { Arc::from_raw(ptr) };
    let clone = arc.clone();
    std::mem::forget(arc);
    clone
}

/// Runtime bootstrap options for [`dg_runtime_init`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DgRuntimeInitOptions {
    /// Must be set to `size_of::<DgRuntimeInitOptions>()` by the caller.
    pub struct_size: u32,
    /// ABI struct version; must be 0 for the current definition.
    pub struct_version: u32,
}

fn lock_engine_write(
    engine: &DgEngine,
) -> Result<std::sync::RwLockWriteGuard<'_, Engine>, (DgStatus, String)> {
    match engine.inner.try_write() {
        Ok(guard) => Ok(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => Err((
            DgStatus::Busy,
            "engine handle is busy; concurrent mutation is not allowed".to_string(),
        )),
    }
}

fn lock_engine_read(
    engine: &DgEngine,
) -> Result<std::sync::RwLockReadGuard<'_, Engine>, (DgStatus, String)> {
    match engine.inner.try_read() {
        Ok(guard) => Ok(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => Err((
            DgStatus::Busy,
            "engine handle is busy; concurrent write holds the engine".to_string(),
        )),
    }
}

fn lock_backend(backend: &DgBackend) -> std::sync::MutexGuard<'_, Box<dyn InferBackend>> {
    backend
        .backend
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[allow(dead_code)]
const MAX_VIEW_LEN: usize = 1 << 40;
#[allow(dead_code)]
const MAX_SHAPE_RANK: usize = 8;
/// Maximum number of bytes (including the terminating NUL) scanned from a
/// `*const c_char` argument. This prevents unbounded reads when a caller passes
/// a non-NUL-terminated or extremely long C string.
const MAX_CSTRING_SCAN: usize = dg_core::ResourcePolicy::DEFAULT_MAX_CONFIG_BYTES.saturating_add(1);

fn check_struct_version(
    name: &str,
    struct_size: u32,
    struct_version: u32,
    expected_size: usize,
) -> Result<(), (DgStatus, String)> {
    if struct_version != 0 {
        return Err((
            DgStatus::InvalidArgument,
            format!("{name}.struct_version {struct_version} is not supported"),
        ));
    }
    let expected = u32::try_from(expected_size).map_err(|_| {
        (
            DgStatus::InternalError,
            format!("{name} expected struct size {expected_size} exceeds u32"),
        )
    })?;
    if struct_size != expected {
        return Err((
            DgStatus::InvalidArgument,
            format!("{name}.struct_size mismatch: got {struct_size}, expected {expected}"),
        ));
    }
    Ok(())
}

/// Parses and validates an external memory descriptor without taking ownership
/// of the underlying handle.
#[allow(clippy::type_complexity)]
fn parse_external_memory_descriptor(
    desc: *const DgExternalMemoryV2,
) -> Result<
    (
        DeviceKind,
        MemoryDomain,
        usize,
        i32,
        u64,
        DgReleaseCallback,
        *mut c_void,
    ),
    (DgStatus, String),
> {
    if desc.is_null() {
        return Err((
            DgStatus::NullPointer,
            "external memory descriptor is null".to_string(),
        ));
    }
    // Read the version fields first, then the rest of the descriptor field by
    // field through raw pointers. This avoids creating a Rust reference to a
    // potentially uninitialized C struct (including padding bytes).
    let (struct_size, struct_version) = unsafe {
        (
            std::ptr::addr_of!((*desc).struct_size).read(),
            std::ptr::addr_of!((*desc).struct_version).read(),
        )
    };
    check_struct_version(
        "DgExternalMemoryV2",
        struct_size,
        struct_version,
        std::mem::size_of::<DgExternalMemoryV2>(),
    )?;
    let (fd, raw, domain, device, size_bytes, release, user_data) = unsafe {
        (
            std::ptr::addr_of!((*desc).fd).read(),
            std::ptr::addr_of!((*desc).raw).read(),
            std::ptr::addr_of!((*desc).domain).read(),
            std::ptr::addr_of!((*desc).device).read(),
            std::ptr::addr_of!((*desc).size_bytes).read(),
            std::ptr::addr_of!((*desc).release).read(),
            std::ptr::addr_of!((*desc).user_data).read(),
        )
    };
    let fd_valid = fd >= 0;
    let raw_valid = raw != 0;
    if fd_valid && raw_valid {
        return Err((
            DgStatus::InvalidArgument,
            "external memory descriptor has both fd and raw set; exactly one must be valid"
                .to_string(),
        ));
    }
    if !fd_valid && !raw_valid {
        return Err((
            DgStatus::InvalidArgument,
            "external memory descriptor has neither fd nor raw set".to_string(),
        ));
    }
    let domain = domain_from_c(domain)?;
    let device = device_from_c(device)?;
    if size_bytes == 0 {
        return Err((
            DgStatus::InvalidArgument,
            "external memory size must be non-zero".to_string(),
        ));
    }
    Ok((device, domain, size_bytes, fd, raw, release, user_data))
}

/// Takes ownership of an already validated external memory handle and builds the
/// guard that releases it exactly once.
fn build_external_handle(
    fd: i32,
    raw: u64,
    release: DgReleaseCallback,
    user_data: *mut c_void,
) -> Result<(ExternalHandle, ExternalDropGuard), (DgStatus, String)> {
    let fd_valid = fd >= 0;
    let raw_valid = raw != 0;
    if fd_valid && raw_valid || !fd_valid && !raw_valid {
        return Err((
            DgStatus::InvalidArgument,
            "external memory descriptor has invalid fd/raw combination".to_string(),
        ));
    }

    if fd_valid {
        // SAFETY: `dup` is a libc call that returns a new descriptor or -1. We
        // only convert a successfully duplicated fd into `OwnedFd`, which closes
        // it when the last reference drops; the original fd remains owned by the
        // caller.
        let dup_fd = unsafe { unix_dup::dup(fd) };
        if dup_fd < 0 {
            return Err((
                DgStatus::InvalidArgument,
                format!(
                    "failed to duplicate external fd: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(dup_fd) };
        let dup_fd = owned.as_raw_fd();
        let guard = ExternalDropGuard::new(move || {
            // `owned` closes the duplicated fd when the last reference drops.
            drop(owned);
        });
        Ok((ExternalHandle::from_fd(dup_fd), guard))
    } else {
        let release = release.ok_or_else(|| {
            (
                DgStatus::InvalidArgument,
                "raw external memory requires a release callback".to_string(),
            )
        })?;
        // Cast the opaque pointer to usize so the closure is Send + 'static.
        let user_data = user_data as usize;
        let guard = ExternalDropGuard::new(move || {
            // SAFETY: `release` was provided by the caller and `user_data` is the
            // same opaque pointer that was passed in.
            unsafe { release(user_data as *mut c_void) };
        });
        Ok((ExternalHandle::from_raw(raw), guard))
    }
}

impl DgStringView {
    #[allow(dead_code)]
    fn as_str(&self) -> Result<&str, (DgStatus, String)> {
        if self.len > MAX_VIEW_LEN {
            return Err((
                DgStatus::InvalidArgument,
                "string view length exceeds maximum".to_string(),
            ));
        }
        if self.len == 0 {
            return Ok("");
        }
        if self.data.is_null() {
            return Err((
                DgStatus::NullPointer,
                "string view data is null".to_string(),
            ));
        }
        let bytes = unsafe { std::slice::from_raw_parts(self.data as *const u8, self.len) };
        std::str::from_utf8(bytes).map_err(|error| {
            (
                DgStatus::InvalidArgument,
                format!("string view is not valid UTF-8: {error}"),
            )
        })
    }
}

impl DgByteView {
    #[allow(dead_code)]
    fn as_bytes(&self) -> Result<&[u8], (DgStatus, String)> {
        if self.len > MAX_VIEW_LEN {
            return Err((
                DgStatus::InvalidArgument,
                "byte view length exceeds maximum".to_string(),
            ));
        }
        if self.len == 0 {
            return Ok(&[]);
        }
        if self.data.is_null() {
            return Err((DgStatus::NullPointer, "byte view data is null".to_string()));
        }
        Ok(unsafe { std::slice::from_raw_parts(self.data, self.len) })
    }
}

impl DgShapeView {
    #[allow(dead_code)]
    fn as_dims(&self) -> Result<&[usize], (DgStatus, String)> {
        if self.rank > MAX_SHAPE_RANK {
            return Err((
                DgStatus::InvalidArgument,
                format!(
                    "shape rank {} exceeds the C ABI limit of {MAX_SHAPE_RANK}",
                    self.rank
                ),
            ));
        }
        if self.rank == 0 {
            return Ok(&[]);
        }
        if self.dims.is_null() {
            return Err((DgStatus::NullPointer, "shape view dims is null".to_string()));
        }
        Ok(unsafe { std::slice::from_raw_parts(self.dims, self.rank) })
    }
}

/// Opaque tensor handle.
pub struct DgTensor {
    tensor: Tensor,
}

/// Opaque buffer handle.
pub struct DgBuffer {
    buffer: Buffer,
}

/// Opaque direct inference backend handle.
pub struct DgBackend {
    backend: Mutex<Box<dyn InferBackend>>,
}

/// Clones the `Arc` backing a `DgBackend` pointer without consuming the pointer.
///
/// SAFETY: `ptr` must have been returned by `dg_backend_create` and not yet freed.
unsafe fn clone_backend_arc(ptr: *const DgBackend) -> Arc<DgBackend> {
    let arc = unsafe { Arc::from_raw(ptr) };
    let clone = arc.clone();
    std::mem::forget(arc);
    clone
}

/// Clones the `Arc` backing a `DgTensor` pointer without consuming the pointer.
///
/// SAFETY: `ptr` must have been returned by a tensor constructor and not yet freed.
unsafe fn clone_tensor_arc(ptr: *const DgTensor) -> Arc<DgTensor> {
    let arc = unsafe { Arc::from_raw(ptr) };
    let clone = arc.clone();
    std::mem::forget(arc);
    clone
}

/// Clones the `Arc` backing a `DgBuffer` pointer without consuming the pointer.
///
/// SAFETY: `ptr` must have been returned by `dg_buffer_import_external` and not yet freed.
unsafe fn clone_buffer_arc(ptr: *const DgBuffer) -> Arc<DgBuffer> {
    let arc = unsafe { Arc::from_raw(ptr) };
    let clone = arc.clone();
    std::mem::forget(arc);
    clone
}

enum EnginePollOutput {
    Tensor(Tensor),
    Pending,
    EndOfStream,
}

struct Engine {
    spec: GraphSpec,
    graph: Option<Graph>,
    running: Option<RunningGraph>,
    outputs: VecDeque<Tensor>,
    pending_inputs: BTreeMap<String, Vec<Tensor>>,
    /// Set when a one-shot run completes or a streaming graph is shut down,
    /// so `poll_output` can report `EndOfStream` after the queue is drained.
    stream_ended: bool,
}

impl Engine {
    fn new() -> Self {
        Self {
            spec: GraphSpec::default(),
            graph: None,
            running: None,
            outputs: VecDeque::new(),
            pending_inputs: BTreeMap::new(),
            stream_ended: false,
        }
    }

    fn invalidate(&mut self) {
        self.graph = None;
        self.running = None;
        self.outputs.clear();
        self.pending_inputs.clear();
        self.stream_ended = false;
    }

    fn reload(&mut self, spec: GraphSpec) -> dg_graph::Result<GraphDiff> {
        if !self.pending_inputs.is_empty() {
            return Err(dg_graph::Error::InvalidState(
                "cannot reload while inputs are pending; run them before reloading".to_string(),
            ));
        }
        self.stream_ended = false;
        spec.validate()?;

        // Live path: apply the full candidate to the RunningGraph so workers
        // and top-level fields pick up the new configuration after dg_engine_init.
        if let Some(running) = self.running.as_mut() {
            let diff = running.apply_hot_update_spec(spec.clone())?;
            if let Some(graph) = self.graph.as_mut() {
                graph.reload(spec.clone())?;
            } else {
                self.graph = Some(Graph::new(spec.clone())?);
            }
            self.spec = spec;
            self.outputs.clear();
            return Ok(diff);
        }

        let diff = match &mut self.graph {
            Some(graph) => graph.reload(spec.clone())?,
            None => {
                let diff = Graph::diff(&self.spec, &spec);
                self.spec = spec;
                self.outputs.clear();
                return Ok(diff);
            }
        };
        self.spec = spec;
        self.outputs.clear();
        Ok(diff)
    }

    fn input_node_name(&self) -> dg_graph::Result<String> {
        let mut names = self
            .spec
            .nodes
            .iter()
            .filter(|node| node.kind == "input")
            .map(|node| node.name.clone());
        match (names.next(), names.next()) {
            (Some(name), None) => Ok(name),
            _ => Err(dg_graph::Error::InvalidState(
                "graph must contain exactly one input node".to_string(),
            )),
        }
    }

    fn push(&mut self, tensor: Tensor) -> dg_graph::Result<()> {
        if self.running.is_some() {
            return Err(dg_graph::Error::InvalidState(
                "cannot push inputs while graph is running; use run for one-shot execution"
                    .to_string(),
            ));
        }
        if self.graph.is_none() {
            return Err(dg_graph::Error::NotBuilt(
                "engine must be built before pushing input".to_string(),
            ));
        }

        // Bound pending inputs so a caller cannot exhaust memory by pushing an
        // unbounded number of tensors before calling run.
        let limits = &self.spec.limits;
        let new_bytes = tensor.desc().storage_bytes()?;
        let (pending_packets, pending_bytes) =
            self.pending_inputs
                .values()
                .fold((0usize, 0usize), |(packets, bytes), tensors| {
                    (
                        packets.saturating_add(tensors.len()),
                        bytes.saturating_add(
                            tensors
                                .iter()
                                .map(|t| t.desc().storage_bytes().unwrap_or(usize::MAX))
                                .fold(0usize, |acc, b| acc.saturating_add(b)),
                        ),
                    )
                });
        let total_packets = pending_packets.saturating_add(1);
        let total_bytes = pending_bytes.saturating_add(new_bytes);
        if total_packets > limits.max_buffer_packets {
            return Err(dg_graph::Error::InvalidState(format!(
                "pending input count {} would exceed max_buffer_packets {}",
                total_packets, limits.max_buffer_packets
            )));
        }
        if total_bytes > limits.max_buffer_bytes {
            return Err(dg_graph::Error::InvalidState(format!(
                "pending input bytes {} would exceed max_buffer_bytes {}",
                total_bytes, limits.max_buffer_bytes
            )));
        }

        let input_name = self.input_node_name()?;
        self.pending_inputs
            .entry(input_name)
            .or_default()
            .push(tensor);
        Ok(())
    }

    fn run(&mut self) -> dg_graph::Result<()> {
        if self.running.is_some() {
            return Err(dg_graph::Error::InvalidState(
                "cannot run one-shot execution while graph is running; shutdown first".to_string(),
            ));
        }
        let graph = self
            .graph
            .as_ref()
            .ok_or_else(|| dg_graph::Error::NotBuilt("engine must be built first".to_string()))?;
        let inputs: HashMap<String, Vec<Tensor>> = self
            .pending_inputs
            .iter()
            .map(|(name, tensors)| (name.clone(), tensors.clone()))
            .collect();
        // A previous one-shot run may have left un-polled outputs; clear them
        // before starting the next execution so failure does not return stale tensors.
        self.outputs.clear();
        self.stream_ended = false;
        let report = match graph.run_with_inputs(inputs) {
            Ok(report) => report,
            Err(error) => {
                // The graph stopped before producing any new outputs; mark the
                // stream as ended so `poll_output` reports EndOfStream instead
                // of spinning on Pending/Again forever.
                self.stream_ended = true;
                return Err(error);
            }
        };
        self.pending_inputs.clear();
        for tensors in report.sinks.into_values() {
            self.outputs.extend(tensors);
        }
        self.stream_ended = true;
        Ok(())
    }

    fn init(&mut self) -> dg_graph::Result<()> {
        if self.running.is_some() {
            return Err(dg_graph::Error::InvalidState(
                "engine is already running".to_string(),
            ));
        }
        if !self.pending_inputs.is_empty() {
            return Err(dg_graph::Error::InvalidState(
                "cannot switch to streaming with pending one-shot inputs; run or clear them first"
                    .to_string(),
            ));
        }
        if self.graph.is_none() {
            self.spec.validate()?;
            self.graph = Some(Graph::new(self.spec.clone())?);
        }
        let graph = self
            .graph
            .as_ref()
            .ok_or_else(|| dg_graph::Error::NotBuilt("engine must be built first".to_string()))?;
        self.running = match graph.start(HashMap::new()) {
            Ok(running) => Some(running),
            Err(error) => {
                // The graph failed to start; there will be no outputs.
                self.outputs.clear();
                self.stream_ended = true;
                return Err(error);
            }
        };
        self.outputs.clear();
        self.stream_ended = false;
        Ok(())
    }

    fn stop(&self) -> dg_graph::Result<()> {
        let running = self.running.as_ref().ok_or(dg_graph::Error::NotRunning)?;
        running.request_stop();
        Ok(())
    }

    fn shutdown(&mut self, timeout_ms: u64) -> dg_graph::Result<()> {
        // `Duration::from_millis` panics on overflow; clamp to the largest
        // representable duration so malicious callers cannot crash the process.
        let max_ms = u64::try_from(std::time::Duration::MAX.as_millis()).unwrap_or(u64::MAX);
        let timeout = std::time::Duration::from_millis(timeout_ms.min(max_ms));
        if let Some(mut running) = self.running.take() {
            if let Err(error) = running.shutdown(timeout) {
                // Only timeouts are retryable; keep the running graph so the
                // caller can call shutdown/destroy again with a larger timeout.
                // Permanent failures (worker panic/element error) consume the
                // running graph so the engine handle can still be destroyed.
                if matches!(error, dg_graph::Error::Timeout(_)) {
                    self.running = Some(running);
                }
                return Err(error);
            }
        }
        // No running graph or shutdown completed; the built graph is kept so
        // `dg_engine_run` can still be used for a final one-shot execution.
        self.stream_ended = true;
        Ok(())
    }

    fn poll_output(&mut self) -> dg_graph::Result<EnginePollOutput> {
        if let Some(running) = self.running.as_mut() {
            let poll_result = running.poll();
            let sink_tensors = running.drain_sinks();
            self.outputs.extend(sink_tensors);

            // Return any outputs that were already produced before propagating a
            // graph failure, so tensors computed up to the failure are not lost.
            if let Some(tensor) = self.outputs.pop_front() {
                return Ok(EnginePollOutput::Tensor(tensor));
            }

            poll_result?;
        }
        if let Some(tensor) = self.outputs.pop_front() {
            return Ok(EnginePollOutput::Tensor(tensor));
        }
        if let Some(running) = self.running.as_ref() {
            let (status, cause) = running.status();
            if status == GraphStatus::Failed {
                return Err(dg_graph::Error::Runtime(cause.unwrap_or_default()));
            }
            if status == GraphStatus::Stopped {
                return Ok(EnginePollOutput::EndOfStream);
            }
        }
        if self.stream_ended {
            return Ok(EnginePollOutput::EndOfStream);
        }
        Ok(EnginePollOutput::Pending)
    }

    fn status(&self) -> (DgGraphStatus, Option<String>) {
        match self.running.as_ref() {
            Some(running) => {
                let (status, cause) = running.status();
                (graph_status_to_c(status), cause)
            }
            None => (DgGraphStatus::NotRunning, None),
        }
    }

    fn metrics_json(&self) -> dg_graph::Result<String> {
        let running = self.running.as_ref().ok_or(dg_graph::Error::NotRunning)?;
        let metrics = running.metrics_snapshot();
        Ok(serde_json::to_string(&MetricsPayload {
            schema_version: METRICS_SCHEMA_VERSION,
            metrics,
        })?)
    }
}

#[derive(serde::Serialize)]
struct MetricsPayload {
    schema_version: u32,
    metrics: BTreeMap<String, ElementMetricsSnapshot>,
}

const METRICS_SCHEMA_VERSION: u32 = 1;

fn graph_status_to_c(status: GraphStatus) -> DgGraphStatus {
    match status {
        GraphStatus::Starting => DgGraphStatus::Starting,
        GraphStatus::Running => DgGraphStatus::Running,
        GraphStatus::Draining => DgGraphStatus::Draining,
        GraphStatus::Stopped => DgGraphStatus::Stopped,
        GraphStatus::Failed => DgGraphStatus::Failed,
        GraphStatus::Reloading => DgGraphStatus::Reloading,
    }
}

/// Formats a C error string with stable diagnostic fields.
///
/// Media failures already use `kind= profile= role= operation= backend= domain=`
/// prefixes from `dg_media::MediaErrorContext`. Other failures are wrapped as
/// `kind=Other operation=Unknown detail=…` so C callers can always parse fields.
fn format_c_error_message(message: impl Into<String>) -> String {
    let message = message.into().replace('\0', " ");
    if let Some(idx) = message.find("kind=") {
        let tail = message[idx..].trim();
        if tail.contains("operation=") {
            return tail.to_string();
        }
        return format!("{tail} operation=Unknown");
    }
    format!("kind=Other operation=Unknown detail={message}")
}

fn write_error(out_error: *mut *mut DgError, status: DgStatus, message: impl Into<String>) {
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid, possibly uninitialised `*mut DgError`.
        unsafe { out_error.write(Box::into_raw(Box::new(DgError::new(status, message)))) };
    }
}

fn ffi_result<T>(
    out_error: *mut *mut DgError,
    operation: impl FnOnce() -> Result<T, (DgStatus, String)>,
) -> Result<T, DgStatus> {
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid pointer; we own writing the initial null.
        unsafe { out_error.write(ptr::null_mut()) };
    }
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err((status, message))) => {
            write_error(out_error, status, message);
            Err(status)
        }
        Err(_) => {
            let status = DgStatus::Panic;
            write_error(out_error, status, "panic crossed C ABI boundary");
            Err(status)
        }
    }
}

fn ffi_result_with_out<T: Copy>(
    out: *mut T,
    out_error: *mut *mut DgError,
    operation: impl FnOnce() -> Result<T, (DgStatus, String)>,
) -> DgStatus {
    if out.is_null() {
        write_error(out_error, DgStatus::NullPointer, "output pointer is null");
        return DgStatus::NullPointer;
    }
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid pointer; we own writing the initial null.
        unsafe { out_error.write(ptr::null_mut()) };
    }
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => {
            // SAFETY: `out` was checked non-null and points to writable caller storage.
            unsafe { out.write(value) };
            DgStatus::Ok
        }
        Ok(Err((status, message))) => {
            write_error(out_error, status, message);
            // SAFETY: `out` was checked non-null; zero is a safe sentinel for Copy types.
            unsafe { out.write(std::mem::zeroed()) };
            status
        }
        Err(_) => {
            let status = DgStatus::Panic;
            write_error(out_error, status, "panic crossed C ABI boundary");
            unsafe { out.write(std::mem::zeroed()) };
            status
        }
    }
}

fn map_core_error(error: &dg_core::Error) -> DgStatus {
    use dg_core::Error as E;
    match error {
        E::InvalidArgument(_) | E::Config(_) => DgStatus::InvalidArgument,
        E::Unsupported(_) | E::UnsupportedDevice(_) => DgStatus::Unsupported,
        E::Timeout(_) | E::Busy(_) => DgStatus::Busy,
        E::OutOfMemory
        | E::Io(_)
        | E::Device(_)
        | E::Backend(_)
        | E::Media(_)
        | E::Shape(_)
        | E::Quantization(_)
        | E::Tensor(_)
        | E::Buffer(_)
        | E::Stream(_)
        | E::Event(_) => DgStatus::RuntimeError,
        E::Cancelled => DgStatus::RuntimeError,
        E::Closed => DgStatus::InvalidHandle,
        E::Protocol(_) | E::Auth(_) | E::RemoteClosed(_) => DgStatus::RuntimeError,
        E::Invariant(_) | E::Internal(_) => DgStatus::InternalError,
    }
}

fn map_graph_error(error: dg_graph::Error) -> (DgStatus, String) {
    use dg_graph::Error as E;
    let status = match &error {
        E::Config(_)
        | E::Validation { .. }
        | E::UnknownFormat(_)
        | E::UnknownNodeKind(_)
        | E::UnknownPort { .. }
        | E::PortTypeMismatch { .. }
        | E::DuplicateNode(_)
        | E::CycleDetected
        | E::Json(_)
        | E::Yaml(_)
        | E::TomlDe(_)
        | E::TomlSer(_) => DgStatus::ParseError,
        E::Element { .. } | E::Runtime(_) | E::Io(_) | E::RuntimeBackend(_) => {
            DgStatus::RuntimeError
        }
        E::NotRunning | E::InvalidState(_) => DgStatus::RuntimeError,
        E::NotBuilt(_) => DgStatus::NotBuilt,
        E::ResourceLimit { .. } => DgStatus::RuntimeError,
        E::Timeout(_) | E::Busy(_) => DgStatus::Busy,
        E::Cancelled => DgStatus::RuntimeError,
        E::Invariant(_) => DgStatus::InternalError,
        E::Core(core) => map_core_error(core),
    };
    (status, error.to_string())
}

fn write_diff_counts(
    diff: &GraphDiff,
    out_added_nodes: *mut usize,
    out_removed_nodes: *mut usize,
    out_updated_nodes: *mut usize,
    out_added_connections: *mut usize,
    out_removed_connections: *mut usize,
) -> Result<(), (DgStatus, String)> {
    prepare_diff_outputs(
        out_added_nodes,
        out_removed_nodes,
        out_updated_nodes,
        out_added_connections,
        out_removed_connections,
    )?;
    // SAFETY: all output pointers were checked non-null and zeroed by `prepare_diff_outputs`.
    unsafe {
        out_added_nodes.write(diff.added_nodes.len());
        out_removed_nodes.write(diff.removed_nodes.len());
        out_updated_nodes.write(diff.updated_nodes.len());
        out_added_connections.write(diff.added_connections.len());
        out_removed_connections.write(diff.removed_connections.len());
    }
    Ok(())
}

fn prepare_diff_outputs(
    out_added_nodes: *mut usize,
    out_removed_nodes: *mut usize,
    out_updated_nodes: *mut usize,
    out_added_connections: *mut usize,
    out_removed_connections: *mut usize,
) -> Result<(), (DgStatus, String)> {
    if out_added_nodes.is_null()
        || out_removed_nodes.is_null()
        || out_updated_nodes.is_null()
        || out_added_connections.is_null()
        || out_removed_connections.is_null()
    {
        return Err((
            DgStatus::NullPointer,
            "diff output pointer is null".to_string(),
        ));
    }
    // SAFETY: all pointers were just checked non-null.
    unsafe {
        out_added_nodes.write(0);
        out_removed_nodes.write(0);
        out_updated_nodes.write(0);
        out_added_connections.write(0);
        out_removed_connections.write(0);
    }
    Ok(())
}

fn format_from_c(format: i32) -> Result<GraphFormat, (DgStatus, String)> {
    match format {
        x if x == DgGraphFormat::Yaml as i32 => Ok(GraphFormat::Yaml),
        x if x == DgGraphFormat::Json as i32 => Ok(GraphFormat::Json),
        x if x == DgGraphFormat::Toml as i32 => Ok(GraphFormat::Toml),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown graph format discriminant {other}"),
        )),
    }
}

fn data_type_from_c(dtype: i32) -> Result<DataType, (DgStatus, String)> {
    match dtype {
        x if x == DgDataType::U8 as i32 => Ok(DataType::U8),
        x if x == DgDataType::U16 as i32 => Ok(DataType::U16),
        x if x == DgDataType::I4 as i32 => Ok(DataType::I4),
        x if x == DgDataType::I8 as i32 => Ok(DataType::I8),
        x if x == DgDataType::I16 as i32 => Ok(DataType::I16),
        x if x == DgDataType::F4 as i32 => Ok(DataType::F4),
        x if x == DgDataType::F8 as i32 => Ok(DataType::F8),
        x if x == DgDataType::F16 as i32 => Ok(DataType::F16),
        x if x == DgDataType::Bf16 as i32 => Ok(DataType::BF16),
        x if x == DgDataType::F32 as i32 => Ok(DataType::F32),
        x if x == DgDataType::F64 as i32 => Ok(DataType::F64),
        x if x == DgDataType::U32 as i32 => Ok(DataType::new(TypeCode::Uint, 32, 1)),
        x if x == DgDataType::I32 as i32 => Ok(DataType::new(TypeCode::Int, 32, 1)),
        x if x == DgDataType::U64 as i32 => Ok(DataType::new(TypeCode::Uint, 64, 1)),
        x if x == DgDataType::I64 as i32 => Ok(DataType::new(TypeCode::Int, 64, 1)),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown data type discriminant {other}"),
        )),
    }
}

fn format_from_c_enum(format: i32) -> Result<DataFormat, (DgStatus, String)> {
    match format {
        x if x == DgDataFormat::Auto as i32 => Ok(DataFormat::Auto),
        x if x == DgDataFormat::N as i32 => Ok(DataFormat::N),
        x if x == DgDataFormat::Nc as i32 => Ok(DataFormat::NC),
        x if x == DgDataFormat::Nchw as i32 => Ok(DataFormat::NCHW),
        x if x == DgDataFormat::Nhwc as i32 => Ok(DataFormat::NHWC),
        x if x == DgDataFormat::Nc4hw as i32 => Ok(DataFormat::NC4HW),
        x if x == DgDataFormat::Nc8hw as i32 => Ok(DataFormat::NC8HW),
        x if x == DgDataFormat::Ncdhw as i32 => Ok(DataFormat::NCDHW),
        x if x == DgDataFormat::Oihw as i32 => Ok(DataFormat::OIHW),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown data format discriminant {other}"),
        )),
    }
}

fn device_from_c(device: i32) -> Result<DeviceKind, (DgStatus, String)> {
    match device {
        x if x == DgDeviceKind::Cpu as i32 => Ok(DeviceKind::Cpu),
        x if x == DgDeviceKind::IntelGpu as i32 => Ok(DeviceKind::IntelGpu),
        x if x == DgDeviceKind::IntelNpu as i32 => Ok(DeviceKind::IntelNpu),
        x if x == DgDeviceKind::CudaGpu as i32 => Ok(DeviceKind::CudaGpu),
        x if x == DgDeviceKind::RknnNpu as i32 => Ok(DeviceKind::RknnNpu),
        x if x == DgDeviceKind::SophonTpu as i32 => Ok(DeviceKind::SophonTpu),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown device kind discriminant {other}"),
        )),
    }
}

fn domain_from_c(domain: i32) -> Result<MemoryDomain, (DgStatus, String)> {
    match domain {
        x if x == DgMemoryDomain::Host as i32 => Ok(MemoryDomain::Host),
        x if x == DgMemoryDomain::DmaBuf as i32 => Ok(MemoryDomain::DmaBuf),
        x if x == DgMemoryDomain::DrmPrime as i32 => Ok(MemoryDomain::DrmPrime),
        x if x == DgMemoryDomain::VaapiSurface as i32 => Ok(MemoryDomain::VaapiSurface),
        x if x == DgMemoryDomain::CudaDevice as i32 => Ok(MemoryDomain::CudaDevice),
        x if x == DgMemoryDomain::MppBuffer as i32 => Ok(MemoryDomain::MppBuffer),
        x if x == DgMemoryDomain::SophonDevice as i32 => Ok(MemoryDomain::SophonDevice),
        x if x == DgMemoryDomain::Opaque as i32 => Ok(MemoryDomain::Opaque),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown memory domain discriminant {other}"),
        )),
    }
}

fn backend_kind_from_c(kind: i32) -> Result<BackendKind, (DgStatus, String)> {
    match kind {
        x if x == DgBackendKind::Mock as i32 => Ok(BackendKind::Mock),
        x if x == DgBackendKind::OpenVino as i32 => Ok(BackendKind::OpenVINO),
        x if x == DgBackendKind::Rknn as i32 => Ok(BackendKind::Rknn),
        x if x == DgBackendKind::TensorRt as i32 => Ok(BackendKind::TensorRt),
        x if x == DgBackendKind::Sophon as i32 => Ok(BackendKind::Sophon),
        other => Err((
            DgStatus::InvalidArgument,
            format!("unknown backend kind discriminant {other}"),
        )),
    }
}

fn backend_name(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::Mock => "mock",
        BackendKind::OpenVINO => "openvino",
        BackendKind::Rknn => "rknn",
        BackendKind::TensorRt => "tensorrt",
        BackendKind::Sophon => "sophon",
    }
}

fn c_dtype(dtype: DataType) -> Result<DgDataType, (DgStatus, String)> {
    let value = match (dtype.code, dtype.bits, dtype.lanes) {
        (TypeCode::Uint, 8, 1) => DgDataType::U8,
        (TypeCode::Uint, 16, 1) => DgDataType::U16,
        (TypeCode::Int, 4, 1) => DgDataType::I4,
        (TypeCode::Int, 8, 1) => DgDataType::I8,
        (TypeCode::Int, 16, 1) => DgDataType::I16,
        (TypeCode::Float4, 4, 1) => DgDataType::F4,
        (TypeCode::Float8, 8, 1) => DgDataType::F8,
        (TypeCode::Float, 16, 1) => DgDataType::F16,
        (TypeCode::Bfloat, 16, 1) => DgDataType::Bf16,
        (TypeCode::Float, 32, 1) => DgDataType::F32,
        (TypeCode::Float, 64, 1) => DgDataType::F64,
        (TypeCode::Uint, 32, 1) => DgDataType::U32,
        (TypeCode::Int, 32, 1) => DgDataType::I32,
        (TypeCode::Uint, 64, 1) => DgDataType::U64,
        (TypeCode::Int, 64, 1) => DgDataType::I64,
        _ => {
            return Err((
                DgStatus::Unsupported,
                format!("unsupported data type: {dtype:?}"),
            ));
        }
    };
    Ok(value)
}

fn c_format(format: DataFormat) -> DgDataFormat {
    match format {
        DataFormat::Auto => DgDataFormat::Auto,
        DataFormat::N => DgDataFormat::N,
        DataFormat::NC => DgDataFormat::Nc,
        DataFormat::NCHW => DgDataFormat::Nchw,
        DataFormat::NHWC => DgDataFormat::Nhwc,
        DataFormat::NC4HW => DgDataFormat::Nc4hw,
        DataFormat::NC8HW => DgDataFormat::Nc8hw,
        DataFormat::NCDHW => DgDataFormat::Ncdhw,
        DataFormat::OIHW => DgDataFormat::Oihw,
    }
}

fn c_tensor_info(info: &dg_runtime::TensorInfo) -> Result<DgTensorInfo, (DgStatus, String)> {
    let dims = info.shape.dims();
    if dims.len() > 8 {
        return Err((
            DgStatus::Unsupported,
            "tensor rank exceeds the C ABI limit of 8".to_string(),
        ));
    }
    let mut shape = [0; 8];
    shape[..dims.len()].copy_from_slice(dims);
    let struct_size = u32::try_from(std::mem::size_of::<DgTensorInfo>()).map_err(|_| {
        (
            DgStatus::InternalError,
            "DgTensorInfo struct size exceeds u32".to_string(),
        )
    })?;
    Ok(DgTensorInfo {
        struct_size,
        struct_version: 0,
        dtype: c_dtype(info.dtype)?,
        format: c_format(info.layout.unwrap_or(DataFormat::Auto)),
        device: c_device(info.device),
        rank: dims.len(),
        shape,
    })
}

fn c_device(device: DeviceKind) -> DgDeviceKind {
    match device {
        DeviceKind::Cpu => DgDeviceKind::Cpu,
        DeviceKind::IntelGpu => DgDeviceKind::IntelGpu,
        DeviceKind::IntelNpu => DgDeviceKind::IntelNpu,
        DeviceKind::CudaGpu => DgDeviceKind::CudaGpu,
        DeviceKind::RknnNpu => DgDeviceKind::RknnNpu,
        DeviceKind::SophonTpu => DgDeviceKind::SophonTpu,
    }
}

unsafe fn bytes<'a>(data: *const u8, length: usize) -> Result<&'a [u8], (DgStatus, String)> {
    if length > MAX_VIEW_LEN {
        return Err((
            DgStatus::InvalidArgument,
            "data length exceeds the C ABI view limit".to_string(),
        ));
    }
    if length == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err((DgStatus::NullPointer, "data pointer is null".to_string()));
    }
    // SAFETY: the caller must provide a readable region of `length` bytes.
    Ok(unsafe { std::slice::from_raw_parts(data, length) })
}

unsafe fn dims<'a>(values: *const usize, rank: usize) -> Result<&'a [usize], (DgStatus, String)> {
    if rank > MAX_SHAPE_RANK {
        return Err((
            DgStatus::InvalidArgument,
            format!("shape rank {rank} exceeds the C ABI limit of {MAX_SHAPE_RANK}"),
        ));
    }
    if rank == 0 {
        return Ok(&[]);
    }
    if values.is_null() {
        return Err((DgStatus::NullPointer, "shape pointer is null".to_string()));
    }
    // SAFETY: the caller must provide `rank` readable shape dimensions.
    Ok(unsafe { std::slice::from_raw_parts(values, rank) })
}

unsafe fn c_string<'a>(value: *const c_char) -> Result<&'a CStr, (DgStatus, String)> {
    unsafe { c_string_bounded(value, MAX_CSTRING_SCAN) }
}

unsafe fn c_string_bounded<'a>(
    value: *const c_char,
    max_len: usize,
) -> Result<&'a CStr, (DgStatus, String)> {
    if value.is_null() {
        return Err((DgStatus::NullPointer, "string pointer is null".to_string()));
    }
    if max_len == 0 {
        return Err((
            DgStatus::InvalidArgument,
            "string max length is zero".to_string(),
        ));
    }
    // Scan for the NUL terminator up front so a dirty/non-NUL-terminated
    // pointer cannot force an unbounded read. The actual C string is then
    // reconstructed with `CStr::from_ptr`, which is safe because the NUL has
    // been located within the scanned region.
    for i in 0..max_len {
        if unsafe { *value.add(i) } == 0 {
            // SAFETY: `value` points to a NUL-terminated C string of length `i`.
            return Ok(unsafe { CStr::from_ptr(value) });
        }
    }
    Err((
        DgStatus::InvalidArgument,
        format!("string exceeds maximum length {max_len}"),
    ))
}

fn tensor_from_bytes(
    data: &[u8],
    shape: &[usize],
    dtype: i32,
    format: i32,
    device: i32,
) -> Result<Tensor, (DgStatus, String)> {
    let desc = TensorDesc::new(
        Shape::new(shape.to_vec()),
        data_type_from_c(dtype)?,
        format_from_c_enum(format)?,
        device_from_c(device)?,
    );
    let expected = desc
        .storage_bytes()
        .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
    ResourcePolicy::default()
        .check_tensor_bytes(expected)
        .map_err(|error| (map_core_error(&error), error.to_string()))?;
    if expected != data.len() {
        return Err((
            DgStatus::InvalidArgument,
            format!(
                "tensor byte length {}/{} does not match shape and dtype",
                data.len(),
                expected
            ),
        ));
    }
    let tensor = Tensor::allocate(&CpuDevice::new(), desc)
        .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
    tensor
        .buffer()
        .write_from_slice(data)
        .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
    Ok(tensor)
}

/// Returns the package version as a static UTF-8 C string.
#[no_mangle]
pub extern "C" fn dg_version() -> *const c_char {
    c"0.1.0".as_ptr()
}

/// Returns the stable C ABI version as a static UTF-8 C string.
#[no_mangle]
pub extern "C" fn dg_abi_version() -> *const c_char {
    c"2.0".as_ptr()
}

/// Returns the diagnostic status code stored in an error handle.
#[no_mangle]
pub unsafe extern "C" fn dg_error_status(error: *const DgError) -> DgStatus {
    if error.is_null() {
        return DgStatus::NullPointer;
    }
    // SAFETY: `error` is a valid `DgError` handle.
    unsafe { (*error).status }
}

/// Returns the error category (e.g. "InvalidArgument") as a stable UTF-8 C
/// string. The pointer is valid as long as `error` is not freed.
#[no_mangle]
pub unsafe extern "C" fn dg_error_category(error: *const DgError) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    // SAFETY: `error` is a valid `DgError` handle.
    unsafe { (*error).category.as_ptr() }
}

/// Returns the operation name associated with the error, or "Unknown".
#[no_mangle]
pub unsafe extern "C" fn dg_error_operation(error: *const DgError) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    // SAFETY: `error` is a valid `DgError` handle.
    unsafe { (*error).operation.as_ptr() }
}

/// Returns the full human-readable error message.
#[no_mangle]
pub unsafe extern "C" fn dg_error_message(error: *const DgError) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    // SAFETY: `error` is a valid `DgError` handle.
    unsafe { (*error).message.as_ptr() }
}

/// Frees an error handle obtained from a fallible v2 ABI call.
#[no_mangle]
pub unsafe extern "C" fn dg_error_free(error: *mut DgError) {
    if !error.is_null() {
        // SAFETY: `error` was obtained from a successful v2 call.
        drop(Box::from_raw(error));
    }
}

/// Returns a pointer to the owned byte buffer's contents.
///
/// The returned pointer is always non-null and aligned, even when the buffer
/// is empty. Callers must use `dg_owned_bytes_len` to determine the number of
/// valid bytes and must not dereference the pointer when the length is zero.
#[no_mangle]
pub unsafe extern "C" fn dg_owned_bytes_data(owned: *const DgOwnedBytes) -> *const u8 {
    if owned.is_null() {
        return ptr::null();
    }
    // SAFETY: `owned` is a valid `DgOwnedBytes` handle.
    unsafe { (*owned).data() }
}

/// Returns the length of the owned byte buffer in bytes.
#[no_mangle]
pub unsafe extern "C" fn dg_owned_bytes_len(owned: *const DgOwnedBytes) -> usize {
    if owned.is_null() {
        return 0;
    }
    // SAFETY: `owned` is a valid `DgOwnedBytes` handle.
    unsafe { (*owned).len() }
}

/// Frees an owned byte handle obtained from a v2 ABI call.
#[no_mangle]
pub unsafe extern "C" fn dg_owned_bytes_free(owned: *mut DgOwnedBytes) {
    if !owned.is_null() {
        // SAFETY: `owned` was obtained from a successful v2 call.
        drop(Box::from_raw(owned));
    }
}

/// Returns a JSON object describing this build's C ABI capabilities.
///
/// On success writes a `DgOwnedBytes` handle to `out`. On failure writes null
/// to `out` and, if non-null, an error handle to `out_error`.
#[no_mangle]
pub unsafe extern "C" fn dg_build_capabilities_json(
    out: *mut *mut DgOwnedBytes,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        let payload = serde_json::json!({
            "abi_version": "2.0",
            "package_version": env!("CARGO_PKG_VERSION"),
            "apis": [
                "dg_runtime_init",
                "dg_engine_init",
                "dg_engine_stop",
                "dg_engine_shutdown",
                "dg_engine_status",
                "dg_engine_metrics",
                "dg_engine_reload_string",
                "dg_engine_reload_file",
                "dg_build_capabilities_json"
            ],
            "hot_reload_live": true,
            "free_shutdown": true,
            "engine_busy_lock": true,
        });
        let text = serde_json::to_string(&payload)
            .map_err(|error| (DgStatus::InternalError, error.to_string()))?;
        Ok(Box::into_raw(Box::new(DgOwnedBytes::new(
            text.into_bytes(),
        ))))
    })
}

/// Idempotent process-level runtime bootstrap (INT5-09).
///
/// Installs built-in stream connectors when the `stream`/`cheetah` features are
/// enabled. `options` may be null for defaults; when non-null, `struct_size`
/// must match `sizeof(DgRuntimeInitOptions)`.
#[no_mangle]
pub unsafe extern "C" fn dg_runtime_init(
    options: *const DgRuntimeInitOptions,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if !options.is_null() {
            // Read only the version fields through a raw pointer to avoid creating
            // a reference to a potentially uninitialized C struct.
            let (struct_size, struct_version) = unsafe {
                (
                    std::ptr::addr_of!((*options).struct_size).read(),
                    std::ptr::addr_of!((*options).struct_version).read(),
                )
            };
            check_struct_version(
                "DgRuntimeInitOptions",
                struct_size,
                struct_version,
                std::mem::size_of::<DgRuntimeInitOptions>(),
            )?;
        }
        static INIT: OnceLock<Result<(), String>> = OnceLock::new();
        let result = INIT.get_or_init(|| {
            #[cfg(all(feature = "stream", feature = "cheetah"))]
            {
                if let Err(error) = dg_stream::install_embedded_cheetah_connector() {
                    return Err(error.to_string());
                }
            }
            Ok(())
        });
        result
            .as_ref()
            .map_err(|error| (DgStatus::RuntimeError, error.clone()))?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Creates an engine handle.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_create(
    out: *mut *mut DgEngine,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        Ok(Arc::into_raw(Arc::new(DgEngine {
            inner: RwLock::new(Engine::new()),
        })) as *mut DgEngine)
    })
}

/// Destroys an engine handle with a timeout in milliseconds. Null is accepted.
///
/// On success the handle is freed. If the running graph cannot be shut down
/// within `timeout_ms`, `DgStatus::Busy` is returned and the handle remains
/// valid so the caller can retry. Other shutdown errors still free the handle
/// because the running graph has already stopped; the returned error handle
/// explains the failure.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_destroy(
    engine: *mut DgEngine,
    timeout_ms: u64,
    out_error: *mut *mut DgError,
) -> DgStatus {
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid pointer; we own writing the initial null.
        unsafe { out_error.write(ptr::null_mut()) };
    }
    match catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return DgStatus::Ok;
        }
        // SAFETY: `engine` is a valid handle returned by `dg_engine_create`.
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut guard = match lock_engine_write(&engine_arc) {
            Ok(guard) => guard,
            Err((status, message)) => {
                write_error(out_error, status, message);
                return status;
            }
        };
        match guard.shutdown(timeout_ms) {
            Ok(()) => {
                drop(guard);
                // Reclaim the handle's owning `Arc` reference. Concurrent calls that
                // cloned the `Arc` keep the engine alive until they finish.
                unsafe { drop(Arc::from_raw(engine as *const DgEngine)) };
                DgStatus::Ok
            }
            Err(dg_graph::Error::Timeout(_)) => {
                drop(guard);
                write_error(
                    out_error,
                    DgStatus::Busy,
                    "shutdown timed out; retry with a larger timeout".to_string(),
                );
                DgStatus::Busy
            }
            Err(error) => {
                let (status, message) = map_graph_error(error);
                drop(guard);
                write_error(out_error, status, message);
                // The running graph has already stopped; reclaim the owning reference.
                unsafe { drop(Arc::from_raw(engine as *const DgEngine)) };
                status
            }
        }
    })) {
        Ok(status) => status,
        Err(_) => {
            write_error(
                out_error,
                DgStatus::Panic,
                "panic crossed C ABI boundary".to_string(),
            );
            DgStatus::Panic
        }
    }
}

/// Loads a graph specification from a UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_load_string(
    engine: *mut DgEngine,
    format: i32,
    content: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let content = unsafe { c_string(content)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::from_str_with_format(content, format_from_c(format)?)
            .map_err(map_graph_error)?;
        spec.validate().map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before loading a new spec".to_string(),
            ));
        }
        engine.spec = spec;
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Loads a graph specification from a UTF-8 path.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_load_file(
    engine: *mut DgEngine,
    path: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let path = unsafe { c_string(path)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::load_from_path(Path::new(path)).map_err(map_graph_error)?;
        spec.validate().map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before loading a new spec".to_string(),
            ));
        }
        engine.spec = spec;
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Reloads a graph specification from a UTF-8 string.
///
/// A built graph is updated in place and remains ready to run. Reload is rejected while inputs
/// are pending so that queued data is never silently interpreted by a changed graph.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_reload_string(
    engine: *mut DgEngine,
    format: i32,
    content: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let content = unsafe { c_string(content)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::from_str_with_format(content, format_from_c(format)?)
            .map_err(map_graph_error)?;
        spec.validate().map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine.reload(spec).map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Reloads a graph specification from a UTF-8 path.
///
/// A built graph is updated in place and remains ready to run. Reload is rejected while inputs
/// are pending so that queued data is never silently interpreted by a changed graph.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_reload_file(
    engine: *mut DgEngine,
    path: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let path = unsafe { c_string(path)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::load_from_path(Path::new(path)).map_err(map_graph_error)?;
        spec.validate().map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine.reload(spec).map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Computes node and connection changes against a UTF-8 graph specification.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_diff_string(
    engine: *const DgEngine,
    format: i32,
    content: *const c_char,
    out_added_nodes: *mut usize,
    out_removed_nodes: *mut usize,
    out_updated_nodes: *mut usize,
    out_added_connections: *mut usize,
    out_removed_connections: *mut usize,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        prepare_diff_outputs(
            out_added_nodes,
            out_removed_nodes,
            out_updated_nodes,
            out_added_connections,
            out_removed_connections,
        )?;
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let content = unsafe { c_string(content)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::from_str_with_format(content, format_from_c(format)?)
            .map_err(map_graph_error)?;
        spec.validate().map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let engine = lock_engine_read(&engine_arc)?;
        let diff = Graph::diff(&engine.spec, &spec);
        write_diff_counts(
            &diff,
            out_added_nodes,
            out_removed_nodes,
            out_updated_nodes,
            out_added_connections,
            out_removed_connections,
        )
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Computes node and connection changes against a UTF-8 graph file.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_diff_file(
    engine: *const DgEngine,
    path: *const c_char,
    out_added_nodes: *mut usize,
    out_removed_nodes: *mut usize,
    out_updated_nodes: *mut usize,
    out_added_connections: *mut usize,
    out_removed_connections: *mut usize,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        prepare_diff_outputs(
            out_added_nodes,
            out_removed_nodes,
            out_updated_nodes,
            out_added_connections,
            out_removed_connections,
        )?;
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let path = unsafe { c_string(path)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let spec = GraphSpec::load_from_path(Path::new(path)).map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let engine = lock_engine_read(&engine_arc)?;
        let diff = Graph::diff(&engine.spec, &spec);
        write_diff_counts(
            &diff,
            out_added_nodes,
            out_removed_nodes,
            out_updated_nodes,
            out_added_connections,
            out_removed_connections,
        )
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Adds a node programmatically. `params_json` may be null for an empty object.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_add_node(
    engine: *mut DgEngine,
    name: *const c_char,
    kind: *const c_char,
    params_json: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let name = unsafe { c_string(name)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?
            .to_string();
        let kind = unsafe { c_string(kind)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?
            .to_string();
        let params = if params_json.is_null() {
            Value::Object(Map::new())
        } else {
            let params = unsafe { c_string(params_json)? }
                .to_str()
                .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
            serde_json::from_str(params)
                .map_err(|error| (DgStatus::ParseError, error.to_string()))?
        };
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before adding nodes".to_string(),
            ));
        }
        engine.spec.nodes.push(NodeSpec {
            name,
            kind,
            template: None,
            params,
            ..NodeSpec::default()
        });
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Removes a node by name and its incident connections.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_remove_node(
    engine: *mut DgEngine,
    name: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let name = unsafe { c_string(name)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before removing nodes".to_string(),
            ));
        }
        engine.spec.nodes.retain(|node| node.name != name);
        engine.spec.connections.retain(|connection| {
            dg_graph::ConnectionSpec::parse(connection)
                .is_ok_and(|parsed| parsed.from_node != name && parsed.to_node != name)
        });
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Adds a graph edge in `source.port -> destination.port` form.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_connect(
    engine: *mut DgEngine,
    connection: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let connection = unsafe { c_string(connection)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        dg_graph::ConnectionSpec::parse(connection).map_err(map_graph_error)?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before modifying connections".to_string(),
            ));
        }
        engine.spec.connections.push(connection.to_string());
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Removes a graph edge.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_disconnect(
    engine: *mut DgEngine,
    connection: *const c_char,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let connection = unsafe { c_string(connection)? }
            .to_str()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before modifying connections".to_string(),
            ));
        }
        engine.spec.connections.retain(|item| item != connection);
        engine.invalidate();
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Validates and builds the current graph specification.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_build(
    engine: *mut DgEngine,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        if engine.running.is_some() {
            return Err((
                DgStatus::RuntimeError,
                "graph is currently running; shutdown before building".to_string(),
            ));
        }
        engine.spec.validate().map_err(map_graph_error)?;
        engine.graph = Some(Graph::new(engine.spec.clone()).map_err(map_graph_error)?);
        engine.outputs.clear();
        engine.stream_ended = false;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Runs the built graph with pending inputs and stores sink outputs for polling.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_run(
    engine: *mut DgEngine,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine.run().map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Starts the engine as a long-running graph.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_init(
    engine: *mut DgEngine,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine.init().map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Requests a cooperative stop of the running graph.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_stop(
    engine: *mut DgEngine,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let engine = lock_engine_read(&engine_arc)?;
        engine.stop().map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Shuts down the running graph with a timeout in milliseconds.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_shutdown(
    engine: *mut DgEngine,
    timeout_ms: u64,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine.shutdown(timeout_ms).map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Returns the current lifecycle status of the engine. On success `out_status`
/// is written; if the status is `Failed` and `out_cause` is non-null, an owned
/// byte handle containing the root cause is written to `out_cause`.
/// On any error `out_status` is set to `DgGraphStatus::NotRunning`.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_status(
    engine: *const DgEngine,
    out_status: *mut DgGraphStatus,
    out_cause: *mut *mut DgOwnedBytes,
    out_error: *mut *mut DgError,
) -> DgStatus {
    if out_status.is_null() {
        write_error(
            out_error,
            DgStatus::NullPointer,
            "out_status pointer is null",
        );
        return DgStatus::NullPointer;
    }
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid pointer; we own writing the initial null.
        unsafe { out_error.write(ptr::null_mut()) };
    }
    if !out_cause.is_null() {
        // SAFETY: `out_cause` is a valid pointer; we own writing the initial null.
        unsafe { out_cause.write(ptr::null_mut()) };
    }
    match catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let engine = lock_engine_read(&engine_arc)?;
        Ok(engine.status())
    })) {
        Ok(Ok((status, cause))) => {
            // SAFETY: `out_status` was checked non-null above.
            unsafe { out_status.write(status) };
            if status == DgGraphStatus::Failed && !out_cause.is_null() {
                let cause = cause.unwrap_or_else(|| "unknown failure".to_string());
                // SAFETY: `out_cause` is non-null and points to writable storage.
                unsafe {
                    out_cause.write(Box::into_raw(Box::new(DgOwnedBytes::new(
                        cause.into_bytes(),
                    ))))
                };
            }
            DgStatus::Ok
        }
        Ok(Err((status, message))) => {
            write_error(out_error, status, message);
            // SAFETY: `out_status` was checked non-null above.
            unsafe { out_status.write(DgGraphStatus::NotRunning) };
            status
        }
        Err(_) => {
            let status = DgStatus::Panic;
            write_error(out_error, status, "panic crossed C ABI boundary");
            // SAFETY: `out_status` was checked non-null above.
            unsafe { out_status.write(DgGraphStatus::NotRunning) };
            status
        }
    }
}

/// Returns a JSON snapshot of per-element metrics as an owned byte handle.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_metrics(
    engine: *const DgEngine,
    out: *mut *mut DgOwnedBytes,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        if engine.is_null() {
            return Err((DgStatus::NullPointer, "engine pointer is null".to_string()));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let engine = lock_engine_read(&engine_arc)?;
        let json = engine.metrics_json().map_err(map_graph_error)?;
        Ok(Box::into_raw(Box::new(DgOwnedBytes::new(
            json.into_bytes(),
        ))))
    })
}

/// Creates and initializes a backend without constructing a graph.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_create(
    kind: i32,
    model_data: *const u8,
    model_length: usize,
    options_json: *const c_char,
    out: *mut *mut DgBackend,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        ResourcePolicy::default()
            .check_model_bytes(model_length)
            .map_err(|error| (map_core_error(&error), error.to_string()))?;
        let model = unsafe { bytes(model_data, model_length)? }.to_vec();
        let options = if options_json.is_null() {
            Value::Object(Map::new())
        } else {
            let text = unsafe { c_string(options_json)? }
                .to_str()
                .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
            serde_json::from_str(text).map_err(|error| (DgStatus::ParseError, error.to_string()))?
        };
        let kind = backend_kind_from_c(kind)?;
        let config = BackendConfig::new(None, options);
        let mut option = configure_backend(backend_name(kind), config)
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        option.model_source = ModelSource::Bytes(model);
        let mut backend =
            create_backend(kind).map_err(|error| (DgStatus::Unsupported, error.to_string()))?;
        backend
            .init(&option)
            .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        Ok(Arc::into_raw(Arc::new(DgBackend {
            backend: Mutex::new(backend),
        })) as *mut DgBackend)
    })
}

/// Frees a direct backend handle. Null is accepted.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_free(backend: *mut DgBackend) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !backend.is_null() {
            // SAFETY: the pointer must have been returned by `dg_backend_create` exactly once.
            // Dropping this `Arc` releases the handle's reference; concurrent calls that
            // cloned the `Arc` will keep the inner backend alive until they finish.
            unsafe { drop(Arc::from_raw(backend as *const DgBackend)) };
        }
    }));
}

/// Returns the number of backend inputs and outputs.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_io_counts(
    backend: *const DgBackend,
    out_inputs: *mut usize,
    out_outputs: *mut usize,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if backend.is_null() || out_inputs.is_null() || out_outputs.is_null() {
            return Err((
                DgStatus::NullPointer,
                "backend or count output pointer is null".to_string(),
            ));
        }
        unsafe {
            out_inputs.write(0);
            out_outputs.write(0);
        }
        let backend_arc = unsafe { clone_backend_arc(backend) };
        let backend = lock_backend(&backend_arc);
        unsafe {
            out_inputs.write(backend.input_count());
            out_outputs.write(backend.output_count());
        }
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Queries runtime device and precision capabilities.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_capabilities(
    backend: *const DgBackend,
    out: *mut DgBackendCapabilities,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        if backend.is_null() || out.is_null() {
            return Err((
                DgStatus::NullPointer,
                "backend or capabilities output pointer is null".to_string(),
            ));
        }
        // Read only the version fields through a raw pointer to avoid creating a
        // reference to a potentially uninitialized C struct.
        let (struct_size, struct_version) = unsafe {
            (
                std::ptr::addr_of!((*out).struct_size).read(),
                std::ptr::addr_of!((*out).struct_version).read(),
            )
        };
        check_struct_version(
            "DgBackendCapabilities",
            struct_size,
            struct_version,
            std::mem::size_of::<DgBackendCapabilities>(),
        )?;
        let backend_arc = unsafe { clone_backend_arc(backend) };
        let backend = lock_backend(&backend_arc);
        let capabilities = backend
            .probe_capabilities()
            .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        if capabilities.devices.len() > 8 || capabilities.precisions.len() > 16 {
            return Err((
                DgStatus::Unsupported,
                "runtime capabilities exceed the C ABI limits".to_string(),
            ));
        }
        let mut devices = [DgDeviceKind::Cpu; 8];
        for (slot, device) in capabilities.devices.iter().copied().enumerate() {
            devices[slot] = c_device(device);
        }
        let mut precisions = [DgDataType::U8; 16];
        for (slot, precision) in capabilities.precisions.iter().copied().enumerate() {
            precisions[slot] = c_dtype(precision)?;
        }
        let struct_size =
            u32::try_from(std::mem::size_of::<DgBackendCapabilities>()).map_err(|_| {
                (
                    DgStatus::InternalError,
                    "DgBackendCapabilities struct size exceeds u32".to_string(),
                )
            })?;
        Ok(DgBackendCapabilities {
            struct_size,
            struct_version: 0,
            device_count: capabilities.devices.len(),
            devices,
            precision_count: capabilities.precisions.len(),
            precisions,
        })
    })
}

/// Queries one input or output tensor description.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_tensor_info(
    backend: *const DgBackend,
    output: bool,
    index: usize,
    out: *mut DgTensorInfo,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        if backend.is_null() || out.is_null() {
            return Err((
                DgStatus::NullPointer,
                "backend or tensor-info output pointer is null".to_string(),
            ));
        }
        // Read only the version fields through a raw pointer to avoid creating a
        // reference to a potentially uninitialized C struct.
        let (struct_size, struct_version) = unsafe {
            (
                std::ptr::addr_of!((*out).struct_size).read(),
                std::ptr::addr_of!((*out).struct_version).read(),
            )
        };
        check_struct_version(
            "DgTensorInfo",
            struct_size,
            struct_version,
            std::mem::size_of::<DgTensorInfo>(),
        )?;
        let backend_arc = unsafe { clone_backend_arc(backend) };
        let backend = lock_backend(&backend_arc);
        let info = if output {
            backend.output_info(index)
        } else {
            backend.input_info(index)
        }
        .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        c_tensor_info(info)
    })
}

/// Runs direct backend inference over caller-owned tensor handles.
#[no_mangle]
pub unsafe extern "C" fn dg_backend_run(
    backend: *mut DgBackend,
    inputs: *const *const DgTensor,
    input_count: usize,
    outputs: *mut *mut DgTensor,
    output_capacity: usize,
    out_count: *mut usize,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if backend.is_null() || out_count.is_null() {
            return Err((
                DgStatus::NullPointer,
                "backend or output-count pointer is null".to_string(),
            ));
        }
        unsafe { out_count.write(0) };
        if input_count > 0 && inputs.is_null() {
            return Err((DgStatus::NullPointer, "input array is null".to_string()));
        }
        let backend_arc = unsafe { clone_backend_arc(backend as *const DgBackend) };
        let mut backend = lock_backend(&backend_arc);
        let expected_inputs = backend.input_count();
        if input_count > expected_inputs {
            return Err((
                DgStatus::InvalidArgument,
                format!("input_count {input_count} exceeds backend input count {expected_inputs}"),
            ));
        }
        let input_handles = if input_count == 0 {
            &[][..]
        } else {
            // SAFETY: the caller supplies `input_count` valid tensor pointers.
            unsafe { std::slice::from_raw_parts(inputs, input_count) }
        };
        let mut tensors = Vec::with_capacity(input_count);
        for handle in input_handles {
            if handle.is_null() {
                return Err((DgStatus::NullPointer, "input tensor is null".to_string()));
            }
            // SAFETY: `clone_tensor_arc` keeps the handle alive for the clone below.
            let tensor_arc = unsafe { clone_tensor_arc(*handle) };
            tensors.push(tensor_arc.tensor.clone());
        }
        let produced = backend
            .run(&tensors)
            .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        let produced_count = produced.len();
        if produced.len() > output_capacity || (!produced.is_empty() && outputs.is_null()) {
            return Err((
                DgStatus::InvalidArgument,
                "output array capacity is too small".to_string(),
            ));
        }
        for (slot, tensor) in produced.into_iter().enumerate() {
            let dg_tensor = Arc::into_raw(Arc::new(DgTensor { tensor })) as *mut DgTensor;
            // SAFETY: `outputs` was checked non-null and has capacity for all produced tensors.
            unsafe { outputs.add(slot).write(dg_tensor) };
        }
        unsafe { out_count.write(produced_count) };
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Creates a host tensor from a caller-owned byte array.
#[no_mangle]
pub unsafe extern "C" fn dg_tensor_create(
    data: *const u8,
    length: usize,
    shape: *const usize,
    rank: usize,
    dtype: i32,
    format: i32,
    device: i32,
    out: *mut *mut DgTensor,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        let data = unsafe { bytes(data, length)? };
        let shape = unsafe { dims(shape, rank)? };
        let tensor = tensor_from_bytes(data, shape, dtype, format, device)?;
        Ok(Arc::into_raw(Arc::new(DgTensor { tensor })) as *mut DgTensor)
    })
}

/// Creates a tensor backed by an imported external buffer.
#[no_mangle]
pub unsafe extern "C" fn dg_tensor_create_external(
    desc: *const DgExternalMemoryV2,
    shape: *const usize,
    rank: usize,
    dtype: i32,
    format: i32,
    out: *mut *mut DgTensor,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        let (device, domain, size_bytes, fd, raw, release, user_data) =
            parse_external_memory_descriptor(desc)?;
        let shape = unsafe { dims(shape, rank)? };
        let tensor_desc = TensorDesc::new(
            Shape::new(shape.to_vec()),
            data_type_from_c(dtype)?,
            format_from_c_enum(format)?,
            device,
        );
        let expected = tensor_desc
            .storage_bytes()
            .map_err(|error| (DgStatus::InvalidArgument, error.to_string()))?;
        ResourcePolicy::default()
            .check_tensor_bytes(expected)
            .map_err(|error| (map_core_error(&error), error.to_string()))?;
        if expected != size_bytes {
            return Err((
                DgStatus::InvalidArgument,
                format!("external size {size_bytes} does not match tensor size {expected}"),
            ));
        }
        let (external, guard) = build_external_handle(fd, raw, release, user_data)?;
        let buffer = Buffer::from_external(
            device,
            domain,
            BufferDesc::new(size_bytes, 1),
            external,
            guard,
        )
        .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        let tensor = Tensor::from_buffer(tensor_desc, buffer)
            .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        Ok(Arc::into_raw(Arc::new(DgTensor { tensor })) as *mut DgTensor)
    })
}

/// Frees a tensor handle. Null is accepted.
#[no_mangle]
pub unsafe extern "C" fn dg_tensor_free(tensor: *mut DgTensor) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !tensor.is_null() {
            // SAFETY: the pointer must have been returned by a tensor constructor exactly once.
            unsafe { drop(Arc::from_raw(tensor as *const DgTensor)) };
        }
    }));
}

/// Returns a copied tensor byte snapshot as an owned byte handle.
#[no_mangle]
pub unsafe extern "C" fn dg_tensor_data(
    tensor: *const DgTensor,
    out: *mut *mut DgOwnedBytes,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        if tensor.is_null() {
            return Err((DgStatus::NullPointer, "tensor pointer is null".to_string()));
        }
        let tensor_arc = unsafe { clone_tensor_arc(tensor) };
        let snapshot = tensor_arc
            .tensor
            .buffer()
            .try_read_bytes()
            .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        Ok(Box::into_raw(Box::new(DgOwnedBytes::new(snapshot))))
    })
}

/// Pushes one tensor into the built graph.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_push(
    engine: *mut DgEngine,
    tensor: *const DgTensor,
    out_error: *mut *mut DgError,
) -> DgStatus {
    match ffi_result(out_error, || {
        if engine.is_null() || tensor.is_null() {
            return Err((
                DgStatus::NullPointer,
                "engine or tensor pointer is null".to_string(),
            ));
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let tensor_arc = unsafe { clone_tensor_arc(tensor) };
        let mut engine = lock_engine_write(&engine_arc)?;
        engine
            .push(tensor_arc.tensor.clone())
            .map_err(map_graph_error)?;
        Ok(())
    }) {
        Ok(()) => DgStatus::Ok,
        Err(status) => status,
    }
}

/// Polls one output tensor. `Again` means the queue is empty and the graph is
/// still running. `EndOfStream` means the running graph has stopped and all
/// queued output tensors have been consumed. On `Again`/`EndOfStream` no
/// `DgError` is written and `*out` is set to null.
#[no_mangle]
pub unsafe extern "C" fn dg_engine_poll(
    engine: *mut DgEngine,
    out: *mut *mut DgTensor,
    out_error: *mut *mut DgError,
) -> DgStatus {
    if out.is_null() {
        write_error(out_error, DgStatus::NullPointer, "output pointer is null");
        return DgStatus::NullPointer;
    }
    if !out_error.is_null() {
        // SAFETY: `out_error` is a valid pointer; we own writing the initial null.
        unsafe { out_error.write(ptr::null_mut()) };
    }
    // SAFETY: `out` was checked non-null above.
    unsafe { out.write(ptr::null_mut()) };

    enum PollResult {
        Tensor(usize),
        Again,
        EndOfStream,
        Error(DgStatus, String),
    }

    match catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return PollResult::Error(DgStatus::NullPointer, "engine pointer is null".to_string());
        }
        let engine_arc = unsafe { clone_engine_arc(engine) };
        let result = match lock_engine_write(&engine_arc) {
            Ok(mut engine) => match engine.poll_output() {
                Ok(EnginePollOutput::Tensor(tensor)) => {
                    PollResult::Tensor(Arc::into_raw(Arc::new(DgTensor { tensor })) as usize)
                }
                Ok(EnginePollOutput::Pending) => PollResult::Again,
                Ok(EnginePollOutput::EndOfStream) => PollResult::EndOfStream,
                Err(error) => {
                    let (status, message) = map_graph_error(error);
                    PollResult::Error(status, message)
                }
            },
            Err((status, message)) => PollResult::Error(status, message),
        };
        // Drop `engine_arc` before returning so the `RwLockWriteGuard` inside the
        // match arms does not outlive the cloned `Arc` reference.
        drop(engine_arc);
        result
    })) {
        Ok(PollResult::Tensor(tensor)) => {
            // SAFETY: `out` was checked non-null above and `tensor` came from `Box::into_raw`.
            unsafe { out.write(tensor as *mut DgTensor) };
            DgStatus::Ok
        }
        Ok(PollResult::Again) => DgStatus::Again,
        Ok(PollResult::EndOfStream) => DgStatus::EndOfStream,
        Ok(PollResult::Error(status, message)) => {
            write_error(out_error, status, message);
            status
        }
        Err(_) => {
            let status = DgStatus::Panic;
            write_error(out_error, status, "panic crossed C ABI boundary");
            status
        }
    }
}

/// Imports an external buffer handle without dereferencing the external address.
#[no_mangle]
pub unsafe extern "C" fn dg_buffer_import_external(
    desc: *const DgExternalMemoryV2,
    out: *mut *mut DgBuffer,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out, out_error, || {
        let (device, domain, size_bytes, fd, raw, release, user_data) =
            parse_external_memory_descriptor(desc)?;
        ResourcePolicy::default()
            .check_buffer_bytes(size_bytes)
            .map_err(|error| (map_core_error(&error), error.to_string()))?;
        let (external, guard) = build_external_handle(fd, raw, release, user_data)?;
        let buffer = Buffer::from_external(
            device,
            domain,
            BufferDesc::new(size_bytes, 1),
            external,
            guard,
        )
        .map_err(|error| (DgStatus::RuntimeError, error.to_string()))?;
        Ok(Arc::into_raw(Arc::new(DgBuffer { buffer })) as *mut DgBuffer)
    })
}

/// Returns the logical size of an imported buffer.
#[no_mangle]
pub unsafe extern "C" fn dg_buffer_size(
    buffer: *const DgBuffer,
    out_size: *mut usize,
    out_error: *mut *mut DgError,
) -> DgStatus {
    ffi_result_with_out(out_size, out_error, || {
        if buffer.is_null() {
            return Err((DgStatus::NullPointer, "buffer pointer is null".to_string()));
        }
        let buffer_arc = unsafe { clone_buffer_arc(buffer) };
        Ok(buffer_arc.buffer.len())
    })
}

/// Frees an external buffer handle.
#[no_mangle]
pub unsafe extern "C" fn dg_buffer_free(buffer: *mut DgBuffer) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !buffer.is_null() {
            // SAFETY: the pointer must have been returned by `dg_buffer_import_external`.
            unsafe { drop(Arc::from_raw(buffer as *const DgBuffer)) };
        }
    }));
}

/// Returns the package version as a Rust string for compatibility with M0.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use dg_graph::{
        inventory, CreatedElement, Element, ElementDescriptor, ElementHandle, ElementIo,
        ParamField, PortSchema,
    };

    const PANIC_IN: PortSchema = PortSchema {
        name: "in",
        dtype: Some(DataType::F32),
        required: true,
    };
    const PANIC_OUT: PortSchema = PortSchema {
        name: "out",
        dtype: Some(DataType::F32),
        required: false,
    };
    const NO_PARAMS: &[ParamField] = &[];

    struct PanicElement;

    impl Element for PanicElement {
        fn run(self: Box<Self>, _io: ElementIo) -> dg_graph::Result<()> {
            panic!("capi test panic");
        }
    }

    fn create_panic(_node: &dg_graph::NodeSpec) -> dg_graph::Result<CreatedElement> {
        Ok(CreatedElement {
            element: Box::new(PanicElement),
            handle: ElementHandle::None,
        })
    }

    inventory::submit! {
        ElementDescriptor {
            kind: "capi_test_panic",
            input_ports: &[PANIC_IN],
            output_ports: &[PANIC_OUT],
            params: NO_PARAMS,
            validate: None,
            create: create_panic,
        }
    }

    unsafe extern "C" fn test_release_callback(user_data: *mut c_void) {
        if !user_data.is_null() {
            let counter = user_data as *mut AtomicUsize;
            // SAFETY: `user_data` was provided by the test as `&AtomicUsize`.
            unsafe { (*counter).fetch_add(1, Ordering::SeqCst) };
        }
    }

    #[test]
    fn unsupported_dtype_is_rejected_instead_of_mapped_to_u8() {
        let dtype = DataType::new(TypeCode::OpaqueHandle, 8, 1);
        let error = c_dtype(dtype).expect_err("opaque dtype must be unsupported");
        assert_eq!(error.0, DgStatus::Unsupported);
        assert!(error.1.contains("unsupported data type"));

        let info = dg_runtime::TensorInfo::new(dg_core::Shape::new([1]), dtype);
        let error = c_tensor_info(&info).expect_err("opaque tensor dtype must be unsupported");
        assert_eq!(error.0, DgStatus::Unsupported);
    }

    fn graph_spec() -> CString {
        CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: input
    kind: input
    params: {}
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - input.out -> infer.in
  - infer.out -> sink.in
"#,
        )
        .expect("valid graph spec")
    }

    fn streaming_graph_spec() -> CString {
        CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1, 4]
      dtype: f32
      format: nc
      start: 0.0
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#,
        )
        .expect("valid graph spec")
    }

    #[cfg(feature = "stream")]
    fn media_stream_graph_spec() -> CString {
        CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: stream_source
    kind: rtsp_src
    params:
      url: mock://app01
  - name: decode
    kind: media_decode
    params:
      width: 4
      height: 4
      channels: 3
  - name: stream_sink
    kind: rtmp_sink
    params:
      url: mock://app01
connections:
  - stream_source.out -> decode.in
  - decode.out -> stream_sink.in
"#,
        )
        .expect("valid media/stream graph spec")
    }

    fn unique_temp_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("dg-capi-invalid-{nanos}.yaml"))
    }

    fn updated_graph_spec() -> CString {
        CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: input
    kind: input
    params: {}
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: false
      fill_value: 7
  - name: sink
    kind: sink
    params: {}
  - name: extra_source
    kind: source
    params:
      count: 0
      shape: [1, 4]
  - name: extra_sink
    kind: sink
    params: {}
connections:
  - input.out -> infer.in
  - infer.out -> sink.in
  - extra_source.out -> extra_sink.in
"#,
        )
        .expect("valid updated graph spec")
    }

    #[test]
    fn c_abi_push_poll_round_trip() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut error = ptr::null_mut();
        let run_status = unsafe { dg_engine_run(engine, &mut error) };
        let error_message = if error.is_null() {
            "<missing error>".to_string()
        } else {
            let message = unsafe { CStr::from_ptr(dg_error_message(error)) }
                .to_string_lossy()
                .into_owned();
            unsafe { dg_error_free(error) };
            message
        };
        assert_eq!(run_status, DgStatus::Ok, "{}", error_message);
        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut output_owned = ptr::null_mut();
        assert_eq!(
            unsafe { dg_tensor_data(output, &mut output_owned, ptr::null_mut()) },
            DgStatus::Ok
        );
        let output_data = unsafe { dg_owned_bytes_data(output_owned) };
        let output_len = unsafe { dg_owned_bytes_len(output_owned) };
        assert_eq!(
            unsafe { std::slice::from_raw_parts(output_data, output_len) },
            input_bytes.as_slice()
        );
        unsafe { dg_owned_bytes_free(output_owned) };
        unsafe {
            dg_tensor_free(output);
            dg_tensor_free(tensor);
            dg_engine_destroy(engine, 5000, ptr::null_mut());
        }
    }

    fn graph_spec_with_tight_limits() -> CString {
        CString::new(
            r#"apiVersion: dg/v1
kind: Graph
limits:
  max_buffer_packets: 1
  max_buffer_bytes: 1
nodes:
  - name: input
    kind: input
    params: {}
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - input.out -> infer.in
  - infer.out -> sink.in
"#,
        )
        .expect("valid graph spec")
    }

    #[test]
    fn push_rejects_exceeding_pending_input_limits() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec_with_tight_limits();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );

        // First push is within the packet limit (1) but exceeds the byte limit (1).
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, &mut error) },
            DgStatus::RuntimeError
        );
        if !error.is_null() {
            unsafe { dg_error_free(error) };
        }

        unsafe {
            dg_tensor_free(tensor);
            dg_engine_destroy(engine, 5000, ptr::null_mut());
        }
    }

    #[test]
    fn engine_poll_empty_returns_again_without_error() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let mut output = ptr::null_mut();
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, &mut error) },
            DgStatus::Again
        );
        assert!(output.is_null());
        assert!(error.is_null(), "Again must not allocate a DgError");

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn engine_poll_returns_end_of_stream_when_streaming_graph_stops() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut output = ptr::null_mut();
        let mut error = ptr::null_mut();
        let mut last_status = DgStatus::Again;
        // The mock graph has no input data, so the input node eventually
        // broadcasts EOS and the graph stops. poll_output should report
        // EndOfStream rather than spinning on Again forever.
        while std::time::Instant::now() < deadline {
            last_status = unsafe { dg_engine_poll(engine, &mut output, &mut error) };
            if last_status == DgStatus::EndOfStream {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            last_status,
            DgStatus::EndOfStream,
            "dg_engine_poll must report EndOfStream after the streaming graph stops"
        );
        assert!(output.is_null());
        assert!(error.is_null(), "EndOfStream must not allocate a DgError");

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn engine_poll_reports_end_of_stream_after_shutdown() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 0
      shape: [1, 4]
      dtype: f32
      format: nc
      start: 0.0
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#,
        )
        .expect("valid streaming spec");
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        // Shut the stream down before the graph stops on its own; polling after
        // shutdown must still reach EndOfStream rather than spinning on Again.
        assert_eq!(
            unsafe { dg_engine_shutdown(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut output = ptr::null_mut();
        let mut error = ptr::null_mut();
        let mut last_status = DgStatus::Again;
        while std::time::Instant::now() < deadline {
            last_status = unsafe { dg_engine_poll(engine, &mut output, &mut error) };
            if last_status == DgStatus::EndOfStream {
                break;
            }
            if last_status == DgStatus::Ok && !output.is_null() {
                unsafe { dg_tensor_free(output) };
                output = ptr::null_mut();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            last_status,
            DgStatus::EndOfStream,
            "dg_engine_poll must report EndOfStream after shutdown"
        );
        assert!(output.is_null());
        assert!(error.is_null(), "EndOfStream must not allocate a DgError");

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn engine_poll_reports_end_of_stream_after_one_shot_run_failure() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: input
    kind: input
    params: {}
  - name: panic
    kind: capi_test_panic
  - name: sink
    kind: sink
    params: {}
connections:
  - input.out -> panic.in
  - panic.out -> sink.in
"#,
        )
        .expect("valid one-shot panic spec");
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
            DgStatus::Ok
        );

        // The panic element stops before producing any output.
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_run(engine, &mut error) },
            DgStatus::RuntimeError
        );
        if !error.is_null() {
            unsafe { dg_error_free(error) };
        }

        // Polling after a failed one-shot run must report EndOfStream, not spin
        // on Again forever.
        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::EndOfStream
        );
        assert!(output.is_null());

        unsafe {
            dg_tensor_free(tensor);
            dg_engine_destroy(engine, 5000, ptr::null_mut());
        }
    }

    #[test]
    fn engine_poll_drains_streaming_sink_tensors_before_end_of_stream() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = streaming_graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut output = ptr::null_mut();
        let mut error = ptr::null_mut();
        let mut saw_tensor = false;
        let mut last_status = DgStatus::Again;
        while std::time::Instant::now() < deadline {
            last_status = unsafe { dg_engine_poll(engine, &mut output, &mut error) };
            if last_status == DgStatus::Ok {
                saw_tensor = true;
                assert!(!output.is_null());
                unsafe { dg_tensor_free(output) };
                output = ptr::null_mut();
            } else if last_status == DgStatus::EndOfStream {
                break;
            } else {
                assert!(output.is_null());
                assert!(error.is_null(), "Again must not allocate a DgError");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            saw_tensor,
            "dg_engine_poll must return the streaming sink tensor"
        );
        assert_eq!(
            last_status,
            DgStatus::EndOfStream,
            "dg_engine_poll must report EndOfStream after the streaming graph stops"
        );
        assert!(output.is_null());
        assert!(error.is_null(), "EndOfStream must not allocate a DgError");

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn reload_after_run_clears_stale_outputs() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let first = [1.0_f32, 2.0, 3.0, 4.0];
        let second = [5.0_f32, 6.0, 7.0, 8.0];

        let run_with_input = |input: [f32; 4]| {
            let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
            let shape = [1_usize, 4];
            let mut tensor = ptr::null_mut();
            assert_eq!(
                unsafe {
                    dg_tensor_create(
                        input_bytes.as_ptr(),
                        input_bytes.len(),
                        shape.as_ptr(),
                        shape.len(),
                        DgDataType::F32 as i32,
                        DgDataFormat::Nc as i32,
                        DgDeviceKind::Cpu as i32,
                        &mut tensor,
                        ptr::null_mut(),
                    )
                },
                DgStatus::Ok
            );
            assert_eq!(
                unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
                DgStatus::Ok
            );
            let mut error = ptr::null_mut();
            let run_status = unsafe { dg_engine_run(engine, &mut error) };
            assert_eq!(run_status, DgStatus::Ok, "run failed");
            unsafe { dg_tensor_free(tensor) };
        };

        run_with_input(first);

        // Do NOT poll the first output; reload should drop stale queued outputs.
        assert_eq!(
            unsafe {
                dg_engine_reload_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );

        // After reload, only a fresh run should produce output.
        run_with_input(second);

        // The first poll must return the second input, not the stale first one.
        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut output_owned = ptr::null_mut();
        assert_eq!(
            unsafe { dg_tensor_data(output, &mut output_owned, ptr::null_mut()) },
            DgStatus::Ok
        );
        let output_data = unsafe { dg_owned_bytes_data(output_owned) };
        let output_len = unsafe { dg_owned_bytes_len(output_owned) };
        let second_bytes: Vec<u8> = second
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        assert_eq!(
            unsafe { std::slice::from_raw_parts(output_data, output_len) },
            second_bytes.as_slice()
        );
        unsafe { dg_owned_bytes_free(output_owned) };

        // One-shot execution completes after the run, so the next poll reports
        // EndOfStream once all outputs have been consumed.
        output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::EndOfStream
        );
        assert!(output.is_null());

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn init_clears_stale_one_shot_outputs() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
            DgStatus::Ok
        );
        unsafe { dg_tensor_free(tensor) };

        assert_eq!(
            unsafe { dg_engine_run(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert!(!output.is_null());
        unsafe { dg_tensor_free(output) };

        // Switching to streaming mode must discard any queued one-shot outputs.
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        output = ptr::null_mut();
        // With no pending input the mock graph stops; poll until EndOfStream
        // and verify no stale one-shot tensor is returned.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut last_status = DgStatus::Again;
        while std::time::Instant::now() < deadline {
            last_status = unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) };
            if last_status == DgStatus::EndOfStream {
                break;
            }
            assert!(output.is_null());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(last_status, DgStatus::EndOfStream);
        assert!(output.is_null());

        unsafe { dg_engine_shutdown(engine, 5000, ptr::null_mut()) };
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn graph_modifications_while_running_are_rejected() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let new_spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    new_spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::RuntimeError
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::RuntimeError
        );

        let connection = CString::new("input.out -> sink.in").expect("valid connection");
        assert_eq!(
            unsafe { dg_engine_connect(engine, connection.as_ptr(), ptr::null_mut()) },
            DgStatus::RuntimeError
        );
        assert_eq!(
            unsafe { dg_engine_disconnect(engine, connection.as_ptr(), ptr::null_mut()) },
            DgStatus::RuntimeError
        );

        let sink = CString::new("sink").expect("valid name");
        assert_eq!(
            unsafe { dg_engine_remove_node(engine, sink.as_ptr(), ptr::null_mut()) },
            DgStatus::RuntimeError
        );

        let node_name = CString::new("extra").expect("valid name");
        let node_kind = CString::new("input").expect("valid kind");
        assert_eq!(
            unsafe {
                dg_engine_add_node(
                    engine,
                    node_name.as_ptr(),
                    node_kind.as_ptr(),
                    ptr::null(),
                    ptr::null_mut(),
                )
            },
            DgStatus::RuntimeError
        );

        unsafe { dg_engine_shutdown(engine, 5000, ptr::null_mut()) };
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[cfg(feature = "stream")]
    #[test]
    fn c_abi_load_discovers_media_and_stream_elements() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = media_stream_graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn invalid_pointer_returns_status_and_error() {
        let mut error = ptr::null_mut();
        let status = unsafe { dg_engine_build(ptr::null_mut(), &mut error) };
        assert_eq!(status, DgStatus::NullPointer);
        assert!(!error.is_null());
        let message = unsafe { CStr::from_ptr(dg_error_message(error)) }
            .to_string_lossy()
            .into_owned();
        unsafe { dg_error_free(error) };
        assert!(
            message.starts_with("kind="),
            "dg_error_message must start with kind=: {message}"
        );
        assert!(
            message.contains("operation="),
            "dg_error_message must include operation=: {message}"
        );
    }

    #[test]
    fn format_c_error_message_preserves_media_fields() {
        let formatted = format_c_error_message(
            "element media_decode: kind=Unsupported profile=rkmpp-host role=decoder operation=Select backend=none detail=no candidate",
        );
        assert!(formatted.starts_with("kind=Unsupported"));
        assert!(formatted.contains("profile=rkmpp-host"));
        assert!(formatted.contains("operation=Select"));
        assert!(formatted.contains("backend=none"));
        assert!(!formatted.contains("element media_decode"));
    }

    #[test]
    fn format_c_error_message_wraps_plain_text() {
        let formatted = format_c_error_message("null engine pointer");
        assert_eq!(
            formatted,
            "kind=Other operation=Unknown detail=null engine pointer"
        );
    }

    #[cfg(feature = "avcodec-profile-software")]
    #[test]
    fn load_rejects_unknown_media_profile_with_structured_error() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: input
    kind: input
    params: {}
  - name: decode
    kind: media_decode
    params:
      profile: not-a-real-profile
      codec: jpeg
  - name: sink
    kind: sink
    params: {}
connections:
  - input.out -> decode.in
  - decode.out -> sink.in
"#,
        )
        .expect("spec");
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    &mut error,
                )
            },
            DgStatus::ParseError
        );
        assert!(!error.is_null());
        let message = unsafe { CStr::from_ptr(dg_error_message(error)) }
            .to_string_lossy()
            .into_owned();
        unsafe { dg_error_free(error) };
        assert!(message.starts_with("kind="), "{message}");
        assert!(
            message.contains("not-a-real-profile") || message.contains("unknown avcodec profile"),
            "{message}"
        );
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn load_file_rejects_invalid_graph_during_load() {
        let path = unique_temp_path();
        fs::write(
            &path,
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: duplicate
    kind: source
    params: {count: 0}
  - name: duplicate
    kind: sink
    params: {}
connections: []
"#,
        )
        .expect("write invalid graph");
        let path_string =
            CString::new(path.to_str().expect("temp path is utf8")).expect("temp path has no nul");
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_load_file(engine, path_string.as_ptr(), &mut error) },
            DgStatus::ParseError
        );
        assert!(!error.is_null());
        unsafe { dg_error_free(error) };
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
        fs::remove_file(path).expect("remove invalid graph");
    }

    #[test]
    fn c_abi_diff_and_reload_preserve_built_graph() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let initial = graph_spec();
        let updated = updated_graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    initial.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let mut added_nodes = 0;
        let mut removed_nodes = 0;
        let mut updated_nodes = 0;
        let mut added_connections = 0;
        let mut removed_connections = 0;
        assert_eq!(
            unsafe {
                dg_engine_diff_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    updated.as_ptr(),
                    &mut added_nodes,
                    &mut removed_nodes,
                    &mut updated_nodes,
                    &mut added_connections,
                    &mut removed_connections,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(added_nodes, 2);
        assert_eq!(removed_nodes, 0);
        assert_eq!(updated_nodes, 1);
        assert_eq!(added_connections, 1);
        assert_eq!(removed_connections, 0);

        let invalid = CString::new("not a graph").expect("valid invalid spec bytes");
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_engine_reload_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    invalid.as_ptr(),
                    &mut error,
                )
            },
            DgStatus::ParseError
        );
        assert!(!error.is_null());
        unsafe { dg_error_free(error) };
        assert_eq!(
            unsafe {
                dg_engine_reload_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    updated.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_engine_reload_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    initial.as_ptr(),
                    &mut error,
                )
            },
            DgStatus::RuntimeError
        );
        assert!(!error.is_null());
        unsafe { dg_error_free(error) };
        assert_eq!(
            unsafe { dg_engine_run(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::Ok
        );
        unsafe {
            dg_tensor_free(output);
            dg_tensor_free(tensor);
        }
        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn external_buffer_import_preserves_handle_metadata() {
        let mut counter = Box::new(AtomicUsize::new(0));
        let counter_ptr: *mut AtomicUsize = &mut *counter;
        let desc = DgExternalMemoryV2 {
            struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
            struct_version: 0,
            fd: -1,
            raw: 0x1234,
            domain: DgMemoryDomain::CudaDevice as i32,
            device: DgDeviceKind::CudaGpu as i32,
            size_bytes: 16,
            release: Some(test_release_callback),
            user_data: counter_ptr as *mut c_void,
        };
        let mut buffer = ptr::null_mut();
        assert_eq!(
            unsafe { dg_buffer_import_external(&desc, &mut buffer, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut size = 0;
        assert_eq!(
            unsafe { dg_buffer_size(buffer, &mut size, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(size, 16);
        unsafe { dg_buffer_free(buffer) };
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "release callback must be invoked exactly once"
        );
    }

    #[test]
    fn direct_backend_lifecycle_queries_and_runs_mock() {
        let options = CString::new(r#"{"shape":[1,4],"echo_inputs":true}"#).expect("options");
        let mut backend = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_backend_create(
                    DgBackendKind::Mock as i32,
                    ptr::null(),
                    0,
                    options.as_ptr(),
                    &mut backend,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        let mut input_count = 0;
        let mut output_count = 0;
        assert_eq!(
            unsafe {
                dg_backend_io_counts(
                    backend,
                    &mut input_count,
                    &mut output_count,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!((input_count, output_count), (1, 1));
        let mut capabilities = DgBackendCapabilities {
            struct_size: std::mem::size_of::<DgBackendCapabilities>() as u32,
            struct_version: 0,
            device_count: 0,
            devices: [DgDeviceKind::Cpu; 8],
            precision_count: 0,
            precisions: [DgDataType::U8; 16],
        };
        assert_eq!(
            unsafe { dg_backend_capabilities(backend, &mut capabilities, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(capabilities.device_count, 1);
        assert_eq!(capabilities.devices[0], DgDeviceKind::Cpu);
        assert!(capabilities.precision_count > 0);
        let mut info = DgTensorInfo {
            struct_size: std::mem::size_of::<DgTensorInfo>() as u32,
            struct_version: 0,
            dtype: DgDataType::U8,
            format: DgDataFormat::Auto,
            device: DgDeviceKind::Cpu,
            rank: 0,
            shape: [0; 8],
        };
        assert_eq!(
            unsafe { dg_backend_tensor_info(backend, false, 0, &mut info, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(info.dtype, DgDataType::F32);
        assert_eq!(&info.shape[..info.rank], &[1, 4]);

        let input_bytes = [1.0_f32, 2.0, 3.0, 4.0]
            .into_iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect::<Vec<_>>();
        let shape = [1_usize, 4];
        let mut input = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut input,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        let input_ptr = input as *const DgTensor;
        let mut output = ptr::null_mut();
        let mut output_count_result = 0;
        assert_eq!(
            unsafe {
                dg_backend_run(
                    backend,
                    &input_ptr,
                    1,
                    &mut output,
                    1,
                    &mut output_count_result,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(output_count_result, 1);
        let mut output_owned = ptr::null_mut();
        assert_eq!(
            unsafe { dg_tensor_data(output, &mut output_owned, ptr::null_mut()) },
            DgStatus::Ok
        );
        let output_data = unsafe { dg_owned_bytes_data(output_owned) };
        let output_length = unsafe { dg_owned_bytes_len(output_owned) };
        assert_eq!(
            unsafe { std::slice::from_raw_parts(output_data, output_length) },
            input_bytes.as_slice()
        );
        unsafe { dg_owned_bytes_free(output_owned) };
        unsafe {
            dg_tensor_free(output);
            dg_tensor_free(input);
            dg_backend_free(backend);
        }
    }

    #[test]
    fn direct_backend_null_and_bad_configuration_are_rejected() {
        assert_eq!(
            unsafe {
                dg_backend_io_counts(
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            },
            DgStatus::NullPointer
        );
        let options = CString::new(r#"{"unknown":true}"#).expect("options");
        let mut backend = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_backend_create(
                    DgBackendKind::Mock as i32,
                    ptr::null(),
                    0,
                    options.as_ptr(),
                    &mut backend,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );
        assert!(backend.is_null());
    }

    #[test]
    fn c_abi_v2_wire_types_reject_invalid_inputs() {
        let abi = unsafe { CStr::from_ptr(dg_abi_version()) };
        assert_eq!(abi.to_string_lossy(), "2.0");

        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe { dg_engine_load_string(engine, 42, spec.as_ptr(), ptr::null_mut()) },
            DgStatus::InvalidArgument
        );
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );

        let input = [1.0_f32];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    999,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );

        let mut backend = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_backend_create(
                    123,
                    ptr::null(),
                    0,
                    ptr::null(),
                    &mut backend,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );
        assert!(backend.is_null());

        let bad_external = DgExternalMemoryV2 {
            struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
            struct_version: 0,
            fd: -1,
            raw: 0x1234,
            domain: 123,
            device: DgDeviceKind::Cpu as i32,
            size_bytes: 16,
            release: Some(test_release_callback),
            user_data: ptr::null_mut(),
        };
        let mut buffer = ptr::null_mut();
        assert_eq!(
            unsafe { dg_buffer_import_external(&bad_external, &mut buffer, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );

        let bad_options = DgRuntimeInitOptions {
            struct_size: 1,
            struct_version: 0,
        };
        assert_eq!(
            unsafe { dg_runtime_init(&bad_options, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );

        let bad_version = DgRuntimeInitOptions {
            struct_size: std::mem::size_of::<DgRuntimeInitOptions>() as u32,
            struct_version: 9,
        };
        assert_eq!(
            unsafe { dg_runtime_init(&bad_version, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );

        assert_eq!(
            unsafe { dg_engine_destroy(engine, 0, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn zero_struct_size_is_rejected_for_output_structs() {
        // A zero struct_size used to be accepted, which would cause the C ABI
        // to write past a caller-allocated buffer that is smaller than the
        // Rust struct. Verify that all output structs now require an exact size.
        let mut backend = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_backend_create(
                    DgBackendKind::Mock as i32,
                    ptr::null(),
                    0,
                    ptr::null(),
                    &mut backend,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert!(!backend.is_null());

        let mut capabilities = DgBackendCapabilities {
            struct_size: 0,
            struct_version: 0,
            device_count: 0,
            devices: [DgDeviceKind::Cpu; 8],
            precision_count: 0,
            precisions: [DgDataType::U8; 16],
        };
        assert_eq!(
            unsafe { dg_backend_capabilities(backend, &mut capabilities, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );

        let mut info = DgTensorInfo {
            struct_size: 0,
            struct_version: 0,
            dtype: DgDataType::U8,
            format: DgDataFormat::Auto,
            device: DgDeviceKind::Cpu,
            rank: 0,
            shape: [0; 8],
        };
        assert_eq!(
            unsafe { dg_backend_tensor_info(backend, false, 0, &mut info, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );

        unsafe {
            dg_backend_free(backend);
        }
    }

    #[test]
    fn engine_run_and_status_without_build_returns_not_built() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let mut status = DgGraphStatus::NotRunning;
        let mut cause: *mut DgOwnedBytes = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_status(engine, &mut status, &mut cause, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(status, DgGraphStatus::NotRunning);
        assert!(cause.is_null());

        assert_eq!(
            unsafe { dg_engine_run(engine, ptr::null_mut()) },
            DgStatus::NotBuilt
        );

        assert_eq!(
            unsafe { dg_engine_destroy(engine, 0, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn engine_shutdown_timeout_returns_busy_when_graph_cannot_stop() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1000000
      shape: [1, 4]
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#,
        )
        .expect("valid graph spec");
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        // A zero-millisecond timeout should not be enough for a long-running graph.
        let mut error = ptr::null_mut();
        let status = unsafe { dg_engine_shutdown(engine, 0, &mut error) };
        assert_eq!(status, DgStatus::Busy, "zero timeout should be busy");
        assert!(!error.is_null());
        unsafe { dg_error_free(error) };

        // Retrying with a generous timeout should eventually succeed.
        assert_eq!(
            unsafe { dg_engine_shutdown(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );

        assert_eq!(
            unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn engine_destroy_frees_handle_after_worker_panic() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1, 4]
      start: 1.0
  - name: panic
    kind: capi_test_panic
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> panic.in
  - panic.out -> sink.in
"#,
        )
        .expect("valid graph spec");
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        // The worker will panic as soon as it receives the source packet.
        // Shutdown joins it, reports the worker failure and consumes the running graph.
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_shutdown(engine, 5000, &mut error) },
            DgStatus::RuntimeError
        );
        if !error.is_null() {
            unsafe { dg_error_free(error) };
        }

        // After a permanent failure, destroy should still be able to free the handle.
        assert_eq!(
            unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn engine_status_error_leaves_not_running_sentinel() {
        let mut status = DgGraphStatus::Starting;
        let mut cause: *mut DgOwnedBytes = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_status(ptr::null(), &mut status, &mut cause, ptr::null_mut()) },
            DgStatus::NullPointer
        );
        assert_eq!(
            status,
            DgGraphStatus::NotRunning,
            "error must leave NotRunning"
        );
        assert!(cause.is_null());
    }

    #[test]
    fn engine_init_rejects_pending_one_shot_inputs() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let input = [1.0_f32, 2.0, 3.0, 4.0];
        let input_bytes: Vec<u8> = input.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let shape = [1_usize, 4];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    input_bytes.as_ptr(),
                    input_bytes.len(),
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
            DgStatus::Ok
        );

        // Streaming cannot start while one-shot inputs are pending.
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_init(engine, &mut error) },
            DgStatus::RuntimeError
        );
        if !error.is_null() {
            unsafe { dg_error_free(error) };
        }

        unsafe {
            dg_tensor_free(tensor);
            dg_engine_destroy(engine, 5000, ptr::null_mut());
        }
    }

    #[test]
    fn engine_poll_detects_worker_failure_without_shutdown() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let spec = CString::new(
            r#"apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1, 4]
      start: 1.0
  - name: panic
    kind: capi_test_panic
  - name: sink
    kind: sink
    params: {}
connections:
  - source.out -> panic.in
  - panic.out -> sink.in
"#,
        )
        .expect("valid graph spec");
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_init(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        // Polling should observe the worker failure instead of returning Again forever.
        let mut error = ptr::null_mut();
        let mut output = ptr::null_mut();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last_status = DgStatus::Again;
        while std::time::Instant::now() < deadline {
            last_status = unsafe { dg_engine_poll(engine, &mut output, &mut error) };
            if last_status == DgStatus::RuntimeError {
                break;
            }
            if last_status == DgStatus::Ok {
                assert!(!output.is_null());
                unsafe { dg_tensor_free(output) };
                output = ptr::null_mut();
            }
            assert!(error.is_null(), "Again/Ok must not allocate a DgError");
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            last_status,
            DgStatus::RuntimeError,
            "dg_engine_poll must report a worker failure, not spin on Again"
        );
        assert!(output.is_null());
        if !error.is_null() {
            unsafe { dg_error_free(error) };
        }

        // Workers have already been joined by poll, so shutdown cleanly completes
        // and consumes the failed running graph.
        assert_eq!(
            unsafe { dg_engine_shutdown(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );

        assert_eq!(
            unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn empty_tensor_data_returns_non_null_aligned_pointer() {
        let shape = [0usize];
        let mut tensor = ptr::null_mut();
        unsafe {
            assert_eq!(
                dg_tensor_create(
                    ptr::null(),
                    0,
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::F32 as i32,
                    DgDataFormat::Nc as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                ),
                DgStatus::Ok
            );
        }
        assert!(!tensor.is_null());

        let mut owned = ptr::null_mut();
        unsafe {
            assert_eq!(
                dg_tensor_data(tensor, &mut owned, ptr::null_mut()),
                DgStatus::Ok
            );
        }
        assert!(!owned.is_null());

        let data = unsafe { dg_owned_bytes_data(owned) };
        let len = unsafe { dg_owned_bytes_len(owned) };
        assert_eq!(len, 0);
        assert!(!data.is_null());
        // SAFETY: `data` is non-null and aligned; length is zero, so no bytes are read.
        let _empty = unsafe { std::slice::from_raw_parts(data, len) };

        unsafe {
            dg_owned_bytes_free(owned);
            dg_tensor_free(tensor);
        }
    }

    #[test]
    fn destroy_with_u64_max_timeout_does_not_panic() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert!(!engine.is_null());
        // A malicious or careless caller could pass u64::MAX. The engine must
        // not panic when converting this to a Duration.
        assert_eq!(
            unsafe { dg_engine_destroy(engine, u64::MAX, ptr::null_mut()) },
            DgStatus::Ok
        );
    }

    #[test]
    fn backend_create_rejects_oversized_model() {
        let mut backend = ptr::null_mut();
        let oversized = ResourcePolicy::DEFAULT_MAX_MODEL_BYTES + 1;
        assert_eq!(
            unsafe {
                dg_backend_create(
                    DgBackendKind::Mock as i32,
                    ptr::null(),
                    oversized,
                    ptr::null(),
                    &mut backend,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );
        assert!(backend.is_null());
    }

    #[test]
    fn run_twice_without_poll_replaces_outputs() {
        let mut engine = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_create(&mut engine, ptr::null_mut()) },
            DgStatus::Ok
        );
        let spec = graph_spec();
        assert_eq!(
            unsafe {
                dg_engine_load_string(
                    engine,
                    DgGraphFormat::Yaml as i32,
                    spec.as_ptr(),
                    ptr::null_mut(),
                )
            },
            DgStatus::Ok
        );
        assert_eq!(
            unsafe { dg_engine_build(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let first = [1.0_f32, 2.0, 3.0, 4.0];
        let second = [5.0_f32, 6.0, 7.0, 8.0];

        let push_input = |input: [f32; 4]| {
            let input_bytes: Vec<u8> = input.iter().flat_map(|v| v.to_ne_bytes()).collect();
            let shape = [1usize, 4];
            let mut tensor = ptr::null_mut();
            assert_eq!(
                unsafe {
                    dg_tensor_create(
                        input_bytes.as_ptr(),
                        input_bytes.len(),
                        shape.as_ptr(),
                        shape.len(),
                        DgDataType::F32 as i32,
                        DgDataFormat::Nc as i32,
                        DgDeviceKind::Cpu as i32,
                        &mut tensor,
                        ptr::null_mut(),
                    )
                },
                DgStatus::Ok
            );
            assert_eq!(
                unsafe { dg_engine_push(engine, tensor, ptr::null_mut()) },
                DgStatus::Ok
            );
            unsafe { dg_tensor_free(tensor) };
        };

        push_input(first);
        assert_eq!(
            unsafe { dg_engine_run(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        push_input(second);
        assert_eq!(
            unsafe { dg_engine_run(engine, ptr::null_mut()) },
            DgStatus::Ok
        );

        let mut output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::Ok
        );
        assert!(!output.is_null());

        let mut owned = ptr::null_mut();
        assert_eq!(
            unsafe { dg_tensor_data(output, &mut owned, ptr::null_mut()) },
            DgStatus::Ok
        );
        let output_data = unsafe { dg_owned_bytes_data(owned) };
        let output_len = unsafe { dg_owned_bytes_len(owned) };
        let second_bytes: Vec<u8> = second.iter().flat_map(|v| v.to_ne_bytes()).collect();
        assert_eq!(
            unsafe { std::slice::from_raw_parts(output_data, output_len) },
            second_bytes.as_slice()
        );

        unsafe {
            dg_owned_bytes_free(owned);
            dg_tensor_free(output);
        }

        // One-shot execution completes after the run, so the next poll reports
        // EndOfStream once all outputs have been consumed.
        output = ptr::null_mut();
        assert_eq!(
            unsafe { dg_engine_poll(engine, &mut output, ptr::null_mut()) },
            DgStatus::EndOfStream
        );
        assert!(output.is_null());

        unsafe { dg_engine_destroy(engine, 5000, ptr::null_mut()) };
    }

    #[test]
    fn tensor_create_rejects_oversized_host_tensor() {
        let oversized = ResourcePolicy::DEFAULT_MAX_TENSOR_BYTES + 1;
        let shape = [oversized];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create(
                    ptr::null(),
                    0,
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::U8 as i32,
                    DgDataFormat::N as i32,
                    DgDeviceKind::Cpu as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );
        assert!(tensor.is_null());
    }

    #[test]
    fn tensor_create_external_rejects_oversized_external_tensor() {
        let oversized = ResourcePolicy::DEFAULT_MAX_TENSOR_BYTES + 1;
        let file = fs::File::open("/dev/null").expect("open /dev/null");
        let fd = file.as_raw_fd();
        let desc = DgExternalMemoryV2 {
            struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
            struct_version: 0,
            fd,
            raw: 0,
            domain: DgMemoryDomain::Host as i32,
            device: DgDeviceKind::Cpu as i32,
            size_bytes: oversized,
            release: None,
            user_data: ptr::null_mut(),
        };
        let shape = [oversized];
        let mut tensor = ptr::null_mut();
        assert_eq!(
            unsafe {
                dg_tensor_create_external(
                    &desc,
                    shape.as_ptr(),
                    shape.len(),
                    DgDataType::U8 as i32,
                    DgDataFormat::N as i32,
                    &mut tensor,
                    ptr::null_mut(),
                )
            },
            DgStatus::InvalidArgument
        );
        assert!(tensor.is_null());
    }

    #[test]
    fn buffer_import_external_rejects_oversized_buffer() {
        let oversized = ResourcePolicy::DEFAULT_MAX_BUFFER_BYTES + 1;
        let file = fs::File::open("/dev/null").expect("open /dev/null");
        let fd = file.as_raw_fd();
        let desc = DgExternalMemoryV2 {
            struct_size: std::mem::size_of::<DgExternalMemoryV2>() as u32,
            struct_version: 0,
            fd,
            raw: 0,
            domain: DgMemoryDomain::Host as i32,
            device: DgDeviceKind::Cpu as i32,
            size_bytes: oversized,
            release: None,
            user_data: ptr::null_mut(),
        };
        let mut buffer = ptr::null_mut();
        assert_eq!(
            unsafe { dg_buffer_import_external(&desc, &mut buffer, ptr::null_mut()) },
            DgStatus::InvalidArgument
        );
        assert!(buffer.is_null());
    }

    #[test]
    fn c_string_bounded_rejects_null_and_missing_terminator() {
        assert!(matches!(
            unsafe { c_string_bounded(ptr::null(), 4) },
            Err((DgStatus::NullPointer, _))
        ));

        let valid = CString::new("hi").unwrap();
        let result = unsafe { c_string_bounded(valid.as_ptr(), 4) };
        assert!(result.is_ok(), "{result:?}");

        assert!(matches!(
            unsafe { c_string_bounded(valid.as_ptr(), 0) },
            Err((DgStatus::InvalidArgument, _))
        ));

        let non_terminated: Vec<u8> = vec![b'a'; 4];
        let result = unsafe { c_string_bounded(non_terminated.as_ptr().cast::<c_char>(), 4) };
        assert!(
            matches!(result, Err((DgStatus::InvalidArgument, _))),
            "{result:?}"
        );
    }
}
