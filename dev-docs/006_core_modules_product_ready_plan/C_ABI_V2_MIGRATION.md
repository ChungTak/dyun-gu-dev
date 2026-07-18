# C ABI v1 → v2 迁移

## 1. 迁移原则

Plan 6 在主线立即停止 v1，不提供同库双 ABI。宿主必须把 header、动态/静态库、bindings 和调用代码原子升级。
v2 的目标是消除非法 enum UB、thread-local 数据悬空和外部内存生命周期不明确，而不是保持源码兼容。

## 2. 制品变化

| v1 | v2 |
|---|---|
| `libdg_capi.so`/ABI 1 | `libdg_capi.so.2`/ABI 2.0 |
| C enum 作为输入参数 | `int32_t` 输入，Rust 内部验证 |
| NUL string/裸 pointer+length | `DgStringView`/`DgByteView`/`DgShapeView` |
| `dg_last_error()` thread-local pointer | per-call `DgError **out_error` + `dg_error_free` |
| `LAST_DATA` 返回 pointer | `DgOwnedBytes` + accessors/free |
| empty external drop guard | `DgExternalMemoryV2.release/user_data` |
| best-effort `void *_free` | 可失败的显式 destroy/shutdown |
| 部分 struct 只有 `struct_size` | 所有 public struct 有 size + version |

## 3. 调用迁移

### 字符串与字节

调用方构造 view，并保证内存在调用返回前有效。v2 不扫描 NUL，允许非 NUL 结尾 UTF-8；路径仍不能含 NUL。
任何 view 的 `len > 0` 时 data 必须非空。

### 错误

每次 fallible 调用：

1. 把 `DgError *error = NULL`；
2. 传 `&error`；
3. status 非 OK 时读取 code/category/operation/message；
4. 调用 `dg_error_free(error)`。

错误 handle 不会被下一次 ABI 调用覆盖。

### 返回数据

metrics、capabilities 和 tensor snapshot 返回 `DgOwnedBytes *`。通过 data/len accessor 读取，使用完调用
`dg_owned_bytes_free`。不得用 libc `free`。

### 外部内存

- FD：library duplicate，caller 继续管理原 FD。
- raw handle：caller 在调用前转移/增加一个引用，并提供 release callback。
- import 失败时 caller 保留引用；成功后 callback 由 library 最终调用一次。
- callback 不得重入同一 engine/backend/tensor，不得抛出 C++ exception。

### Engine 销毁

先 request_stop，再调用带 timeout 的 destroy。timeout 时 handle 仍有效，修复外部阻塞后重试；不能直接释放
内存并留下 worker。

## 4. Binding 更新

- C++ RAII wrapper 为 `DgError`、`DgOwnedBytes`、Engine/Backend/Tensor/Buffer 分别提供 move-only owner。
- Python/Go/Rust binding 不缓存 view pointer 超过 owner 生命周期。
- callback trampoline 保存语言 runtime 所需的 stable user_data，并处理线程 attach；不得从 callback panic/throw。
- 绑定版本必须检查 ABI major==2 和 capability schema version。

## 5. 升级检查

```text
[ ] 构建只链接 libdg_capi.so.2
[ ] 编译使用候选制品内 dg_capi.h
[ ] 所有 enum 输入改为 int32_t 常量
[ ] 所有 string/bytes/shape 改为 view
[ ] 删除 dg_last_error/LAST_DATA 指针缓存
[ ] owned bytes/error 均 exactly-once free
[ ] external raw handle 提供 release callback
[ ] engine/backend 使用显式 destroy 并处理 Busy/Timeout
[ ] C11/C++17 smoke、ASan/LSan 和故障测试通过
```

## 6. 回滚

只能回滚到上一份完整 v2 宿主 + header + library 制品。v1 宿主不能只替换为 v2 library，v2 宿主也不能加载
v1 library。具体演练见 `ROLLBACK.md`。

