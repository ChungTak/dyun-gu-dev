# 02. 资源权限与不可信输入上限

> 需求 ID：CORE6-02

## 1. 权限模型

新增进程级 `dg_core::ResourcePolicy`。它由可信 bootstrap 创建，并在进程生命周期内不可被图、网络输入或模型修改。
`GraphSpec.limits` 保持 `dg/v1` 兼容，但只能收紧进程硬上限：

```text
effective_limit = min(process_hard_limit, graph_requested_limit)
```

GraphSpec 请求高于硬上限时加载失败，并返回字段路径、请求值和硬上限。不能静默 clamp，否则运维无法发现配置错误。
Rust 默认构造器使用安全默认 policy；CLI 使用独立可信 runtime-limits 文件或默认值；C ABI v2 通过
`DgRuntimeOptionsV2` 初始化。

## 2. 默认上限

已发布字段保持现有默认值：

| 限制 | 默认值 | 执行位置 |
|---|---:|---|
| config 累计字节 | 8 MiB | 根配置、全部 include、string/file 入口 |
| include depth/count | 16 / 64 | canonical include resolver |
| graph nodes/connections | 1024 / 8192 | normalize/validate 前后 |
| 单 tensor/frame | 512 MiB | copy、allocate、import、bridge 前 |
| 单 model artifact | 2 GiB | metadata/read/copy/backend init 前 |

同一 8 MiB config 预算覆盖根文件和所有 include 的累计原始字节，防止每个 include 单独满足限制但总量失控。
节点上 `threads` 展开后的 worker 总数不得超过 `max_nodes`；`execution.queue_capacity` 不得超过
`max_connections`，且 runtime 仍按 packet bytes 计入有效内存预算。

## 3. 接口与传递

- `ResourcePolicy::new(hard_limits)` 验证非零、平台可表示性和字段关系。
- `ResourcePolicy::effective_for(&GraphSpec::limits)` 生成不可变 effective policy。
- `Graph::new_with_policy`、`Runtime::new_with_policy` 和 element create/run context 持有同一 `Arc<ResourcePolicy>`。
- `Graph::new`、`Runtime::new` 使用默认硬上限，不提供无限制隐式路径。
- backend `RuntimeOption` 携带 policy；vendor backend 读取文件或分配 I/O 前必须使用它。
- `ElementIo` 提供只读 policy，用于 source、media、stream、algorithm 和 sink 的运行期输入。

`dg-core` 不依赖 GraphSpec；Graph 层负责把序列化的 `ResourceLimits` 转成 core policy。Graph 可继续
re-export 配置类型，避免无关 Rust 调用方迁移。

## 4. 配置与模型

文件加载先检查 metadata，再用 `take(limit + 1)` 或等价限长读取确认实际字节，避免 TOCTOU 和稀疏/伪 metadata。
string API 必须显式接收长度并在 parse 前检查。include resolver 使用候选图的 effective depth/count，
维护 canonical path、累计文件数和累计字节；循环、重复引用和 rename-save 行为保持确定。

模型规则：

- `ModelSource::Bytes` 在 clone 前检查；
- `ModelSource::File` 在 read/mmap 前检查 metadata 和实际读取量；
- OpenVINO IR 的 XML、BIN 及相关 artifact 按总模型预算计算；
- backend 产生的 input/output metadata 在 tensor 分配前再校验 shape、stride 和 physical bytes；
- C API direct backend 不得先把超大输入 `.to_vec()` 再检查。

## 5. Tensor、Frame、队列与输出

- tensor logical bytes、physical stride bytes 和外部 buffer size 都不得超过 effective tensor limit。
- encoded packet 和 decoded image buffer 都按 frame limit；codec config 仍同时受 item/count/total 限制。
- connector 在把上游 payload clone 到 `Vec`/`Bytes` 前执行 frame limit。
- bounded queue 同时记录 packet count 和估算 payload bytes；超过预算按元素策略 backpressure 或 typed limit error。
- sequential collector 达预算后返回 `ResourceLimit`，不得因没有并发 consumer 而永久 Full 自旋。
- sink、C pending input/output、algorithm candidate/result 和 metrics serialization 均使用相同预算框架。

## 6. 测试

- 每个数字限制覆盖 `limit-1`、`limit`、`limit+1`。
- 使用计数 allocator/mapper 断言超限时未发生分配、复制、文件完整读取或 callback 转移。
- 多 include 单文件合规但累计超限；配置提高硬上限失败，降低后生效。
- 超大/溢出 shape、stride、rank、queue、model、frame、codec config 和 C length。
- 32-bit target 的 2 GiB/usize 边界只允许可表示配置；不可表示时 bootstrap 明确失败。
- reload 试图提高 limit 时旧图继续运行；降低 limit 对新输入立即生效，对已持有资源不追溯破坏。

## 7. 完成条件

- [ ] 进程硬上限与 GraphSpec 下调语义固定并文档化。
- [ ] 所有公开 limit 在真实消费资源前执行。
- [ ] file/string/Rust/C/stream/backend 入口使用同一 effective policy。
- [ ] 无隐式 unlimited、静默 clamp 或先分配后拒绝路径。
- [ ] 边界、累计、reload 和 32-bit 测试通过。
