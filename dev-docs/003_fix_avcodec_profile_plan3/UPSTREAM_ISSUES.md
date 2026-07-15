# avcodec-rs 上游问题记录

> 初始无条目。只有无法在 dyun 正确解决、且已有最小复现的问题写入本文件。不得在 dyun 复制 backend、
> policy、domain 或 staging 作为临时修复。

## 条目模板

### UP3-XXX — 标题

- 状态：Open / Fixed in candidate / Verified / Closed
- avcodec commit：
- dyun commit：
- 影响 Profile/role：
- 期望行为：
- 实际行为：
- 最小复现命令：
- 结构化 error/report：
- 环境与设备：
- 上游 fixture/test 位置：
- 修复 commit：
- dyun 重验命令/artifact：
- 临时处置：禁用受影响 Profile；不得添加低层绕过

关闭要求：上游 commit 可定位，最小测试通过，dyun 固定包含修复的不可变候选并完成受影响 Profile 重验。

---

### UP3-01 — `shiguredo_nvcodec 2026.2.0` 破坏 `avcodec-backend-nvcodec` 编译

- 状态：Open / Worked-around in dyun
- avcodec commit：`84a2832796717f46a1009ee064c914b0ad66ac19`
- dyun commit：当前 PR（`Cargo.lock` 锁定 `shiguredo_nvcodec = 2026.1.0`）
- 影响 Profile/role：`avcodec-profile-nvcodec-host`、`avcodec-profile-nvcodec-host-fallback`、`avcodec-profile-nvcodec-device-frame`
- 期望行为：`cargo check -p dg-media --features avcodec-profile-nvcodec-host` 应通过
- 实际行为：
  - `shiguredo_nvcodec::Encoder`/`Decoder` 在 `2026.2.0` 变为泛型（`Encoder<H>` / `Decoder<H>`），上游 `avcodec-backend-nvcodec` 仍按非泛型使用。
  - `Encoder::query_caps` / `Decoder::query_caps` 在 `2026.2.0` 已不存在，上游仍在调用。
  - 构造 `Encoder::new` / `Decoder::new` 需要额外 handler 参数。
- 最小复现命令：
  ```bash
  cargo update -p shiguredo_nvcodec --precise 2026.2.0
  cargo check -p dg-media --features avcodec-profile-nvcodec-host
  ```
- 结构化 error/report：
  ```
  error[E0107]: missing generics for struct `shiguredo_nvcodec::Encoder`
  error[E0599]: no function or associated item named `query_caps` found
  error[E0061]: this function takes 2 arguments but 1 argument was supplied
  ```
- 环境与设备：Ubuntu 22.04 / x86_64，无 NVIDIA GPU（compile-only）
- 上游 fixture/test 位置：`avcodec-rs-develop/crates/backend/avcodec-backend-nvcodec/src/lib.rs`
- 修复 commit：待上游更新 `avcodec-backend-nvcodec` 以适配 `shiguredo_nvcodec 2026.2.0`
- dyun 重验命令/artifact：
  ```bash
  cargo update -p shiguredo_nvcodec --precise 2026.2.0
  cargo check -p dg-media --features avcodec-profile-nvcodec-host
  ```
- 临时处置：将 `Cargo.lock` 中 `shiguredo_nvcodec` 固定到 `2026.1.0`，使 NV 相关 profile 通过 compile-only 验证。未在 dyun 添加任何 backend 绕过。

### UP3-02 — `shiguredo_amf 2026.3.0` 破坏 `avcodec-codec-amf` 在 Linux 编译

- 状态：Open / Worked-around in dyun
- avcodec commit：`84a2832796717f46a1009ee064c914b0ad66ac19`
- dyun commit：当前 PR（`Cargo.lock` 锁定 `shiguredo_amf = 2026.1.0`）
- 影响 Profile/role：`avcodec-profile-amf-host`、`avcodec-profile-amf-host-fallback`
- 期望行为：`cargo check -p dg-media --features avcodec-profile-amf-host` 应通过
- 实际行为：
  - `avcodec-codec-amf/src/linux.rs:126` 将 `sys::AMFVideoDecoderUVD_H264_AVC` 作为 `&'static str` 返回，但 `shiguredo_amf 2026.3.0` 中该常量类型为 `&[u8; 28]`。
- 最小复现命令：
  ```bash
  cargo update -p shiguredo_amf --precise 2026.3.0
  cargo check -p dg-media --features avcodec-profile-amf-host
  ```
- 结构化 error/report：
  ```
  error[E0308]: mismatched types
    expected `&'static str`
       found `&'static [u8; 28]`
  ```
- 环境与设备：Ubuntu 22.04 / x86_64，无 AMD GPU（compile-only）
- 上游 fixture/test 位置：`avcodec-rs-develop/crates/codec/avcodec-codec-amf/src/linux.rs`
- 修复 commit：待上游 `avcodec-codec-amf` 适配 `shiguredo_amf 2026.3.0` 的常量类型变更
- dyun 重验命令/artifact：
  ```bash
  cargo update -p shiguredo_amf --precise 2026.3.0
  cargo check -p dg-media --features avcodec-profile-amf-host
  ```
- 临时处置：将 `Cargo.lock` 中 `shiguredo_amf` 固定到 `2026.1.0`，使 AMF profile 通过 compile-only 验证。未在 dyun 修改上游源码。
