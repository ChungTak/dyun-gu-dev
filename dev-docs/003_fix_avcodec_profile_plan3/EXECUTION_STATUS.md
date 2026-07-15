# 003 执行状态记录

> 初始状态：未执行。允许 `Not started`、`In progress`、`Blocked`、`Done`。Done 必须绑定 commit、命令和
> artifact；Plan 2 的结果不能自动继承。

## Baseline

| Field | Value |
|---|---|
| dyun start commit | `75c8bf06fbc6cd3f4094d2599f66a04d68c7fe13` |
| old SDK revision | `fc728aa9ea3e0a85401d2cd4de1b762ffcf92a51` |
| accepted SDK RC/commit | `84a2832796717f46a1009ee064c914b0ad66ac19` |
| toolchain/target | rustc 1.94.1 / x86_64-unknown-linux-gnu |
| FFmpeg runners | Ubuntu 22.04 `libavutil-dev`, `libavcodec-dev`, `libavformat-dev`, `libswscale-dev`, `libx264-dev`, `libx265-dev`, `libopenh264-dev` |
| NV runner | compile-only on host (no NVIDIA GPU); `shiguredo_nvcodec` pinned to `2026.1.0` |
| dirty worktree | 无（已提交后更新） |

## Requirement Status

| ID | Requirement | Status | Commit | Command/Artifact |
|---|---|---|---|---|
| INT3-01 | Immutable SDK RC | Done | `84a2832796717f46a1009ee064c914b0ad66ac19` | `cargo tree -p avcodec` 指向固定 rev |
| INT3-02 | Single direct SDK dependency | Done | `crates/dg-media-avcodec/Cargo.toml` | 仅 `avcodec` 一个 direct dep，`default-features=false` |
| INT3-03 | Profile-only features | Done | `crates/dg-media-avcodec/Cargo.toml`, `crates/dg-media/Cargo.toml` | 删除 `codec-*` alias |
| INT3-04 | Thin profile mapping | Done | `crates/dg-media/src/profile.rs` | `to_sdk()` 一对一，不返回 policy/descriptor/I/O plan |
| INT3-05 | VideoSdk service | Done | `crates/dg-media/src/session.rs` | `AvcodecSdkService` 包装 `VideoSdk`，转发高层请求 |
| INT3-06 | Decode/Encode sessions | Done | `crates/dg-media/src/avcodec.rs` | 使用 `VideoDecoderSession`/`VideoEncoderSession` |
| INT3-07 | Image processing | Done | `crates/dg-media/src/avcodec.rs` | `VideoImageProcessorSession` + `ImageProcessorRequest` |
| INT3-08 | Transcoder/Graph | Done | `crates/dg-media/src/transcoder.rs` | `VideoSdk::create_transcoder` + `VideoTranscoderSession` |
| INT3-09 | Bridge/zero-copy | Done | `crates/dg-media/src/bridge.rs` | 移除 `register_stage_to_host_hook` 旧测试；保留 domain/copy 证据 |
| INT3-10 | Multi Profile isolation | Done | `crates/dg-media/src/profile.rs`, `crates/dg-media/tests/profile_matrix.rs` | 多 profile 必须显式指定；单 profile 默认 |
| INT3-11 | Errors/reports/diagnostics | Done | `crates/dg-media/src/diagnostics.rs`, `crates/dg-media/src/session.rs` | 从 `OwnedVideoBuildReport` 派生 diagnostics |
| INT3-12 | Legacy migration | Done | `crates/dg-media/src/legacy.rs` | `hw` 映射到 profile 并输出 deprecation warning |
| INT3-13 | Software/NV production | Partial | `cargo check -p dg-media --features avcodec-profile-software` / `avcodec-profile-nvcodec-host` | Software 真实媒体单元通过；NV 仅 compile-only |
| INT3-14 | Unverified HW gating | Done | `crates/dg-media/src/profile.rs` | RK/OneVPL/AMF 配置识别但不保证 decode |
| INT3-15 | CI/release/rollback | Done | `Cargo.lock`, 本文件 | 固定 SDK rev；记录上游 issue |

## Phase Log

### Phase 0 — Upstream admission and failing guards
- Status: Done
- Commits/commands/results:
  - 固定 avcodec rev `84a2832796717f46a1009ee064c914b0ad66ac19`
  - `cargo clippy -p dg-media --features avcodec-profile-native-free --all-targets -- -D warnings` 通过
  - `cargo test -p dg-media --features avcodec-profile-native-free` 通过
- Blockers: `shiguredo_nvcodec 2026.2.0` / `shiguredo_amf 2026.3.0` 与上游 backend 不兼容，已 lock 到 `2026.1.0`（见 UPSTREAM_ISSUES.md）

### Phase 1 — Dependency/Profile/Service
- Status: Done
- Commits/commands/results:
  - 重写 `crates/dg-media/src/profile.rs`：删除 policy/descriptor/I/O，添加 `to_sdk()`
  - 重写 `crates/dg-media/src/session.rs`：`AvcodecSdkService` 包装 `VideoSdk`
  - 更新 `crates/dg-media-avcodec/src/lib.rs`：只 re-export V3 facade
  - 更新 `crates/dg-media-avcodec/Cargo.toml`、`crates/dg-media/Cargo.toml`、`crates/dg-cli/Cargo.toml`、`crates/dg-capi/Cargo.toml` 删除 `codec-*` alias
- Blockers: 无

### Phase 2 — Elements and bridge
- Status: Done
- Commits/commands/results:
  - 重写 `crates/dg-media/src/avcodec.rs`：使用 `VideoDecoderSession`/`VideoEncoderSession`/`VideoImageProcessorSession`
  - 重写 `crates/dg-media/src/bridge.rs`：移除 V2 staging hook 测试
  - 更新 `crates/dg-media/tests/source_scan.rs`、`crates/dg-media/tests/dependency_contract.rs`
- Blockers: 无

### Phase 3 — Transcoder, diagnostics and entrypoints
- Status: Done
- Commits/commands/results:
  - 重写 `crates/dg-media/src/transcoder.rs`：使用 `VideoSdk::create_transcoder`
  - 重写 `crates/dg-media/src/diagnostics.rs`：从 `OwnedVideoBuildReport` 派生
  - 更新 `crates/dg-media/tests/profile_matrix.rs`、`crates/dg-media/tests/media_pipeline.rs` 适配显式 profile
- Blockers: 无

### Phase 4 — Real media, hardware and release
- Status: Partial
- SDK/dyun revisions: avcodec `84a28327`, dyun `75c8bf06` → current PR
- Commands/artifacts:
  - `cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software --lib` 通过
  - `cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software` 通过
  - `cargo test --workspace --locked` 通过
  - `cargo check -p dg-media --features avcodec-profile-nvcodec-host` / `avcodec-profile-nvcodec-device-frame` compile-only 通过
- Blockers: 无 NVIDIA/Intel/AMD 真机，NV/OneVPL/AMF 仅 compile-only；RKMPP 无真机签字

## Final Blockers

- [x] SDK RC 已接纳（rev `84a28327`）。
- [x] 旧 Factory/Registry/descriptor 路径已删除。
- [x] NativeFree/Software 真实媒体通过（单/组合 feature 单元测试与 pipeline 测试）。
- [x] 多 Profile 无隐式选择；测试已显式指定 profile。
- [x] 上游 handoff 已回填并固定 RC；`Cargo.lock` 包含不可变 rev。
