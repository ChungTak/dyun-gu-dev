use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use serde::{Deserialize, Serialize};

use crate::{Buffer, BufferDesc, DeviceKind, Error, Result};

type ReleaseFn = Box<dyn FnMut() + Send + 'static>;
type ReleaseCell = Arc<Mutex<Option<ReleaseFn>>>;

/// Framework-level memory domains used for zero-copy planning and external buffer import.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryDomain {
    Host,
    DmaBuf,
    DrmPrime,
    VaapiSurface,
    CudaDevice,
    MppBuffer,
    SophonDevice,
    Opaque,
}

/// External ownership metadata carried alongside imported buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalHandle {
    pub fd: Option<i32>,
    pub raw: u64,
}

impl ExternalHandle {
    pub const fn none() -> Self {
        Self { fd: None, raw: 0 }
    }

    pub const fn from_fd(fd: i32) -> Self {
        Self {
            fd: Some(fd),
            raw: 0,
        }
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self { fd: None, raw }
    }
}

/// RAII drop guard used to release imported external ownership exactly once.
///
/// The guard stores a boxed callback and calls it when the final guard reference is dropped.
/// The callback is taken under the lock and executed outside it so a panicking callback cannot
/// poison the mutex or be invoked a second time.
pub struct ExternalDropGuard {
    callback: ReleaseCell,
}

impl ExternalDropGuard {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        let mut release = Some(release);
        let callback: ReleaseFn = Box::new(move || {
            if let Some(release) = release.take() {
                release();
            }
        });
        Self {
            callback: Arc::new(Mutex::new(Some(callback))),
        }
    }
}

impl Drop for ExternalDropGuard {
    fn drop(&mut self) {
        let mut callback = match self.callback.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(mut release) = callback.take() {
            // Take under the lock, run outside the critical path for poison safety
            // via `into_inner` above. A panicking callback is absorbed so the
            // release is still treated as consumed exactly once.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut release));
        }
    }
}

impl core::fmt::Debug for ExternalDropGuard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExternalDropGuard").finish_non_exhaustive()
    }
}

/// Memory allocation interface used by devices and reusable pools.
pub trait Allocator: Send + Sync {
    fn allocate(&self, desc: BufferDesc) -> Result<Buffer>;
    fn deallocate(&self, buffer: Buffer) -> Result<()>;
}

/// CPU allocator backed by ordinary host memory.
#[derive(Debug, Default)]
pub struct CpuAllocator;

impl Allocator for CpuAllocator {
    fn allocate(&self, desc: BufferDesc) -> Result<Buffer> {
        if desc.align == 0 {
            return Err(Error::InvalidArgument(
                "buffer alignment must be non-zero".to_string(),
            ));
        }
        Buffer::try_new_host(DeviceKind::Cpu, desc)
    }

    fn deallocate(&self, _buffer: Buffer) -> Result<()> {
        Ok(())
    }
}

/// Capacity contract for [`MemoryPool`] host-side cache.
///
/// Limits apply only to buffers retained in the pool after `deallocate`. Different
/// `(size_bytes, align)` descriptors never share slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryPoolConfig {
    /// Maximum total cached host bytes across all descriptors.
    pub max_cached_bytes: usize,
    /// Maximum number of cached buffer entries.
    pub max_cached_entries: usize,
    /// Maximum entries retained for a single `(size, align)` key.
    pub max_per_descriptor: usize,
}

impl MemoryPoolConfig {
    pub const DEFAULT_MAX_CACHED_BYTES: usize = 256 * 1024 * 1024;
    pub const DEFAULT_MAX_CACHED_ENTRIES: usize = 256;
    pub const DEFAULT_MAX_PER_DESCRIPTOR: usize = 16;

