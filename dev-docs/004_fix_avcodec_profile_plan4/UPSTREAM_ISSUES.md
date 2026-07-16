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
- 状态：Open
- SDK tag/commit：`0.2.0-rc.2` / `2068432426793c94cd5d415b56a4b2e9a3c1ee73`
- dyun commit：`137b7b80896395cf8164e8c2172a345d9bc857fd` + pin update
- Profile/role：`avcodec-profile-software` / encoder
- 期望行为：在同一 build 同时启用 `avcodec-profile-native-free` 与 `avcodec-profile-software` 时，Software profile 的 `CreateEncoder(H264, 32x32, Yuv420p, 1/30, 1_000_000)` 成功。
- 实际行为：encoder create 返回 `Classified { kind: SelectionFailed, detail: BackendHintCapabilityMismatch }`；将 FFmpeg 从 4.4.2 升级至 6.1 后仍然复现，排除 FFmpeg 版本因素。
- 最小复现命令：
  ```bash
  # 1.94.1 toolchain, FFmpeg 6.1 (libavcodec 60.31.102)
  source scripts/env-software-avcodec.sh
  cargo test -p dg-media --locked \
    --features avcodec-profile-native-free,avcodec-profile-software \
    software_h264_encode_decode_preserves_timing_and_stream_index \
    multi_profile_encoder_backends_do_not_cross_stack \
    software_h264_transcode_stays_on_ffmpeg_stack
  ```
- structured error/report/diagnostics：
  ```
  kind=video build failed profile=software role=encoder operation=CreateEncoder backend=ffmpeg
  detail=video session build failed for role Encoder: WithContext {
    error: Classified { kind: SelectionFailed, detail: BackendHintCapabilityMismatch },
    context: AvErrorContext {
      codec: Some(H264), source_format: Some(Yuv420p),
      memory_domain: Some(Host), allow_staging: Some(true), profile_name: Some("software")
    }
  }
  ```
  上游 `VideoProfile::Software` 的 `ProfileMeta` 中 `encoder: &["ffmpeg"]`、`allow_staging: true`；`VideoProfileDescriptor::to_encoder_config` 按 `self.io.allow_staging` 注入 `EncoderConfig`，因此请求携带 `allow_staging=true` + `memory_domain=Host`。`avcodec-codec-ffmpeg` encoder 在后端选择阶段被 `BackendSelectionPolicy::Required("ffmpeg")` 命中，但 `supports(backend)` 能力模型拒绝，产生 `BackendHintCapabilityMismatch`。
- toolchain/environment/device：
  - `rustc 1.94.1` / `x86_64-unknown-linux-gnu`
  - 已验证 FFmpeg 4.4.2 (`libavcodec 58.134.100`) 与 FFmpeg 6.1 (`libavcodec 60.31.102`) 行为一致
  - `ffmpeg` / `libavcodec-dev` / `libavformat-dev` / `libavutil-dev` / `libswscale-dev` 6.1
- 上游 fixture/test：上游 `crates/sdk/avcodec/src/video_sdk.rs` 等功能测试使用 `VideoProfile::NativeFree` 测试真实 encoder；未发现对 `VideoProfile::Software` encoder create 的真实媒体测试。
- 修复 commit/新候选：待上游确认是 `VideoProfile::Software` 描述符问题（`allow_staging` / memory domain 与 `avcodec-codec-ffmpeg` encoder 能力不匹配）还是 `avcodec-codec-ffmpeg` encoder capability 声明过窄。
- dyun 重验 artifact：
  ```bash
  cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software
  cargo test -p dg-media --locked --features avcodec-profile-software
  ```
- 临时处置：Software profile 不能标记为 production；不修改 dyun backend 绕过。NativeFree 路径保持生产可用。
