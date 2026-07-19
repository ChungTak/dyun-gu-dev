# dyun-gu-dev Plan 7：核心模块 Product-Ready 闭环

## 1. 定位

Plan 7 承接 Plan 6 未兑现的产品合同，不新增厂商算子、zero-copy 专项或控制面功能。目标是把
`dg-core → dg-runtime/dg-scheduler → dg-graph → dg-media/dg-stream/dg-elements → dg-capi/dg-cli`
的软件合同、验证基础设施和发布证据真正闭环。

Plan 6 的代码改造大部分已合入，但 `CORE_PRODUCT_ACCEPTANCE.md` 仍为 Pending。“CORE6-01～11
代码路径 Done”不等于 product-ready；本计划以可重复执行的测试、长稳和同一候选制品为准。

## 2. 审计基线

| 字段 | 当前事实 |
|---|---|
| Plan 7 创建基线 | `main@feddd3add23ec8647f91b61fd3c15837342b790a` |
| 工作树 | 创建计划前 clean |
| Rust | `rustc/cargo 1.94.1`，`x86_64-unknown-linux-gnu` |
| Cargo.lock SHA-256 | `a8e90170594e0ae54295eb6fbf45433fc255e65bed57c5ffa07b29c7b890bb87` |
| 本地基础门禁 | fmt、workspace clippy、workspace tests 通过；本机未安装 cargo-deny |
| main CI | `feddd3a` 的 CI 成功 |
| Plan 6 acceptance | Pending；记录的候选仍为旧 SHA `1a9a0a5` |
| Nightly | 最近一次 `a86413c` nightly 中 `reload-transitions` fuzz 失败；当前 SHA 未重跑 |
| C ABI | owned result/external callback/destroy 已有；公开函数仍大量使用 C string/裸 pointer+length |
| ResourcePolicy | Graph/Runtime 部分接线；CLI/C bootstrap、vendor bounded read 和 effective pre-copy 未闭环 |
| Stream | mock/hub timeout 已有；Cheetah adapter 仍以每次 timeout 创建 timer thread |
| 长稳 | `tools/soak.sh` 只循环 workspace tests，不满足资源曲线、真实长流和性能门禁 |

执行时必须重新记录实际 HEAD、dirty 状态、工具链、CI run、Cargo.lock 与候选制品，不能直接复用本表。

## 3. 范围

### 本轮包含

- Plan 6 完成状态与风险证据重算；
- 可信进程 policy bootstrap、Graph 下调和全链路传递；
- model/tensor/frame/device 在消费资源前的统一限制；
- backend async/cancel/capability 诚实性和共享合同测试；
- frame-local/node-fatal/graph-fatal 失败语义与必需指标；
- Cheetah 原生 deadline/close、frame pre-copy validation 和上游准入；
- C ABI v2 view/runtime options/ABI 制品的首次发布闭环；
- Miri、sanitizer、并发模型、fuzz、2h/24h soak、性能与回滚。

### 本轮不包含

- 新增数据库、serving、远程控制面、Web UI 或认证系统；
- 新增 OpenVINO/TensorRT/RKNN/Sophon 厂商算子或性能专项；
- 用 mock/compile-only 为真实 backend、codec 或协议授予 product support；
- 为尚未 Accepted 的不完整 C ABI v2 保留兼容符号；
- 将硬件资格缺失伪装成核心软件风险 Closed。

## 4. 需求矩阵

| ID | 主题 | 核心发布阻塞 |
|---|---|---|
| CORE7-01 | Plan 6 缺口审计、风险重开与准入 | 是 |
| CORE7-02 | 可信进程 Policy 与 Bootstrap | 是 |
| CORE7-03 | Model/Tensor/Frame/Device 消费前限制 | 是 |
| CORE7-04 | Runtime、Backend Cancel 与 Capability | 是 |
| CORE7-05 | Graph/Elements 失败隔离与可观测性 | 是 |
| CORE7-06 | Stream 原生 Deadline 与 Pre-Copy 安全 | 是 |
| CORE7-07 | C ABI v2 Wire、Runtime 与制品闭环 | 是 |
| CORE7-08 | CI、Miri、Sanitizer、并发模型与 Fuzz | 是 |
| CORE7-09 | Nightly、24h Soak 与性能门禁 | 是 |
| CORE7-10 | 发布制品、支持矩阵与回滚 | 是 |
| CORE7-11 | 执行顺序、最终接纳与交接 | 是 |

