# 003 执行状态记录

> Done 必须绑定证据；Plan 2 结果不自动继承。NV 真机与 RC2 签字仍阻塞全局完成。

## Baseline

| Field | Value |
|---|---|
| accepted SDK RC | `7faba6fe264aa5ae5bd2f1666084f4bc52aa7d0f` (`0.2.0-rc.1` Plan5 RC1) |
| previous pin | `84a2832796717f46a1009ee064c914b0ad66ac19` (`0.2.0-rc.0`) |
| toolchain | rustc stable / x86_64-unknown-linux-gnu |
| Software env | `scripts/env-software-avcodec.sh` (+ local FFmpeg `PKG_CONFIG_PATH` when needed) |
| NV | compile-only（`shiguredo_nvcodec` lock `2026.1.0`）；无 GPU 真机 |

## Requirement Status

| ID | Status | Evidence |
|---|---|---|
| INT3-01～05 | Done | pin rev、单 facade dep、profile feature、`to_sdk`、`VideoSdk` service |
| INT3-06 | Done | H.264 encode/decode；CSC flush/reset；`Core::reset`；CSC→RGB24 二代测试 |
| INT3-07 | Done | resize + libyuv report；decode-side CSC；device-frame 拒 resize |
| INT3-08 | Done | 融合 TranscodeCore + **`media_transcode` graph element**；H.264→H.265 / software re-encode |
| INT3-09 | Done | bridge domain/copy；无隐式 CSC 旁路 |
| INT3-10 | Done | 多 profile 显式选择；rust-h264 vs ffmpeg 不串栈 |
| INT3-11 | Done | build error 上下文；session diagnostics |
| INT3-12 | Done | `hw` 映射 + warning；**移除目标 0.2.0** |
| INT3-13 | **Partial** | Software 真实媒体 Done；NV 真机缺失（compile-only hard-fail CI） |
| INT3-14 | Done | `support_level`；unverified 创建时 warn；示例标注 |
| INT3-15 | Done | CI matrix（native-free / software / combo / NV compile hard-fail） |

## Phase 7 — Audit fixes (this pass)

Bugs fixed:
1. Decode CSC `flush`/`reset` 未处理 CSC session / `csc_pending` → 已 flush CSC 并在 reset 时 drop
2. H266 静默降级为 H265 → 统一走 `format_map` 拒绝
3. Transcoder bitstream 映射缺 Avcc/Hvcc → 用 `format_map`
4. `Core::reset` 未暴露 → Decode/Encode/Resize/Transcode 均暴露
5. NV CI soft-fail → `cargo check` hard-fail
6. `memory_domain` 静默无效 → 配置时 warn（Profile 拥有 domain）
7. 示例仍写 Factory V2 / 违规 domain → 清理

Incomplete filled:
- 注册 `media_transcode` 图元素
- unverified profile 广告 warn
- CSC 真实像素 + reset 二代测试
- legacy 移除版本冻结为 0.2.0

## Phase 8 — Upstream Plan5 RC1 bump (`7faba6fe`)

Bugs / gaps fixed:
1. `avcodec` pin 从 `84a28327` (`0.2.0-rc.0`) 升至 `7faba6fe` (`0.2.0-rc.1`)
2. `map_video_runtime_error` 丢弃 `VideoRuntimeError` 的 profile/role/backend/domains → 通过 `AvError::with_context` 保留
3. `append_av_error_context` 未导出 profile / domain / allow_staging → 补全
4. Plan 11 runtime diagnostics 未对上 SDK5-01 四会话契约 → 增加 `MediaRuntimeDiagnostics` 与 Decode/Encode/Resize/Transcode 的 `runtime_diagnostics()`
5. `dependency_contract` 仍锁定旧 rev → 同步新 pin

```bash
RUSTUP_TOOLCHAIN=stable cargo test -p dg-media --features avcodec-profile-native-free
RUSTUP_TOOLCHAIN=stable cargo check -p dg-media --features avcodec-profile-nvcodec-host
RUSTUP_TOOLCHAIN=stable cargo check -p dg-media --features avcodec-profile-amf-host
# Software (needs FFmpeg + libclang):
# source scripts/env-software-avcodec.sh
# export PKG_CONFIG_PATH=/path/to/ffmpeg/lib/pkgconfig:$PKG_CONFIG_PATH
# cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software
```

## Phase 9 — External import + transcoder context fix

Bugs / gaps fixed:
1. `transcoder.rs` 本地 `map_video_runtime_error` 仍只返回 `error.source` → 改为共用 `avcodec::map_video_runtime_error`
2. `try_import_external_image` stub → 实现 Host 安全导入 + `import_external_image/packet`（unsafe 仅在 facade）
3. re-export `External*Descriptor` / `ExternalDropGuard`；drop-guard 单测 + Host image/packet roundtrip

## Phase 10 — Review pass

Bugs / gaps fixed:
1. Decode-side CSC：`submit` 遇 `Again` 时原 Image 被 move 丢弃 → `csc_input` 缓存 + clone 重试
2. 暴露 `csc_runtime_diagnostics()`（decoder 与 CSC 计数分离）
3. decode `time_base` 会话不变量强制校验 + 单测
4. `buffer_into_avcodec_handle`：独占 Host 零拷贝 move；共享则 clone
5. pump/SDK counters 文档与 summary 命名分离（`input_queued` / `backend_accepted` / `sdk_submitted`）
6. CI toolchain 与 `rust-toolchain.toml` 对齐为 `1.94.1`
7. CSC `EndOfStream` 在 flush 后不再直接 `InvalidState`（清空 pending 后继续 drain）
8. Encoder 会话固定 geometry/format/time_base；打开会话不再全帧 clone
9. Transcoder 会话不变量（codec/bitstream/stream_index/time_base）与 reset 清空

```bash
RUSTUP_TOOLCHAIN=stable cargo test -p dg-media --features avcodec-profile-native-free
RUSTUP_TOOLCHAIN=stable cargo clippy -p dg-media -p dg-media-avcodec \
  --features avcodec-profile-native-free --all-targets -- -D warnings
```

## Final blockers (global Done)

- [ ] NV Host/device-frame **真机**媒体证据
- [ ] 上游 RC2 重验与正式发布签字（文档 15）
