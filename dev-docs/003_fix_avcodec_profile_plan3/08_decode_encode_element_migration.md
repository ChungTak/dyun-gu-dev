# 08. Decode 与 Encode Element 迁移

## 1. DecodeCore

创建时解析 `AvcodecProfile`→`VideoProfile`，构造 `VideoDecoderRequest`，通过 service 获得
`VideoDecoderSession` 和 report。运行时只调用 `submit_packet`、`poll_image`、`flush`、`reset`、`status/caps`。

## 2. EncodeCore

构造 `VideoEncoderRequest` 并持有 `VideoEncoderSession`。输入 Image 必须满足 Request 的格式/尺寸；需要转换
时由显式 processor Element 或 Transcode processing spec 完成，不因 backend 临时插入隐式 CSC。

## 3. Pump 状态机

- submit Ok：输入已接受；
- Again：保留当前输入，先 poll/drain 后重试；
- poll Ready：转换并输出；Pending：让出调度；EOS：结束 drain；
- end-of-input 只调用一次逻辑 flush，可按 Again 重试；
- reset 成功后清空 dyun pending 状态并同步 generation。

不得 busy loop、sleep、后台线程自动 pump 或将 Pending 映射 Again。

## 4. 诊断

创建时保存 Owned report；运行时读取 Session diagnostics。selected backend 直接来自 report。错误映射保留
profile/role/operation/backend/domain/source。

## 5. 测试

fake 状态机覆盖 submit Again、poll Pending、flush Again、多输出、EOS、reset；NativeFree 真实 H.264
decode/encode/roundtrip；Software 等价路径；多 Profile backend 断言。

## 6. 完成条件

- [ ] Decode/Encode 不再使用 Factory V2/raw trait。
- [ ] 状态机无丢帧、重复消费或 busy loop。
- [ ] report/diagnostics 对外可用。
- [ ] 真实媒体闭环通过。

