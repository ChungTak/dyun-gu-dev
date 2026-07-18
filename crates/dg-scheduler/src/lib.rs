#![forbid(unsafe_code)]

//! Device/core scheduling and load balancing.
//!
//! `dg-scheduler` owns the pure Rust resource planner for device selection,
//! core masks, affinity, and policy-driven allocation. It intentionally stays
//! independent from graph/runtime wiring in this milestone; later crates will
//! consume the scheduler to map backend requests onto concrete device/core
//! placements.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use dg_core::CoreSelection;
use dg_core::{DeployMode, DeviceKind};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("topology cannot be empty")]
    EmptyTopology,
    #[error("duplicate device: kind={kind:?}, id={id}")]
    DuplicateDevice { kind: DeviceKind, id: u16 },
    #[error("device {kind:?}:{id} has no cores")]
    EmptyDevice { kind: DeviceKind, id: u16 },
    #[error("core id {core_id} on device {kind:?}:{id} exceeds mask capacity")]
    CoreIdOutOfRange {
        kind: DeviceKind,
        id: u16,
        core_id: u8,
    },
    #[error("duplicate core id {core_id} on device {kind:?}:{id}")]
    DuplicateCore {
        kind: DeviceKind,
        id: u16,
        core_id: u8,
    },
    #[error("unknown device {kind:?}:{id}")]
    UnknownDevice { kind: DeviceKind, id: u16 },
    #[error("no device of kind {kind:?} matched the request")]
    NoMatchingDevice { kind: DeviceKind },
    #[error("requested core mask {mask:#010x} does not match any core")]
    InvalidCoreMask { mask: u32 },
    #[error("requested core mask {mask:#010x} selects unavailable core {core_id}")]
    MissingCore { mask: u32, core_id: u8 },
    #[error("scheduler has no available cores for the request")]
    NoAvailableCore,
    #[error("instance pool must contain at least one instance")]
    InvalidInstanceCount,
    #[error("scheduler core load overflow")]
    LoadOverflow,
}

/// A schedulable device with a numeric card id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub kind: DeviceKind,
    pub id: u16,
    pub cores: Vec<Core>,
}

/// A schedulable core identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Core {
    pub id: u8,
}

/// A device/core topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    deployment: DeployMode,
    devices: Vec<Device>,
}

impl Topology {
    pub fn new(deployment: DeployMode, devices: Vec<Device>) -> Result<Self> {
        validate_topology(&devices)?;
        Ok(Self {
            deployment,
            devices,
        })
    }

    pub fn single_chip(kind: DeviceKind, core_count: u8) -> Result<Self> {
        Self::new(
            DeployMode::SoC,
            vec![Device {
                kind,
                id: 0,
                cores: cores_from_count(core_count),
            }],
        )
    }

    /// Builds a topology from adapters registered in `dg-core`.
    ///
    /// Discovery intentionally uses a caller-provided uniform core hint until
    /// runtime capability probes can supply device-specific counts.
    pub fn from_registered_devices(core_count_hint: u8) -> Result<Self> {
        let core_count = core_count_hint.max(1);
        let mut seen = HashSet::new();
        let devices = dg_core::registered_device_kinds()
            .into_iter()
            .filter(|kind| seen.insert(*kind))
            .map(|kind| Device {
                kind,
                id: 0,
                cores: cores_from_count(core_count),
            })
            .collect();
        Self::new(DeployMode::Host, devices)
    }

    pub fn single_card_multi_core(kind: DeviceKind, card: u16, core_count: u8) -> Result<Self> {
        Self::new(
            DeployMode::Host,
            vec![Device {
                kind,
                id: card,
                cores: cores_from_count(core_count),
            }],
        )
    }

    pub fn multi_card_multi_core<I>(kind: DeviceKind, cards: I) -> Result<Self>
    where
        I: IntoIterator<Item = (u16, u8)>,
    {
        let devices = cards
            .into_iter()
            .map(|(id, core_count)| Device {
                kind,
                id,
                cores: cores_from_count(core_count),
            })
            .collect();
        Self::new(DeployMode::Host, devices)
    }

    pub fn deployment(&self) -> DeployMode {
        self.deployment
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
}

/// Scheduling strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulingMode {
    Auto,
    Explicit,
}

