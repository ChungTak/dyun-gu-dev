# avcodec-rs RC2 上游问题记录

> 初始无条目。只记录无法在 dyun 正确修复且有最小复现的问题。禁止复制 backend、policy、domain或 staging
> 作为绕过。

## 模板

### UP4-XXX — 标题

- 状态：Open / Fixed candidate / Verified / Closed
- SDK tag/commit：
- dyun commit：
- Profile/role：
- 期望行为：
- 实际行为：
- 最小复现命令：
- structured error/report/diagnostics：
- toolchain/environment/device：
- 上游 fixture/test：
- 修复 commit/新候选：
- dyun 重验 artifact：
- 临时处置：禁用受影响 Profile；不得添加低层绕过

关闭要求：上游不可变候选含修复，最小测试通过，dyun 更新 pin并重跑受影响矩阵。

