# 04. 事务式热更新与配置依赖监控

## 1. 修正 CLI 语义

watch 模式不得先同步 `graph.run()`。watcher 只负责解析候选配置并把 `(spec,diff)` 发给 supervisor；
supervisor 在持有 live `RunningGraph` 的线程应用更新并报告结果。

## 2. 事务边界

热更新按 `load/normalize/validate → prepare resources → quiesce affected subgraph → switch routes → resume` 执行。
prepare 或 switch 失败必须恢复旧 routes/workers/readiness；不得只依赖候选 GraphSpec 校验来宣称原子性。

不受影响节点、队列、metrics 和有状态 element 保持；受影响节点按配置的 drain timeout 排空。
无法保证状态迁移的 element 明确重建并增加 `state_reset_total` 指标。

## 3. 文件监控

记录根配置和所有 include 的 canonical path。监控新增/删除/替换、mtime 与 inode 变化；100 ms debounce 后读取稳定文件。
非法保存只告警一次并保持旧图；下一次合法保存仍可应用。include 环和越界仍由 GraphSpec 校验拒绝。

## 4. 并发与故障测试

- 长流在 reload 时持续送帧，不丢未受影响分支；
- element create 失败、connector 打开失败、route switch 失败均回滚；
- include 文件变化触发 reload；编辑器 rename-save 可识别；
- 连续十次 reload 无线程/队列增长；
- SIGTERM 与 reload 同时发生时 shutdown 优先且不死锁。

## 5. 完成条件

- [ ] `dg run --watch` 实际修改 live graph。
- [ ] reload 失败不改变旧图和 readiness。
- [ ] include、原子替换和 debounce 有覆盖。
- [ ] reload 结果进入结构化日志和 metrics。

