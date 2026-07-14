# 08. 异步 Core 状态机与 Element Pump

## 1. 设计目标

avcodec 的 `submit_* / poll_* / flush / reset` 是非阻塞协议。`Again` 表示调用方必须先 poll 并保留尚未提交成功的输入；`Pending` 表示当前没有输出，不是 EOS。dyun 必须以状态机驱动，不能用同步函数假设包装。

## 2. 统一 Core 状态

每个 avcodec core 至少维护：

```text
Accepting
  pending_input: Option<MediaFrame>
  pending_backend_value: Option<Packet|Image|Request>
  pending_output_adaptation: queue
Flushing
  flush_sent: bool
Ended
Failed
```

输入队列默认容量 1；输出 adaptation 队列必须有固定上限。不得为了绕过 Again 建立无界 VecDeque。

## 3. submit 规则

1. core 已有 pending input 时，driver 不再从 graph recv 新输入。
2. bridge 转换成功但 backend 返回 Again 时，保存转换后的 backend value，避免重复 bridge/copy。
3. poll backend 直到 Ready/Pending/EOS；Ready 输出入有界队列。
4. 产生可用空间后重试 pending backend value。
5. backend 返回 EndOfStream 而尚未 flush 时视为协议错误，不吞掉输入。
6. Failed/Ended 后 submit 返回 InvalidState。

## 4. flush 与 EOS

- graph EOS 只切换 `Flushing`，不立即 broadcast EOS。
- pending input 必须先成功提交；随后调用 backend flush，且最多一次。
- flush 返回 Again 时继续 poll，不能重复 flush。
- 只有 backend poll 返回 EndOfStream 且 adaptation 队列为空时，core 返回 EOS。
- Processor Pending 不得在 flushing 状态自动改写为 EOS。
- encoder 从未收到 frame 时，flush 可直接完成，但必须走显式状态迁移。

## 5. Element 驱动循环

每轮最多执行 64 个 core step：

1. 先发送已有 Ready 输出；
2. pump core；
3. core 可接收输入时调用 `io.recv("in")`；
4. core 有 in-flight 且无进展时等待不超过 `send_backoff` 后再次 pump；
5. 检查 stop flag 和 30 秒默认 drain deadline；
6. 真实 EOS 后 broadcast 一次并退出。

不得 busy-spin；不得因 `recv` timeout 将其解释为 stream EOS。

## 6. Reset

虽然 graph element 当前不热重用 session，core reset contract 仍须测试：

- 清除 pending input/output/error；
- 调用 backend reset；
- 恢复 Accepting；
- 不复用旧 stream codec parameters；
- selection Profile 与 registry 可保留。

## 7. Fake backend 脚本

实现测试专用 backend，可为每次调用返回脚本结果：

```text
submit: Again, Ok, Again, Ok
poll: Pending, Ready(A), Ready(B), Pending, EndOfStream
flush: Again/Ok/Error
```

记录每个输入 id 的 submit 次数、接受次数、输出 id 和调用顺序。

## 8. 执行体任务

- [ ] 定义 driver 可查询的 `can_accept_input/has_in_flight/is_flushing` 接口。
- [ ] 为 DecodeCore、EncodeCore、ResizeCore 实现同一状态模板，避免三套不同 EOS 语义。
- [ ] 从 submit 分支删除 `Again | EndOfStream => Ok(())`。
- [ ] 修改 MediaElement run loop，在 timeout 时继续 pump。
- [ ] 增加 pump step budget 和 drain deadline 配置校验。
- [ ] 实现 fake backend 与表驱动状态机测试。
- [ ] 测试 downstream send backpressure 时不继续无限 poll/recv。

## 9. 必测序列

- submit Again→poll Ready→retry成功；输入只被接受一次。
- 连续两个输入在第一个 pending 时不交换顺序。
- flush 前仍有 pending input。
- flush 返回 Again，poll 两帧后 EOS。
- poll 长时间 Pending，driver 可停止且不 busy-spin。
- backend error 带原始上下文到 element error。
- reset 后同一 fake script 可再次运行。

## 10. 完成条件

任何 Again/Pending 序列都不会丢输入、重复接受、乱序或提前 EOS；所有 queue、pump 和等待都有上限。

