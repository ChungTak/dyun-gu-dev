# avcodec-rs RC2 上游问题记录

> 初始无条目。只记录无法在 dyun 正确修复且有最小复现的问题。禁止复制 backend、policy、domain或 staging
> 作为绕过。

## 模板

### UP4-XXX — 标题
- 状态：Open / Fixed candidate / Verified / Closed
- SDK tag/commit：
- dyun commit：
- Profile/role：
- 期望行为：
- 实际行为：
- 最小复现命令：
- structured error/report/diagnostics：
- toolchain/environment/device：
- 上游 fixture/test：
- 修复 commit/新候选：
- dyun 重验 artifact：
- 临时处置：禁用受影响 Profile；不得添加低层绕过

关闭要求：上游不可变候选含修复，最小测试通过，dyun 更新 pin并重跑受影响矩阵。

---

### UP4-001 — `0.2.0-rc.2` annotated tag 未发布
- 状态：Closed
- SDK tag/commit：`0.2.0-rc.2` tag object `06ac7302f83a94fe40cb321c01bbc3cb794d9e64`；dereferenced commit `2068432426793c94cd5d415b56a4b2e9a3c1ee73`
- dyun commit：`137b7b80896395cf8164e8c2172a345d9bc857fd`
- Profile/role：全部（pin 迁移前置条件）
- 期望行为：远端存在不可变 `0.2.0-rc.2` tag 及解引用 SHA；dyun 可原子更新 manifest/lock/contract。
- 实际行为：tag 已发布；dyun 已更新 pin 并验证 `cargo fetch --locked` 与 `dependency_contract`。
- 最小复现命令：
  ```bash
  git ls-remote --tags https://github.com/TimothyWalker6922/avcodec-rs-develop.git | grep '0.2.0-rc.2'
  ```
- structured error/report/diagnostics：N/A
- toolchain/environment/device：N/A
- 上游 fixture/test：N/A
- 修复 commit/新候选：`0.2.0-rc.2`
- dyun 重验 artifact：PR 中 `crates/dg-media-avcodec/Cargo.toml`、`Cargo.lock`、`crates/dg-media/tests/dependency_contract.rs` 均指向 `20684324`。
- 临时处置：无。

### UP4-002 — Software profile H.264 encoder `CreateEncoder` 返回 `BackendHintCapabilityMismatch`
- 状态：Fixed candidate（本地 avcodec-rs 已提交，待 tag/dyun pin）
- SDK tag/commit：基线 `0.2.0-rc.2` / `20684324`；修复 commit `f3c1c04`（local avcodec-rs main）
- dyun：path pin `../../../avcodec-rs/crates/sdk/avcodec`（调试用）
- Profile/role：`avcodec-profile-software` / encoder
- 期望行为：Software H.264 encoder create 成功；JPEG 亦可走 Software profile。
- 根因：
  1. `avcodec-codec-ffmpeg` 将 libavcodec major **仅 60–62** 标为支持；**58（FFmpeg 4.x）** 被标 `Unsupported`，`enc_h264=false` → capability 拒绝 → `BackendHintCapabilityMismatch`。
  2. Software profile 编码器 allow-list **仅 `ffmpeg`**，而 ffmpeg backend **不实现 JPEG** create_*，Software JPEG 永远失败。
- 上游修复（本地）：
  1. `version_series_for_major`：支持 58/59（V4/V5），≥63 前向兼容 V8 策略。
  2. `VideoProfile::Software`：`decoder/encoder = ["ffmpeg","jpeg"]`（Ordered）；`profile-software` feature 加入 `jpeg`。
- 最小复现 / 重验：
  ```bash
  # avcodec-rs
  source ../dyun-gu-dev/scripts/env-software-avcodec.sh
  cargo test -p avcodec-codec-ffmpeg --lib version_series
  cargo test -p avcodec --features profile-software --test v3_facade_contract

  # dyun（path pin）
  cargo test -p dg-media --features avcodec-profile-software --test media_pipeline
  cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software software_h264
  ```
- 仍需：`libx264` 在运行时可用；否则 `enc_h264` 仍为 false（能力诚实失败，非版本门禁）。
- 临时处置：修复进 avcodec-rs 后打 RC3/补丁 tag；dyun 再原子改 pin。
