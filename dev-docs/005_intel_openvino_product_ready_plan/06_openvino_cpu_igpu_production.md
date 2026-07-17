# 06. OpenVINO CPU+iGPU 生产化

## 1. 设备配置收敛

Graph 顶层 `device` 是规范设备入口：`cpu → CPU`、`intel_gpu → GPU`。旧 `options.device` 保留一个 `dg/v1`
兼容周期并发出 deprecated warning；两者同时出现且不一致时加载失败。显式 GPU 不可用时禁止回退 CPU。

OpenVINO Rust API 中保留原始 device string 只用于 `AUTO/MULTI/HETERO` 后续实验；首发 schema 只宣称 CPU/GPU。

## 2. Live capability probe

backend 初始化 Core 后查询并记录：OpenVINO runtime version、available devices、完整 plugin device name、
目标 device id、支持的基础属性。若社区 crate 缺少所需安全 API，在 `dg-openvino-sys` 增加最小 wrapper，
不得用静态表伪造 device count 或 SDK version。

`RuntimeCapabilities` 扩展为设备级记录，至少包含 kind、logical id、runtime name、async、external-memory、
remote-tensor 和已验证 precision。静态 capability 仅用于加载期语法 preflight，不能进入运行期 readiness。

## 3. 模型与精度

CPU/iGPU 使用同一确定性 IR fixture，覆盖 F32、GPU 支持时的 F16、动态 batch、多输出和错误 shape。
真实业务验收增加一个小型卷积/分类或检测 IR，而不是只用 Identity。模型 hash 写入证据。

设备 precision 以实际 compile/infer 结果为准。请求 precision 与模型/设备不匹配时返回 device/model context，
不得改变 precision 或设备继续运行。

## 4. Host 内存合同

首发 OpenVINO CPU/iGPU 都使用 Host tensor 输入输出；每次 H2D/D2H/Host copy 记录 bytes、count 和 latency。
`zero_copy=true` 或 external/remote tensor 请求必须返回 Unsupported。remote tensor 独立后续计划验收后再改能力表。

## 5. CPU/iGPU 测试

- CPU job：公开 runner执行 load→reshape→infer→compare；
- iGPU job：自托管 runner从最终 OCI 执行同一 fixture；
- 断言 RuntimeCapabilities 只包含 live probe 设备；
- GPU plugin/`/dev/dri` 缺失时 readiness 失败且诊断可操作；
- CPU/GPU 结果按 fixture tolerance 比对并记录最差 tensor/index。

## 6. 完成条件

- [ ] `device` 配置唯一、兼容迁移明确。
- [ ] CPU+iGPU capability 来自 live runtime。
- [ ] iGPU 真实模型精度回归通过。
- [ ] Host copy 诚实计量，未宣称 remote/zero-copy。

