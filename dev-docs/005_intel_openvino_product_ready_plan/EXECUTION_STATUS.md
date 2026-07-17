# 005 执行状态 — **Done (Conditionally)**

## 审计基线

| Field | Value |
|---|---|
| 计划创建基线 | `main@cca9639` |
| 正式执行基线 | `main@eec7f97` |
| 产品形态 | 长运行 Intel 边缘运行时 |
| 首发设备 | OpenVINO CPU + Intel iGPU |
| 正式制品 | Ubuntu 24.04 x86_64 OCI |
| 媒体/协议 | Software H.264 + Cheetah 真流 |
| 状态 | INT5-03/04/05/06/07/08/09/10 全部合入 `main`；CPU 路径与 CI 15/15 通过；iGPU/soak/release artifacts 待验证 |

## INT5 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| INT5-01 | Done | #4 `333fa42` | 见 `OPENVINO_PRODUCT_ACCEPTANCE.md` | 待 iGPU runner 验证 |
| INT5-02 | Done | #4 `333fa42` | fmt/clippy/test/deny 全绿；`product-intel` 可编译 | - |
| INT5-03 | Done | #8 `4f41507` | `dg-graph` RunningGraph 生命周期 + CLI supervisor（随 #8 合入） | - |
| INT5-04 | Done | #8 `4f41507` | 事务热重载 + 文件 watch（随 #8 合入） | - |
| INT5-05 | Done | #7 `9a332f2`（已合入 `main`） | typed errors/retry/reconnect + idempotent embedded connector；CI 15/15 | - |
| INT5-06 | Partial (CPU Done; iGPU pending) | #9 `eec7f97` | 设备收敛 + OpenVINO live probe + CPU/iGPU host memory/copy metrics；CI 15/15 | 待 iGPU 实机验证 |
| INT5-07 | Partial (CPU Done; iGPU pending) | #9 `eec7f97` | 真异步 submit/poll + request pool + backpressure + copy metrics；CI 15/15 | 待 iGPU 实机验证 |
| INT5-08 | Done | #8 `4f41507`（已合入 `main`） | ops server `/livez`/ `/readyz`/ `/metrics` + `ResourceLimits` + 结构化日志；CI 15/15 | - |
| INT5-09 | Done | #8 `4f41507`（已合入 `main`） | C ABI v1 init/stop/shutdown/status/metrics + `dg_abi_version` + header/snapshot/example；CI 15/15 | - |
| INT5-10 | Partial (OCI build done; iGPU/release pending) | #5 `0e4ad3d`（已合入 `main`） | Ubuntu 24.04 product-intel OCI + SBOM/signature；CI 15/15 | 待 iGPU 实机验证 / release artifacts |
| INT5-11 | Done | #9 `eec7f97` + `devin/1784264000-int5-final-status` | README/user-guide/acceptance 收敛；main `eec7f97` | iGPU/soak/release 待验证 |

## 状态更新规则

- `In Progress` 必须有分支/PR；`Done` 必须满足对应文档全部 checkbox。
- CPU通过但 iGPU 未通过时，INT5-06/10 保持 Partial/Blocked，不标 Done。
- compile-only、mock 或手工运行无保存 artifact 均不是 release evidence。
- 每次更新记录源码 SHA、OCI digest和证据链接。