/// Policy used to choose among otherwise-equivalent scheduler candidates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchedulingPolicy {
    #[default]
    LeastLoaded,
    RoundRobin,
}

/// Request used to acquire a device/core lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub kind: DeviceKind,
    pub device_id: Option<u16>,
    pub mode: SchedulingMode,
    pub core_selection: CoreSelection,
    pub affinity_key: Option<String>,
    pub policy: SchedulingPolicy,
    allowed_placements: Option<Vec<(u16, u8)>>,
}

impl Request {
    pub fn auto(kind: DeviceKind) -> Self {
        Self {
            kind,
            device_id: None,
            mode: SchedulingMode::Auto,
            core_selection: CoreSelection::Auto,
            affinity_key: None,
            policy: SchedulingPolicy::LeastLoaded,
            allowed_placements: None,
        }
    }

    pub fn explicit(kind: DeviceKind, device_id: u16, core_selection: CoreSelection) -> Self {
        Self {
            kind,
            device_id: Some(device_id),
            mode: SchedulingMode::Explicit,
            core_selection,
            affinity_key: None,
            policy: SchedulingPolicy::LeastLoaded,
            allowed_placements: None,
        }
    }

    pub fn with_affinity_key(mut self, affinity_key: impl Into<String>) -> Self {
        self.affinity_key = Some(affinity_key.into());
        self
    }

    pub fn with_device_id(mut self, device_id: u16) -> Self {
        self.device_id = Some(device_id);
        self
    }

    pub fn with_core_selection(mut self, core_selection: CoreSelection) -> Self {
        self.core_selection = core_selection;
        self
    }

    pub fn with_policy(mut self, policy: SchedulingPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_allowed_placements(mut self, placements: Vec<(u16, u8)>) -> Self {
        self.allowed_placements = Some(placements);
        self
    }
}

/// Snapshot of a core including current load and invariant violations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreLoad {
    pub id: u8,
    pub load: usize,
    pub overflow_count: u64,
    pub underflow_count: u64,
}

/// Snapshot of a device including current core loads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLoad {
    pub kind: DeviceKind,
    pub id: u16,
    pub cores: Vec<CoreLoad>,
}

/// Affinity table usage counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AffinityMetrics {
    pub entries: usize,
    pub evictions: u64,
    pub expired: u64,
}

#[derive(Clone, Debug)]
struct CoreState {
    id: u8,
    load: usize,
    overflow_count: u64,
    underflow_count: u64,
}

#[derive(Clone, Debug)]
struct DeviceState {
    kind: DeviceKind,
    id: u16,
    cores: Vec<CoreState>,
}

#[derive(Clone, Debug)]
struct SchedulerState {
    devices: Vec<DeviceState>,
    affinity: BoundedAffinityTable<Allocation>,
    round_robin_cursor: usize,
}

/// A bounded affinity table with TTL and LRU eviction.
#[derive(Clone, Debug)]
struct BoundedAffinityTable<T: Clone> {
    capacity: usize,
    ttl: Duration,
    entries: HashMap<String, AffinityEntry<T>>,
    evictions: u64,
    expired: u64,
}

#[derive(Clone, Debug)]
struct AffinityEntry<T: Clone> {
    value: T,
    last_used: Instant,
}

impl<T: Clone> BoundedAffinityTable<T> {
    fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity: capacity.max(1),
            ttl,
            entries: HashMap::new(),
            evictions: 0,
            expired: 0,
        }
    }

    fn get(&mut self, key: &str) -> Option<T> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(key) {
            if now.duration_since(entry.last_used) > self.ttl {
                self.entries.remove(key);
                self.expired += 1;
                return None;
            }
            entry.last_used = now;
            return Some(entry.value.clone());
        }
        None
    }

    fn insert(&mut self, key: String, value: T) {
        let now = Instant::now();
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.last_used)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
                self.evictions += 1;
            }
        }
        self.entries.insert(
            key,
            AffinityEntry {
                value,
                last_used: now,
            },
        );
    }

    fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    fn metrics(&self) -> AffinityMetrics {
        AffinityMetrics {
            entries: self.entries.len(),
            evictions: self.evictions,
            expired: self.expired,
        }
    }

    #[cfg(test)]
    fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
    }

    #[cfg(test)]
    fn set_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Allocation {
    device_index: usize,
    core_index: usize,
}

