# Core 7 Product Acceptance

> 本文件是 Plan 7 的唯一最终接纳记录。实施和候选验证完成前保持 Pending，不得预先勾选或沿用
> Plan 6 的候选身份与证据。

## 1. 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | 待填写；必须为 clean、已合入 main 的完整 SHA |
| Cargo.lock SHA-256 | 待填写 |
| ProcessRuntimePolicy schema/hash | 待填写 |
| GraphSpec API/schema/hash | 待填写 |
| C ABI version/header hash | v2 / 待填写 |
| library SONAME/hash | `libdg_capi.so.2` / 待填写 |
| package/OCI reference | 待填写 |
| package/OCI digest | 待填写 |
| SBOM/provenance/signature | 待填写 |
| risk register revision | `CORE7_RISK_REGISTER.md` / 待填写 revision |
| evidence manifest | 待填写不可变 URL 与 hash |

以上字段必须来自同一候选构建。任一源码、lock、policy/schema、header、library 或 digest 不一致时，本文件自动
失效并回到 Pending。

## 2. Core Software 门禁

| Gate | 环境 | 结果 | 证据 |
|---|---|---|---|
| admission/fmt/clippy/test/deny/lock | clean Linux runner | Pending | 待填写 |
| process policy 与消费前限制 | Rust/CLI/C；constrained-memory | Pending | 待填写 |
| runtime/backend/graph cancel 与错误域 | CPU/mock + fault runner | Pending | 待填写 |
| media/stream/elements 软件合同 | CPU/mock/file/hub | Pending | 待填写 |
| C ABI v2 package | C11/C++17 + dynamic/static package smoke | Pending | 待填写 |
| Miri | pinned nightly | Pending | 待填写 |
| ASan/LSan/TSan | sanitizer runners | Pending | 待填写 |
| 并发模型与 fuzz | model checker + 全 target | Pending | 待填写 |
| nightly 2h | 固定 CPU runner | Pending | 待填写 |
| release 24h | 同一候选制品、固定 CPU runner | Pending | 待填写 |
| performance/rollback | 基线与候选、前一接纳制品 | Pending | 待填写 |

## 3. Capability 资格

| Capability | 状态 | 所需证据 | Evidence |
|---|---|---|---|
| Cheetah protocol | Blocked | 真实连接 deadline/close/reconnect/长稳 | 待填写 |
| avcodec profile | Blocked | oversized input、资源上限、长流与 sanitizer 支持子集 | 待填写 |
| 各 vendor backend/device | Blocked | 对应实机 cancel、allocator、正确性、zero-copy 与 soak | 待填写 |

`Blocked` capability 不阻塞不包含该能力的 Core Software 接纳，但对应制品、CLI、C capabilities、支持矩阵和
用户文档必须显示 `Blocked` 或 `Unverified`。不得依据 mock、compile-only 或其他设备的证据升级状态。

## 4. 接纳清单

- [ ] CORE7-01～11 全部 Done，且每项证据来自 main。
- [ ] 核心软件 P0/P1 全部 Closed；P2 例外有 owner、到期日和监控。
- [ ] Rust、CLI、C 三入口使用同一可信 process policy，Graph 只能下调。
- [ ] model/tensor/frame/device/queue/output 均在读取、复制、分配、导入或 SDK 调用前拒绝超限。
- [ ] runtime/backend capability 诚实，永久 pending、满队列和关闭路径在 deadline 内结束。
- [ ] frame-local 错误不误杀其他流；node/graph fatal 保留稳定 typed root cause。
- [ ] registry、pool、affinity、cache、collector、sink、metrics 存储与线程/FD 长期有界。
- [ ] Cheetah 产品路径不按 timeout 创建线程或遗留 detached task。
- [ ] C ABI v2 view、options、owned handle、destroy、symbol、SONAME 与 package examples 一致。
- [ ] Miri、sanitizer、并发模型和全部 fuzz target 在候选 SHA 无报告。
- [ ] 同一候选通过 2h、24h、性能、100 次 reload/reconnect/shutdown 和 rollback。
- [ ] support matrix 只授予具有独立实机证据的 protocol/backend/device。

## 5. 例外

| Risk ID | 等级 | 理由 | Owner | 到期日 | 监控与关闭条件 |
|---|---|---|---|---|---|
| - | - | - | - | - | - |

只允许 P2 例外。例外到期、监控失效或条件变化时，acceptance 自动回到 Pending。

## 6. 当前阻塞

| 阻塞 | 等级 | 关闭条件 |
|---|---|---|
| CORE7-02～07 产品合同尚未实施 | P0/P1 | 对应 CORE7 状态 Done，边界/故障测试与 main 证据齐全 |
| Miri/sanitizer/model-check/fuzz 候选证据缺失 | P0/P1 | 当前候选 required jobs 无报告 |
| 真实 2h/24h soak、性能与 rollback 缺失 | P0 | 同一候选制品和固定 runner 的完整曲线、阈值与演练记录 |
| Cheetah 与 vendor/device qualification 缺失 | Capability | 保持 Blocked，或分别完成真实协议/实机验收 |

## 7. 决定

`Pending / Accepted / Rejected`：**Pending**

- 决定人：待填写
- 决定时间：待填写
- 适用制品：待填写
- 备注：本文创建时仅定义接纳合同，不代表 Plan 7 已实施或候选已验证。

## 8. CORE7 任务 PR 追踪

> 临时记录当前 CORE7 各任务独立 PR。所有 PR 合入 main、对应门禁 green、风险关闭后，本文件进入 Accepted 流程。

| CORE7 ID | 主题 | PR | 状态 |
|---|---|---|---|
| CORE7-01 | Plan 6 gap audit and admission baseline | #86 | Merged |
| CORE7-02 | Trusted process policy and bootstrap | #87 | Merged |
| CORE7-03 | Bounded model loader and pre-consumption model checks | #88 | Merged |
| CORE7-04 | Runtime/backend cancel diagnostics and execution-mode capabilities | #89 | Open |
| CORE7-05 | Graph error scopes and readiness contract | #90 | Open |
| CORE7-06 | Stream native deadline and pre-copy safety | #91 | Open |
| CORE7-07 | C ABI v2 structured ABI version | #92 | Open |
| CORE7-08 | reload-transitions fuzz cleanup | #93 | Open |
| CORE7-09 | Soak driver with candidate/spec support | #95 | Open |
| CORE7-10 | Release package layout and support matrix | #94 | Open |
| CORE7-11 | Execution order and acceptance tracking | #96 | Open |

最终候选身份与 evidence 必须在所有上述 PR 合入后重新以 main 的完整 SHA 填写。
