# 11. 执行顺序与最终接纳

> 需求 ID：CORE7-11

## Phase 0：审计与失败基线

执行 CORE7-01。冻结 Plan 6 gap matrix、当前候选事实、nightly crash corpus和风险 owner。先提交 P0/P1 最小失败
测试，不在同一提交顺手修复。

退出条件：每个 Carry Forward 有 CORE7 ID/risk/test/owner；capability qualification 与 core risk 已分开。

## Phase 1：Policy 与消费边界

执行 CORE7-02/03：

1. `ProcessRuntimePolicy` 和 CLI/C bootstrap；
2. Graph effective policy 与 Runtime/backend/stream 传递；
3. bounded model loader；
4. tensor/frame/device pre-consumption enforcement；
5. queue/output budget 和 typed resource errors。

退出条件：Rust/CLI/C 三入口一致，所有 limit 边界测试证明先拒绝后消费。

## Phase 2：Runtime、Graph 与 Stream

执行 CORE7-04/05/06：

1. backend execution/cancel capability；
2. frame-local/stream-local/node/graph error scope；
3. metrics/readiness 完整性；
4. Cheetah 上游 native deadline；
5. reconnect/close/shutdown 和 registry 长稳。

退出条件：永久 pending 可停止，无 detached timer thread；坏帧不误杀 graph；fatal 保留根因。

## Phase 3：C ABI v2 首发闭环

执行 CORE7-07：

1. structured ABI version；
2. 全部 borrowed input/view；
3. runtime options/process policy；
4. owned/error/external/destroy/concurrency；
5. header/symbol/SONAME/pkg-config/C/C++ package smoke。

退出条件：归档解压后可独立编译运行 examples，实际动态库与 snapshot 一致。

## Phase 4：验证与长稳

执行 CORE7-08/09：

1. 修复 nightly fuzz failure；
2. Miri/sanitizer/并发模型；
3. 真实 core soak driver；
4. 当前候选 2h nightly；
5. 固定 CPU runner 24h soak 和性能比较。

退出条件：无 report/crash/resource growth，所有 artifact identity 一致。

## Phase 5：发布与接纳

执行 CORE7-10/11。生成候选 package/OCI、support matrix、SBOM/provenance/signature，完成 rollback 演练并填写
`CORE7_PRODUCT_ACCEPTANCE.md`。

## PR 划分

- 每个 CORE7 ID 至少一个独立 PR；CORE7-02/03、06/上游升级、07 ABI switch 不混入无关重构。
- ABI switch 可拆准备/实现/制品三个 PR，但对外 release 只有原子完成态。
- 测试基础设施 PR 不把未运行的 workflow 标 Passed。
- status 只在 main 合入且证据 URL 可访问后改 Done。
- reviewer 必须复核风险关闭测试在撤销修复后会失败。

## 最终清单

- [ ] CORE7-01～11 Done，核心 P0/P1 Closed。
- [ ] Plan 6 gap matrix 无未归类 Carry Forward。
- [ ] process policy 贯通所有产品入口和真实消费边界。
- [ ] Runtime/backend cancel/capability 诚实，未验证硬件保持 Blocked。
- [ ] Graph/element error scope、metrics、readiness 完整。
- [ ] Cheetah native deadline 和 core shutdown 无 detached task/thread。
- [ ] C ABI v2 package 从归档独立通过 C11/C++17/symbol/SONAME/sanitizer。
- [ ] Miri、sanitizer、并发模型和 fuzz 无报告。
- [ ] 同一候选 2h、24h、性能和 rollback 通过。
- [ ] support matrix、CLI/C capabilities 和用户文档引用同一 evidence。

## 最终决定

只有 `CORE7_PRODUCT_ACCEPTANCE.md` 为 Accepted，且 evidence manifest 的 source/artifact identity 与发布候选完全
一致，才能标记 Core Software product-ready。

Protocol/Device capability 独立决定；核心 Accepted 不改变任何 Blocked/Unverified 行。

