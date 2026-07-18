# 08. C ABI v2 所有权、线程与迁移

> 需求 ID：CORE6-08

## 1. 版本决策

主线直接切换 C ABI v2：

- `dg_abi_version` 返回 major `2`、minor `0` 的结构化结果；
- `dg_capi.h`、ABI snapshot、C examples、pkg-config 和发行包只包含 v2；
- Linux SONAME 为 `libdg_capi.so.2`；
- 不在同一库导出 v1 compatibility symbols，不提供 v1 维护制品；
- 调用方必须原子升级 header、library 和 bindings。

迁移表见 `C_ABI_V2_MIGRATION.md`。

## 2. Wire 类型

所有 C 输入枚举参数使用 `int32_t`，Rust 入口先转换并验证，再构造内部 enum。返回 status 可继续用固定宽度
`int32_t` 常量，禁止让未知 C 值直接形成 Rust enum。

统一 view：

```c
typedef struct { const uint8_t *data; size_t len; } DgByteView;
typedef struct { const char *data; size_t len; } DgStringView;
typedef struct { const size_t *dims; size_t rank; } DgShapeView;
```

view 只在调用期间借用。rank/len 在构造 Rust slice 前检查硬上限、null/zero 组合和乘法溢出。字符串按 UTF-8
显式长度读取，不要求 NUL；路径另拒绝内嵌 NUL。

所有 public struct 首字段为 `uint32_t struct_size`，其后为 `uint32_t struct_version`。库只读取 caller
声明且当前版本已知的前缀；小于必需前缀返回 InvalidArgument，大于当前 struct 可忽略尾部。

## 3. Owned result 与错误

删除 `LAST_DATA`、`LAST_ERROR`。新增 opaque：

```c
typedef struct DgOwnedBytes DgOwnedBytes;
typedef struct DgError DgError;
```

- metrics JSON、capability JSON、tensor snapshot 返回 `DgOwnedBytes **out`；
- `dg_owned_bytes_data/len` 在 handle free 前稳定，`dg_owned_bytes_free` 释放；
- 所有 fallible API 最后接受可空 `DgError **out_error`；
- `DgError` 提供 stable code/category、operation、message 和 source chain 的只读 view；
- `dg_error_free` 释放，错误 handle 与下一次 ABI 调用无关。

失败时所有 output handle/count 初始化为 null/0；成功时 `out_error` 保持 null。allocator 始终由同一 dg library
分配和释放，调用方不得 `free()` Rust 内存。

## 4. 外部内存

统一描述符：

```c
typedef void (*DgReleaseCallback)(void *user_data);

typedef struct {
    uint32_t struct_size;
    uint32_t struct_version;
    int32_t fd;
    uint64_t raw;
    int32_t domain;
    int32_t device;
    size_t size_bytes;
    DgReleaseCallback release;
    void *user_data;
} DgExternalMemoryV2;
```

规则：

- fd/raw 必须恰有一种有效；domain/device/size 在转移所有权前验证。
- FD import 成功后 duplicate，框架关闭 duplicate；caller 的原 FD 仍归 caller。
- raw handle 要求非空 release callback；调用前 caller 转移或增加一个引用。
- import 成功后 library 拥有该引用，最终 buffer/tensor clone drop 时 callback 恰好一次。
- import 失败不调用 callback，所有权仍归 caller。
- callback 可能在线程池或销毁线程执行，必须 thread-safe、不得抛异常或重入同一 handle。
- callback 在内部锁外执行；C++ binding 用 `noexcept` trampoline。

不提供“空 callback 但由 caller 保证更久”的 product API。

## 5. Handle 与线程

- opaque pointer 只有 null 和“由对应 create 返回、尚未 free”的指针是合法输入；ABI 不声称验证任意坏地址。
- Engine mutation 使用 try-lock，冲突返回 Busy；status/metrics 使用独立 snapshot，可与运行并发。
- direct backend handle 默认 exclusive mutable；并发 run/reshape/destroy 返回 Busy。
- tensor/owned bytes 只读查询可并发；free 与任何调用并发属于 caller error。
- callback 不在 engine/backend lock 下调用。

`dg_engine_destroy(engine, timeout_ms, out_error)` 返回 status，内部先 request_stop/shutdown；timeout 时 handle
仍有效、可重试 destroy。删除无法报告失败的 best-effort `void dg_engine_free` 产品语义。

## 6. Runtime options

`DgRuntimeOptionsV2` 包含 ABI struct prefix、ResourcePolicy 硬上限、可选 allocator/callback 配置和 flags。
process bootstrap 幂等：完全相同配置重复成功，不同配置返回 AlreadyInitialized，不静默保留首次值。

## 7. 验收

- C11/C++17 compile、link、run；动态库和 header 来自同一 artifact。
- 未知 enum、短/长 struct、null/zero view、huge rank/length、UTF-8、output capacity。
- error/owned bytes 跨多次 ABI 调用仍有效，free exactly once，错误路径输出为 null。
- external import success/failure、多 clone、并发 drop、callback thread、FD close。
- Engine Busy、并发 status/metrics/stop、destroy timeout/retry、重复 destroy 的 caller contract。
- libFuzzer 覆盖有效地址范围的 views/options/descriptor；ASan/LSan/UBSan-equivalent 无报告。
- symbol/version/SONAME snapshot 断言 v1 符号和 SONAME 不再发布。

## 8. 完成条件

- [ ] ABI v2 wire 类型不会接收非法 Rust enum。
- [ ] 返回数据和错误均有独立 owned handle。
- [ ] 外部内存无空 guard，callback 所有权 exactly once。
- [ ] destroy、并发和 Busy 合同可测试。
- [ ] v2 header/library/examples/SONAME 和迁移文档完整。
