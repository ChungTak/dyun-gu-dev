# Plan 7 发布与回滚

## 1. 原子回滚单位

回滚以不可变 package/OCI digest 为单位，并原子绑定：

- dyun commit、Cargo.lock、构建 provenance 与 SBOM；
- `ProcessRuntimePolicy` schema、部署硬上限与 GraphSpec；
- C ABI v2 header、动态/静态库、SONAME、pkg-config 和 bindings；
- backend/runtime/codec/connector 版本及 capability 状态；
- 模型/测试 workload 兼容性声明、support matrix、risk register 和 acceptance。

禁止只替换动态库而保留不匹配的 header/binding，也禁止只回滚 GraphSpec 或 policy 文件而保留不兼容 runtime。

| 项 | 候选值 | 前一 Accepted 值 |
|---|---|---|
| dyun commit | 待填写 | 待填写 |
| artifact digest | 待填写 | 待填写 |
| Cargo.lock/policy/schema hash | 待填写 | 待填写 |
| C ABI/header/library | v2 / 待填写 | v2 / 待填写 |
| support matrix revision | 待填写 | 待填写 |

## 2. 兼容约束

- 本计划不提供 v1 runtime fallback；C 宿主、header、library 和 bindings 必须作为完整 v2 制品回滚。
- policy 安全收紧可能拒绝旧配置。发布前必须导出请求值与 effective 值，不得在回滚时静默改成 unlimited。
- capability 状态随制品回滚；前一制品没有实机证据的能力不能因当前候选证据自动升级。
- 本轮无数据库迁移。若后续引入状态格式变化，必须另附双向兼容或数据恢复方案，不能复用本文假设。

## 3. 发布前演练

1. 以 digest 启动前一 Accepted 制品，执行 policy/config/model、C ABI、stream 和 CPU backend smoke。
2. 保存 schema、policy、header/library、模型、support matrix 和 artifact hash。
3. 启动候选，验证有效配置仍可运行，超限配置在消费资源前以 typed error 失败。
4. 执行 start/metrics/reload/reconnect/cancel/shutdown、C external callback 和 package examples。
5. 注入 backend pending、坏帧、断网、满队列和 reload 失败，验证 readiness 与 root cause。
6. 切回前一 digest，恢复其完整 policy/config/bindings，重复 smoke。
7. 保存切换时间、可用性、丢帧/重连、资源曲线、日志与 reviewer 结论。

演练必须使用 release 候选制品和部署入口，不得用源码树中的另一份 debug binary 代替。

## 4. 停止推广与回滚触发

以下任一项立即停止推广；若候选已承载流量则执行回滚：

- UB/UAF、callback 重复或遗漏、Miri/sanitizer/model-check failure；
- shutdown/cancel 超过合同、deadlock、线程/任务/FD 遗留；
- limit 未执行、先消费后拒绝或 Graph 放大进程硬上限；
- tensor stride、外部内存、frame metadata 或算法结果错误；
- RSS/device memory/metrics/registry/cache/queue/sink 持续增长；
- frame-local 错误误杀其他流，或 fatal 后仍报告 ready；
- C header/library/symbol/SONAME/package digest 不匹配；
- 吞吐、p95/p99、copy 或 metrics overhead 超门禁；
- support matrix 高报能力，或 secret 出现在日志、metrics、错误或证据中。

## 5. 回滚后核验

- 前一 digest、policy/schema/header/library 与 support matrix 完整恢复；
- readiness、根因、流量与资源曲线恢复到演练基线；
- 外部 callback acquired/released 平衡，无遗留 worker、request、FD；
- 保存触发时间、影响范围、回滚耗时、数据/帧损失与 incident 链接；
- 当前候选标 Rejected 或 Pending，并重开对应 CORE7 risk；修复后使用新 SHA/digest 重新跑全部 required gate。

## 6. 禁止项

- 禁止静默提高 hard limit、关闭 callback、取消 deadline 或使用无界队列应急。
- 禁止切换 backend/device/codec/protocol 来掩盖核心合同错误。
- 禁止重写 release tag、复用 digest 标签或删除失败证据。
- 禁止 v1/v2、候选/前一制品的 header、library、bindings 在同一进程混用。
