# AGENT.md — dyun-gu-dev 智能体编码规范

> 本文件是**智能体（AI Agent）与人类贡献者共同遵循**的编码与协作规范，服务于 `dyun-gu-dev`：一个用 Rust 编写的多芯片流式推理框架（OpenVINO / TensorRT / RKNN2 / Sophon）。
> 完整技术方案见 [docs/design.md](docs/design.md)。本规范中的约定若与 `docs/design.md` 冲突，以 `docs/design.md` 的架构决策为准，并同步修订本文件。

---

## 0. 首要原则（TL;DR）

1. **先读 `docs/design.md`**，理解分层架构与本次改动所属的 crate/里程碑，再动手。
2. **最小化改动**：只改与任务相关的文件；不做无关重构、不动无关模块与测试。
3. **`unsafe` 只允许出现在 `-sys` / FFI adapter crate**；对外抽象 crate 一律 `#![forbid(unsafe_code)]`。
4. **零拷贝优先**：跨 element / 跨设备传递图像与张量时优先共享 buffer 句柄，禁止无谓的 host 拷贝。
5. **不静默失败**：能力不支持、后端不可用、精度不匹配都要显式报错并定位到具体节点/字段。
6. **提交前必须本地通过** `cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`（见 §9）。
7. **不提交非功能产物**（计划、临时脚本、截图、模型权重、生成的大文件）到源码树。

---

## 1. 仓库与目录约定

### 1.1 Cargo workspace 布局
项目是单一 Cargo workspace，crate 命名统一 `dg-` 前缀，按 [docs/design.md §4.2](docs/design.md) 划分：

| 层 | crate | 允许依赖方向 |
|----|-------|--------------|
| 核心抽象 | `dg-core` | 不依赖任何其他 `dg-*` |
| 运行时 | `dg-runtime` | 仅 `dg-core` |
| 后端 | `dg-openvino(-sys)` / `dg-tensorrt(-sys)` / `dg-rknn(-sys)` / `dg-sophon(-sys)` | `dg-core` / `dg-runtime` |
| 调度 | `dg-scheduler` | `dg-core` / `dg-runtime` |
| 编排 | `dg-graph` | `dg-runtime` / `dg-scheduler` |
| 媒体/流 | `dg-media` / `dg-stream` | `dg-core` / `dg-graph` + 外部依赖 |
| 上层 | `dg-elements` / `dg-capi` / `dg-cli` | 按需 |

**依赖只能自下而上**：底层 crate 不得依赖上层 crate；出现反向依赖需求时，说明抽象放错了层，应把接口下沉到 `dg-core`/`dg-runtime`。禁止引入循环依赖。

### 1.2 `-sys` 与安全封装分离
每个厂商后端拆成两个 crate：
- `dg-<backend>-sys`：仅 FFI 绑定与链接（`bindgen` 生成的 bindings + `build.rs` 定位/链接 SDK）。**唯一允许 `unsafe` 的地方之一**。
- `dg-<backend>`：安全封装。用 RAII 管理 C 资源、转换错误码、标注 `Send/Sync`、实现 `InferBackend` trait。对上层只暴露安全 API。

### 1.3 文件与模块
- 每个文件聚焦单一职责，避免超大文件；公共类型与 trait 放在 crate 的 `lib.rs` 或清晰命名的模块中导出。
- 导入语句一律置于文件顶部，禁止函数/块内 `use`（除非 `#[cfg]` 条件编译或测试模块内确有必要）。
- 生成物（bindgen bindings、cbindgen 头文件）若提交入库，须放在约定目录并在 `build.rs` 注释说明再生方式；不要手改生成文件。

---

## 2. Rust 语言与风格

- **Edition / 版本**：以 workspace 根 `Cargo.toml` 的 `[workspace.package]` 为准；新增 crate 继承 workspace 的 edition / lints / 依赖版本，不各自为政。
- **命名**：类型/trait `UpperCamelCase`，函数/变量/模块 `snake_case`，常量 `SCREAMING_SNAKE_CASE`。缩写按 Rust 惯例（`Id`、`Npu`、`Fp16`）。
- **格式化**：统一用 `rustfmt`（默认配置，若有 `rustfmt.toml` 以其为准），不手工排版。
- **lint**：`clippy` 必须零告警（CI 用 `-D warnings`）；可在选定 crate 开启 `clippy::pedantic` 并对个别规则 `#[allow]` + 注释理由。
- **禁止的偷懒写法**：不使用 `unwrap()`/`expect()` 于库代码正常路径（测试、`build.rs`、`main` 启动期可酌情）；不使用 `as` 做可能截断的数值转换（用 `TryFrom` / `try_into`）；不用 `Any`/反射式访问绕过类型系统。
- **可见性**：默认最小可见性；对外 API 显式 `pub`，内部实现用 `pub(crate)`。公共 API 变更需在 PR 描述中说明。