/// Scheduler that performs policy-driven allocation with optional affinity.
#[derive(Clone, Debug)]
pub struct Scheduler {
    topology: Topology,
    state: Arc<Mutex<SchedulerState>>,
}

impl Scheduler {
    pub fn new(topology: Topology) -> Result<Self> {
        let state = SchedulerState::from_topology(&topology)?;
        Ok(Self {
            topology,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn topology(&self) -> &Topology {
        &self.topology
    }

    pub fn snapshot(&self) -> Result<Vec<DeviceLoad>> {
        let state = self.state.lock().map_err(|_| Error::NoAvailableCore)?;
        Ok(state.snapshot())
    }

    pub fn affinity_metrics(&self) -> AffinityMetrics {
        self.state
            .lock()
            .map_or(AffinityMetrics::default(), |state| state.affinity.metrics())
    }

    pub fn acquire(&self, request: Request) -> Result<Lease> {
        let mut state = self.state.lock().map_err(|_| Error::NoAvailableCore)?;
        let allocation = state.acquire(request)?;
        let (device_kind, device_id, core_id) = {
            let device = &state.devices[allocation.device_index];
            (
                device.kind,
                device.id,
                device.cores[allocation.core_index].id,
            )
        };
        Ok(Lease {
            state: Arc::clone(&self.state),
            allocation,
            device: (device_kind, device_id),
            core_id,
        })
    }
}

/// RAII lease returned by the scheduler.
#[derive(Debug)]
pub struct Lease {
    state: Arc<Mutex<SchedulerState>>,
    allocation: Allocation,
    device: (DeviceKind, u16),
    core_id: u8,
}

impl Lease {
    pub fn device(&self) -> (DeviceKind, u16) {
        self.device
    }

    pub fn core_id(&self) -> u8 {
        self.core_id
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.release(self.allocation);
        } else {
            // Mutex poisoned: the scheduler invariant is broken. The load is
            // permanently leaked rather than panicking, but callers can detect
            // this through load imbalance in tests/metrics.
        }
    }
}

/// A concrete device/core placement assigned to one model instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub kind: DeviceKind,
    pub device_id: u16,
    pub core_id: u8,
}

/// A pool of model-instance placements backed by a shared scheduler.
///
/// When the topology has fewer placements than requested instances, placements
/// are reused. This keeps SDK-free single-core topologies honest while still
/// tracking each checkout's in-flight load through a scheduler lease.
#[derive(Clone, Debug)]
pub struct InstancePool {
    scheduler: Scheduler,
    placements: Vec<Placement>,
    allowed_placements: Vec<(u16, u8)>,
    next_instance: Arc<Mutex<usize>>,
    affinity: Arc<Mutex<BoundedAffinityTable<usize>>>,
}

impl InstancePool {
    pub fn new(
        scheduler: Scheduler,
        kind: DeviceKind,
        instance_count: usize,
        core_selection: CoreSelection,
    ) -> Result<Self> {
        if instance_count == 0 {
            return Err(Error::InvalidInstanceCount);
        }
        let available = scheduler
            .topology()
            .devices()
            .iter()
            .filter(|device| device.kind == kind)
            .flat_map(|device| {
                device.cores.iter().filter_map(move |core| {
                    core_selection.contains(core.id).then_some(Placement {
                        kind,
                        device_id: device.id,
                        core_id: core.id,
                    })
                })
            })
            .collect::<Vec<_>>();
        if available.is_empty() {
            return Err(Error::NoMatchingDevice { kind });
        }
        let placements: Vec<Placement> = (0..instance_count)
            .map(|index| available[index % available.len()])
            .collect();
        let allowed_placements = placements
            .iter()
            .map(|placement| (placement.device_id, placement.core_id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Ok(Self {
            scheduler,
            placements,
            allowed_placements,
            next_instance: Arc::new(Mutex::new(0)),
            affinity: Arc::new(Mutex::new(BoundedAffinityTable::new(
                instance_count,
                Duration::from_secs(600),
            ))),
        })
    }

    pub fn instance_count(&self) -> usize {
        self.placements.len()
    }

    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }

    pub fn checkout(
        &self,
        policy: SchedulingPolicy,
        affinity_key: Option<&str>,
    ) -> Result<PooledLease> {
        if let Some(key) = affinity_key {
            if let Some(instance_index) = self
                .affinity
                .lock()
                .map_err(|_| Error::NoAvailableCore)?
                .get(key)
            {
                let placement = self.placements[instance_index];
                let lease = self.scheduler.acquire(
                    Request::explicit(
                        placement.kind,
                        placement.device_id,
                        CoreSelection::Single(placement.core_id),
                    )
                    .with_allowed_placements(vec![(placement.device_id, placement.core_id)]),
                )?;
                return Ok(PooledLease {
                    _lease: lease,
                    instance_index,
                    placement,
                });
            }
        }
        let mut request = Request::auto(self.placements[0].kind)
            .with_policy(policy)
            .with_allowed_placements(self.allowed_placements.clone());
        if let Some(key) = affinity_key {
            request = request.with_affinity_key(key);
        }
        let checkout = self.checkout_request(request)?;
        if let Some(key) = affinity_key {
            self.affinity
                .lock()
                .map_err(|_| Error::NoAvailableCore)?
                .insert(key.to_string(), checkout.instance_index());
        }
        Ok(checkout)
    }

    pub fn checkout_explicit(&self, device_id: u16, core_id: u8) -> Result<PooledLease> {
        let placement = (device_id, core_id);
        if !self.allowed_placements.contains(&placement) {
            return Err(Error::NoAvailableCore);
        }
        self.checkout_request(
            Request::explicit(
                self.placements[0].kind,
                device_id,
                CoreSelection::Single(core_id),
            )
            .with_allowed_placements(vec![placement]),
        )
    }

    pub fn affinity_metrics(&self) -> AffinityMetrics {
        self.affinity
            .lock()
            .map_or(AffinityMetrics::default(), |table| table.metrics())
    }

    /// Removes an affinity entry when its stream ends or the node reloads.
    pub fn remove_affinity(&self, key: &str) {
        if let Ok(mut table) = self.affinity.lock() {
            table.remove(key);
        }
    }

    #[cfg(test)]
    fn set_affinity_capacity(&self, capacity: usize) {
        if let Ok(mut table) = self.affinity.lock() {
            table.set_capacity(capacity);
        }
    }

    #[cfg(test)]
    fn set_affinity_ttl(&self, ttl: Duration) {
        if let Ok(mut table) = self.affinity.lock() {
            table.set_ttl(ttl);
        }
    }

    fn checkout_request(&self, request: Request) -> Result<PooledLease> {
        let lease = self.scheduler.acquire(request)?;
        let placement = Placement {
            kind: lease.device().0,
            device_id: lease.device().1,
            core_id: lease.core_id(),
        };
        let matching = self
            .placements
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (*candidate == placement).then_some(index))
            .collect::<Vec<_>>();
        let instance_index = if matching.len() == 1 {
            matching[0]
        } else {
            let mut cursor = self
                .next_instance
                .lock()
                .map_err(|_| Error::NoAvailableCore)?;
            let index = matching[*cursor % matching.len()];
            *cursor = cursor.wrapping_add(1);
            index
        };
        Ok(PooledLease {
            _lease: lease,
            instance_index,
            placement,
        })
    }
}

/// RAII checkout for one instance in an [`InstancePool`].
#[derive(Debug)]
pub struct PooledLease {
    _lease: Lease,
    instance_index: usize,
    placement: Placement,
}

impl PooledLease {
    pub fn instance_index(&self) -> usize {
        self.instance_index
    }

