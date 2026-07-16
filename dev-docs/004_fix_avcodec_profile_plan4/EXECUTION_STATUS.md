# 004 执行状态记录

> 初始状态：未执行。允许 `Not started`、`In progress`、`Blocked`、`Done`。Plan 3 结果只作 baseline；
> Done 必须绑定 commit、命令和 artifact。

## Baseline

| Field | Value |
|---|---|
| Plan start dyun commit | `137b7b80896395cf8164e8c2172a345d9bc857fd` |
| current SDK pin | `7faba6fe264aa5ae5bd2f1666084f4bc52aa7d0f`（post-RC1 main） |
| accepted RC2 | `Blocked` — 远端不存在 `0.2.0-rc.2` tag，HEAD 为 `b0f98dfafb95134a41307f3e5706e5d2518f0207` |
| toolchain/target | `rustc 1.94.1` / `x86_64-unknown-linux-gnu` |
| Software | NativeFree 通过；Software profile 在当前 FFmpeg 4.4.2 下 `CreateEncoder` 报 `BackendHintCapabilityMismatch` |
| NV | compile-only 通过（`cargo check` nvcodec-host / device-frame）；无 GPU 真机 |

## Requirement Status

| ID | Requirement | Status | Commit | Command/Artifact |
|---|---|---|---|---|
| INT4-01 | RC2 admission | Blocked | — | `git ls-remote --tags` 仅见 `0.2.0-rc.0` / `0.2.0-rc.1` |
| INT4-02 | Revision/lock consistency | Blocked | 当前 pin=`7faba6f` | 待 RC2 tag 发布后再改 pin/lock |
| INT4-03 | Toolchain/environment | In progress | `137b7b8` | `rustc 1.94.1` 通过；FFmpeg 4.4.2 不满足 6/7/8 要求 |
| INT4-04 | High-level boundary | Done | `137b7b8` | `source_scan` / `dependency_contract` 在当前 pin 通过 |
| INT4-05 | NativeFree/Software | Partial | `137b7b8` | NativeFree 全通过；Software `CreateEncoder` 失败（见 UP4-002） |
| INT4-06 | Multi Profile | Partial | `137b7b8` | NativeFree+Software 构建通过；Software encoder 失败导致多 Profile 组合测试失败 |
| INT4-07 | NV production | Blocked | — | compile-only 通过；真机无 GPU |
| INT4-08 | External/zero-copy | In progress | `137b7b8` | NativeFree bridge 测试通过；device-frame 待 NV 真机 |
| INT4-09 | CI/status/handoff | In progress | `137b7b8` | 状态文件正在更新 |
| INT4-10 | Stable/rollback | Not started | — | 待 RC2 接纳后执行 |

## Phase Log

### Phase 0 — RC2 admission
- Status: Blocked
- Evidence/blockers: 远端 `TimothyWalker6922/avcodec-rs-develop` 无 `0.2.0-rc.2` tag；当前 pin 为 post-RC1 main commit `7faba6f`。

### Phase 1 — Pin and environment
- Status: In progress
- Commits/commands: `137b7b8` 已验证 `rustc 1.94.1`、`cargo fmt`、`cargo clippy` 通过；FFmpeg 4.4.2 与计划要求 6/7/8 不符，记录为 UP4-002。

### Phase 2 — Software revalidation
- Status: Partial
- Commands/artifacts:
  - `cargo test -p dg-media --locked --features avcodec-profile-native-free` -> 84 passed / 0 failed
  - `cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software` -> 65 passed / 3 failed (`software_h264_encode_decode_preserves_timing_and_stream_index`, `software_h264_transcode_stays_on_ffmpeg_stack`, `multi_profile_encoder_backends_do_not_cross_stack`)
  - 失败根因：`BackendHintCapabilityMismatch` on `CreateEncoder` for `profile=software backend=ffmpeg`。

### Phase 3 — NV hardware
- Status: Partial
- Device/driver/commands/artifacts: 无 GPU；`cargo check -p dg-media --locked --features avcodec-profile-nvcodec-host` / `avcodec-profile-nvcodec-device-frame` 通过。

### Phase 4 — Handoff and stable
- Status: Not started
- SDK/dyun commits/tags: 待 RC2 tag 发布、Software 环境补齐、NV 真机后执行。

## Final Blockers

- [ ] RC2 不可变候选可接纳。（远端 `0.2.0-rc.2` tag 缺失）
- [ ] Software/组合在 RC2 上通过。（FFmpeg 4.4.2 导致 `BackendHintCapabilityMismatch`）
- [ ] dyun NV Host/device-frame 真机通过。（本地无 GPU）
- [ ] 上游接受 handoff。
- [ ] stable/rollback 完成。
