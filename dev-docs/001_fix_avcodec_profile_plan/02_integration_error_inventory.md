# 02. 当前接入错误清单与责任边界

## 1. 目标

本章是实现前的强制审计表。执行体必须为每项问题补充回归测试，不能只替换类型名或 Cargo feature 后宣告完成。

## 2. 已确认问题

| ID | 当前行为 | 后果 | 责任 |
| --- | --- | --- | --- |
| DY-SEL-01 | `HwPreference` 生成手工 backend candidate 数组 | 与 SDK capability/profile 漂移 | dyun |
| DY-SEL-02 | decoder/encoder/processor 分别选择，缺少统一 build report | 后端组合不可解释 | dyun |
| DY-CFG-01 | Decoder 固定 `TimeBase(1,25)` | 时间戳解释错误 | dyun |
| DY-CFG-02 | Encoder 固定 bitrate=1 | 视频 encoder 配置无效 | dyun |
| DY-CFG-03 | 所有路径固定 Host，部分路径固定 staging=true | 设备能力被掩盖 | dyun |
| DY-FMT-01 | codec 不识别时返回 JPEG bitstream format | 非 JPEG payload 被错误解释 | dyun |
| DY-FMT-02 | codec 只解析 JPEG/MJPEG/H264 | H265/VP/AV1 无法接入 | dyun |
| DY-FMT-03 | decode 自动 YUV420P→RGB24 | 不必要复制，硬件链中断 | dyun |
| DY-MEM-01 | `buffer_to_avcodec_handle` 总是读取 Host bytes | device handle 无法直通 | dyun |
| DY-MEM-02 | Packet 非 Host/读取失败转换为空 Vec | 数据损坏被伪装为成功 | dyun |
| DY-MEM-03 | Image 拒绝多 plane 和 padded stride | NV12/I420/硬件输出不可用 | dyun |
| DY-ASYNC-01 | submit 的 Again/EOS 被转为 Ok | 输入所有权丢失 | dyun |
| DY-ASYNC-02 | processor 只 poll 一次 | 合法 Pending 被当错误 | dyun |
| DY-ASYNC-03 | resize 在 flush 后把 Pending 合成为 EOS | 尾帧丢失 | dyun |
| DY-DRV-01 | element 只在收到输入后 drain | 异步后端 Pending 后可能停住 | dyun |
| DY-DRV-02 | EOS 后要求一次 drain 到 EOS | 硬件 flush 被误判失败 | dyun |
| DY-META-01 | Packet stream_index 固定 0 | 多 track 身份丢失 | dyun |
| DY-META-02 | Graph Packet 只保留 sequence/stream_id/tags | codec/timebase/extradata/layout 丢失 | dyun |
| DY-FEAT-01 | CLI/C API 不转发 codec features | 外部入口无法构建真实 Profile | dyun |

## 3. 已由当前 SDK 提供、必须复用的能力

- `BackendSelectionPolicy::{RegistryOrder, Ordered, Required}`。
- `HostVideoSessionFactory`、`ZeroCopyVideoSessionFactory` 与结构化 selection trace。
- `VideoBackendPolicy` 的平台策略。
- `ExternalImageDescriptor` 及 plane/bounds 校验。
- `CodecParameters`、Packet flags、bitstream format、timebase。
- Runtime Graph 按 decoder/processor/encoder role 的 backend policy。
- 已修复的 Transcoder pending input、flush/reset 状态。

## 4. 仍需上游 plan2 提供的能力

- 角色级输入/输出 domain，而不是一个 session domain。
- processor domain transition preflight。
- RKMPP/RGA 内存拓扑 Profile。
- External Packet 导入描述符。
- 诚实命名的 NV device-frame Profile。
- Transcoder 内部 processor 完整继承 Profile policy 与 target operation。

## 5. 明确不做

- 不在 dyun 实现第二套 Registry 或 capability engine。
- 不把 backend id 放入业务 YAML 的正常路径。
- 不在 `dg-core` 引入 avcodec 类型。
- 不在媒体 bridge 中实现容器解析。
- 不通过自动回 Host 解决设备 processor 缺失。
- 不修改 avcodec-rs 源码；上游任务只记录为 `UP-*`。

## 6. 执行体任务

- [ ] 为 DY-* 每项登记当前代码位置、旧测试覆盖和新增测试名称。
- [ ] 删除/隔离正常路径中的 backend candidate 数组；Legacy 路径只能存在于单独模块。
- [ ] 将每个问题映射到 03–13 的具体 checkbox。
- [ ] 对已由 SDK 提供的能力标记“复用 API”，禁止在 dyun 重新实现。
- [ ] 对 UP-* 建立编译期或运行期明确门禁，不使用假实现占位。

## 7. 完成条件

所有 DY-* 都有失败复现和目标测试；所有 UP-* 都有明确的最小上游 revision 条件；不存在“后续自行决定”的未冻结项。

