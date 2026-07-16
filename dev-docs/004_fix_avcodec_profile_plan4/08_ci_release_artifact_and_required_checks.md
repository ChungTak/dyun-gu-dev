# 08. CI、Release Artifact 与 Required Checks

## 1. 普通 CI

fmt/clippy/test 使用精确 1.94.1和 `--locked`。Profile matrix required：NativeFree、Software、组合；source/
dependency guard在每次 SDK pin变化时执行。命令结束后 lock不得变化。

## 2. NV

CPU runner保留 NV Host/device-frame compile-only hard fail。新增带 GPU label 的 gated runtime job，运行本计划
真实媒体和 bridge测试；候选发布必须消费其 passed artifact，不能用 compile job代替。

## 3. Artifact

机器报告包含 dyun SHA、SDK SHA/tag、toolchain、target、features、FFmpeg/GPU环境、命令、退出码、skip reason、
selected backend、diagnostics 和媒体 hash。总状态根据 required suites聚合。

## 4. 防降级

禁止 `|| true`、soft-fail、未知 stable、修改 lock、无设备却标 passed。环境缺失可以使开发 job skip，但 RC2
acceptance 和生产签字失败。

## 5. 完成条件

- [ ] Software/组合 jobs required且锁定。
- [ ] NV compile/runtime证据分开。
- [ ] release acceptance消费 artifact状态。
- [ ] commit/lock/toolchain一致性自动检查。

