# dyun-gu-dev avcodec Profile 接入修复执行计划

## 1. 文档定位

本目录把 avcodec-rs 接入审查结论转换为可直接交给编程智能体执行的 dyun-gu-dev 仓内开发任务。执行体不得修改 avcodec-rs；需要上游新增的能力统一以 `UP-*` 门禁表示，并以 avcodec-rs `114_enhance_sdk_plan2` 完成时登记的 commit 作为最终依赖版本。

本计划只处理 codec/image processing 接入，不增加 MP4、RTSP、RTMP、TS、PS 等 demux/mux 或协议实现。协议和容器仍属于 `dg-stream` 及其 connector，`dg-media` 只接收规范化 Packet/Image。

## 2. 已冻结的审查结论

1. 当前主要错误在 dyun 适配层：后端候选硬编码、Profile 未透传、Host 和设备路径混用、元数据丢失、`Again` 被吞、EOS 过早结束、单次 poll 假设和隐式复制。
2. avcodec-rs 当前基线已经提供角色选择策略、Session Factory、Profile features、外部 Image、结构化错误和 Runtime Graph；dyun 必须消费这些 API，不能重复实现 Registry 选择器。
3. dyun 的 decode/encode/resize 是独立 graph element，本计划不新增 `media_transcode`。未来若融合压缩转码，应使用具备完整 Profile policy 的上游 Runtime Graph。
4. Host 路径先交付；RKMPP/RGA 零拷贝在上游 Profile V2 完成后交付；NV 当前只允许称为 device-frame 路径，不能称完整 CUDA Packet/Image 零拷贝。
5. 所有“零拷贝”断言必须同时证明 MemoryDomain、external handle、plane layout、所有权 guard 和 `copy_count == 0`。

## 3. 执行规则

1. 严格按阶段依赖执行；上游门禁未满足时只能完成不依赖该门禁的任务。
2. 每项 `[ ]` 是独立可评审交付。完成后改为 `[x]`，并追加 commit、测试命令和结果摘要。
3. 禁止 `todo!()`、`unimplemented!()`、生产路径 `unwrap()`、吞错、空字节替代设备缓冲、静默 fallback。
4. 新公共类型先写 contract test，再修改 bridge、element 和外部入口。
5. 未命名为 `*-fallback` 的 Profile 不得回退到其他 backend。
6. 旧 `hw`、`avcodec`、`codec-*` 仅保留一个发布周期；兼容路径也必须经过新状态机和新元数据契约。
7. 文档和代码不得依赖 vendor、Cargo checkout 或其他仓库本地路径。
8. 硬件缺失测试必须输出确定性 skip 原因；硬件专用 CI 不允许全量 skip 后成功。

## 4. 文档索引与阶段

| Phase | 文档 | 交付 |
| --- | --- | --- |
| 0 | [01](01_baseline_and_dependency_contract.md)、[02](02_integration_error_inventory.md) | 基线、责任边界、上游门禁 |
| 1 | [03](03_media_metadata_model.md)、[04](04_graph_and_stream_metadata_transport.md) | 跨 graph 的无损媒体契约 |
| 2 | [05](05_profile_features_and_configuration.md) | Profile feature 与配置收敛 |
| 3 | [06](06_host_packet_image_bridge.md)、[07](07_external_memory_ownership_bridge.md) | Host bridge 与外部内存所有权 |
| 4 | [08](08_async_cores_and_element_pump.md)、[09](09_decode_encode_resize_elements.md) | 正确异步状态机和 element |
| 5 | [10](10_hardware_profile_integration.md) | RKMPP/RGA、NV device-frame |
| 6 | [11](11_errors_diagnostics_and_observability.md)、[12](12_entrypoints_compatibility_and_examples.md) | 诊断、CLI/C API、迁移 |
| 7 | [13](13_test_matrix_and_release.md) | 完整测试矩阵与发布 |

## 5. 上游门禁

| ID | 必须由 avcodec-rs 提供的能力 | dyun 阻塞项 |
| --- | --- | --- |
| UP-01 | 角色级 Packet/Image 输入输出 MemoryDomain Profile | 硬件 Profile 不再手工拼 domain |
| UP-02 | ImageProcessor 输入→输出 domain transition preflight | DrmPrime→DmaBuf RGA 链 |
| UP-03 | Profile V2 Session Factory 与 build report | 硬件 decode/resize/encode 创建 |
| UP-04 | ExternalPacketDescriptor | 非 Host Packet 安全互操作 |
| UP-05 | policy-aware Transcoder/FrameAdapter | 后续融合转码；本期 element 不阻塞 |
| UP-06 | `nvcodec-device-frame` 诚实契约 | NV device-frame 生产启用 |

## 6. 全局完成定义

- [ ] 01–13 所有任务完成，无未登记 TODO。
- [ ] Rust 工具链可获取，默认 workspace 构建不要求 codec 或厂商 SDK。
- [ ] `media_decode → media_resize → media_encode` 不丢 codec、format、PTS/DTS、timebase、flags、extradata 和 image plane layout。
- [ ] fake backend 的 `Again/Pending/flush/EOS` 组合无输入丢失、重复或乱序。
- [ ] software/native-free Host 路径有真实 H264/H265/JPEG fixture。
- [ ] RKMPP/RGA 图像链硬件验收 `copy_count == 0`。
- [ ] NV 路径只标记为 device-frame；完整 CUDA zero-copy 保持门禁。
- [ ] CLI/C API 能通过同名 feature 编译并加载 Profile 配置。
- [ ] 旧入口有弃用告警、兼容测试和明确移除条件。

## 7. 问题覆盖矩阵

| 审查问题 | 负责文档 |
| --- | --- |
| feature 未从 CLI/C API 透传 | 05、12 |
| 业务层硬编码 backend candidates | 05、09 |
| 固定 Host/allow_staging | 05、07、10 |
| 固定 timebase/bitrate/format | 03、09 |
| Packet/Image metadata 丢失 | 03、04、06 |
| 多平面和 padded stride 不支持 | 03、06、07 |
| `Again`/`Pending` 被吞 | 08、09 |
| EOS/flush 过早 | 08、13 |
| 设备缓冲变空 Vec | 06、07、13 |
| 无证据宣称 zero-copy | 07、10、11、13 |
| 错误上下文退化为字符串 | 11 |

