# 01. 当前状态、风险台账与接纳门禁

> 需求 ID：CORE6-01

## 1. 基线采集

每次开始 CORE6 实施或生成候选制品时保存：

```bash
git status --short
git rev-parse HEAD
git log -5 --oneline
rustup show
rustc --version --verbose
cargo --version --verbose
sha256sum Cargo.lock
cargo metadata --locked --no-deps --format-version 1
```

基础门禁：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
git diff --exit-code Cargo.lock
```

记录测试数量、ignored/skipped 数量、执行时长和 artifact URL。默认 workspace 通过只证明 SDK-free 路径，
不得替代 optional feature、协议、真实 backend 或硬件结果。

## 2. 审计方法

每个核心 crate 至少完成以下审计，并把发现写入 `CORE_RISK_REGISTER.md`：

1. **输入面**：配置、模型、shape、stride、frame、metadata、C pointer/length、URL 和 callback。
2. **资源面**：分配、复制、缓存、队列、collector、worker、affinity、metrics 和 retry。
3. **并发面**：锁顺序、poison、阻塞 I/O、取消、Drop、reload、shutdown 与 callback 重入。
4. **FFI 面**：非法判别值、整数转换、slice 构造、句柄状态、panic 边界和所有权。
5. **失败面**：是否静默 fallback、吞错、返回空数据、丢根因、泄漏资源或留下半更新状态。
6. **证据面**：单元/属性/fuzz/故障注入/长稳是否能复现和关闭风险。

审计条目必须包含文件、symbol、当前行为、最小复现、影响、目标行为、测试和关闭 commit。

## 3. 已确认事实

- `max_config_bytes` 在文件路径加载时有默认值和配置值检查，但 `from_str_with_format`/C string 入口不执行。
- include depth 使用默认常量，未使用 GraphSpec 中配置的 `max_include_depth`。
- `max_tensor_bytes`、`max_frame_bytes`、`max_model_bytes` 没有传入实际分配、导入、bridge 和 backend 文件读取。
- `StreamPullElement` 调用 `recv_blocking()`；真实 connector 卡住时 worker 无法观察 stop。
- inference pool 只把第一个 Runtime 的 `BackendMetrics` 挂到 element。
- backend latency 将每次观测存入 `Mutex<Vec<u64>>`，长流会持续增长。
- scheduler 和 InstancePool 的 stream affinity `HashMap` 无容量或 TTL。
- sequential/task 路径使用 unbounded pipe；sink collector 和部分输出队列也无产品预算。
- `PipeReceiver::try_recv()` 不递减 depth，reload route drain 可污染队列指标。
- `Buffer::read_bytes()` 对 device-only external buffer 返回空向量，多个 backend/element 仍使用该接口。
- `TensorDesc::storage_bytes()` 只按逻辑 shape 计算，未覆盖 stride/padding；contiguous stride 计算使用饱和乘法。
- C 外部 tensor/buffer 导入使用空 `ExternalDropGuard`；调用方提前释放可造成 use-after-free。
- C enum 参数在函数入口已是 Rust enum，之后再 `as i32` 校验不能消除非法判别值 UB。
- `LAST_DATA`/`LAST_ERROR` 指针由后续 ABI 调用覆盖；返回数据没有独立所有权。
- scheduler `Lease` getter 在 lock poison 时 `expect` panic；graph 只在 element worker 边界 catch panic。
- stream bridge 存在复制前无统一 frame limit、ID 饱和转换和 metadata 构造错误被忽略。

## 4. 风险等级

| 等级 | 定义 | 接纳规则 |
|---|---|---|
| P0 | 可导致 UB、UAF、数据破坏、无法停止或安全边界失效 | 立即阻塞后续 release；必须先修 |
| P1 | 可导致 OOM、长期泄漏、死锁、错误结果、不可观测故障 | 阻塞 product-ready 接纳 |
| P2 | 诊断、性能、兼容或低概率恢复缺陷 | 允许带有负责人和到期日的例外 |
| P3 | 清理、文档或非产品路径改进 | 进入 backlog，不阻塞本轮 |

风险从 Open 降级或关闭必须由 reviewer 核对最小复现与回归测试，不能只修改严重度。

## 5. 接纳门禁

软件核心接纳必须同时满足：

- clean tree，required checks 无 skip/soft-fail；
- P0/P1 全关闭，P2 例外有负责人、理由、到期日和监控；
- 资源、取消、外部内存、pool 指标和 C ABI v2 验收全部通过；
- nightly 2h 无 sanitizer、并发或 fuzz failure；
- release 24h soak 与性能阈值通过；
- ABI/schema/header/library/制品证据绑定同一 commit。

本计划的软件核心接纳不替代产品硬件矩阵。声明 OpenVINO iGPU、TensorRT、RKNN 或 Sophon production
仍必须引用对应实机证据。

## 6. 完成条件

- [ ] 当前基线和全部模块审计已保存。
- [ ] 每个已确认风险在台账中有唯一 ID、等级和 owner。
- [ ] P0/P1 的最小失败测试先于修复提交。
- [ ] required checks、nightly 和 release 门禁负责人明确。
- [ ] 接纳结论写入 `CORE_PRODUCT_ACCEPTANCE.md`。
