# dyun-gu-dev Plan 5：Intel OpenVINO 边缘运行时产品化

## 1. 定位

本计划把当前“默认/mock 路径阶段完成”的仓库推进为可发布、可长期运行的 Intel 边缘推理产品。
首发生产范围固定为：Ubuntu 24.04 x86_64 OCI、OpenVINO CPU + Intel iGPU、软件 H.264
编解码、Cheetah RTSP/HTTP-FLV 拉流与 RTMP/WebRTC 推流、本地健康/指标端点。

本计划不新增模型 serving、远程配置控制面或 Web 管理台；Intel NPU、OpenVINO remote tensor、
OneVPL zero-copy、NVIDIA、RKNN 和 Sophon 不阻塞本轮首发。

## 2. 审计基线

| 字段 | 当前事实 |
|---|---|
| dyun 基线 | `main@cca9639`（执行时必须重新记录实际 HEAD） |
| 默认验证 | workspace tests 与 clippy 通过；fmt 当前存在失败 |
| OpenVINO | CPU identity/dynamic-shape/regression CI；iGPU 未验收 |
| 设备探测 | 当前为静态 capability，不是 OpenVINO live device probe |
| 长运行 | `RunningGraph` 无公开 stop/shutdown/live metrics |
| watch | CLI 只在图结束后打印 diff，不更新 live graph |
| 真流入口 | CLI/C API 未转发并安装 Cheetah connector |
| 发布 | tar artifact；无 production OCI、SBOM、签名或 iGPU gate |

## 3. 需求矩阵

| ID | 主题 | 首发阻塞 |
|---|---|---|
| INT5-01 | 基线、范围与接纳门禁 | 是 |
| INT5-02 | 工具链、质量门禁与可复现构建 | 是 |
| INT5-03 | RunningGraph 生命周期与 CLI supervisor | 是 |
| INT5-04 | 事务式热更新与配置依赖监控 | 是 |
| INT5-05 | Cheetah 真流入口、重连与错误分类 | 是 |
| INT5-06 | OpenVINO CPU+iGPU live capability 与配置收敛 | 是 |
| INT5-07 | OpenVINO 异步推理、背压与 copy 诊断 | 是 |
| INT5-08 | 健康、指标、日志与输入安全 | 是 |
| INT5-09 | C ABI v1 与资源上限 | 是 |
| INT5-10 | OCI、CI、iGPU runner 与发布证据 | 是 |
| INT5-11 | 文档收敛、最终验收与交接 | 是 |

## 4. 执行规则

- 每个 INT5 项独立 PR；PR 同步更新 `EXECUTION_STATUS.md`。
- 不以 mock、静态 capability、compile-only 或人工日志替代 CPU/iGPU 实机证据。
- 不静默回退设备、codec、协议或内存路径；回退必须由显式配置允许。
- 所有候选发布使用同一源码 SHA、Cargo.lock、OCI digest 和验收报告。
- 生产 OCI 是正式 release gate；原生压缩包仅作为辅助 SDK 制品。

## 5. 文档索引

[01](01_current_state_and_release_admission.md)～[11](11_execution_order_and_final_acceptance.md)；
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)；
[OPENVINO_PRODUCT_ACCEPTANCE.md](OPENVINO_PRODUCT_ACCEPTANCE.md)；
[RELEASE_EVIDENCE_TEMPLATE.md](RELEASE_EVIDENCE_TEMPLATE.md)；
[ROLLBACK.md](ROLLBACK.md)；[UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)；
[OPENVINO_LOCAL_DEBUG.md](OPENVINO_LOCAL_DEBUG.md)。

## 6. 完成定义

- [ ] INT5-01～11 全部 Done。
- [ ] 最终 OCI 在 OpenVINO CPU 和 Intel iGPU 上执行同一回归与端到端图。
- [ ] SIGTERM、断流重连、热更新失败和 GPU 不可用均有确定行为。
- [ ] `/livez`、`/readyz`、`/metrics` 与结构化日志可用于生产运维。
- [ ] OCI digest、SBOM、签名、性能/稳定性报告和回滚步骤完整。

