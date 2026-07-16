# 004 执行状态记录

> 初始状态：未执行。允许 `Not started`、`In progress`、`Blocked`、`Done`。Plan 3 结果只作 baseline；
> Done 必须绑定 commit、命令和 artifact。

## Baseline

| Field | Value |
|---|---|
| Plan start dyun commit | `137b7b80896395cf8164e8c2172a345d9bc857fd` |
| current SDK pin | `2068432426793c94cd5d415b56a4b2e9a3c1ee73`（`0.2.0-rc.2` dereferenced commit） |
| accepted RC2 | `Done` — tag `0.2.0-rc.2` / tag object `06ac7302f83a94fe40cb321c01bbc3cb794d9e64` / commit `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| toolchain/target | `rustc 1.94.1` / `x86_64-unknown-linux-gnu` |
| Software | NativeFree 通过；Software profile H.264 encoder create 报 `BackendHintCapabilityMismatch`（FFmpeg 4.4.2 / 6.1 均同） |
| NV | compile-only 通过（`cargo check` nvcodec-host / device-frame）；无 GPU 真机 |

## Requirement Status

| ID | Requirement | Status | Commit | Command/Artifact |
|---|---|---|---|---|
| INT4-01 | RC2 admission | Done | `20684324` | `git ls-remote --tags` 确认 `0.2.0-rc.2` tag 存在 |
| INT4-02 | Revision/lock consistency | Done | `20684324` | manifest/lock/dependency_contract 均指向 `20684324` |
| INT4-03 | Toolchain/environment | Done | `20684324` | `rustc 1.94.1`；FFmpeg 升级至 6.1（PPA） |
| INT4-04 | High-level boundary | Done | `20684324` | `source_scan` / `dependency_contract` 在 RC2 通过 |
| INT4-05 | NativeFree/Software | Partial | `20684324` | NativeFree 全通过；Software `CreateEncoder` 失败（见 UP4-002，非 FFmpeg 版本问题） |
| INT4-06 | Multi Profile | Partial | `20684324` | NativeFree+Software 构建通过；Software encoder 失败导致 3 个组合测试失败 |
| INT4-07 | NV production | Blocked | — | compile-only 通过；真机无 GPU |
| INT4-08 | External/zero-copy | Done | `20684324` | NativeFree bridge / ownership 测试通过；device-frame 待 NV 真机 |
| INT4-09 | CI/status/handoff | In progress | `20684324` | 状态文件已更新；上游 handoff 待确认 |
| INT4-10 | Stable/rollback | Not started | — | 待 Software/NV 通过及 stable tag 后执行 |

## Phase Log

### Phase 0 — RC2 admission
- Status: Done
- Evidence: 远端 `0.2.0-rc.2` tag 已验证；tag object `06ac7302f83a94fe40cb321c01bbc3cb794d9e64`；dereferenced commit `2068432426793c94cd5d415b56a4b2e9a3c1ee73`。

### Phase 1 — Pin and environment
- Status: Done
- Commits/commands:
  - `crates/dg-media-avcodec/Cargo.toml` avcodec `rev` 更新为 `20684324`。
  - `Cargo.lock` 中所有 avcodec workspace git packages 同步到 `20684324` / `0.2.0-rc.2`。
  - `crates/dg-media/tests/dependency_contract.rs` 中的预期 SHA 更新为 `20684324`。
  - 验证 `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 通过。
  - 为验证 Software profile，通过 PPA 将 FFmpeg 从 4.4.2 升级至 6.1。

### Phase 2 — Software revalidation
- Status: Partial
- Commands/artifacts:
  - `cargo test -p dg-media --locked --features avcodec-profile-native-free` -> 84 passed / 0 failed
  - `cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software` -> 65 passed / 3 failed
  - 失败测试：`software_h264_encode_decode_preserves_timing_and_stream_index`、`software_h264_transcode_stays_on_ffmpeg_stack`、`multi_profile_encoder_backends_do_not_cross_stack`
  - 失败根因：`BackendHintCapabilityMismatch` on `CreateEncoder` for `profile=software backend=ffmpeg`；在 FFmpeg 4.4.2 与 6.1 下一致复现，排除 FFmpeg 版本问题，判定为上游 `avcodec-codec-ffmpeg` encoder capability/Profile 描述符问题（UP4-002 已更新）。

### Phase 3 — NV hardware
- Status: Partial
- Device/driver/commands/artifacts: 无 GPU；`cargo check -p dg-media --locked --features avcodec-profile-nvcodec-host` / `avcodec-profile-nvcodec-device-frame` 通过。

### Phase 4 — Handoff and stable
- Status: Not started
- SDK/dyun commits/tags: 待 UP4-002 上游确认/修复、NV 真机后执行。

## Final Blockers

- [x] RC2 不可变候选可接纳。
- [ ] Software/组合在 RC2 上通过。（UP4-002：Software H.264 encoder `BackendHintCapabilityMismatch`）
- [ ] dyun NV Host/device-frame 真机通过。（本地无 GPU）
- [ ] 上游接受 handoff。
- [ ] stable/rollback 完成。