    pub fn placement(&self) -> Placement {
        self.placement
    }
}

impl SchedulerState {
    fn from_topology(topology: &Topology) -> Result<Self> {
        validate_topology(&topology.devices)?;
        let total_cores = topology
            .devices
            .iter()
            .map(|device| device.cores.len())
            .sum::<usize>()
            .max(1);
        let devices = topology
            .devices
            .iter()
            .map(|device| DeviceState {
                kind: device.kind,
                id: device.id,
                cores: device
                    .cores
                    .iter()
                    .map(|core| CoreState {
                        id: core.id,
                        load: 0,
                        overflow_count: 0,
                        underflow_count: 0,
                    })
                    .collect(),
            })
            .collect();
        Ok(Self {
            devices,
            affinity: BoundedAffinityTable::new(total_cores, Duration::from_secs(600)),
            round_robin_cursor: 0,
        })
    }

    fn snapshot(&self) -> Vec<DeviceLoad> {
        self.devices
            .iter()
            .map(|device| DeviceLoad {
                kind: device.kind,
                id: device.id,
                cores: device
                    .cores
                    .iter()
                    .map(|core| CoreLoad {
                        id: core.id,
                        load: core.load,
                        overflow_count: core.overflow_count,
                        underflow_count: core.underflow_count,
                    })
                    .collect(),
            })
            .collect()
    }

