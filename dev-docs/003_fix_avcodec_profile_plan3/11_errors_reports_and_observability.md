# 11. 错误、Report 与可观测性

## 1. 错误映射

分别实现 Build 与 Runtime 映射。Build 保留 stage、profile、role、operation、codec、source/target format/domain、
selection failure 和 source；Runtime 保留 operation、backend、domain、Again 判断和 source。dg error message 可以
增加业务 element/stream context，但不得丢失结构字段或只做字符串解析。

## 2. Build Report

创建成功后保存 Owned report，导出 profile、intent、各 role backend、I/O plan、fallback allowed/occurred/reason、
selection trace 和 transcoder mode。report 是事实来源；配置值只作为期望值单独记录。

## 3. Runtime diagnostics

导出 submitted/output/again/pending/errors、flush/reset attempts/successes、generation。Plan 5 修复后 submitted
只表示成功提交。dyun 的 pump 指标可增加队列等待/业务丢弃，但不能覆盖 SDK 字段含义。

## 4. 日志与安全

创建成功记录一次结构化摘要；fallback 和 runtime unavailable 使用稳定事件名。不要每帧打印普通成功；
限流 Again/Pending。不得记录裸 Host 地址、Cuda/DmaBuf handle、媒体 payload 或秘密路径。

## 5. 测试

错误 stage snapshots、source chain、fallback/no-fallback、Again 非终止错误、diagnostics 单调性、reset generation、
敏感字段 redaction。避免依赖 Debug 的不稳定字段顺序。

## 6. 完成条件

- [ ] Build/Runtime 错误未混用。
- [ ] report 无本地猜测字段。
- [ ] diagnostics 与上游契约一致。
- [ ] 日志可定位且不泄漏资源信息。

