# 10. OCI、CI、硬件 Runner 与发布

## 1. OCI 合同

正式制品为 Ubuntu 24.04 x86_64 OCI。基础镜像按 digest固定，安装固定 OpenVINO 2026.2.1 用户态 runtime、
software codec运行库和产品二进制；运行用户非 root，写目录和模型/config mount明确。

Intel iGPU 由宿主提供内核驱动并映射 `/dev/dri`；文档记录所需 render group。镜像不得打包未知宿主 kernel module。

## 2. CI 分层

- PR：fmt/clippy/test/deny、SDK-free targets、software media、Cheetah loopback、OpenVINO CPU；
- iGPU required：自托管 runner使用候选 OCI执行真实 GPU regression和 E2E；
- nightly：协议故障注入、fuzz 15 min/target、2h soak；
- release：24h soak、性能比较、OCI vulnerability scan、SBOM、签名与回滚 smoke。

硬件 runner 无设备、权限或 plugin 时 job 失败/blocked，不允许 success skip。

## 3. 产品端到端

至少4路本地 RTSP H.264，经 software decode→resize→OpenVINO GPU→后处理/track→OSD→software encode→RTMP。
验证 PTS/DTS/timebase、关键帧、extradata、断流重连、背压、热更新和 SIGTERM。CPU运行同图作为功能基线。

## 4. 性能与稳定性门禁

固定 runner保存硬件、模型、流和配置 hash。候选相对已接纳基线：吞吐下降≤10%，p95端到端延迟上升≤15%。
24h soak无未恢复错误，RSS净增长≤128 MiB，线程/fd/request数量回到稳定区间。

## 5. 发布产物

发布 OCI digest、SHA-256、SPDX/CycloneDX SBOM、license notice、provenance和签名；GitHub Release保存验收报告、
配置/schema、C header/library辅助包。release workflow使用 tag对应 commit，不从未验证分支重建不同内容。

## 6. 完成条件

- [ ] CPU+iGPU required jobs消费同一 OCI digest。
- [ ] E2E、24h soak和性能阈值通过。
- [ ] SBOM、扫描、签名、provenance完整。
- [ ] 干净宿主按文档可启动、健康检查和回滚。

