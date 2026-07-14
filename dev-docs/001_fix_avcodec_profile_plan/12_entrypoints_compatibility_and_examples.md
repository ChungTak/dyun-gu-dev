# 12. CLI/C API、兼容迁移与示例

## 1. CLI feature 转发

`dg-cli` 的每个 `avcodec-profile-*` 同时启用 `media` 和 `dg-media` 同名 feature。默认 feature 仍不引入厂商 SDK；用户通过显式 feature 构建目标平台二进制。

示例形式：

```bash
cargo run -p dg-cli --no-default-features \
  --features media,avcodec-profile-native-free -- run --config graph.yaml
```

## 2. C API feature 转发

`dg-capi` 使用同名 Profile feature。C ABI 不新增 avcodec handle 或 backend API；调用方仍通过 graph JSON/YAML 配置 `profile`。头文件 ABI 无变化时必须通过 snapshot 检查。

## 3. 配置示例

至少提供：

- native-free H264 decode→resize→encode；
- software FFmpeg Host；
- RKMPP Host fallback；
- RKMPP/RGA zero-copy image chain；
- NV device-frame decode/encode，明确无 resize；
- 无 avcodec raw adapter。

每个示例写明编译 feature、系统依赖、预期 selected backend、MemoryDomain 和是否发生 copy。

## 4. 兼容期

- 旧 `avcodec` feature 映射 native-free compatibility profile。
- 旧 `codec-*` 保留直接 backend feature。
- 旧 `hw` 解析后输出替代 Profile。
- `hw=cuda` 不升级为 device-frame，保持 NV Host 兼容语义。
- 本改造发布后的一个实际发布周期内保留；移除需独立变更和 release note。

## 5. 文档更新

更新 README、design、user-guide 和 upstream 状态说明：

- 默认 media_decode 仍可能是 raw adapter；
- 真实 codec 必须选择 Profile；
- FFmpeg 是 codec-only；
- zero-copy 是有条件路径；
- Cheetah 负责协议/容器规范化。

## 6. 执行体任务

- [ ] 增加 CLI/C API Profile features 和 compile matrix。
- [ ] 增加配置 schema 示例和 load-time validation tests。
- [ ] 更新 element listing/help，展示 profile 参数。
- [ ] 为旧 hw/feature 写弃用测试和日志。
- [ ] 检查 C 头文件无意外 ABI diff。
- [ ] 运行所有示例的 parse/build smoke；硬件示例按 gate skip。

## 7. 完成条件

Rust CLI 与 C API 用户都只需选择一个 Profile feature 并提供同名配置；示例不要求用户列出底层 codec 库或 backend candidates。

