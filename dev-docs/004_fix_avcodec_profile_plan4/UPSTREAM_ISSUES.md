# avcodec-rs 上游问题记录

### UP4-001 — `0.2.0-rc.2` annotated tag 未发布
- 状态：Closed（tag 已发布）

### UP4-002 — Software profile H.264 `BackendHintCapabilityMismatch`
- 状态：**Verified**
- 修复：`f3c1c04`…；发布身份 **`0.2.0-rc.3` / `3f80f558e48ced6d3dc2c1e067307bfd12bec89d`**
- dyun pin：同上
- 内容：libavcodec 58/59+；Software `ffmpeg+jpeg`
- 重验：native-free / software / combo / NV Host+device-frame on RC3 — pass
