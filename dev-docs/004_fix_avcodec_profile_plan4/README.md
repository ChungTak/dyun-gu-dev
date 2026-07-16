# dyun-gu-dev Plan 4：avcodec-rs RC2 生产验收与稳定升级

## 1. 定位

Plan 3 已完成 V3 高层 SDK 重构：生产路径只依赖 `avcodec`，使用 `VideoSdk` 和拥有型 Session，不再组装
Registry、Factory V2、BackendPolicy、descriptor、domain 或 staging。本计划不重做这些工作，只接纳上游
Plan 6 的不可变 RC2，重验 NativeFree/Software/多 Profile，并完成 dyun 自身 NV Host/device-frame 真机签字。

首发生产范围：NativeFree、Software、NV Host、NV Device-frame。RKMPP、OneVPL、AMF 保持
`unverified`。

## 2. 当前事实

- dyun HEAD/origin：`872b449222164fc08c2d69ba13a6463190c8483d`。
- 当前 SDK pin：`7faba6fe264aa5ae5bd2f1666084f4bc52aa7d0f`。
- 该 commit 是 avcodec main，不是 `0.2.0-rc.1` tag commit（RC1 tag 为 `91f2dbc`）。
- NativeFree、Software、组合 Profile、source/dependency guard 已通过。
- NV CI 只有 compile-only；真实 dyun NV 媒体尚未签字。
- toolchain/MSRV/CI 均声明 1.94.1；本地镜像同步失败不等于源码不支持，需 clean runner 证明。

## 3. 需求

| ID | 要求 | 证据 |
|---|---|---|
| INT4-01 | 接受不可变 RC2 | tag/commit/artifact |
| INT4-02 | manifest/lock/contract 同 pin | dependency tests |
| INT4-03 | 1.94.1 与平台环境可重现 | clean runner log |
| INT4-04 | 高层 SDK 边界无回归 | source/dependency guard |
| INT4-05 | NativeFree/Software 重新签字 | 真实媒体 |
| INT4-06 | 多 Profile 不串栈 | report assertions |
| INT4-07 | NV Host/device-frame 真机 | hardware artifact |
| INT4-08 | external bridge/zero-copy 正确 | ownership/copy tests |
| INT4-09 | CI/status/handoff 一致 | required checks |
| INT4-10 | RC2→stable 可升级/回滚 | release record |

## 4. 文档索引

按 [01](01_current_state_and_rc2_admission.md)～
[11](11_execution_order_and_final_acceptance.md) 执行。状态见
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)，上游问题见 [UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)，
最终接纳见 [AVCODEC_RC2_ACCEPTANCE.md](AVCODEC_RC2_ACCEPTANCE.md)。

## 5. 执行规则

1. RC2 handoff 不完整时不得先改 pin。
2. pin 改变后全部软件/硬件签字重跑。
3. 不恢复低层 SDK 组装，不在 dyun 修 backend 缺陷。
4. 所有 cargo 命令使用锁文件；manifest/lock/contract 一次更新。
5. NV 无真机不能把 compile-only 标为 production passed。
6. `allow_staging=false` 不允许 bridge 静默 Host copy。
7. 不引用 `vendor`，必要契约写入计划或测试。
8. 状态 Done 必须绑定 commit、命令和 artifact。

## 6. 完成定义

- [x] INT4-01～10 Done（stable `0.2.0` 为后续 freeze，非本计划阻塞）。
- [x] dyun pin = RC3 tag commit `3f80f55` / `0.2.0-rc.3`。
- [x] NativeFree/Software/组合/NV 在 RC3 上通过。
- [x] handoff：`AVCODEC_RC2_ACCEPTANCE.md`；回滚：`ROLLBACK.md`。

