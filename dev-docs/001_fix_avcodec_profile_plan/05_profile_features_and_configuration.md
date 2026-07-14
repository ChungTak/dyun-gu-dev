# 05. Cargo Profile、运行配置与兼容入口

## 1. Feature 分层

四层 crate 使用同名公开 feature：

| dyun feature | 上游 profile |
| --- | --- |
| `avcodec-profile-native-free` | `profile-native-free` |
| `avcodec-profile-software` | `profile-software` |
| `avcodec-profile-rkmpp-host` | `profile-rkmpp-host` |
| `avcodec-profile-rkmpp-host-fallback` | `profile-rkmpp-host-fallback` |
| `avcodec-profile-rkmpp-zero-copy` | Profile V2 RKMPP/RGA |
| `avcodec-profile-nvcodec-host` | `profile-nvcodec-host` |
| `avcodec-profile-nvcodec-host-fallback` | `profile-nvcodec-host-fallback` |
| `avcodec-profile-nvcodec-device-frame` | Profile V2 device-frame |
| `avcodec-profile-onevpl-host` | `profile-onevpl-host` |
| `avcodec-profile-onevpl-host-fallback` | `profile-onevpl-host-fallback` |
| `avcodec-profile-amf-host` | `profile-amf-host` |
| `avcodec-profile-amf-host-fallback` | `profile-amf-host-fallback` |

`dg-media-avcodec` 增加内部 marker `avcodec-sdk = [dep:avcodec]`。每个新 Profile 直接依赖 marker 和对应上游 feature，不通过旧 `avcodec` 叠加其他 backend。

## 2. Runtime Profile

定义 `AvcodecProfile`，包含：

- stable config name；
- 上游 `VideoProfileDescriptor`；
- 是否编译；
- 是否允许 fallback；
- 支持的 element role；
- legacy alias 列表。

选择规则：

1. YAML 显式 `profile` 时，必须已编译，否则加载图时报配置错误。
2. 未填写 profile 且只编译一个新 Profile 时使用该 Profile。
3. 未填写且编译多个新 Profile 时要求显式选择。
4. 仅使用旧 `avcodec` feature 时映射 native-free 兼容 Profile并告警。
5. `profile` 与 `hw` 同时出现时失败。
6. 无 `fallback` 后缀的 Profile 使用 Required policy，探测失败直接返回结构化错误。

## 3. Element 参数

### media_decode

- `codec`：可选约束；优先从 packet metadata 获取。
- `profile`、legacy `hw`。
- `bitstream_format`：可选约束。
- `output_format`：缺省保留 decoder 原生格式。
- `memory_domain`：高级覆盖，必须被 Profile role 允许。
- 旧 `width/height/channels`：avcodec 模式下作为可选输出断言；raw 模式仍必需。
- `drain_timeout_ms`：默认 30000，范围 1–300000。

### media_encode

- `codec`：旧配置缺失时兼容 JPEG；生产视频示例必须显式填写。
- `profile`、legacy `hw`。
- `bitstream_format`、`encoder_format`、`memory_domain`。
- `bitrate`：H264/H265/VP8/VP9/AV1 必需且非零；JPEG/MJPEG忽略。
- `time_base_num/time_base_den`：优先输入 metadata，显式值用于约束；均缺失时兼容 1/25 并告警。
- `drain_timeout_ms`。

### media_resize

- `width/height` 必需且非零。
- `profile`、`memory_domain`、`drain_timeout_ms`。
- 保留输入 pixel format；格式转换应由显式配置或独立 processor operation 完成。

## 4. Legacy 兼容

- `hw=auto` 映射 LegacySoftware，不再根据编译顺序自动选硬件。
- `hw=rk/rockchip/...` 映射 legacy RK Host；`cuda` 仍映射 NV Host，不能暗示 device-frame。
- `codec-ffmpeg/x264/x265/openh264/rkmpp/librga/nvcodec/onevpl/amf` 保留原直接 backend 启用语义。
- Legacy policy 与新 Profile policy 分模块；新配置不得经过 legacy candidate list。
- 每次 legacy 解析输出一次结构化 warn，包含替代 profile 名。

## 5. 执行体任务

- [ ] 在四层 Cargo.toml 添加全部 Profile 转发。
- [ ] 将代码 cfg 从旧 `avcodec` 收敛到内部 marker，确保任意 Profile 会编译集成代码。
- [ ] 实现 `compiled_profiles()` 和无歧义默认选择。
- [ ] 实现参数 schema、解析、未知字段和冲突测试。
- [ ] 实现 Profile→SDK descriptor mapper，不复制 backend id 列表。
- [ ] 将旧 HwPreference 移入 legacy 模块并标弃用。
- [ ] 为每个 Profile 写 `cargo tree -e features` golden 断言或脚本检查。

## 6. 完成条件

外部用户只需选择一个 `avcodec-profile-*` feature 和一个同名运行 profile；非 fallback Profile 绝不静默换 backend；旧入口仍能编译但有明确告警。

