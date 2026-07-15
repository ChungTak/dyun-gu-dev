# 06. 高层 Request 映射

## 1. 原则

dyun 将业务配置转换为上游高层 Request，不创建 DecoderConfig、EncoderConfig、ImageProcessorConfig、
VideoProfileDescriptor 或 VideoTranscoderRequest。Profile 负责 backend/domain/staging。

## 2. Decoder

映射 input codec、time base 和公开的 codec 参数。codec/container 解析仍属于现有上游媒体入口；本计划不
增加 demux。零分母、未知 codec 和不支持 role 在创建前返回结构化错误。

## 3. Encoder

映射 codec、width、height、ImageInfo、time base、bitrate 和 SDK Request 已公开的质量参数。业务默认值在
dyun 配置层确定一次；不得根据 backend id 改写。

## 4. Processor

从 resize/CSC/crop/rotate 配置构造 `ImageProcessorRequest` 和 `ImageOp`，设置必要 input/output format。
不要自行启用 processor topology。Profile 不支持处理时保留上游 ProfileResolve/Unsupported。

## 5. Transcode

组合高层 decoder/encoder request，按业务设置 `VideoProcessingSpec` 和 linked preference；不传 descriptor、
低层 config 或 Registry。处理目标尺寸/格式只有一处权威来源。

## 6. 测试表

字段合法值、边界尺寸、零 bitrate/time base、处理尺寸、codec 不支持、Profile role 不支持、构建阶段和
错误字段逐项断言。测试重点是 Request 内容和上游 report，不检查私有 backend 实现。

## 7. 完成条件

- [ ] 生产代码无低层 config stamp。
- [ ] 业务默认值不依赖 backend。
- [ ] 四类 Request 映射有表驱动测试。
- [ ] Normalize/ProfileResolve/Preflight/Create 未被压平。

