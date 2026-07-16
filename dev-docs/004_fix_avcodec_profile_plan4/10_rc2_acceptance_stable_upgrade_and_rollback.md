# 10. RC2 接纳、稳定升级与回滚

## 1. RC2 接纳

验证上游 tag后原子更新 pin/lock/contract，执行 source/dependency、NativeFree、Software、组合和 NV。全部结果
绑定同一 dyun commit；失败时保持候选分支，不合并 production main。

## 2. 上游反馈

SDK 缺陷写入 UP4条目，包含最小复现、结构化 error/report和环境。上游修复产生 RC3，dyun不复制 backend
逻辑。业务 bridge/pump缺陷在 dyun独立修复并重跑受影响矩阵。

## 3. 稳定 0.2.0

上游发布 stable后，将 pin更新为稳定 tag解引用 commit或正式 registry version（由上游交付方式冻结），同步 lock
和 capability。先在候选环境重跑，再升级生产。

## 4. 回滚

回滚同时恢复 manifest、Cargo.lock、dependency contract、Profile features、示例和能力表。保存前一签字 commit/
artifact。不得运行期回退到低层 backend或隐式改变 Profile。

## 5. 完成条件

- [x] RC2 接纳全矩阵通过（本环境；见 `EXECUTION_STATUS.md`）。
- [x] 上游 handoff已确认（外部）。
- [x] stable升级路径文档化（`ROLLBACK.md`；待 `0.2.0` tag 执行 pin）。
- [x] RC2 pin 回滚步骤可重放（`ROLLBACK.md`）。

