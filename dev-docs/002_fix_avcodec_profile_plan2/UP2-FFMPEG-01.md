# UP2-FFMPEG-01 — avcodec-codec-ffmpeg 无法在 FFmpeg 8.x 上编译

## 状态

**阻塞** `avcodec-profile-software` 在本机（FFmpeg 8 / libavcodec 62.x）上的 `cargo check/test`。

计划规则：不修改 avcodec-rs；上游缺陷登记为 `UP2-*`。

## 环境

| 项 | 值 |
|---|---|
| dyun avcodec-rs rev | `fc728aa9ea3e0a85401d2cd4de1b762ffcf92a51` |
| libavcodec | 62.11.100（FFmpeg 8 系列） |
| libavutil | 60.8.100 |
| rustc | 1.97.0 |

## 复现

```bash
# 1) bindgen 需要 libclang + GCC 内建头（本机无完整 clang 包时）
export LIBYUV_TARGET=ubuntu-24.04_x86_64
export RUSTUP_TOOLCHAIN=stable
source scripts/env-software-avcodec.sh   # 见仓库 scripts/

# 2) 启用 software Profile
cargo check -p dg-media --features avcodec-profile-software
```

### 阶段 A：bindgen（可被 env 解决）

未设置 `LIBCLANG_PATH` / `BINDGEN_EXTRA_CLANG_ARGS` 时：

```text
/usr/include/limits.h: fatal error: 'limits.h' file not found
ffmpeg-sys-next build.rs Unable to generate bindings
```

本机 `libclang1-21` 有库无完整 resource headers；用 GCC 15 的 `-isystem` 可过 bindgen。

### 阶段 B：上游类型错误（阻塞）

bindgen 成功后，`avcodec-codec-ffmpeg` 编译失败：

```text
error[E0308]: mismatched types
  --> avcodec-codec-ffmpeg/src/audio.rs:308:16
  expected raw pointer `*mut AVCodec`
     found raw pointer `*const AVCodec`
```

`ffmpeg-sys-next` 生成的绑定（FFmpeg 8）：

```rust
pub fn avcodec_find_encoder(id: AVCodecID) -> *const AVCodec;
pub fn avcodec_find_encoder_by_name(name: *const c_char) -> *const AVCodec;
```

上游代码仍假定 `*mut AVCodec`（见 `audio.rs` 中 `audio_encoder_for` / `probe_codec` 等）。

## 建议上游修复（不在本仓实现）

在 `avcodec-codec-ffmpeg` 将 find 结果统一为：

```rust
let encoder = unsafe { ffi::avcodec_find_encoder(id) as *mut ffi::AVCodec };
```

或把内部存储类型改为 `*const AVCodec`（若后续 API 只读）。

修复后把 dyun 的 `rev` 前移到不可变 hash 并重跑 software 矩阵。

## 对 dyun 的影响

| 项 | 影响 |
|---|---|
| `avcodec-profile-native-free` | 不受影响（不链 FFmpeg） |
| `avcodec-profile-software` | **本机 FFmpeg 8 上无法编译** |
| CI `ubuntu-latest` | 取决于 runner 的 FFmpeg 主版本；若仍为 6/7 可能通过，若升到 8 会同样失败 |
| 硬件 fallback Profile（`*-host-fallback`） | 同样启用 `ffmpeg` feature，共享此阻塞 |

## 本地临时绕过（非计划允许项）

- 降级系统 FFmpeg 到 6.x/7.x（`avcodec_find_encoder` 仍为 `*mut` 的时代），或  
- 等待上游合入 const 指针修复后再升 `rev`。

**禁止**在 dyun 中 `patch` / vendor 修改 avcodec-rs 源码以绕过（计划 §2 规则 7）。