    fn acquire(&mut self, request: Request) -> Result<Allocation> {
        let device_indexes = self
            .devices
            .iter()
            .enumerate()
            .filter(|(_, device)| device.kind == request.kind)
            .filter(|(_, device)| request.device_id.is_none_or(|id| device.id == id))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        if device_indexes.is_empty() {
            return if let Some(id) = request.device_id {
                Err(Error::UnknownDevice {
                    kind: request.kind,
                    id,
                })
            } else {
                Err(Error::NoMatchingDevice { kind: request.kind })
            };
        }

        if request.mode == SchedulingMode::Auto {
            if let Some(key) = &request.affinity_key {
                if let Some(allocation) = self.affinity.get(key) {
                    if self.allocation_is_valid(
                        request.kind,
                        &device_indexes,
                        allocation,
                        request.core_selection,
                    ) {
                        self.increment(allocation)?;
                        return Ok(allocation);
                    }
                }
            }
        }

        let allowed_placements = request.allowed_placements.as_ref();
        let candidates = device_indexes
            .iter()
            .flat_map(|&device_index| {
                let device = &self.devices[device_index];
                device
                    .cores
                    .iter()
                    .enumerate()
                    .filter(move |(_, core)| request.core_selection.contains(core.id))
                    .filter(move |(_, core)| {
                        allowed_placements
                            .is_none_or(|placements| placements.contains(&(device.id, core.id)))
                    })
                    .filter(move |(_, core)| {
                        request.core_selection.is_explicit()
                            || request.mode == SchedulingMode::Auto
                            || request.core_selection.contains(core.id)
                    })
                    .map(move |(core_index, core)| {
                        (device_index, core_index, core.load, device.id, core.id)
                    })
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return match request.core_selection {
                CoreSelection::Mask(mask) if mask == 0 => Err(Error::InvalidCoreMask { mask }),
                CoreSelection::Mask(mask) => Err(Error::InvalidCoreMask { mask }),
                CoreSelection::Single(core_id) => Err(Error::MissingCore {
                    mask: 1u32 << u32::from(core_id),
                    core_id,
                }),
                _ => Err(Error::NoAvailableCore),
            };
        }

        let selected = if request.mode == SchedulingMode::Auto
            && !request.core_selection.is_explicit()
            && request.policy == SchedulingPolicy::RoundRobin
        {
            let index = self.round_robin_cursor % candidates.len();
            self.round_robin_cursor = self.round_robin_cursor.wrapping_add(1);
            candidates[index]
        } else {
            candidates
                .into_iter()
                .min_by_key(|(_, _, load, device_id, core_id)| (*load, *device_id, *core_id))
                .expect("candidates not empty")
        };
        let allocation = Allocation {
            device_index: selected.0,
            core_index: selected.1,
        };
        self.increment(allocation)?;

        if let Some(key) = request.affinity_key {
            self.affinity.insert(key, allocation);
        }

        Ok(allocation)
    }

    fn allocation_is_valid(
        &self,
        kind: DeviceKind,
        device_indexes: &[usize],
        allocation: Allocation,
        selection: CoreSelection,
    ) -> bool {
        let Some(device) = self.devices.get(allocation.device_index) else {
            return false;
        };
        if device.kind != kind || !device_indexes.contains(&allocation.device_index) {
            return false;
        }
        let Some(core) = device.cores.get(allocation.core_index) else {
            return false;
        };
        selection.contains(core.id)
    }

    fn increment(&mut self, allocation: Allocation) -> Result<()> {
        let core = &mut self.devices[allocation.device_index].cores[allocation.core_index];
        match core.load.checked_add(1) {
            Some(new_load) => {
                core.load = new_load;
                Ok(())
            }
            None => {
                core.overflow_count += 1;
                Err(Error::LoadOverflow)
            }
        }
    }

    fn release(&mut self, allocation: Allocation) {
        let core = &mut self.devices[allocation.device_index].cores[allocation.core_index];
        match core.load.checked_sub(1) {
            Some(new_load) => core.load = new_load,
            None => {
                core.underflow_count += 1;
                core.load = 0;
            }
        }
    }
}

fn validate_topology(devices: &[Device]) -> Result<()> {
    if devices.is_empty() {
        return Err(Error::EmptyTopology);
    }

    let mut seen = HashMap::new();
    for device in devices {
        if seen.insert((device.kind, device.id), ()).is_some() {
            return Err(Error::DuplicateDevice {
                kind: device.kind,
                id: device.id,
            });
        }
        if device.cores.is_empty() {
            return Err(Error::EmptyDevice {
                kind: device.kind,
                id: device.id,
            });
        }
        let mut core_ids = HashMap::new();
        for core in &device.cores {
            if core.id >= 32 {
                return Err(Error::CoreIdOutOfRange {
                    kind: device.kind,
                    id: device.id,
                    core_id: core.id,
                });
            }
            if core_ids.insert(core.id, ()).is_some() {
                return Err(Error::DuplicateCore {
                    kind: device.kind,
                    id: device.id,
                    core_id: core.id,
                });
            }
        }
    }
    Ok(())
}

fn cores_from_count(core_count: u8) -> Vec<Core> {
    (0..core_count).map(|id| Core { id }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use proptest::prelude::*;

    fn rknn_topology() -> Scheduler {
        Scheduler::new(
            Topology::multi_card_multi_core(DeviceKind::RknnNpu, [(0, 3), (1, 2)])
                .expect("topology"),
        )
        .expect("scheduler")
    }

    #[test]
    fn least_loaded_spreads_across_cores() {
        let scheduler = rknn_topology();
        let lease_a = scheduler
            .acquire(Request::auto(DeviceKind::RknnNpu))
            .expect("lease a");
        let lease_b = scheduler
            .acquire(Request::auto(DeviceKind::RknnNpu))
            .expect("lease b");
        let snapshot = scheduler.snapshot().expect("snapshot");
        let loads = snapshot
            .iter()
            .flat_map(|device| device.cores.iter().map(|core| core.load))
            .collect::<Vec<_>>();
        assert_eq!(loads.iter().copied().sum::<usize>(), 2);
        assert!(loads.iter().filter(|&&load| load == 1).count() >= 2);
        drop(lease_a);
        drop(lease_b);
        assert!(scheduler
            .snapshot()
            .expect("snapshot")
            .iter()
            .all(|device| device.cores.iter().all(|core| core.load == 0)));
    }

    #[test]
    fn affinity_prefers_previous_core() {
        let scheduler = rknn_topology();
        let lease_a = scheduler
            .acquire(Request::auto(DeviceKind::RknnNpu).with_affinity_key("stream-a"))
            .expect("lease a");
        let first = lease_a.core_id();
        drop(lease_a);
        let lease_b = scheduler
            .acquire(Request::auto(DeviceKind::RknnNpu).with_affinity_key("stream-a"))
            .expect("lease b");
        assert_eq!(lease_b.core_id(), first);
    }

    #[test]
    fn explicit_mask_is_respected() {
        let scheduler = rknn_topology();
        let lease = scheduler
            .acquire(Request::explicit(
                DeviceKind::RknnNpu,
                0,
                CoreSelection::Mask(0b010),
            ))
            .expect("lease");
        assert_eq!(lease.device(), (DeviceKind::RknnNpu, 0));
        assert_eq!(lease.core_id(), 1);
    }

    #[test]
    fn invalid_mask_and_empty_topology_error_cleanly() {
        let err = Topology::new(DeployMode::Host, Vec::new()).expect_err("empty");
        assert!(matches!(err, Error::EmptyTopology));

        let scheduler = rknn_topology();
        let err = scheduler
            .acquire(Request::explicit(
                DeviceKind::RknnNpu,
                0,
                CoreSelection::Mask(0b1000),
            ))
            .expect_err("invalid mask");
        assert!(matches!(err, Error::InvalidCoreMask { mask } if mask == 0b1000));
    }

    #[test]
    fn multi_card_selection_prefers_least_loaded_device() {
        let scheduler = Scheduler::new(
            Topology::multi_card_multi_core(DeviceKind::SophonTpu, [(7, 1), (8, 1)])
                .expect("topology"),
        )
        .expect("scheduler");
        let lease_a = scheduler
            .acquire(Request::auto(DeviceKind::SophonTpu))
            .expect("lease a");
        let lease_b = scheduler
            .acquire(Request::auto(DeviceKind::SophonTpu))
            .expect("lease b");
        assert_ne!(lease_a.device(), lease_b.device());
    }

    #[test]
    fn round_robin_rotates_across_cards_and_cores() {
        let scheduler = Scheduler::new(
            Topology::multi_card_multi_core(DeviceKind::RknnNpu, [(0, 2), (1, 2)])
                .expect("topology"),
        )
        .expect("scheduler");
        let leases = (0..4)
            .map(|_| {
                scheduler
                    .acquire(
                        Request::auto(DeviceKind::RknnNpu)
                            .with_policy(SchedulingPolicy::RoundRobin),
                    )
                    .expect("lease")
            })
            .collect::<Vec<_>>();
        let placements = leases
            .iter()
            .map(|lease| (lease.device().1, lease.core_id()))
            .collect::<Vec<_>>();
        assert_eq!(placements, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn instance_pool_tracks_checkout_load_and_affinity() {
        let scheduler = Scheduler::new(
            Topology::multi_card_multi_core(DeviceKind::RknnNpu, [(0, 2), (1, 2)])
                .expect("topology"),
        )
        .expect("scheduler");
        let pool = InstancePool::new(
            scheduler.clone(),
            DeviceKind::RknnNpu,
            2,
            CoreSelection::All,
        )
        .expect("pool");
        let checkout = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("checkout");
        let first = checkout.placement();
        assert_eq!(
            scheduler
                .snapshot()
                .expect("snapshot")
                .iter()
                .flat_map(|device| device.cores.iter())
                .map(|core| core.load)
                .sum::<usize>(),
            1
        );
        drop(checkout);
        let affinity = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("affinity checkout");
        assert_eq!(affinity.placement(), first);
        drop(affinity);
        assert!(scheduler
            .snapshot()
            .expect("snapshot")
            .iter()
            .all(|device| device.cores.iter().all(|core| core.load == 0)));
    }

    #[test]
    fn instance_pool_explicit_checkout_precedes_policy() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::RknnNpu, 2).expect("topology"))
                .expect("scheduler");
        let pool = InstancePool::new(
            scheduler.clone(),
            DeviceKind::RknnNpu,
            2,
            CoreSelection::All,
        )
        .expect("pool");
        let checkout = pool.checkout_explicit(0, 1).expect("explicit checkout");
        assert_eq!(checkout.instance_index(), 1);
        assert_eq!(checkout.placement().core_id, 1);
    }

    #[test]
    fn instance_pool_affinity_keeps_logical_instance_on_shared_core() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::RknnNpu, 1).expect("topology"))
                .expect("scheduler");
        let pool =
            InstancePool::new(scheduler, DeviceKind::RknnNpu, 3, CoreSelection::All).expect("pool");
        let first = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("first checkout");
        let first_index = first.instance_index();
        drop(first);
        let second = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("second checkout");
        assert_eq!(second.instance_index(), first_index);
    }

    #[test]
    fn scheduler_affinity_table_is_bounded_by_core_count() {
        let scheduler = rknn_topology();
        let mut leases = Vec::new();
        for i in 0..6 {
            let lease = scheduler
                .acquire(
                    Request::auto(DeviceKind::RknnNpu).with_affinity_key(format!("stream-{i}")),
                )
                .expect("lease");
            leases.push(lease);
        }
        drop(leases);
        let metrics = scheduler.affinity_metrics();
        assert_eq!(metrics.entries, 5);
        assert!(metrics.evictions >= 1);
        assert!(scheduler
            .snapshot()
            .expect("snapshot")
            .iter()
            .all(|device| device.cores.iter().all(|core| core.load == 0)));
    }

    #[test]
    fn instance_pool_affinity_capacity_evicts_lru() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::RknnNpu, 2).expect("topology"))
                .expect("scheduler");
        let pool = InstancePool::new(
            scheduler.clone(),
            DeviceKind::RknnNpu,
            2,
            CoreSelection::All,
        )
        .expect("pool");
        pool.set_affinity_capacity(2);

        let mut checkouts = Vec::new();
        for i in 0..3 {
            let checkout = pool
                .checkout(SchedulingPolicy::RoundRobin, Some(&format!("stream-{i}")))
                .expect("checkout");
            checkouts.push(checkout);
        }
        drop(checkouts);

        let metrics = pool.affinity_metrics();
        assert_eq!(metrics.entries, 2);
        assert!(metrics.evictions >= 1);
        assert!(scheduler
            .snapshot()
            .expect("snapshot")
            .iter()
            .all(|device| device.cores.iter().all(|core| core.load == 0)));
    }

    #[test]
    fn instance_pool_affinity_expires_after_ttl() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::RknnNpu, 1).expect("topology"))
                .expect("scheduler");
        let pool = InstancePool::new(
            scheduler.clone(),
            DeviceKind::RknnNpu,
            1,
            CoreSelection::All,
        )
        .expect("pool");
        pool.set_affinity_ttl(Duration::from_millis(1));

        let first = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("first");
        drop(first);
        std::thread::sleep(Duration::from_millis(5));

        let second = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("second");
        drop(second);

        let metrics = pool.affinity_metrics();
        assert!(metrics.expired >= 1);
    }

    #[test]
    fn instance_pool_remove_affinity_drops_entry_and_load() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::RknnNpu, 1).expect("topology"))
                .expect("scheduler");
        let pool = InstancePool::new(
            scheduler.clone(),
            DeviceKind::RknnNpu,
            1,
            CoreSelection::All,
        )
        .expect("pool");

        let first = pool
            .checkout(SchedulingPolicy::RoundRobin, Some("stream-a"))
            .expect("first");
        drop(first);
        pool.remove_affinity("stream-a");

        let metrics = pool.affinity_metrics();
        assert_eq!(metrics.entries, 0);
    }

    #[test]
    fn no_matching_device_is_reported() {
        let scheduler =
            Scheduler::new(Topology::single_chip(DeviceKind::IntelGpu, 1).expect("topology"))
                .expect("scheduler");
        let err = scheduler
            .acquire(Request::auto(DeviceKind::CudaGpu))
            .expect_err("no device");
        assert!(matches!(err, Error::NoMatchingDevice { kind } if kind == DeviceKind::CudaGpu));
    }

    #[test]
    fn topology_discovers_registered_devices() {
        let topology = Topology::from_registered_devices(0).expect("registered topology");
        assert!(topology
            .devices()
            .iter()
            .any(|device| device.kind == DeviceKind::Cpu));
        assert!(topology
            .devices()
            .iter()
            .all(|device| device.cores.len() == 1));
        assert_eq!(topology.deployment(), DeployMode::Host);
    }

    proptest! {
        #[test]
        fn allocation_respects_mask_and_lease_drops_restore_load(mask in 1u32..(1u32 << 3), count in 1usize..8) {
            let scheduler = Scheduler::new(
                Topology::single_chip(DeviceKind::RknnNpu, 3).expect("topology"),
            )
            .expect("scheduler");
            let mut leases = Vec::new();
            for _ in 0..count {
                let lease = scheduler
                    .acquire(Request::explicit(DeviceKind::RknnNpu, 0, CoreSelection::Mask(mask)))
                    .expect("lease");
                prop_assert!(mask & (1u32 << u32::from(lease.core_id())) != 0);
                leases.push(lease);
            }
            let total_load: usize = scheduler
                .snapshot()
                .expect("snapshot")
                .iter()
                .flat_map(|device| device.cores.iter().map(|core| core.load))
                .sum();
            prop_assert_eq!(total_load, count);
            drop(leases);
            let total_load_after_drop: usize = scheduler
                .snapshot()
                .expect("snapshot")
                .iter()
                .flat_map(|device| device.cores.iter().map(|core| core.load))
                .sum();
            prop_assert_eq!(total_load_after_drop, 0);
        }

        #[test]
        fn round_robin_cycles_over_eligible_cores(count in 1usize..16) {
            let scheduler = Scheduler::new(
                Topology::single_chip(DeviceKind::RknnNpu, 2).expect("topology"),
            )
            .expect("scheduler");
            let leases = (0..count)
                .map(|_| scheduler
                    .acquire(Request::auto(DeviceKind::RknnNpu)
                        .with_policy(SchedulingPolicy::RoundRobin))
                    .expect("lease"))
                .collect::<Vec<_>>();
            for (index, lease) in leases.iter().enumerate() {
                prop_assert_eq!(usize::from(lease.core_id()), index % 2);
            }
        }
    }
}