    pub fn new(
        max_cached_bytes: usize,
        max_cached_entries: usize,
        max_per_descriptor: usize,
    ) -> Result<Self> {
        let config = Self {
            max_cached_bytes,
            max_cached_entries,
            max_per_descriptor,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<()> {
        if self.max_cached_bytes == 0 {
            return Err(Error::InvalidArgument(
                "max_cached_bytes must be > 0".to_string(),
            ));
        }
        if self.max_cached_entries == 0 {
            return Err(Error::InvalidArgument(
                "max_cached_entries must be > 0".to_string(),
            ));
        }
        if self.max_per_descriptor == 0 {
            return Err(Error::InvalidArgument(
                "max_per_descriptor must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            max_cached_bytes: Self::DEFAULT_MAX_CACHED_BYTES,
            max_cached_entries: Self::DEFAULT_MAX_CACHED_ENTRIES,
            max_per_descriptor: Self::DEFAULT_MAX_PER_DESCRIPTOR,
        }
    }
}

/// Snapshot of pool cache counters. All values are cumulative or instantaneous
/// process-local diagnostics and remain bounded independently of churn volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryPoolMetrics {
    pub cached_entries: u64,
    pub cached_bytes: u64,
    pub allocations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub returns: u64,
    pub evictions: u64,
    pub evicted_bytes: u64,
    pub rejected_returns: u64,
}

#[derive(Default)]
struct PoolMetricsState {
    allocations: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    returns: AtomicU64,
    evictions: AtomicU64,
    evicted_bytes: AtomicU64,
    rejected_returns: AtomicU64,
}

/// Reusable allocation pool keyed by buffer size and alignment.
pub struct MemoryPool {
    allocator: Arc<dyn Allocator>,
    config: MemoryPoolConfig,
    buffers: Mutex<PoolCache>,
    metrics: PoolMetricsState,
}

struct PoolCache {
    /// Cached host buffers keyed by `(size_bytes, align)`.
    by_key: HashMap<(usize, usize), VecDeque<Buffer>>,
    /// Global LRU order of cache keys (front = oldest).
    lru: VecDeque<(usize, usize)>,
    total_entries: usize,
    total_bytes: usize,
}

impl PoolCache {
    fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            lru: VecDeque::new(),
            total_entries: 0,
            total_bytes: 0,
        }
    }
}

impl core::fmt::Debug for MemoryPool {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryPool")
            .field("config", &self.config)
            .field("cached_buffer_count", &self.cached_buffer_count())
            .finish_non_exhaustive()
    }
}

impl MemoryPool {
    pub fn new(allocator: Arc<dyn Allocator>) -> Self {
        Self::with_config(allocator, MemoryPoolConfig::default())
    }

    pub fn with_config(allocator: Arc<dyn Allocator>, config: MemoryPoolConfig) -> Self {
        Self {
            allocator,
            config,
            buffers: Mutex::new(PoolCache::new()),
            metrics: PoolMetricsState::default(),
        }
    }

    pub fn config(&self) -> MemoryPoolConfig {
        self.config
    }

    pub fn cached_buffer_count(&self) -> usize {
        match self.buffers.lock() {
            Ok(cache) => cache.total_entries,
            Err(poisoned) => poisoned.into_inner().total_entries,
        }
    }

    pub fn cached_bytes(&self) -> usize {
        match self.buffers.lock() {
            Ok(cache) => cache.total_bytes,
            Err(poisoned) => poisoned.into_inner().total_bytes,
        }
    }

    pub fn metrics_snapshot(&self) -> MemoryPoolMetrics {
        let (cached_entries, cached_bytes) = match self.buffers.lock() {
            Ok(cache) => (cache.total_entries as u64, cache.total_bytes as u64),
            Err(poisoned) => {
                let cache = poisoned.into_inner();
                (cache.total_entries as u64, cache.total_bytes as u64)
            }
        };
        MemoryPoolMetrics {
            cached_entries,
            cached_bytes,
            allocations: self.metrics.allocations.load(Ordering::Relaxed),
            cache_hits: self.metrics.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.metrics.cache_misses.load(Ordering::Relaxed),
            returns: self.metrics.returns.load(Ordering::Relaxed),
            evictions: self.metrics.evictions.load(Ordering::Relaxed),
            evicted_bytes: self.metrics.evicted_bytes.load(Ordering::Relaxed),
            rejected_returns: self.metrics.rejected_returns.load(Ordering::Relaxed),
        }
    }

    fn take_cached(&self, desc: BufferDesc) -> Option<Buffer> {
        let mut cache = match self.buffers.lock() {
            Ok(cache) => cache,
            Err(poisoned) => poisoned.into_inner(),
        };
        let key = (desc.size_bytes, desc.align);
        let buffer = {
            let slot = cache.by_key.get_mut(&key)?;
            slot.pop_back()?
        };
        if let Some(pos) = cache.lru.iter().rposition(|entry| *entry == key) {
            cache.lru.remove(pos);
        }
        if cache.by_key.get(&key).is_none_or(|slot| slot.is_empty()) {
            cache.by_key.remove(&key);
        }
        cache.total_entries = cache.total_entries.saturating_sub(1);
        cache.total_bytes = cache.total_bytes.saturating_sub(desc.size_bytes);
        Some(buffer)
    }

