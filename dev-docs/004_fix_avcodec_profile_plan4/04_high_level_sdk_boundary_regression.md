# 04. 高层 SDK 边界回归

## 1. 必须保持

`AvcodecSdkService` 只持有 `VideoSdk`；Profile 只一对一映射 `VideoProfile`；Decode/Encode/Resize/Transcode
持有对应高层 Session。bridge/pump/Graph 是 dyun 业务职责。

## 2. 禁止符号

生产源码继续禁止 `default_registry_builder`、`RegistryBuilder`、`VideoSessionFactoryV2`、
`VideoBackendPolicy`、`VideoProfileDescriptor`、`VideoIoMemoryPlan`、低层 `VideoTranscoderRequest`、
`leak_registry` 和 backend candidate array。

## 3. Feature/依赖

只转发 `avcodec-profile-*`；不恢复 `codec-*` alias；不直接依赖 backend/codec/sys crate。多 Profile 缺省仍
要求显式选择，fallback 只由显式 fallback Profile启用。

## 4. Public bridge

允许重导出高层 Request/Session/report/error 和 core Packet/Image/external descriptor；unsafe 只在
`dg-media-avcodec::external` 薄 facade，`dg-media` 继续 forbid unsafe。

## 5. 完成条件

- [ ] source/dependency guard 在 RC2 上通过。
- [ ] 没有旧新双路径。
- [ ] API 变化仅来自上游 RC2 必要变更。
- [ ] 普通业务代码不接触底层库。

