# 13. 入口、Legacy 与示例迁移

## 1. 统一配置

YAML、CLI、Graph element 和内部 API 使用同一稳定 `profile` 字段。值与上游名称一致。多个 Profile 编译时
profile 必填；配置验证在初始化媒体会话前完成。

## 2. Legacy `hw`

只给 `hw` 时执行文档化映射并产生一次 deprecation warning；同时给 `profile`/`hw` 为错误。冻结删除版本，
迁移指南列出逐值替换。`hw=auto` 不扫描 Registry 或猜硬件，按冻结兼容策略处理。

## 3. 示例

更新 NativeFree、Software、NV Host/device-frame、显式 fallback 和多 Profile 示例。示例只出现业务 codec、
尺寸、bitrate、处理参数和 profile；禁止 backend id、Registry、Factory、descriptor、domain、staging。

## 4. C API/CLI

本计划不扩展 avcodec C ABI。dyun 自身 C/CLI 入口若暴露 profile，保持字符串和错误码稳定；内部调用同一
Rust service，不能保留旧底层路径。

## 5. 测试

解析、冲突、未知/未编译/未签字 profile、legacy warning、示例配置 load 和 source guard。示例应进入 CI
而非仅作为注释。

## 6. 完成条件

- [ ] 所有入口行为一致。
- [ ] legacy 有期限和自动测试。
- [ ] 示例无低层 SDK 概念。
- [ ] C/CLI 不绕过 Rust service。

