# 04. Profile 配置与一对一映射

## 1. 薄映射

`AvcodecProfile` 只承担稳定字符串、parse/display、feature 是否编译和 `to_sdk() -> VideoProfile`。每个变体
必须一对一，不返回 policy、descriptor 或 I/O plan。特别断言 NativeFree→`VideoProfile::NativeFree`，
禁止再次映射到 Software。

## 2. 选择规则

- 配置显式给出 profile：校验编译后使用。
- 未给 profile 且只编译一个：允许选择唯一值并记录来源。
- 未给 profile 且编译多个：返回配置错误，列出稳定名称。
- fallback 只有显式 `*-fallback` Profile 才允许。
- 未签字硬件可被解析/编译，但生产支持查询必须返回 unverified。

## 3. Legacy `hw`

`profile` 与 `hw` 同时出现直接冲突。单独 `hw` 按冻结表映射并输出 deprecation warning；`auto` 不探测并
猜测任意硬件，默认映射策略必须写入配置迁移文档。兼容期结束日期/版本在 release 文档中冻结。

## 4. 测试

覆盖全部名称 roundtrip、全部变体一对一映射、feature 未编译、唯一/多 Profile 缺省、profile/hw 冲突、
legacy warning 和未签字状态。

## 5. 完成条件

- [ ] profile 模块不 import policy/descriptor/domain。
- [ ] NativeFree 映射缺陷有回归测试。
- [ ] 多 Profile 无隐式选择。
- [ ] legacy 行为可迁移且有期限。

