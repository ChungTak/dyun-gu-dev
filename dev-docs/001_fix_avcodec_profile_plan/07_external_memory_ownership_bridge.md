# 07. 外部内存、零拷贝与所有权边界

## 1. Unsafe 隔离

`dg-media`、`dg-core` 保持 `#![forbid(unsafe_code)]`。仅 `dg-media-avcodec` 可将 crate 级别调整为 `#![deny(unsafe_code)]`，并在单一 `external` 模块局部 `#[allow(unsafe_code)]`。

该模块只允许：

- 构造 avcodec external Image/Packet；
- 将 `dg_core::Buffer` clone 转为 drop token；
- 在 drop callback 中还原并释放该 token；
- 验证后调用上游 unsafe constructor。

每个 unsafe block 前必须写明 allocation、size、thread-safety、aliasing 和 drop-once 不变量。

## 2. dg Buffer→avcodec 所有权

1. clone 源 Buffer，使其现有 ExternalDropGuard 在 avcodec 对象生命周期内存活。
2. `Box::into_raw(Box::new(clone))` 生成非零 token。
3. avcodec drop callback 只对该精确类型执行一次 `Box::from_raw`。
4. constructor 失败时必须回收 token，不能泄漏。
5. fd/raw handle 不拥有第二份资源，不能在两侧各自 close/free。

## 3. avcodec→dg Buffer 所有权

- dg ExternalDropGuard 捕获 avcodec BufferHandle clone。
- 原 Image/Packet drop 后，dg Buffer clone 仍保持资源有效。
- 最后一个 dg Buffer clone drop 时释放捕获 handle。
- Host-readable external memory与 device external memory使用不同构造路径。

## 4. 验证顺序

构造前按顺序验证：

1. Profile role 允许 source/target domain；
2. external handle 符合 domain（fd 或 raw 非空）；
3. buffer size 非零且不小于所有 slice/plane end；
4. pixel format plane count；
5. plane stride、effective row bytes、height 和 len；
6. visible/crop rect 在 coded bounds 内；
7. allow_staging 和目标 operation；
8. lifetime guard 存在。

任一步失败不得调用 unsafe constructor。

## 5. 零拷贝判定

只有以下条件全部成立才返回 `TransferMode::Shared`：

- source_domain == target role input domain，或命中上游声明的 direct domain transition；
- handle kind 被目标 backend 支持；
- layout 完全兼容；
- guard 能证明生命周期；
- 无 row repack、download、upload；
- `copy_count == 0`。

domain 名称相同本身不构成零拷贝证据。

## 6. CUDA 隔离

- NV Host Profile 只能接收 Host Image/Packet bridge。
- NV device-frame encoder 只能接收 `CudaDevice` NV12 external Image。
- device-frame decoder 可接收上游声明的 Host compressed Packet，并输出 CudaDevice Image；必须标记为非对称 device-frame，而非完整 CUDA zero-copy。
- device-frame 路径不得调用 Host NV wrapper或 `try_read_bytes`。
- CUDA resize 未提供 processor 时返回 Unsupported，不自动下载。

## 7. 执行体任务

- [ ] 新建唯一 unsafe external bridge 模块和安全 facade。
- [ ] 实现构造失败 token 回收 guard。
- [ ] 为 fd、raw pointer、Host external 和 device external 分别测试。
- [ ] 测试原对象先 drop、clone 交错 drop 和并发 clone，callback 恰好一次。
- [ ] 测试 offset+len overflow、短 plane、错 stride、缺 handle、错 domain。
- [ ] 确保 allow_staging=false 负向测试无 stage hook 调用。
- [ ] 对所有 public safe wrapper 增加 rustdoc safety contract。

## 8. 完成条件

unsafe 只存在于一个可审计模块；Miri/单测可证明无 double free 和常见泄漏；所有零拷贝报告均有 handle、layout、guard 和 copy-count 证据。

