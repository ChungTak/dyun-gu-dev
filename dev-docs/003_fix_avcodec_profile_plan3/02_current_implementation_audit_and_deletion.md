# 02. 当前实现审计与删除清单

## 1. 必须删除

- `profile.rs` 的 `backend_policy`、`io_memory_plan`、`profile_to_sdk_descriptor*`；
- `session.rs` 的 `VideoSessionFactoryV2` service、低层 `VideoSessionRequest` 和 config align/stamp；
- `transcoder.rs` 的低层 `VideoTranscoderRequest`、`default_registry_builder`、`leak_registry`；
- `dg-media-avcodec` 对 Factory V2、BackendPolicy、descriptor/I/O plan 的重导出；
- 生产 feature 中 `codec-ffmpeg/x264/x265/openh264/rkmpp/librga/nvcodec/onevpl/amf` alias；
- 根据 backend id 猜测 fallback/copy 的本地逻辑。

## 2. 必须保留

- `AvcodecProfile` 名称解析和业务配置错误；
- `MediaFrame` 与 SDK Packet/Image 的 bridge；
- `AsyncPump` 与 Element/Graph 调度；
- stream index、PTS/DTS、time base 和业务 metadata；
- dg 错误/日志/指标入口，但数据来自上游结构化 error/report/diagnostics；
- legacy `hw` 的限期兼容映射。

## 3. Source guard

为生产目录添加自动扫描，禁止：`default_registry_builder`、`RegistryBuilder`、
`VideoSessionFactoryV2`、`VideoBackendPolicy`、`VideoProfileDescriptor`、`VideoIoMemoryPlan`、
`VideoTranscoderRequest`、`leak_registry`。测试 fixture 若必须验证禁止项，应放在明确 allowlist。

## 4. 执行顺序

先让 guard 对当前代码失败；建立新的 VideoSdk service 和一个 Element 迁移样板；再按 decode、encode、
processor、transcoder 删除旧路径。每步保持 workspace 编译，禁止一次性删除后长期红线。

## 5. 完成条件

- [ ] 删除/保留清单逐符号核对。
- [ ] guard 在旧基线失败、迁移后通过。
- [ ] 不存在行为相同的旧新双路径。
- [ ] bridge/Graph 的业务职责未误删。

