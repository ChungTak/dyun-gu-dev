# Plan 5 发布与回滚

## 1. 回滚单位

生产回滚以不可变 OCI digest为单位，同时绑定 GraphSpec、C ABI、OpenVINO runtime、Cargo.lock和文档支持矩阵。
禁止只替换二进制或动态库而保留不匹配的 runtime/header。

| 项 | 当前接纳值 | 前一接纳值 |
|---|---|---|
| dyun commit | 待填写 | 待填写 |
| OCI digest | 待填写 | 待填写 |
| C ABI | 待填写 | 待填写 |
| OpenVINO | 待填写 | 待填写 |

## 2. 发布前演练

1. 启动前一 digest并通过 `/readyz` 与最小 CPU/iGPU smoke；
2. 启动候选 digest，执行配置兼容和E2E；
3. 保存正在使用的模型/config hash；
4. 切回前一 digest，确认 GraphSpec可加载、流可恢复、指标连续性预期明确。

## 3. 触发条件

GPU plugin失败或回退CPU、崩溃/死锁、数据/时间戳错误、持续重连风暴、性能超过门禁、RSS/句柄泄漏、
签名/SBOM身份不匹配均触发停止推广或回滚。

## 4. 数据与配置

本轮无数据库迁移。`dg/v1` 保持向后兼容；新增字段均有默认值。若 C ABI v1 与预发布0.1不兼容，
宿主应用和 header/library必须原子升级/回滚，不混用。

## 5. 禁止项

- 禁止运行时静默切换 CPU/GPU、protocol或codec作为回滚。
- 禁止重写 release tag或复用相同 tag推送新 digest。
- 禁止使用未完成相同硬件验收的临时镜像。