## 5. 接纳分层

| 层级 | 完成条件 | 对产品声明的影响 |
|---|---|---|
| Core Software | CPU/mock、C ABI、Miri/sanitizer/fuzz、2h/24h 核心 soak 全部通过 | 可接纳 SDK-free/CPU 核心制品 |
| Protocol Capability | Cheetah 真实网络 deadline/reconnect/long soak 通过 | 才可声明对应协议 product-supported |
| Device Capability | 对应 device allocator、cancel、精度、zero-copy 和 soak 通过 | 才可声明对应 backend/device product-supported |

核心软件接纳不自动提升任何协议或硬件能力。启用未验收 capability 的制品必须显示 `Unverified` 或
`Blocked`，不能使用 production 标签。

## 6. 执行规则

- 每个 CORE7 ID 独立 PR；公共接口变更同步更新测试、用户文档、schema/header/snapshot/examples。
- PR 必须更新 `EXECUTION_STATUS.md` 和 `CORE7_RISK_REGISTER.md`；合入 main 后才能标 Done。
- 所有 P0/P1 软件风险以失败测试、修复 commit 和运行证据关闭；评论、compile-only 或手工日志不算。
- 上游能力不足时先记录固定 revision 的最小复现；不得在 dyun 用 detached thread 或无界 wrapper 掩盖。
- Core release 的每一条证据绑定同一源码 SHA、Cargo.lock、Graph schema、C header/library 和制品 digest。
- capability job 缺设备时标 `Blocked`，不得 success skip，也不得阻塞不包含该 capability 的核心制品。

## 7. 文档索引

[01](01_plan6_gap_audit_and_admission.md)～
[11](11_execution_order_and_final_acceptance.md)；
[PLAN6_GAP_MATRIX.md](PLAN6_GAP_MATRIX.md)；
[ADMISSION_BASELINE.md](ADMISSION_BASELINE.md)；
[CORE7_RISK_REGISTER.md](CORE7_RISK_REGISTER.md)；
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)；
[CORE7_PRODUCT_ACCEPTANCE.md](CORE7_PRODUCT_ACCEPTANCE.md)；
[RELEASE_EVIDENCE_TEMPLATE.md](RELEASE_EVIDENCE_TEMPLATE.md)；
[ROLLBACK.md](ROLLBACK.md)；
[UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)；
[C_ABI_V2_COMPLETION.md](C_ABI_V2_COMPLETION.md)。

## 8. 总完成定义

- [ ] CORE7-01～11 全部 Done，核心软件 P0/P1 风险全部 Closed。
- [ ] process policy 从 CLI/C bootstrap 传入 Graph、Runtime、backend、stream、C direct API。
- [ ] model/tensor/frame/device 超限均在读取、复制、分配、导入或 SDK 调用前拒绝。
- [ ] Cheetah 产品路径不使用 detached timer thread，deadline/close 可在真实连接上验证。
- [ ] C ABI v2 view、runtime options、owned handle、symbol、SONAME、C/C++ examples 和制品一致。
- [ ] frame-local 错误不误杀其他流，配置/模型/invariant 错误 fail-closed 并保留根因。
- [ ] Miri、ASan/LSan/TSan、并发模型与全部 fuzz target 在候选 SHA 无报告。
- [ ] 2h nightly、24h 核心 soak、性能和 rollback 使用同一候选制品并通过。
- [ ] support matrix 只授予有对应实机证据的 protocol/backend/device product support。
