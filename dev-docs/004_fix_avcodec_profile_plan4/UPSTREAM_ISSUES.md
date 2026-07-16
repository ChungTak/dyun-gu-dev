# avcodec-rs RC2 上游问题记录

> 只记录无法在 dyun 正确修复且有最小复现的问题。禁止复制 backend、policy、domain或 staging 作为绕过。

---

### UP4-001 — `0.2.0-rc.2` annotated tag 未发布
- 状态：Closed
- SDK tag/commit：`0.2.0-rc.2` / `2068432426793c94cd5d415b56a4b2e9a3c1ee73`
- 关闭：tag 已发布；dyun 曾 pin 并验证。

### UP4-002 — Software profile H.264 encoder `CreateEncoder` 返回 `BackendHintCapabilityMismatch`
- 状态：**Verified**
- SDK 修复 commit：`f3c1c04b87edd7b61e45feaf5adb3797bfa9ea5f`（pushed to `avcodec-rs-develop` main）
- 基线 RC2：`2068432426793c94cd5d415b56a4b2e9a3c1ee73`
- dyun pin：`f3c1c04b87edd7b61e45feaf5adb3797bfa9ea5f`
- Profile/role：`avcodec-profile-software` / encoder + JPEG
- 根因与修复：
  1. libavcodec major 仅 60–62 为 supported → 扩展至 58/59（及 ≥63 best-effort）
  2. Software allow-list 仅 `ffmpeg` → `["ffmpeg","jpeg"]` Ordered + feature `jpeg`
- 重验（`--locked`，pin `f3c1c04`）：
  ```bash
  source scripts/env-software-avcodec.sh
  cargo test -p dg-media --locked --features avcodec-profile-native-free
  cargo test -p dg-media --locked --features avcodec-profile-software
  cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software
  DYUN_NV_HW=1 cargo test -p dg-media --locked --features avcodec-profile-nvcodec-host --lib nvcodec_host -- --test-threads=1
  ```
- 结果：全部通过（含 Software JPEG roundtrip、H.264、multi-profile、NV Host）
- 仍需：H.264 encode 运行时存在 `libx264`
