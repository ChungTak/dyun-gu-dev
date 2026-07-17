# 005 执行状态 — **Planned**

## 审计基线

| Field | Value |
|---|---|
| 计划创建基线 | `main@cca9639` |
| 正式执行基线 | 待 Phase 0 填写 |
| 产品形态 | 长运行 Intel 边缘运行时 |
| 首发设备 | OpenVINO CPU + Intel iGPU |
| 正式制品 | Ubuntu 24.04 x86_64 OCI |
| 媒体/协议 | Software H.264 + Cheetah 真流 |
| 状态 | 尚未开始实施 |

## INT5 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| INT5-01 | Planned | - | - | - |
| INT5-02 | Planned | - | - | - |
| INT5-03 | Planned | - | - | - |
| INT5-04 | Planned | - | - | - |
| INT5-05 | Planned | - | - | - |
| INT5-06 | Planned | - | - | Intel iGPU runner |
| INT5-07 | Planned | - | - | INT5-06 |
| INT5-08 | Planned | - | - | INT5-03/05 |
| INT5-09 | Planned | - | - | INT5-03/08 |
| INT5-10 | Planned | - | - | Intel iGPU runner |
| INT5-11 | Planned | - | - | INT5-01～10 |

## 状态更新规则

- `In Progress` 必须有分支/PR；`Done` 必须满足对应文档全部 checkbox。
- CPU通过但 iGPU 未通过时，INT5-06/10 保持 Partial/Blocked，不标 Done。
- compile-only、mock 或手工运行无保存 artifact 均不是 release evidence。
- 每次更新记录源码 SHA、OCI digest和证据链接。

