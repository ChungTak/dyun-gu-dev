# dyun-gu Capability Support Matrix

> Evidence-driven support status for the CORE7 product-ready closure.
> `Blocked` or `CompileOnly` means the capability is not claimed as product-supported.
> A row can only be `SoftwareVerified` or `HardwareVerified` when the evidence comes
> from the same candidate artifact and the required CI/soak/fuzz jobs are green.

| Capability | Status | Evidence Source | Notes |
|---|---|---|---|
| SDK-free core (mock/CPU) | SoftwareVerified | `cargo test --workspace --locked`, `core7_*` tests | Full unit/integration coverage on every PR. |
| OpenVINO CPU | Blocked | No fixed runner with OpenVINO hardware evidence | Compile-only and Python-runtime tests exist; product support requires real device soak. |
| OpenVINO GPU | Blocked | No fixed runner with iGPU evidence | Blocked until dedicated Intel iGPU runner qualifies. |
| OpenVINO NPU | Blocked | No fixed runner with NPU evidence | Blocked until dedicated Intel NPU runner qualifies. |
| TensorRT CUDA | Blocked | No CUDA runner | Compile-only in `check` matrix; real GPU soak missing. |
| RKNN NPU | Blocked | No RKNN device runner | Compile-only in `check` matrix. |
| Sophon Host/SoC | Blocked | No Sophon device runner | Compile-only in `check` matrix. |
| Cheetah RTSP | Blocked | No real protocol runner | SDK-free engine-loopback tests only. |
| Cheetah HTTP-FLV | Blocked | No real protocol runner | SDK-free engine-loopback tests only. |
| Cheetah RTMP | Blocked | No real protocol runner | SDK-free engine-loopback tests only. |
| Cheetah WebRTC | Blocked | No real protocol runner | SDK-free engine-loopback tests only. |
| avcodec software | SoftwareVerified | `avcodec-profile-software` CI job with FFmpeg/libavcodec >= 58 | Runs decode/encode/pipeline tests on every PR. |
| avcodec nvcodec host | CompileOnly | `avcodec-nvcodec-compile` CI job | Build-only; NVIDIA GPU runner required for product support. |
| avcodec nvcodec device frame | CompileOnly | `avcodec-nvcodec-compile` CI job | Build-only; NVIDIA GPU runner required for product support. |
| avcodec rkmpp host/zero-copy | CompileOnly | `cargo check` with profile feature | No Rockchip device runner. |
| avcodec onevpl host/fallback | CompileOnly | `cargo check` with profile feature | No Intel VPL device runner. |
| avcodec amf host/fallback | CompileOnly | `cargo check` with profile feature | No AMD device runner. |

## Status Definitions

- `Disabled`: not compiled into this artifact.
- `CompileOnly`: compiles, but no runtime evidence on this artifact.
- `MockVerified`: passes mock/unit tests only.
- `SoftwareVerified`: passes CPU/software runtime tests in CI.
- `HardwareVerified`: passes real device/soak tests on target hardware.
- `Blocked`: requires upstream runner, SDK, or hardware qualification before product support.