    fn push_cached(&self, buffer: Buffer) -> Result<()> {
        let desc = buffer.desc();
        let key = (desc.size_bytes, desc.align);

        // Buffers larger than the entire cache budget are never retained.
        if desc.size_bytes > self.config.max_cached_bytes {
            self.metrics
                .rejected_returns
                .fetch_add(1, Ordering::Relaxed);
            return self.allocator.deallocate(buffer);
        }

        let mut cache = match self.buffers.lock() {
            Ok(cache) => cache,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Enforce per-descriptor bound first (drop oldest entry for this key).
        let mut per_key_evictions = Vec::new();
        let current_len = cache.by_key.get(&key).map_or(0, VecDeque::len);
        let overflow = current_len
            .saturating_add(1)
            .saturating_sub(self.config.max_per_descriptor);
        for _ in 0..overflow {
            let evicted = cache.by_key.get_mut(&key).and_then(|slot| slot.pop_front());
            let Some(evicted) = evicted else {
                break;
            };
            let size = evicted.desc().size_bytes;
            cache.total_entries = cache.total_entries.saturating_sub(1);
            cache.total_bytes = cache.total_bytes.saturating_sub(size);
            per_key_evictions.push((evicted, size));
            if let Some(pos) = cache.lru.iter().position(|entry| *entry == key) {
                cache.lru.remove(pos);
            }
            if cache.by_key.get(&key).is_none_or(|slot| slot.is_empty()) {
                cache.by_key.remove(&key);
            }
        }
        for (evicted, size) in per_key_evictions {
            self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
            self.metrics
                .evicted_bytes
                .fetch_add(size as u64, Ordering::Relaxed);
            let _ = self.allocator.deallocate(evicted);
        }

        // Evict globally until there is room for this buffer.
        while cache.total_entries >= self.config.max_cached_entries
            || cache.total_bytes.saturating_add(desc.size_bytes) > self.config.max_cached_bytes
        {
            if !self.evict_one_locked(&mut cache)? {
                // Nothing left to evict; fall through to underlying free.
                self.metrics
                    .rejected_returns
                    .fetch_add(1, Ordering::Relaxed);
                drop(cache);
                return self.allocator.deallocate(buffer);
            }
        }

        cache.by_key.entry(key).or_default().push_back(buffer);
        cache.lru.push_back(key);
        cache.total_entries = cache.total_entries.saturating_add(1);
        cache.total_bytes = cache.total_bytes.saturating_add(desc.size_bytes);
        self.metrics.returns.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Evict the oldest LRU entry. Returns `false` when the cache is empty.
    fn evict_one_locked(&self, cache: &mut PoolCache) -> Result<bool> {
        let Some(key) = cache.lru.pop_front() else {
            return Ok(false);
        };
        let evicted = {
            let Some(slot) = cache.by_key.get_mut(&key) else {
                return Ok(true);
            };
            let Some(evicted) = slot.pop_front() else {
                if slot.is_empty() {
                    cache.by_key.remove(&key);
                }
                return Ok(true);
            };
            let empty = slot.is_empty();
            if empty {
                cache.by_key.remove(&key);
            }
            evicted
        };
        let size = evicted.desc().size_bytes;
        cache.total_entries = cache.total_entries.saturating_sub(1);
        cache.total_bytes = cache.total_bytes.saturating_sub(size);
        self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .evicted_bytes
            .fetch_add(size as u64, Ordering::Relaxed);
        self.allocator.deallocate(evicted)?;
        Ok(true)
    }
}

impl Allocator for MemoryPool {
    fn allocate(&self, desc: BufferDesc) -> Result<Buffer> {
        if desc.align == 0 {
            return Err(Error::InvalidArgument(
                "buffer alignment must be non-zero".to_string(),
            ));
        }
        self.metrics.allocations.fetch_add(1, Ordering::Relaxed);
        if let Some(buffer) = self.take_cached(desc) {
            self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(buffer);
        }
        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
        self.allocator.allocate(desc)
    }

    fn deallocate(&self, buffer: Buffer) -> Result<()> {
        if buffer.domain() != MemoryDomain::Host || buffer.ref_count() != 1 {
            self.metrics
                .rejected_returns
                .fetch_add(1, Ordering::Relaxed);
            return self.allocator.deallocate(buffer);
        }
        self.push_cached(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::{Allocator, CpuAllocator, ExternalDropGuard, MemoryPool, MemoryPoolConfig};
    use crate::BufferDesc;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn memory_pool_reuses_matching_buffers() {
        let pool = MemoryPool::new(Arc::new(CpuAllocator));
        let first = pool
            .allocate(BufferDesc::new(64, 16))
            .expect("allocate first buffer");
        pool.deallocate(first).expect("deallocate first buffer");
        assert_eq!(pool.cached_buffer_count(), 1);

        let second = pool
            .allocate(BufferDesc::new(64, 16))
            .expect("allocate second buffer");
        assert_eq!(pool.cached_buffer_count(), 0);
        assert_eq!(second.desc(), BufferDesc::new(64, 16));
        let metrics = pool.metrics_snapshot();
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.returns, 1);
    }

    #[test]
    fn memory_pool_does_not_reuse_different_descriptors() {
        let pool = MemoryPool::new(Arc::new(CpuAllocator));
        let first = pool
            .allocate(BufferDesc::new(64, 16))
            .expect("allocate first buffer");
        pool.deallocate(first).expect("deallocate first buffer");

        let second = pool
            .allocate(BufferDesc::new(32, 16))
            .expect("allocate second buffer");
        assert_eq!(second.desc(), BufferDesc::new(32, 16));
        assert_eq!(pool.cached_buffer_count(), 1);
        assert_eq!(pool.metrics_snapshot().cache_misses, 2);
    }

    #[test]
    fn memory_pool_evicts_when_entry_limit_reached() {
        let config = MemoryPoolConfig::new(1024 * 1024, 2, 8).expect("config");
        let pool = MemoryPool::with_config(Arc::new(CpuAllocator), config);

        for size in [16usize, 32, 64] {
            let buffer = pool.allocate(BufferDesc::new(size, 1)).expect("allocate");
            pool.deallocate(buffer).expect("deallocate");
        }

        assert!(pool.cached_buffer_count() <= 2);
        assert!(pool.metrics_snapshot().evictions >= 1);
        assert!(pool.metrics_snapshot().evicted_bytes >= 16);
    }

    #[test]
    fn memory_pool_evicts_when_byte_budget_exceeded() {
        let config = MemoryPoolConfig::new(100, 64, 16).expect("config");
        let pool = MemoryPool::with_config(Arc::new(CpuAllocator), config);

        let a = pool.allocate(BufferDesc::new(60, 1)).expect("a");
        let b = pool.allocate(BufferDesc::new(60, 1)).expect("b");
        pool.deallocate(a).expect("return a");
        pool.deallocate(b).expect("return b");

        assert!(pool.cached_bytes() <= 100);
        assert!(pool.metrics_snapshot().evictions >= 1);
    }

    #[test]
    fn memory_pool_enforces_per_descriptor_cap() {
        let config = MemoryPoolConfig::new(1024 * 1024, 64, 2).expect("config");
        let pool = MemoryPool::with_config(Arc::new(CpuAllocator), config);
        let buffers: Vec<_> = (0..4)
            .map(|_| pool.allocate(BufferDesc::new(8, 1)).expect("allocate"))
            .collect();
        for buffer in buffers {
            pool.deallocate(buffer).expect("deallocate");
        }
        assert_eq!(pool.cached_buffer_count(), 2);
        assert!(pool.metrics_snapshot().evictions >= 2);
    }

    #[test]
    fn memory_pool_rejects_buffer_larger_than_cache_budget() {
        let config = MemoryPoolConfig::new(32, 8, 4).expect("config");
        let pool = MemoryPool::with_config(Arc::new(CpuAllocator), config);
        let large = pool
            .allocate(BufferDesc::new(64, 1))
            .expect("allocate large");
        pool.deallocate(large).expect("deallocate large");
        assert_eq!(pool.cached_buffer_count(), 0);
        assert_eq!(pool.metrics_snapshot().rejected_returns, 1);
    }

    #[test]
    fn external_drop_guard_releases_exactly_once_across_arc_clones() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guard = Arc::new(ExternalDropGuard::new({
            let counter = Arc::clone(&counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }));
        let clone = Arc::clone(&guard);
        drop(guard);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        drop(clone);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn external_drop_guard_absorbs_callback_panic_exactly_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let guard = ExternalDropGuard::new({
            let counter = Arc::clone(&counter);
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                panic!("callback boom");
            }
        });
        drop(guard);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
