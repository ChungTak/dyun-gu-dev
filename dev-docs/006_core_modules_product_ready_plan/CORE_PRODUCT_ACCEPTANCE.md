# Core Modules Product Acceptance

> 本文件是最终接纳记录。实施完成前保持 Pending，不得预先勾选。

## 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | 待填写 |
| Cargo.lock SHA-256 | 待填写 |
| GraphSpec API/schema hash | `dg/v1` / 待填写 |
| C ABI/header hash | v2 / 待填写 |
| library SONAME/hash | `libdg_capi.so.2` / 待填写 |
| OCI/artifact reference | 待填写 |
| artifact digest | 待填写 |
| risk register revision | 待填写 |

## 环境与门禁

| Gate | Environment | Result | Evidence |
|---|---|---|---|
| fmt/clippy/test/deny | clean Linux runner | Pending | - |
| Resource/Core contract | SDK-free + Miri | Pending | - |
| Runtime/Scheduler/Graph | concurrency/fault runner | Pending | - |
| Media/Stream/Elements | software/Cheetah runner | Pending | - |
| C ABI v2 | C11/C++17 + ASan/LSan | Pending | - |
| Nightly 2h | fixed runner | Pending | - |
| Release 24h | candidate artifact | Pending | - |
| Product hardware | per support matrix | Pending/External | - |

## 接纳清单

- [ ] CORE6-01～11 全部 Done。
- [ ] P0/P1 风险全部 Closed。
- [ ] 每个资源限制在复制/解析/分配/导入前执行并有边界测试。
- [ ] external buffer/tensor 和 callback 所有权 exactly once。
- [ ] stream/backend/queue pending 均在正式 deadline 内 shutdown。
- [ ] pool metrics、histogram、affinity、cache、sink 和 registry 长期有界。
- [ ] Graph reload 故障注入证明 rollback 或明确 fail-closed。
- [ ] C ABI v2 unknown discriminant、view、owned result/error、destroy 和线程合同通过。
- [ ] Miri、sanitizer、并发模型和 fuzz 无报告。
- [ ] Nightly 2h 与 release 24h soak 通过。
- [ ] 性能、copy 和 metrics scrape 阈值通过。
- [ ] support matrix 只声明有实机证据的 backend/device。
- [ ] rollback 演练通过且前一制品可恢复。

## 例外

| Risk ID | 等级 | 理由 | Owner | 到期日 | 监控/关闭条件 |
|---|---|---|---|---|---|
| - | - | - | - | - | - |

只允许 P2 例外；到期未关闭时 acceptance 自动回到 Pending。

## 决定

`Pending / Accepted / Rejected`：**Pending**

- 决定人：待填写
- 时间：待填写
- 适用制品：待填写
