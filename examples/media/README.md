# Media graph examples (avcodec Profile)

These GraphSpec samples exercise `media_decode` / `media_encode` / `media_resize` /
`media_transcode` after the avcodec V3 Profile integration. They do **not** include
demux/mux or protocol I/O; pair with `dg-stream` connectors when needed.

Examples only use business fields (`profile`, codec, size, bitrate). I/O domains are
owned by the selected Profile—do not set low-level backend ids or rely on `memory_domain`.

## Feature matrix

| Example | Cargo features | Runtime `profile` | Support | Notes |
| --- | --- | --- | --- | --- |
| [`raw-adapter.yaml`](raw-adapter.yaml) | `media` (default) | n/a | n/a | No avcodec SDK; raw payload relabel only |
| [`native-free-jpeg.yaml`](native-free-jpeg.yaml) | `media,avcodec-profile-native-free` | `native-free` | production | Pure Rust JPEG encode/decode |
| [`software-host.yaml`](software-host.yaml) | `media,avcodec-profile-software` | `software` | production | libavcodec ≥ 58 + libx264 for H.264 |
| [`nvcodec-device-frame.yaml`](nvcodec-device-frame.yaml) | `media,avcodec-profile-nvcodec-device-frame` | `nvcodec-device-frame` | production* | CUDA runtime; no resize; `allow_staging=false` |
| [`rkmpp-host-fallback.yaml`](rkmpp-host-fallback.yaml) | `media,avcodec-profile-rkmpp-host-fallback` | `rkmpp-host-fallback` | **unverified** | Compile/config only until signed |
| [`rkmpp-zero-copy.yaml`](rkmpp-zero-copy.yaml) | `media,avcodec-profile-rkmpp-zero-copy` | `rkmpp-zero-copy` | **unverified** | Compile/config only until signed |

\* NV Host/device-frame are production after RC2 hardware sign-off (`DYUN_NV_HW=1`). CPU CI only
enforces compile-only; do not treat `avcodec-nvcodec-compile` as media passed.

Legacy `avcodec` feature maps to the native-free compatibility profile. Prefer an
explicit `avcodec-profile-*` feature and matching `profile` field. Do not set both
`profile` and legacy `hw` (`hw` is removed in **0.2.0**).

## Commands

```bash
# Raw adapter (no codec SDK)
cargo run -p dg-cli --no-default-features --features media -- \
  validate --config examples/media/raw-adapter.yaml

# Native-free Host JPEG
LIBYUV_TARGET=ubuntu-24.04_x86_64 \
cargo run -p dg-cli --no-default-features \
  --features media,avcodec-profile-native-free -- \
  validate --config examples/media/native-free-jpeg.yaml

# Software Host (needs FFmpeg + scripts/env-software-avcodec.sh)
source scripts/env-software-avcodec.sh
cargo run -p dg-cli --no-default-features \
  --features media,avcodec-profile-software -- \
  validate --config examples/media/software-host.yaml
```

## Zero-copy claims

Only paths that report `TransferReport.copy_count == 0` with matching MemoryDomain,
external handle, plane layout, and ownership guard may be called zero-copy. Host
compressed Packet ingress/egress is **not** part of the image chain copy budget.
