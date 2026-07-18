# dyun-gu-dev Plan 6：核心模块 Product-Ready

## 1. 定位

本计划在一期功能与 Plan 5 软件路径完成后，对产品核心链路做横向加固：

`dg-core → dg-runtime/dg-scheduler → dg-graph → dg-media/dg-stream/dg-elements → dg-capi/dg-cli`

目标不是继续堆叠功能，而是把资源边界、所有权、取消、并发、错误、指标和长期运行行为变成可验证的产品合同。
OpenVINO、TensorRT、RKNN 和 Sophon 本轮只纳入统一 runtime 合同与真实硬件门禁；各厂商 SDK 的专项优化和
硬件产品化仍由独立计划负责。

## 2. 审计基线

| 字段 | 当前事实 |
|---|---|
| 计划创建基线 | `main@f0230e946dc05561d830581df277b79aadb1b807` |
| 工作树 | 创建计划前 clean |
| 默认门禁 | `fmt`、workspace `clippy -D warnings`、workspace tests 通过 |
| ResourceLimits | config/nodes/connections/include count 部分执行；tensor/frame/model 未下沉 |
| 长流取消 | graph pipe 可轮询 stop；stream pull 的网络 recv 仍可能无限阻塞 |
| 指标 | inference pool 只挂首实例；latency 原始样本无界保存 |
| 外部内存 | Rust guard 模型存在；C ABI 导入使用空 guard，生命周期不安全 |
| C ABI | 当前声称 v1；enum 入参和 thread-local 返回数据仍不满足最终安全合同 |
| 长稳证据 | 无核心模块 24h soak、sanitizer/Miri/并发故障证据 |

执行时必须重新记录实际 HEAD、dirty 状态、工具链和 CI 结果，不得把本表 SHA 当作执行 SHA。

## 3. 产品范围

### 本轮包含

- 核心内存、tensor、media metadata 和外部资源所有权；
- runtime/backend 公共异步合同、scheduler 和聚合指标；
- graph 队列、worker、停止、shutdown、reload 和 collector；
- media/stream 桥接、可超时接收、重连与不可信 frame；
- 算法 element 的输入、复杂度和输出预算；
- C ABI v2、CLI bootstrap、ops/metrics 和失败语义；
- PR、nightly、fuzz、sanitizer、Miri、性能与 24h soak。

### 本轮不包含

- 新增 serving、数据库、远程控制面或 Web 管理台；
- 为四套厂商 SDK 分别实现新的算子、zero-copy 或性能专项；
- 以 mock/compile-only 关闭真实硬件验收；
- 在同一动态库保留 C ABI v1/v2 双符号。

## 4. 需求矩阵

| ID | 主题 | 首发阻塞 |
|---|---|---|
| CORE6-01 | 当前状态、风险台账与接纳门禁 | 是 |
| CORE6-02 | 资源权限与不可信输入上限 | 是 |
| CORE6-03 | Core 内存、Tensor 与媒体不变量 | 是 |
| CORE6-04 | Runtime、Scheduler 与聚合指标 | 是 |
| CORE6-05 | Graph 执行、生命周期与热更新 | 是 |
| CORE6-06 | Media/Stream 可取消 I/O 与桥接 | 是 |
| CORE6-07 | Elements 正确性与故障隔离 | 是 |
| CORE6-08 | C ABI v2 所有权、线程与迁移 | 是 |
| CORE6-09 | 可观测性、安全与失败语义 | 是 |
| CORE6-10 | CI、Fuzz、并发测试与长稳 | 是 |
| CORE6-11 | 执行顺序、最终验收与交接 | 是 |

## 5. 执行规则

- 每个 CORE6 项独立 PR；PR 同步更新 `EXECUTION_STATUS.md` 和 `CORE_RISK_REGISTER.md`。
- 风险只能以代码、自动测试和保存的运行证据关闭；review 结论或注释不算证据。
- 进程级资源策略是硬上限；GraphSpec 只能下调，不能由不可信配置提高。
- 所有复制、解析、分配、导入和队列扩容必须在消耗资源前检查上限。
- 所有可能等待外部系统的 recv/send/poll/connect/drain 必须有 deadline 或可观察的取消点。
- 不以静默空数据、饱和转换、默认设备、默认 codec 或 fallback 掩盖合同错误。
- P0/P1 全部关闭后才允许进入 release soak；P2 必须有负责人、到期日和可执行关闭条件。
- release 证据必须绑定同一源码 SHA、Cargo.lock、Graph schema、C header/library 和制品 digest。

## 6. 文档索引

[01](01_current_state_risk_inventory_and_admission.md)～
[11](11_execution_order_and_final_acceptance.md)；
[CORE_RISK_REGISTER.md](CORE_RISK_REGISTER.md)；
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)；
[CORE_PRODUCT_ACCEPTANCE.md](CORE_PRODUCT_ACCEPTANCE.md)；
[RELEASE_EVIDENCE_TEMPLATE.md](RELEASE_EVIDENCE_TEMPLATE.md)；
[ROLLBACK.md](ROLLBACK.md)；
[UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)；
[C_ABI_V2_MIGRATION.md](C_ABI_V2_MIGRATION.md)。

## 7. 总完成定义

- [ ] CORE6-01～11 全部 Done，P0/P1 风险为 Closed。
- [ ] 所有发布的 limit 都在实际消费资源之前执行，边界测试覆盖 `limit-1/limit/limit+1`。
- [ ] stream、runtime 和 graph 在断流、满队列、reload、backend pending 时均能在 deadline 内 shutdown。
- [ ] 外部内存、C 返回数据和错误的所有权明确，无空 drop guard 或 thread-local 悬空指针。
- [ ] pool 指标完整聚合，metrics、affinity、cache、sink 和队列不会无界增长。
- [ ] PR 门禁、nightly 2h、sanitizer/Miri/fuzz 和 release 24h soak 证据完整。
- [ ] C ABI v2 header/library/snapshot/examples 与 GraphSpec schema 来自同一候选 commit。

