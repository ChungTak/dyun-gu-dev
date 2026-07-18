# Plan 5 上游与外部问题

## 记录格式

每项包含：ID、状态、影响的 INT5、上游 revision、最小复现、环境、期望/实际、typed error、临时策略和关闭证据。
不得在 dyun 复制上游 backend/protocol 实现作为永久 workaround。

### UP5-001 — OpenVINO live device/plugin API

- 状态：`Closed (Audited)`
- 影响：INT5-06
- 问题：确认当前 `openvino` crate 是否安全暴露 runtime version、available devices 和 plugin properties。
- 证据：`dg-openvino/src/backend.rs` `probe_live_capabilities` 使用
  `dg_openvino_sys::version()`、`Core::available_devices()`、
  `PropertyKey::{DeviceFullName, DeviceCapabilities, SupportedProperties, RangeForAsyncInferRequests}`。
- 策略：live property 为空时 **不** 把静态精度表写入 `verified_precisions`；请求精度在 empty 时走
  静态 backend matrix 并 `warn!`，有 live 列表时严格匹配。
- 残余：iGPU 实机证据仍属 EXT5-001。

### UP5-002 — OpenVINO async infer request

- 状态：`Closed`
- 影响：INT5-07
- 问题：确认异步 start/wait/poll API 及 tensor/request 生命周期。
- 证据：`OpenVinoBackend::submit` 调用 `infer_async()`；`poll` 使用 `request.wait(0)` 与
  `ResultNotReady`/`RequestBusy` 映射；`async_capable` 来自 live
  `RANGE_FOR_ASYNC_INFER_REQUESTS`；request pool + `max_in_flight` 背压；
  CPU 测试 `openvino_cpu_async_inflight_*`。
- 策略：不以线程包装同步 `infer()` 冒充原生 async。
- 残余：iGPU async 矩阵仍需硬件 runner。

### UP5-003 — Cheetah reconnect/error passthrough

- 状态：`Audited / software-done`（真协议 E2E 残余）
- 影响：INT5-05
- 问题：核对四协议 connector 的 retryable、timeout、cancel、readiness 和 source chain。
- 证据：`dg-stream` typed `Error` + `redact_url`；`map_connector_error` 保留 `retryable`；
  pull/push `open_with_retry` + 指数退避；重连后 keyframe/discontinuity；
  `reconnecting` 指标驱动 `/readyz`；CLI/`dg_runtime_init` 安装 embedded connector。
- 策略：dyun 只实现产品策略与脱敏；协议状态机缺陷回传 Cheetah。
- 残余：真实 RTSP/RTMP/WebRTC E2E 与 4 路 soak 需 runner。

### EXT5-001 — Intel iGPU self-hosted runner

- 状态：`External Required`
- 影响：INT5-06、INT5-10
- 要求：稳定 `/dev/dri`、固定 PCI ID/driver、容器权限和 artifact retention。
- 关闭：required job 连续通过并由 release acceptance 引用；无设备 skip 不能关闭。
