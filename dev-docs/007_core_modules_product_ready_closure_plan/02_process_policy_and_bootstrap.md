# 02. 可信进程 Policy 与 Bootstrap

> 需求 ID：CORE7-02

## 1. 组合策略

新增可信的 `dg_core::ProcessRuntimePolicy`，组合：

- `ResourcePolicy`：config/include/node/connection/model/tensor/frame/buffer；
- `MemoryPoolConfig`：cache bytes、entries、per-descriptor entries；
- `StreamRegistryLimits`：stream、subscriber、bootstrap bytes/frames、idle TTL；
- `DeadlinePolicy`：connect、recv poll、I/O、drain、shutdown；
- affinity capacity/TTL 与 metrics serialization 上限。

只有进程 bootstrap 能创建 hard policy。GraphSpec `limits` 继续保持 `dg/v1`，仅能下调图级
`ResourcePolicy` 字段；不能修改 cache、registry 或全局 deadline hard maximum。

## 2. CLI Bootstrap

`dg run`、`dg validate` 和 `dg demo` 增加：

```text
--runtime-limits <path>
```

文件使用严格 serde schema，支持 YAML/JSON/TOML，拒绝未知字段、零值、平台不可表示值和非法字段关系。
CLI 必须先加载 process policy，再用它加载/验证 GraphSpec，并调用 `Graph::new_with_policy`。不提供该参数时
使用安全默认值。

runtime limits 属于可信启动配置：

- `--watch` 不监视该文件；
- 运行期变更要求进程重启；
- 日志记录 hash 和非敏感 effective 值，不记录私有 endpoint；
- `validate` 与 `run` 使用完全相同的 policy，不允许 validate 通过而 run 使用更小上限。

## 3. Rust 传递

- `Graph`/`RunningGraph`/`ElementIo` 持有同一 `Arc<ProcessRuntimePolicy>`。
- `RuntimeOption` 携带只读 policy；`Runtime::new_with_policy` 在 backend init 前注入。
- inference pool 的每个 Runtime 使用同一 policy 和 metrics handle。
- Device/Allocator、media/stream bridge、algorithm 和 C direct backend 从上下文取得 policy，不调用
  `ResourcePolicy::default()` 绕过部署配置。
- 保留无参便利构造器时只能创建安全默认 policy，并在 docs 中标注不适合受控生产 bootstrap。

## 4. C Bootstrap

扩展 `DgRuntimeInitOptions`，通过 fixed-width 字段表达所有 process hard limits。规则：

- null options 使用安全默认值；
- 非 null 时 `struct_size` 至少覆盖本版本必需前缀，`struct_version` 必须受支持；
- 所有数值显式非零，Rust 侧用 checked conversion；
- 完全相同的重复初始化返回 Ok；
- 不同配置返回稳定的 AlreadyInitialized/InvalidState 错误，不静默保留首次配置；
- `dg_engine_create`、direct backend、tensor/buffer import 都读取已安装 policy。

不允许某个 C API 单独回退到 `ResourcePolicy::default()`。

## 5. Effective 规则

```text
effective graph field = min(process hard limit, graph requested limit)
```

Graph 请求高于 hard limit 时加载失败并指出字段、requested、hard。reload：

- 提高到 hard 以内允许候选验证；超过 hard 直接拒绝；
- 降低仅约束新输入/新分配，不破坏已持有资源；
- bootstrap policy 不参与 hot reload；
- 失败保持旧图和旧 effective policy。

## 6. 测试

- CLI default/三种格式/未知字段/不可表示值；
- validate/run 使用相同 policy；
- C null/default、短/长 struct、相同/不同重复 init；
- Rust/CLI/C 三入口计算相同 effective policy；
- Graph 不能提高 process hard limit，降低后新 frame/tensor 立即受限；
- 代码扫描禁止产品路径出现无上下文的 `ResourcePolicy::default()`；
- 32-bit target 对 byte/count/deadline conversion 明确失败。

## 7. 完成条件

- [ ] process policy 只有可信 bootstrap 可设置。
- [ ] CLI、Rust、C 和 Graph 使用同一策略对象。
- [ ] GraphSpec 只能下调，reload 不可替换 process hard policy。
- [ ] cache/registry/deadline 与资源上限均有默认值和验证。
- [ ] 产品路径不存在隐式 unlimited 或 default-policy 绕过。

