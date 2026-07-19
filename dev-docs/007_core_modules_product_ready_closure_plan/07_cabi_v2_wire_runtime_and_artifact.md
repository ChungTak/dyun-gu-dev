# 07. C ABI v2 Wire、Runtime 与制品闭环

> 需求 ID：CORE7-07

## 1. 版本决策

当前 v2 从未形成 Accepted 制品，因此在首次发布前直接完成 v2，不保留当前不完整签名的兼容符号，也不新增
v3。最终制品只发布：

- ABI major 2；
- `dg_capi.h`；
- `libdg_capi.so.2`、开发 symlink 和静态库；
- ABI snapshot、pkg-config、C11/C++17 examples；
- `C_ABI_V2_COMPLETION.md`。

## 2. Structured Version

用带 prefix 的输出 struct 替换静态 C string ABI version：

```c
typedef struct {
    uint32_t struct_size;
    uint32_t struct_version;
    uint32_t major;
    uint32_t minor;
} DgAbiVersion;
```

`dg_abi_version(DgAbiVersion *out, DgError **out_error)` 失败时先清零输出。package version 可返回
`DgStringView`，不要求 NUL。

## 3. 统一 View

所有 borrowed 输入改为：

- graph config/path/node/kind/params/connection/options：`DgStringView`；
- model/tensor bytes：`DgByteView`；
- shape：`DgShapeView`。

规则：

- `len/rank > 0` 时 data 非空；零长允许空指针；
- 在构造 Rust slice 前检查 hard max、乘法和平台转换；
- UTF-8 string 按显式长度读取；
- config 可包含非 NUL 结尾数据；path 拒绝内嵌 NUL；
- 删除产品 API 的 bounded `strlen` 扫描路径。

`DgError` 的 category/operation/message accessor 返回 `DgStringView`，生命周期绑定 error handle。

## 4. Runtime Options

`DgRuntimeInitOptions` 包含 struct prefix 和 process policy fixed-width 字段；转换规则见 CORE7-02。
engine/backend/tensor/buffer 创建均读取已初始化 policy。init 前调用 create 的行为固定为二选一：

- 自动以默认 policy 幂等初始化；或
- 返回 NotInitialized。

本计划选择前者以保持现有默认易用性；之后用不同 options 调用 init 返回 AlreadyInitialized。

## 5. Owned 与 Handle

保留并补测：

- `DgOwnedBytes`/`DgError` 跨调用稳定、exactly-once free；
- external raw callback/FD duplicate；
- engine destroy timeout/Busy/retry；
- Engine/Backend/Tensor/Buffer 的 Arc-backed concurrency；
- 失败、Again、EOS 时所有 output pointer/count 先清零；
- callback 不在 engine/backend lock 下执行。

`free` 与同一 handle 的并发调用仍属于 caller error；文档和 C++ wrapper 明确这一点。

## 6. ABI 与制品验证

测试不能只搜索 header 文本，必须：

1. 生成 header 后 `git diff --exit-code`；
2. C11/C++17 `-Werror` 编译、链接并运行所有 examples；
3. 从动态库读取实际 exported symbols，与精确 allowlist 比较；
4. `readelf/objdump` 断言 SONAME 为 `libdg_capi.so.2`；
5. package 中同时存在 `.so.2`、开发 symlink、header、pkg-config 和 examples；
6. examples 使用 package 内 header/library，不引用工作树 target；
7. ASan/LSan 下循环 create/import/run/destroy。

## 7. Fuzz

更新 C ABI fuzz target 使用 view/runtime options/external descriptor。覆盖 null/zero/huge/unaligned-valid-address、
short/long struct、invalid UTF-8、unknown discriminant、output capacity 和 callback ownership。

fuzzer 只能传入其拥有的有效地址范围；不声称 C ABI 能验证任意野指针。

## 8. 完成条件

- [ ] 所有 borrowed string/bytes/shape 公开入口使用 view。
- [ ] ABI version、runtime options 和 struct prefix 合同稳定。
- [ ] owned/error/external/destroy/concurrency 测试完整。
- [ ] header、symbols、SONAME、C/C++ examples 与 package 来自同一 commit。
- [ ] 首次 Accepted v2 之前不存在旧不完整签名兼容义务。

