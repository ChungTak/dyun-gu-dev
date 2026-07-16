//! Per-session media counters for diagnostics (plan 11).
//!
//! # Semantics vs SDK runtime diagnostics
//!
//! | Field | Meaning |
//! |---|---|
//! | [`MediaSessionStats::submitted`] / `input_queued` | Frames accepted into the **pump** via `submit_input` (may still wait for backend accept). |
//! | [`MediaSessionStats::accepted`] / `backend_accepted` | Frames the **backend** accepted on submit (not `Again`). |
//! | SDK [`crate::MediaRuntimeDiagnostics::submitted`] | Successful submits **inside** the SDK session only. |
//!
//! Do not compare pump `submitted` 1:1 with SDK `submitted`; use pump `accepted` for
//! backend accept counts, and SDK diagnostics for session-level Again/Pending/generation.

/// Counters maintained by async media cores and bridges for one session lifetime.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSessionStats {
    /// Frames queued into the pump (`submit_input`). Alias concept: `input_queued`.
    pub submitted: u64,
    /// Frames accepted by the backend after convert+submit (not `Again`). Alias: `backend_accepted`.
    pub accepted: u64,
    pub output_frames: u64,
    pub again_count: u64,
    pub pending_count: u64,
    pub flush_calls: u64,
    pub flush_polls: u64,
    pub host_clone_count: u64,
    pub row_repack_count: u64,
    pub domain_staging_count: u64,
    pub copied_bytes: u64,
    pub dropped_frames: u64,
}

impl MediaSessionStats {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            submitted: 0,
            accepted: 0,
            output_frames: 0,
            again_count: 0,
            pending_count: 0,
            flush_calls: 0,
            flush_polls: 0,
            host_clone_count: 0,
            row_repack_count: 0,
            domain_staging_count: 0,
            copied_bytes: 0,
            dropped_frames: 0,
        }
    }

    pub fn record_transfer(
        &mut self,
        path_kind: crate::TransferPathKind,
        copy_count: usize,
        bytes: usize,
    ) {
        match path_kind {
            crate::TransferPathKind::HostClone => {
                self.host_clone_count = self.host_clone_count.saturating_add(1);
            }
            crate::TransferPathKind::RowRepack => {
                self.row_repack_count = self.row_repack_count.saturating_add(1);
            }
            crate::TransferPathKind::DomainStaging => {
                self.domain_staging_count = self.domain_staging_count.saturating_add(1);
            }
            crate::TransferPathKind::OwnershipMove | crate::TransferPathKind::SharedExternal => {}
        }
        if copy_count > 0 {
            self.copied_bytes = self
                .copied_bytes
                .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        }
    }

    /// Compact, field-stable string for logs (no pointers / fd / extradata).
    ///
    /// Uses explicit names `input_queued` / `backend_accepted` so operators do not
    /// confuse pump counters with SDK `VideoRuntimeDiagnostics.submitted`.
    pub fn summary(&self) -> String {
        format!(
            "input_queued={} backend_accepted={} outputs={} again={} pending={} flush_calls={} \
             host_clone={} row_repack={} domain_staging={} copied_bytes={} dropped={}",
            self.submitted,
            self.accepted,
            self.output_frames,
            self.again_count,
            self.pending_count,
            self.flush_calls,
            self.host_clone_count,
            self.row_repack_count,
            self.domain_staging_count,
            self.copied_bytes,
            self.dropped_frames
        )
    }

    /// Alias for [`Self::submitted`] (frames queued into the pump).
    #[must_use]
    pub const fn input_queued(&self) -> u64 {
        self.submitted
    }

    /// Alias for [`Self::accepted`] (backend-accepted submits).
    #[must_use]
    pub const fn backend_accepted(&self) -> u64 {
        self.accepted
    }
}

#[cfg(test)]
mod tests {
    use super::MediaSessionStats;
    use crate::TransferPathKind;

    #[test]
    fn record_transfer_accounts_paths_and_bytes() {
        let mut stats = MediaSessionStats::new();
        stats.record_transfer(TransferPathKind::HostClone, 1, 100);
        stats.record_transfer(TransferPathKind::RowRepack, 1, 50);
        stats.record_transfer(TransferPathKind::OwnershipMove, 0, 999);
        assert_eq!(stats.host_clone_count, 1);
        assert_eq!(stats.row_repack_count, 1);
        assert_eq!(stats.copied_bytes, 150);
        assert!(stats.summary().contains("copied_bytes=150"));
    }
}