---

## 3. 类型系统与张量约定

遵循 [docs/design.md §5、§6](docs/design.md)：
- 数值类型统一用 `dg-core` 的组合式 `DataType(code/bits/lanes)` 表达（fp32/fp16/bf16/fp8/fp4/int16/uint16/int8/uint8/int4）；**不要**在各后端各自定义平行的数据类型枚举。
- 量化张量必须携带量化元信息（`scale` / `zero_point` / 量化方案 / axis）；RKNN 等后端不得只传 dtype 而丢弃 `zp/scale`。
- 亚字节类型（int4/fp4）以 packed 形式存储，逻辑元素数与物理字节数分别记录；pack/unpack 走 `dg-core` 统一实现并有属性测试。
- 布局用 `DataFormat`（NCHW/NHWC/…）+ `strides`，保留 RKNN 的 `w_stride/size_with_stride` 语义。

---

## 4. 内存、Buffer 与零拷贝

零拷贝是本项目的核心性能约束（[docs/design.md §6.5](docs/design.md)）：
- 图像/张量在 element 与后端之间传递时，通过统一 `Buffer` 句柄**共享**底层内存（dma-buf fd / CUDA ptr / MppBuffer / device addr），而非复制。
- 外部内存导入用 `Buffer::from_external` + `ExternalDropGuard` 托管生命周期与引用计数；不得裸存 C 指针而不管理所有权。
- 仅当源内存域与目标后端不兼容（跨卡/跨异构设备）时才走 staging 拷贝，并在日志中标注实际路径与拷贝次数。
- 新增数据搬运路径时，默认实现零拷贝分支 + staging 兜底，并说明何时走哪条。

---

## 5. 错误处理与日志

- 每个 crate 用 `thiserror` 定义分层错误枚举；跨 crate 边界转换用 `#[from]` 或显式 map，**保留上下文**（哪个后端、哪个节点、原始错误码）。
- 对外统一 `pub type Result<T> = std::result::Result<T, Error>`。库代码返回 `Result`，不 `panic!`（除非违反不可能发生的内部不变量，且注明）。
- FFI 返回码在 `dg-<backend>` 层转换为具体 `Error` variant，保留原始整型码。
- 日志用 `tracing`（结构化字段），不用 `println!`/`eprintln!`。关键路径记录设备/核心、精度、队列深度、吞吐/时延、零拷贝或 staging 的实际路径。
- 不记录密钥、令牌、完整帧数据等敏感/巨量内容。

---

## 6. 并发与图执行

遵循 [docs/design.md §8](docs/design.md)：
- element 的核心逻辑遵循 **Sans-I/O**：状态机/调度/张量运算不直接做阻塞 I/O，I/O 由 driver/adapter 注入。
- 提交-轮询采用 `Poll{Ready/Pending/EndOfStream}` 非阻塞模型，配合背压（有界队列，满则软阻塞并上报事件）。
- 共享状态优先用消息传递 / 有界 channel；必须共享可变状态时用最小粒度锁，避免在持锁期间跨 await/阻塞调用。
- 线程与 DataPipe 的对应关系、`ParallelType`（Sequential/Task/Pipeline）遵循设计文档，不自创并行模型。

---

## 7. 后端适配约定

新增或修改后端（`dg-<backend>`）时：
- 必须实现 `InferBackend` trait（`init/reshape/num_inputs/num_outputs/input_info/output_info/run`），并通过静态注册进入全局工厂（`inventory`/`ctor` 风格），不在上层写 `match backend {}` 硬编码分支。
- 后端专属参数放在对应的 `RuntimeOption` / `InferenceParam` 派生结构，不污染通用配置。
- **能力探测**：在 `init` 时查询 SDK 版本与设备能力，校验请求的精度/核心/内存模式是否可用，不支持则清晰报错（含建议），不静默降级。
- 每个后端提供最小可跑 sample + 精度回归基线。

---

## 8. 配置模型（GraphSpec）

