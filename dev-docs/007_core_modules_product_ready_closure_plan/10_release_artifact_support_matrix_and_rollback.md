# 10. 发布制品、支持矩阵与回滚

> 需求 ID：CORE7-10

## 1. 候选身份

一个候选由以下不可分割字段标识：

- dyun commit/Cargo.lock；
- Graph schema 和默认 process policy hash；
- C header、symbol snapshot、library hash/SONAME；
- CLI/C API package 与 OCI digest；
- backend/codec/connector revision；
- risk register、acceptance 和 evidence manifest revision。

任一字段变化都产生新候选并重跑受影响门禁。

## 2. Package 布局

Linux C ABI package 至少包含：

```text
bin/dg-cli
lib/libdg_capi.so.2
lib/libdg_capi.so -> libdg_capi.so.2
lib/libdg_capi.a
lib/pkgconfig/dg-capi.pc
include/dg_capi.h
examples/c/*.c
docs/user-guide.md
manifest.json
```

`manifest.json` 保存 hash、SONAME、ABI version、Graph schema、features 和 source SHA。打包 job 对归档解压后
执行 C/C++ smoke，不能只检查文件存在。

## 3. Support Matrix

每行记录 `Disabled / CompileOnly / MockVerified / SoftwareVerified / HardwareVerified / Blocked`：

- SDK-free core；
- OpenVINO CPU/GPU/NPU；
- TensorRT CUDA；
- RKNN；
- Sophon Host/SoC；
- Cheetah RTSP/HTTP-FLV/RTMP/WebRTC；
- avcodec software/各硬件 profile。

只有 SoftwareVerified/HardwareVerified 且证据属于候选 artifact 时可显示 product-supported。feature 默认值、
CLI capabilities、C JSON 和用户文档使用同一矩阵源。

## 4. Release Gate

release workflow 在推送 tag/OCI 前验证：

- CORE7 acceptance 为 Accepted；
- 无 Open P0/P1、无过期 P2 exception；
- required workflow run 的 head SHA 等于 tag commit；
- 24h/performance/rollback artifact digest 匹配；
- SBOM/provenance/signature 已生成；
- capability 声明没有超出 evidence。

手工 workflow_dispatch 可构建候选，但未 Accepted 时不得推 production tag。

## 5. 回滚

以完整 artifact digest 回滚，不能只替换 `.so`、header、policy 或 GraphSpec。演练：

1. 启动前一 Accepted core artifact；
2. 运行 config/C ABI/stream/mock backend smoke；
3. 切到候选，运行 reload/reconnect/external callback/shutdown；
4. 切回前一 digest；
5. 验证 policy、GraphSpec、C host、模型和 readiness 恢复；
6. 保存切换时长、错误、丢帧、资源曲线和结论。

若没有前一 v2 artifact，首次发布执行“候选卸载后重新安装同一 digest”的安装/清理演练，并记录首次发布无
历史回滚目标。

## 6. 完成条件

- [ ] package 文件名、SONAME、symlink、header、symbols 和 examples 一致。
- [ ] support matrix 由 evidence 驱动且在所有输出面一致。
- [ ] 未 Accepted 的 workflow 不发布 production tag/OCI。
- [ ] rollback 使用完整 digest 并有运行证据。
- [ ] manifest/SBOM/provenance/signature 绑定同一候选。

