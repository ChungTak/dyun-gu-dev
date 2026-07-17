# 09. C ABI v1 与资源合同

## 1. ABI 冻结前整改

当前版本仍为 0.1，可在首次 production 前完成一次明确 ABI 收敛。所有 extern enum 入参改用 `int32_t` 并在 Rust
内部校验，避免 C 传入未知 discriminant。返回 struct 增加首字段 `struct_size`，提供 ABI major/minor 查询。

Opaque pointer 继续作为句柄；非空且来自对应 create 是调用者安全前提。Rust 校验 handle state、长度、rank、UTF-8、
output capacity 和调用顺序。`InvalidHandle` 的可保证范围必须与头文件一致，不宣称能安全识别任意坏地址。

## 2. 新增接口

- `dg_runtime_init(options)`：配置 limits、初始化内置 registry/connector，幂等；
- `dg_engine_request_stop` / `dg_engine_shutdown(timeout_ms)`；
- `dg_engine_status` / `dg_engine_metrics_json`；
- `dg_abi_version` / `dg_build_capabilities_json`。

返回字符串/数据的所有权和有效期逐函数写入 header；线程局部 last error保持，但增加稳定 error code/category。

## 3. 线程与重入

同一 Engine handle默认非并发可变；并发调用返回 Busy，而不是数据竞争。stop/status/metrics允许与 run 并发。
free 前必须 shutdown；free 仍做取消兜底。callback 不在内部锁下调用。

## 4. 兼容与测试

- 更新 cbindgen header、ABI snapshot和 C examples；
- C 编译/link/run smoke使用 release `libdg_capi.so`；
- 未知 enum、空指针、溢出 length/rank、错误状态序列、并发 stop、重复 shutdown；
- libFuzzer覆盖有效地址范围内的 load/options/size组合；
- Linux x86_64 检查导出 symbol 和 SONAME。

## 5. 完成条件

- [ ] ABI v1 版本和兼容规则文档化。
- [ ] 生命周期/metrics/初始化可从 C 使用。
- [ ] 非法值不会形成 Rust enum UB 或越界分配。
- [ ] header、snapshot、example、动态库来自同一 commit。

