# avcodec-rs 上游问题记录

> 初始无条目。只有无法在 dyun 正确解决、且已有最小复现的问题写入本文件。不得在 dyun 复制 backend、
> policy、domain 或 staging 作为临时修复。

## 条目模板

### UP3-XXX — 标题

- 状态：Open / Fixed in candidate / Verified / Closed
- avcodec commit：
- dyun commit：
- 影响 Profile/role：
- 期望行为：
- 实际行为：
- 最小复现命令：
- 结构化 error/report：
- 环境与设备：
- 上游 fixture/test 位置：
- 修复 commit：
- dyun 重验命令/artifact：
- 临时处置：禁用受影响 Profile；不得添加低层绕过

关闭要求：上游 commit 可定位，最小测试通过，dyun 固定包含修复的不可变候选并完成受影响 Profile 重验。

