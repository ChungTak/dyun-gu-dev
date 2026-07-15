# 10. Transcoder 与 Graph 迁移

## 1. 两种模式

融合模式使用 `VideoSdk::create_transcoder` 返回的 `VideoTranscoderSession`。Graph 模式仍可 decode→process→
encode，但每个节点都使用对应高层 Session。二者不得保留第三套低层 Factory/Transcoder 实现。

## 2. 融合请求

将 input/output codec、time base、尺寸、format、bitrate 和处理意图构造成高层
`VideoTranscodeRequest`。linked/adapted 由上游 options 和 report 表达；dyun 不探测 backend 后自行选择模式。

## 3. 生命周期

`TranscodeCore` 直接拥有 `VideoTranscoderSession`，可移动到 worker；删除借用 Registry 和 `Box::leak`。
submit/poll/flush/reset 与 Decode/Encode 状态机相同，`is_idle` 只用于判断无 pending work，不替代 drain EOS。

## 4. Report

保存 decoder/processor/encoder selected backend、selection trace、fallback 和 mode。不得在 dyun 根据 Profile
名补 report。fallback-only role 和 create/preflight 错误保持上游结构。

## 5. 测试

NativeFree H.264→H.265、带 resize 的 transcode、Software transcode、NV 支持路径、submit Again、flush/drain、
Session Send、无 Registry leak、report 非空和多 Profile isolation。

## 6. 完成条件

- [ ] 低层 VideoTranscoderRequest/Registry 删除。
- [ ] TranscodeCore 不再泄漏内存。
- [ ] 融合与 Graph 职责唯一。
- [ ] 真实媒体和 report 验收通过。

