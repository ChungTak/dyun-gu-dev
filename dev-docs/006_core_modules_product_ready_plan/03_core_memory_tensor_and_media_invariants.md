# 03. Core 内存、Tensor 与媒体不变量

> 需求 ID：CORE6-03

## 1. Fallible allocation

Host buffer 不再直接依赖 `vec![0; size]` 的不可恢复分配。CPU allocator 使用 `try_reserve_exact` 后初始化，
把 capacity overflow/allocator failure 映射为带 requested bytes 的 `ResourceExhausted`。设备 allocator
同样在调用 SDK 前检查 policy 和整数转换。

`MemoryPool` 增加最大缓存 bytes、entry 数和单 descriptor 数；超过时按确定 LRU/最大块优先策略释放，
并暴露 cached/current/evicted bytes 指标。不同 device/domain/align/size 的 buffer 不得互相复用。

## 2. Buffer 读写语义

删除产品路径上的静默接口：

- `Buffer::read_bytes()` 不再对不可读外部内存返回空 `Vec`；
- `Buffer::into_host_bytes()` 不再把 device-only buffer 变成空数据；
- backend/element 全部使用 `try_read_bytes()`、`try_into_host_bytes()` 或显式 `map_with/stage`；
- device-only 输入未提供 mapper 时返回 `UnsupportedMemoryDomain`，不得继续推理。

共享 host buffer 的 clone copy 必须计入 copy metrics；唯一所有权移动不计 copy。所有读取都校验 descriptor
与实际 storage 长度一致。

## 3. Shape、Stride 与物理字节

新增 checked stride/physical span 计算：

```text
physical_elements = max((dim[i] - 1) * stride[i]) + 1
physical_bytes = dtype.storage_bytes_for_elements(physical_elements)
```

规则：

- shape 与 stride rank 必须一致；除合法零尺寸 tensor 外 stride 不得为零；
- 所有乘加使用 checked 运算，禁止 saturating 后继续执行；
- packed I4/F4 按物理 element count 向上取整；
- `size_with_stride` 必须不小于 logical bytes，并与计算出的物理 span 一致；
- reshape 只允许 contiguous 且 logical/physical 合同保持的 tensor；
- `Tensor::from_buffer` 按 physical bytes 校验，不能只比较 logical bytes。

保留便捷 contiguous 构造，但改为可失败 API或在此前已验证 shape 的内部函数。

## 4. 外部资源所有权

`ExternalDropGuard` 在锁内只 `take` callback，解锁后执行，避免 callback panic 造成 mutex poison 和永久泄漏。
Rust callback panic 被 catch 并记录一次 fatal ownership diagnostic；callback 仍视为已经消费，绝不重复调用。

外部 buffer clone 共享同一 guard，最终 clone drop 时恰好释放一次。import 失败不转移所有权；成功后调用方
不得释放已转移引用。C ABI 的 callback/user_data 规则在 CORE6-08 固定。

FD 导入优先 duplicate，框架只关闭自己的 FD；raw/device handle 要求调用方在 import 前转移或增加一个引用。
callback 不得在 core lock 下执行。

## 5. Media metadata

- `MediaRect` 必须同时验证 overflow 和位于 coded/visible 范围内。
- plane offset、stride、row count、last row end 和 buffer size 使用 checked 运算。
- codec config item/count/total 保持限制，并纳入 process policy。
- timebase denominator 非零，必要时限制 numerator/denominator 防止下游换算溢出。
- `MediaFrame::with_meta` 增加 fallible validated 构造；仅内部测试可使用 unchecked 构造。
- frame kind、shape、dtype、format、domain、buffer device 和 `MediaInfo` payload 必须一致。

## 6. 测试

- allocator failure、capacity overflow、pool eviction、不同 descriptor 不复用；
- external-only read 返回 typed error，所有 backend/element 不再消费空字节；
- padded NCHW/NHWC、packed dtype、zero dimension、rank mismatch 和 stride overflow；
- guard 多 clone、import failure、callback panic、并发 final drop、FD duplicate/close exactly once；
- image plane overlap/越界、可见区域越界、非法 timebase 和 codec config 累计上限；
- Miri 覆盖 ownership、Arc、callback 和 buffer/tensor 转换。

## 7. 完成条件

- [ ] Host/device 分配均可失败并受 policy 控制。
- [ ] 不可读外部内存不会变成空输入或错误推理结果。
- [ ] Tensor logical/physical/stride 字节合同统一。
- [ ] 外部资源在成功、失败、clone、panic 和并发 drop 中恰好释放一次。
- [ ] MediaFrame 只能通过验证后进入产品 graph。
