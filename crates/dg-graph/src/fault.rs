//! Hot-update fault injection for CORE6-05/R6-018 regression tests.
//!
//! Armed faults are consumed once. Product code never arms these points; only
//! tests and diagnostics should call [`exclusive`] + [`HotUpdateFaultGuard::arm`].
//! Concurrent tests must hold [`HotUpdateFaultGuard`] for the full injection
//! sequence so the process-global armed point is not racy.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::error::{Error, Result};

/// Phase boundary where a one-shot fault may be injected during hot update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HotUpdateFaultPoint {
    /// After prepare/create of replacement nodes, before quiesce.
    AfterPrepare = 1,
    /// After affected workers are joined, before routes are switched.
    AfterQuiesce = 2,
    /// After routes are switched, before replacement workers are spawned.
    AfterSwitch = 3,
    /// Force drain to time out immediately after spawn.
    DrainTimeout = 4,
}

static ARMED: AtomicU8 = AtomicU8::new(0);
static FAULT_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that serializes fault-injection tests and clears on drop.
pub struct HotUpdateFaultGuard {
    _lock: MutexGuard<'static, ()>,
}

impl HotUpdateFaultGuard {
    /// Arms a one-shot fault at `point` (replaces any previous arm).
    pub fn arm(&self, point: HotUpdateFaultPoint) {
        ARMED.store(point as u8, Ordering::SeqCst);
    }

    /// Clears any armed point while keeping the exclusive lock.
    pub fn clear(&self) {
        ARMED.store(0, Ordering::SeqCst);
    }
}

impl Drop for HotUpdateFaultGuard {
    fn drop(&mut self) {
        ARMED.store(0, Ordering::SeqCst);
    }
}

/// Acquires the process-global fault mutex. Hold for the entire test sequence.
pub fn exclusive() -> HotUpdateFaultGuard {
    let lock = FAULT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ARMED.store(0, Ordering::SeqCst);
    HotUpdateFaultGuard { _lock: lock }
}

/// Arms a one-shot fault and holds the mutex until the guard drops.
///
/// Prefer [`exclusive`] when the test needs multiple arm/clear cycles.
pub fn arm(point: HotUpdateFaultPoint) -> HotUpdateFaultGuard {
    let guard = exclusive();
    guard.arm(point);
    guard
}

/// Clears any armed hot-update fault without taking the mutex.
pub fn clear() {
    ARMED.store(0, Ordering::SeqCst);
}

/// Returns true and disarms if `point` is currently armed.
pub(crate) fn take(point: HotUpdateFaultPoint) -> bool {
    ARMED
        .compare_exchange(point as u8, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Returns `Err` when `point` is armed (and disarms it).
pub(crate) fn check(point: HotUpdateFaultPoint) -> Result<()> {
    if take(point) {
        return Err(Error::Runtime(format!(
            "injected hot-update fault at {point:?}"
        )));
    }
    Ok(())
}
