# C ABI v2 完成与首发合同

## 1. 当前结论

Plan 6 已实现 external release callback、owned error/result 的部分路径和可失败 destroy，但不能据此把整个公开面
视为 v2 Accepted。当前仍需完成 borrowed views、runtime options/process policy、结构化版本、package 级
header/library/symbol/SONAME/examples 一致性。

Plan 7 不保留一个“部分 v2”兼容层。首次对外发布前允许修正尚未 Accepted 的签名；发布 Accepted 的
`libdg_capi.so.2` 后，任何破坏性变更必须走新的 ABI major。

## 2. 公开数据合同

| 类别 | v2 完成态 |
|---|---|
| ABI version | 结构化 major/minor/patch 与 capability schema version；宿主先校验 major |
| string/bytes/shape 输入 | `DgStringView`、`DgByteView`、`DgShapeView`，不扫描 NUL |
| enum/tag 输入 | 固定宽度整数，Rust 边界验证 unknown discriminant |
| public struct | 首字段为 `struct_size` 与版本；尾部扩展；reserved 必须为零 |
| 运行初始化 | `DgRuntimeInitOptions` 携带 process policy、deadline、日志/指标选项 |
| 返回数据 | opaque owned handle + accessor/free，不返回会被下次调用覆盖的指针 |
| error | per-call owned `DgError`，稳定 code/category/operation/message/root cause |
| external memory | 明确引用转移、release callback、user_data、exactly-once 与线程约束 |
| destroy | request-stop + timeout；Busy/Timeout 后 handle 仍有效且可重试 |

任何 view 在 `len > 0` 时 `data` 必须非空；长度、乘法和 UTF-8/路径语义在读取前验证。view 只在调用期间借用，
library 不缓存 caller pointer。

## 3. Runtime options 与 Policy

`DgRuntimeInitOptions` 至少表达：

- struct/ABI 版本与 reserved；
- `ProcessRuntimePolicy` 的 config/model/tensor/frame/device/queue/output 上限；
- connect/recv/send/drain/cancel/shutdown deadline；
- metrics/logging 开关和有界 cardinality 选项；
- 可选 allocator/callback 的明确 owner、thread 和 reentrancy 合同。

C 入口与 CLI/Rust 入口必须生成同一内部 policy 类型。缺字段使用受信默认值；caller 请求只能下调部署硬上限。
未知 struct 尾部按版本规则处理，非法、冲突或超限字段返回 typed error，不能静默截断。

## 4. 所有权与并发矩阵

每个公开 handle 在 header 和生成文档中列出：

| 项 | 必须说明 |
|---|---|
| create/import 成功与失败 | 哪一方拥有输入、何时转移引用 |
| clone/borrow/accessor | 是否增加引用，返回 pointer 有效期 |
| free/destroy | exactly-once、是否可失败、失败后 handle 状态 |
| callback | 线程来源、允许阻塞时间、是否可重入、panic/exception 禁止 |
| 并发 | Send/Sync 等价语义、允许的并发调用、外部同步责任 |

测试必须覆盖 create 每个失败阶段、callback 恰好一次、destroy timeout 后重试、并发 stop/destroy/accessor、
NULL/zero-length/unknown tag/oversized length。

## 5. 制品布局

release archive 至少包含：

```text
include/dg_capi.h
lib/libdg_capi.so.2
lib/libdg_capi.a
lib/pkgconfig/dg-capi.pc
examples/c11/
examples/cpp17/
share/dyun/abi-manifest.json
share/dyun/LICENSES/
share/dyun/SBOM.*
```

实际平台可调整库后缀，但 manifest 必须记录 header hash、exported symbols、SONAME/install-name、target、features、
library hash 和 ABI version。examples 必须只依赖解压后的 archive 编译、链接、运行，不读取源码树。

## 6. 自动门禁

- cbindgen 输出与 committed header 无 diff；
- exported symbol allowlist 无缺失/意外符号；
- SONAME/install-name、pkg-config 和实际文件名一致；
- C11/C++17 dynamic 与 static smoke；
- header 可从 C/C++ 独立包含，size/alignment/offset snapshot 符合目标平台；
- view、unknown discriminant、size/version、overflow、owned error/result 的负向测试；
- ASan/LSan/TSan 与 Rust concurrency test 无报告；
- archive 解压到空目录后的 build/run smoke；
- ABI manifest、package digest 与 `CORE7_PRODUCT_ACCEPTANCE.md` 同一候选身份。

## 7. 宿主迁移检查

```text
[ ] 只加载 Accepted package 中的 libdg_capi ABI major 2
[ ] 使用同一 package 内的 dg_capi.h 与 pkg-config
[ ] ABI major/capability schema 在初始化前通过
[ ] 所有字符串、字节、shape 改为 view，且借用期不越过调用
[ ] 所有 enum/tag 使用固定宽度整数并处理 Unsupported/InvalidArgument
[ ] runtime init 显式提供或接受受信默认 process policy
[ ] owned bytes/error/handle exactly-once free/destroy
[ ] external raw handle 提供 release callback 并遵守线程/重入合同
[ ] Busy/Timeout destroy 保留 handle 并在解除阻塞后重试
[ ] C11/C++17 package smoke 与 sanitizer 通过
```

## 8. 回滚

只能回滚到上一份完整、Accepted 的 v2 宿主 + header + library + policy 制品。不得让 v1/v2 或候选/前一版本的
header、library、bindings 混用。演练与触发条件见 [ROLLBACK.md](ROLLBACK.md)。
