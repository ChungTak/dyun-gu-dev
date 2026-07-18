# avcodec-rs：MEDIA-01 与上游能力现状

> **当前 pin（以本仓库 `dg-media-avcodec` 与 `Cargo.lock` 为准）：**
> `cff861a8893c3391fafce7815f24be42cc9554d2`
> （上游 `TimothyWalker6922/avcodec-rs-develop` main；稳定 tag `0.2.0` 对应
> `dd3190008f2b544b51a74a9f4a225d52befc120a`，当前 pin **新于** 0.2.0，包含 Plan 8
> `ImageSdk` / `AudioSdk` / MP3 等增量。）
>
> 本文取代历史上基于 `621a708` / `3e61b5b` / `8ef5a72` 的 gap 叙述。历史 gap 文档
> [`avcodec-rs-gaps.md`](avcodec-rs-gaps.md) 仅作考古，不再作为集成门禁。

---

## 0. 集成方向（当前正确前提）

1. **生产视频路径** = 跟随推理硬件的硬件后端 + FFmpeg/x264 软件回退：
   - 软件：`VideoProfile::Software`（`profile-software` → ffmpeg + jpeg + libyuv）
   - Intel：`OnevplHost` / `OnevplHostFallback`
   - NVIDIA：`NvcodecHost` / `NvcodecHostFallback` / `NvcodecDeviceFrame`
   - Rockchip：`RkmppHost` / `RkmppHostFallback` / `RkmppZeroCopy`
   - AMD：`AmfHost`（encode 为主）/ `AmfHostFallback`
2. **NativeFree** 仅用于无系统 SDK 的 JPEG/MJPEG 与本地验证；**不是**产品视频默认路径。
   dyun 将其标为 `Unverified`，不在 CI 中以 pure-Rust H.264 作为 MEDIA-01 闭环验收。
3. 高层 API：`VideoSdk` + `VideoProfile` + role request；禁止在 dyun 内手写
   Registry/policy/I/O plan 选择器。
4. 默认 workspace build **不**启用任何 `avcodec-profile-*`；真实 codec 由 feature gate。

---

## 1. 上游能力对照（`cff861a`）

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| `VideoSdk` / owned sessions / transcoder | 已满足 | README / `docs/sdk-integration-guide.md` |
| Profile feature 矩阵 | 已满足 | 见上游 README Feature profile presets |
| Software H.264（libavcodec ≥ 58 + x264） | 已满足 | FFmpeg 4.x/5.x major 58/59 已接纳 |
| NativeFree JPEG | 已满足 | zune/jpeg + libyuv |
| 10-bit / HDR metadata | 已满足（上游） | dyun 映射 10-bit 像素为 `CorePixel::Unknown` 直至业务需要 |
| `ImageSdk` / `AudioSdk` / MP3 | 上游已有 | **本仓库暂不消费**；媒体图仍走 Video 路径 |
| RKMPP / OneVPL / AMF 生产签收 | 上游 experimental | dyun 标 `Unverified` |

---

## 2. 对本仓库的约束

- `crates/dg-media-avcodec/Cargo.toml` 是唯一直接依赖 avcodec 的 crate；`rev` 必须与
  `Cargo.lock` 及 `tests/dependency_contract.rs` 一致。
- Profile 一一映射到上游 `VideoProfile`，不在 dyun 侧重写 backend allow-list。
- 设备感知选择：`resolve_profile_from_device`（CPU→software，intel_gpu→onevpl-host-fallback，…）。
- 不得重新引入「默认 pure-Rust H.264 视频路径」或把 `native-free` 标为 production。

---

## 3. 已关闭的历史需求

| 历史项 | 结论 |
| --- | --- |
| Req A：新增 native-free 纯 Rust 视频 encoder | **撤回**；生产不需要该路径 |
| 删除 rust-h264 的上游请求 | 上游仍可提供该 backend；**dyun 不启用、不验收** |
| Gap 2/3/5（registry / error / host convert） | 在 0.2 线已关闭 |
| MEDIA-01 真实软件视频闭环 | 由 `avcodec-profile-software` + 真实码流测试覆盖 |

---

## 4. 升级检查清单

升级 avcodec-rs pin 时：

1. 更新 `dg-media-avcodec` 的 `rev` 与 `Cargo.lock`。
2. 跑 `cargo test -p dg-media --features avcodec-profile-native-free` 与
   `avcodec-profile-software`（环境允许时）。
3. 核对 `ExternalImageDescriptor` / `AvError` / `ImageInfo` 字段是否破坏 bridge。
4. 同步本文件顶部 pin 与 `docs/user-guide.md` §5.1。
