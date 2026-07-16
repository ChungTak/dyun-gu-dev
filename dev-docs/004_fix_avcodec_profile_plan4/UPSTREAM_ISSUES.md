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
- 状态：Open
- SDK tag/commit：期望 `0.2.0-rc.2`；实际 `refs/tags` 仅 `0.2.0-rc.0` / `0.2.0-rc.1`
- dyun commit：`137b7b80896395cf8164e8c2172a345d9bc857fd`
- Profile/role：全部（pin 迁移前置条件）
- 期望行为：远端存在不可变 `0.2.0-rc.2` tag 及解引用 SHA；dyun 可原子更新 manifest/lock/contract。
- 实际行为：`git ls-remote --tags https://github.com/TimothyWalker6922/avcodec-rs-develop.git` 无 `0.2.0-rc.2`；当前 HEAD 为 `b0f98dfafb95134a41307f3e5706e5d2518f0207`。
- 最小复现命令：
  ```bash
  git ls-remote --tags https://github.com/TimothyWalker6922/avcodec-rs-develop.git | grep '0.2.0-rc.2'
  ```
- structured error/report/diagnostics：Plan 4 RC2 admission 被阻塞；INT4-01/02/10 无法进入 Done。
- toolchain/environment/device：N/A
- 上游 fixture/test：N/A
- 修复 commit/新候选：发布 annotated `0.2.0-rc.2` 并提供 tag object + dereferenced commit。
- dyun 重验 artifact：更新 `crates/dg-media-avcodec/Cargo.toml` 与 `Cargo.lock` 中 avcodec `rev` 为新 SHA，执行 Phase 1～4 全矩阵。
- 临时处置：保持当前 post-RC1 main pin `7faba6f`；不改 pin 直至 RC2 tag 可用。

### UP4-002 — Software profile H.264 encoder `CreateEncoder` 在 FFmpeg 4.4.2 上报 `BackendHintCapabilityMismatch`
- 状态：Open
- SDK tag/commit：`7faba6fe264aa5ae5bd2f1666084f4bc52aa7d0f`（post-RC1 main，crate version `0.2.0-rc.1`）
- dyun commit：`137b7b80896395cf8164e8c2172a345d9bc857fd`
- Profile/role：`avcodec-profile-software` / encoder
- 期望行为：在同一 build 同时启用 `avcodec-profile-native-free` 与 `avcodec-profile-software` 时，Software profile 的 `CreateEncoder(H264, 32x32, Yuv420p, 1/30, 1_000_000)` 成功。
- 实际行为：encoder create 返回 `Classified { kind: SelectionFailed, detail: BackendHintCapabilityMismatch }`。
- 最小复现命令：
  ```bash
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
- toolchain/environment/device：
  - `rustc 1.94.1` / `x86_64-unknown-linux-gnu`
  - `ffmpeg 4.4.2` / `libavcodec 58.134.100`
  - `libavformat-dev`, `libavcodec-dev`, `libavutil-dev` 均来自 Ubuntu 22.04 4.4.2 包
  - 计划要求 canonical Software 验证 FFmpeg 6/7/8
- 上游 fixture/test：N/A
- 修复 commit/新候选：待确定；需要上游在 RC2 声明 FFmpeg 4.4.2 是否在支持矩阵内，或在 FFmpeg 6/7/8 上提供 clean evidence。
- dyun 重验 artifact：在 FFmpeg 6/7/8 clean runner 上重跑 `cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software`。
- 临时处置：Software profile 在当前 FFmpeg 4.4.2 环境下不能标记为 production；不修改 dyun backend 绕过。
