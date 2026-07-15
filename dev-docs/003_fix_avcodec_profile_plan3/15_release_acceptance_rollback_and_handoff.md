# 15. 发布验收、回滚与上游回传

## 1. 执行顺序

1. 验证上游 RC1 handoff。
2. 提交失败 guard 和基线。
3. 更新 manifest/lock 和薄 Profile mapping。
4. 建立 VideoSdk service，迁移 decode/encode/process/transcode。
5. 删除旧路径和低层 feature/re-export。
6. 执行软件/NV/多 Profile/CI。
7. 回填上游 dyun handoff，等待 RC2。
8. 固定 RC2 重跑后发布 dyun 候选。

## 2. 发布阻断

- SDK revision 浮动或 lock 不一致；
- source/dependency guard 失败；
- 任一首发 Profile 无真实媒体；
- selected backend 串栈；
- device-frame 发生 Host staging；
- error/report/diagnostics 上下文丢失；
- 未签字硬件被标 production；
- 上游修改后未重跑。

## 3. 回滚

回滚提交必须同时恢复 SDK revision、Cargo.lock、Profile feature、配置示例和能力表。不得只切代码而保留新
lock。运行期回滚选择前一已签字 Profile/版本，不动态退回低层 backend。

## 4. 上游回传

提供 dyun commit、lock 中 SDK commit、toolchain/target、features、source/dependency result、每个 Profile
命令与 artifact、selected backend/report、硬件/驱动和 upstream issue。上游仓内 fixture 不替代这些字段。

## 5. 最终签字表

| 项目 | Revision/Artifact | 结果 | 签字人/日期 |
|---|---|---|---|
| SDK RC1 接纳 | 待填写 | 待填写 | 待填写 |
| Source/dependency | 待填写 | 待填写 | 待填写 |
| NativeFree | 待填写 | 待填写 | 待填写 |
| Software 6/7/8 | 待填写 | 待填写 | 待填写 |
| NV Host/device | 待填写 | 待填写 | 待填写 |
| Multi Profile | 待填写 | 待填写 | 待填写 |
| SDK RC2 重验 | 待填写 | 待填写 | 待填写 |

## 6. 完成条件

- [ ] INT3-01～15 全部 Done。
- [ ] 真实 dyun 证据已被上游接收。
- [ ] RC2 重跑无公共行为变化。
- [ ] 回滚步骤在候选环境演练。

