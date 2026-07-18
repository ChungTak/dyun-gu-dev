# Plan 6 发布与回滚

## 1. 回滚单位

回滚以不可变制品 digest 为单位，同时绑定：

- dyun commit 与 Cargo.lock；
- GraphSpec `dg/v1` schema；
- ResourcePolicy 默认值和部署硬上限；
- C ABI v2 header、`libdg_capi.so.2` 和 bindings；
- backend/runtime/codec/connector 版本；
- risk register 与 acceptance。

禁止只替换动态库但保留不匹配的 header/binding，也禁止只回滚 GraphSpec 而保留不兼容 runtime limits。

| 项 | 候选值 | 前一接纳值 |
|---|---|---|
| dyun commit | 待填写 | 待填写 |
| artifact digest | 待填写 | 待填写 |
| Graph schema hash | 待填写 | 待填写 |
| ResourcePolicy hash | 待填写 | 待填写 |
| C ABI/header/library | v2 / 待填写 | 待填写 |

## 2. C ABI v2 约束

本计划不提供 v1 runtime fallback。使用 v1 的宿主应用必须先停机并原子升级到 v2 header/library/bindings；
回滚也只能回到上一份完整 v2 制品。不得把 `libdg_capi.so.1` 与 v2 宿主混用。

## 3. 发布前演练

1. 启动前一接纳制品，执行 config/load、C ABI、stream 和最小 backend smoke。
2. 保存 GraphSpec、runtime limits、model 和 artifact hash。
3. 启动候选，验证 limits 安全收紧不会把有效生产配置意外拒绝。
4. 执行 start/metrics/reload/reconnect/shutdown 和 C external callback smoke。
5. 切回前一 v2 digest，确认配置、模型、流和 bindings 可恢复。
6. 保存切换时间、readiness、丢帧/重连、资源曲线和结论。

## 4. 触发条件

以下任一项停止推广或回滚：

- UB/UAF、callback 重复/未释放、sanitizer/Miri failure；
- 无法在 deadline shutdown、deadlock、detached worker；
- limit 未执行、先分配后拒绝或合法配置出现非预期大面积拒绝；
- tensor stride/外部内存产生错误结果；
- metrics/affinity/cache/queue/sink/RSS 持续增长；
- reload 破坏旧图但仍报告 ready；
- C header/library/SONAME/digest 不匹配；
- 吞吐、p95 或 metrics overhead 超门禁；
- secret 出现在日志/metrics/error/evidence。

## 5. 数据与配置

本轮无数据库迁移。GraphSpec 保持 `dg/v1`，但超过进程硬上限会明确失败。发布前必须扫描生产配置，
列出每个 limit 请求值与候选 hard limit；不允许通过临时提高到无限值规避。

## 6. 禁止项

- 禁止静默提高 hard limit、关闭 callback 或改用 unlimited queue 作为应急回滚。
- 禁止以切换 backend/device/codec/protocol 掩盖核心错误。
- 禁止重写 release tag、复用相同 digest 名称或使用未完成同等验证的临时镜像。
- 禁止让 ABI v1/v2 在同一进程混用。

