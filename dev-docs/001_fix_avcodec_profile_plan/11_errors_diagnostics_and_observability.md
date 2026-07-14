# 11. 错误、诊断与可观察性

## 1. 结构化媒体错误

保留现有 `Error::Media(String)` 兼容，新增中立结构化 variant 或 context：

```text
kind: InvalidArgument | Unsupported | Again | EndOfStream | Backend | DeviceLost | Oom | Timeout
operation: CreateDecoder | CreateEncoder | CreateProcessor | Submit | Poll | Flush | Bridge
node / role / profile / backend
codec / bitstream_format / pixel_format
source_domain / target_domain / allow_staging
selection attempts
```

dg-core 不直接保存 avcodec 类型；mapper 将其转换为中立 enum/string snapshot。

## 2. Selection 诊断

每次 session 创建记录：

- requested Profile 和 role；
- compiled profile features；
- candidate backend、capability rejection、probe rejection；
- selected backend；
- role input/output domain；
- fallback 是否发生及原因。

Required policy 没有 selected backend 时返回完整 failure report。

## 3. Transfer 诊断

每次 bridge 记录 operation、source/target domain、handle kind、layout、mode、copy_count。默认 INFO 只记录 session 汇总，逐帧记录使用 DEBUG/TRACE，避免高吞吐日志洪泛。

每 session 维护计数：

- submitted/accepted/output frames；
- Again、Pending、flush poll 次数；
- HostClone、RowRepack、DomainStaging 次数；
- copied bytes；
- flush duration；
- dropped frames（正常 codec 路径应为 0）。

## 4. 外部错误展示

- Graph element error 保留 node name 和结构化摘要。
- CLI 使用 tracing fields，不拼接不可解析长字符串。
- C API `dg_last_error()` 至少稳定输出 `kind= profile= role= operation= backend= domain=`。
- 不把 external raw pointer、fd 内容、codec config bytes 写入日志。

## 5. 执行体任务

- [ ] 定义中立 MediaErrorContext 和 mapper。
- [ ] 替换当前只枚举 AvError 文本的 map 函数。
- [ ] 保留 error source/detail 和 selection failure。
- [ ] 增加 session/transfer counters 与 tracing spans。
- [ ] 为 Required/fallback/domain mismatch/timeout 写 golden diagnostics tests。
- [ ] 验证日志不泄露指针、fd 或 extradata。

## 6. 完成条件

用户仅凭一次失败日志即可确定 Profile、角色、backend 候选、domain、staging 和失败操作；测试可稳定断言字段而非整段易变文本。

