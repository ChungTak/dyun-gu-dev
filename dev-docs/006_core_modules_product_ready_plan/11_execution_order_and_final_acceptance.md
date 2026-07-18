# 11. 执行顺序与最终验收

> 需求 ID：CORE6-11

## Phase 0：审计与失败基线

执行 CORE6-01。重新采集 HEAD/工具链/CI，完成所有 crate 审计，为 P0/P1 先提交最小失败测试。

退出条件：风险台账有 owner/严重度/复现，默认基线 clean，未用修复提交覆盖失败基线。

## Phase 1：资源与 Core 不变量

执行 CORE6-02/03：

1. 进程 ResourcePolicy 与 Graph effective limits；
2. config/model/tensor/frame pre-allocation enforcement；
3. fallible buffer、stride physical bytes、external guard；
4. memory pool 和 collector/cache 预算。

退出条件：所有 limit 边界测试通过，external-only buffer 不再静默为空，C/stream 尚未接入的入口明确保持失败。

## Phase 2：Runtime、Scheduler 与 Graph

执行 CORE6-04/05：

1. async/cancel/sequence 合同；
2. pool 共享 metrics 和固定 histogram；
3. affinity capacity/TTL 和 poison/invariant；
4. bounded execution、worker/collector budget；
5. shutdown deadline 与 transactional reload。

退出条件：backend/queue pending 可停止，pool 指标对账，100 次 reload/start/stop 无增长。

## Phase 3：Media、Stream 与 Elements

执行 CORE6-06/07：

1. SubscriberSource timeout/close；
2. frame pre-copy limits 与 bridge typed errors；
3. reconnect/registry/cache 生命周期；
4. algorithm complexity/state/output budget；
5. frame-local 与 graph-fatal 分类。

退出条件：永久网络 pending 可 shutdown，超大 frame 未分配，bridge 无吞错/fallback，worst-case element 有界。

## Phase 4：C ABI v2 与运维

执行 CORE6-08/09：

1. int32 input discriminant、view/struct prefix；
2. owned bytes/error handle；
3. external release callback 与 explicit destroy；
4. header/symbol/SONAME/examples/bindings；
5. bounded metrics、health、typed error 和 redaction。

退出条件：v1 不再发布，C/C++、fuzz、ASan/LSan 和并发调用合同通过。

## Phase 5：质量与发布接纳

执行 CORE6-10/11。先 PR 全绿，再 nightly 2h，最后同一候选制品 24h soak、性能比较和 shutdown。
更新 status、risk、acceptance、evidence、rollback 和用户/设计文档。

## PR 划分

- 每个 CORE6 ID 独立 PR，不能把全部核心改动合成不可审计的大提交。
- 公共接口改动 PR 必须同时更新 Rust docs、Graph schema、C header/snapshot/examples 和迁移文档。
- 风险关闭 PR 引用 risk ID 和失败测试；status 只在合入 main 后改 Done。
- C ABI v2 切换是一个原子 PR；不得出现 header v2/library v1 或相反的中间 release。

## 最终清单

- [ ] CORE6-01～11 Done，P0/P1 Closed。
- [ ] ResourcePolicy 从 bootstrap 传到 config/model/tensor/frame/queue/backend/C API。
- [ ] Core allocation/stride/external ownership通过 Miri和故障测试。
- [ ] Runtime pool metrics完整，histogram/affinity/cache有界。
- [ ] Graph所有 execution mode、reload和shutdown在预算/deadline内。
- [ ] Stream永久 pending可取消，bridge不吞错且frame在copy前校验。
- [ ] Elements复杂度、state和output受控。
- [ ] C ABI v2 header/library/SONAME/owned result/callback通过C/C++和sanitizer。
- [ ] Health、metrics、error taxonomy和redaction一致。
- [ ] Nightly 2h、release 24h、性能和100次故障循环通过。
- [ ] acceptance、evidence、rollback和support matrix引用同一 commit/artifact。

## 最终决定

只有 `CORE_PRODUCT_ACCEPTANCE.md` 为 Accepted，且无过期例外，才可把核心模块标记为 product-ready。
本决定不自动提升任何未完成实机验收的厂商 backend。