遵循 [docs/design.md §8.3](docs/design.md)：
- 只维护一份强类型 `GraphSpec`（`serde` derive）；外部格式 YAML（默认）/JSON/TOML 互转必须无损（属性测试保证 round-trip）。
- 节点用**具名字符串 id**，端口用具名端口，连线用声明式 `edges: ["a.out -> b.in"]`；禁止引入魔法数字节点 id。
- 加载期做校验：未知字段拒绝、DAG 无环、端口连通、后端/精度 preflight；错误定位到具体节点/字段。
- 新增节点类型时同步更新 schema 导出与示例配置。

---

## 9. 测试与质量门禁

- **提交前本地必跑**（CI 同样执行）：
  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  涉及交叉编译目标时补 `cargo check --target <triple>`（见 [docs/design.md §10.2](docs/design.md)）。
- **测试类型**：
  - 单元 + 集成测试：硬件相关逻辑用 **mock 后端**，保证无硬件 CI 可跑。
  - **属性测试**（`proptest`）：张量 pack/unpack、DataType/DataFormat 转换、配置 round-trip、图 diff/热更新不变量。
  - **fuzz**（`cargo-fuzz`）：配置解析、C ABI 边界入参、模型/码流解析等不可信输入面。
  - 精度回归：固定输入比对后端输出与参考（余弦相似度阈值）。
- **不得为了让测试通过而修改测试**（除非任务本身就是修正错误的测试，并说明理由）。发现测试/需求不合理时，提出而非绕过。
- `cargo-deny` 检查许可证/漏洞/供应链；新增依赖优先选发布 ≥7 天、无 floating range 的版本。

---

## 10. FFI 与 C ABI

- `unsafe` 集中在 `-sys` 与 FFI adapter；每个 `unsafe` 块须有注释说明其安全前提（invariant）。
- `dg-capi` 导出的 C ABI：句柄用不透明指针 + 显式 `*_free`，跨 ABI 不暴露 Rust 类型；错误以 `DgStatus` 整型 code + 可选的 `DgError **out_error` 返回（调用方负责 `dg_error_free`），不再使用线程局部 `dg_last_error()`；字节/字符串/诊断输出通过 `DgOwnedBytes **out` 句柄返回（调用方负责 `dg_owned_bytes_free`）。头文件由 `cbindgen` 生成，作为一等交付物随接口变更同步更新。
- 修改 C ABI 属于破坏性变更，须在 PR 中显著标注并更新示例与头文件。

---

## 11. Git、提交与 PR 规范

- **分支**：从最新 `main` 切出，命名 `devin/<timestamp>-<short-topic>` 或团队约定；一个 PR 聚焦一件事。
- **提交信息**：使用祈使句、类型前缀（`feat:`/`fix:`/`docs:`/`refactor:`/`test:`/`chore:`），标题简洁；正文说明「为什么」。不要把「修复上一版」类信息写进代码注释。
- **禁止**：`git add .`（可能带入无关文件）；提交 `.env`/凭据/模型权重/大二进制；`--no-verify` 跳过钩子；`push --force` 到 `main`；修改 git 配置或安全策略来绕过 CI。
- **PR 描述**：高信息量、面向没看过 diff 的读者，突出「改了什么、为什么」；接口变更给出伪代码/示例；不复述可从代码轻易看懂的内容。
- 若仓库启用 pre-commit 钩子（`.pre-commit-config.yaml`），先 `pre-commit install` 再提交。

---

## 12. 注释与文档

- 默认少注释、靠好命名；只在「为什么这么做 / 非显然的约束 / unsafe 前提」处注释，不写解释 diff 的注释。
- 公共 API 用 `///` doc 注释说明用途、参数、错误与 panic 条件；复杂模块加模块级 `//!` 概述。
- 架构层面的变更同步更新 [docs/design.md](docs/design.md)，保持文档与代码一致。

---

## 13. 智能体专属注意事项

- 动手前明确任务属于哪个里程碑（M0–M6）与 crate；跨里程碑或偏离 `docs/design.md` 的改动，先与维护者确认。
- 遇到缺失的凭据、SDK、目标硬件等外部阻塞，明确上报而非猜测或伪造实现。
- 不确定库/接口是否可用时，先在邻近代码、`Cargo.toml`、依赖仓库中核实，不臆测 API。
- 保持改动可复核：小步提交、清晰 PR、附带测试与本地验证结果。
