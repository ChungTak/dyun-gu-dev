# Media graph examples (avcodec Profile)

These GraphSpec samples exercise `media_decode` / `media_encode` / `media_resize`
after the avcodec Profile integration. They do **not** include demux/mux or
protocol I/O; pair with `dg-stream` connectors when needed.

## Feature matrix

| Example | Cargo features | Runtime `profile` | Memory path | Notes |
| --- | --- | --- | --- | --- |
| [`raw-adapter.yaml`](raw-adapter.yaml) | `media` (default) | n/a | Host | No avcodec SDK; raw payload relabel only |
| [`native-free-jpeg.yaml`](native-free-jpeg.yaml) | `media,avcodec-profile-native-free` | `native-free` | Host | Pure Rust JPEG encode/decode |
| [`software-host.yaml`](software-host.yaml) | `media,avcodec-profile-software` | `software` | Host | FFmpeg/openh264 when linked |
| [`rkmpp-host-fallback.yaml`](rkmpp-host-fallback.yaml) | `media,avcodec-profile-rkmpp-host-fallback` | `rkmpp-host-fallback` | Host + staging | May fall back to software |
| [`rkmpp-zero-copy.yaml`](rkmpp-zero-copy.yaml) | `media,avcodec-profile-rkmpp-zero-copy` | `rkmpp-zero-copy` | DrmPrime→DmaBuf | **Gated** on UP-03; session create fails until Profile V2 |
| [`nvcodec-device-frame.yaml`](nvcodec-device-frame.yaml) | `media,avcodec-profile-nvcodec-device-frame` | `nvcodec-device-frame` | CudaDevice NV12 | **Gated** on UP-06; no resize; not full CUDA zero-copy |

Legacy `avcodec` feature maps to the native-free compatibility profile. Prefer
an explicit `avcodec-profile-*` feature and matching `profile` field. Do not set
both `profile` and legacy `hw`.

## Commands

```bash
# Raw adapter (no codec SDK)
cargo run -p dg-cli --no-default-features --features media -- \
  validate --config examples/media/raw-adapter.yaml

# Native-free Host JPEG (Ubuntu 26.04 may need LIBYUV_TARGET)
LIBYUV_TARGET=ubuntu-24.04_x86_64 \
cargo run -p dg-cli --no-default-features \
  --features media,avcodec-profile-native-free -- \
  validate --config examples/media/native-free-jpeg.yaml

# C API
LIBYUV_TARGET=ubuntu-24.04_x86_64 \
cargo check -p dg-capi --no-default-features \
  --features media,avcodec-profile-native-free
```

## Zero-copy claims

Only paths that report `TransferReport.copy_count == 0` with matching
MemoryDomain, external handle, plane layout, and ownership guard may be called
zero-copy. Host compressed Packet ingress/egress is **not** part of the image
chain copy budget. NV device-frame is **not** full CUDA Packet/Image zero-copy.
