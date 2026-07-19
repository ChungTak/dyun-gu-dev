# 03. Model、Tensor、Frame 与 Device 消费前限制

> 需求 ID：CORE7-03

## 1. 统一消费边界

所有不可信资源遵循：

```text
derive metadata → checked size → policy check → reserve/allocate/read/copy/import/SDK call
```

检查发生在第一笔可能消耗显著资源的操作之前。后置 output 校验仍保留，但不能替代 pre-consumption check。

## 2. Bounded Model Loader

在 `dg-runtime` 提供共享 model preparation API，vendor backend 不再直接 `fs::read`：

- 文件先检查 metadata，再以 `take(limit + 1)` 或等价方式限长读取；
- 实际读取超过 limit 时丢弃候选并返回 typed `ResourceLimit`；
- `ModelSource::Bytes` 在 clone 前检查；
- preparation 产生 model identity/hash、artifact role 和总字节数；
- backend init 只接收 prepared/validated artifact，不重新从原路径无界读取。

OpenVINO IR 将 XML、BIN 和关联 artifact 按总模型预算计算；缺失/替换/rename race 明确失败。若 SDK 只能接收
路径，必须使用固定文件 identity 并在 SDK 调用前复核，不得只依赖早期 metadata。

## 3. Tensor 与 Device

- `TensorInfo` 的 shape/dtype/stride/physical bytes 在 init、reshape 和 output refresh 后统一验证。
- 每个 backend output allocation 在 SDK allocation/mapping 前检查 effective tensor bytes。
- Device allocator 接受 policy 或已验证 token；不能只依赖上层约定。
- external buffer size、descriptor、domain/device/handle compatibility 在 ownership transfer 前检查。
- host staging 通过 fallible allocation；H2D/D2H/host copy 记录 count/bytes/time。
- `Tensor::allocate` 仅作为安全默认便利接口；产品 graph/backend 使用 `allocate_with_policy`。

## 4. Frame 与 Media

- connector/bridge 接收 effective policy，而不是内部构造默认 policy；
- encoded payload 在 `Bytes::to_vec`、buffer clone 或 codec parse 前检查；
- decoded width/height/planes/stride/physical bytes 在 decoder allocation hook 前检查；
- codec config、tracks、tags 和 metadata 具有 count/item/total 上限；
- bridge 复核 timebase、track ID、format/codec 和 buffer layout，错误不生成默认帧；
- effective graph limit 小于 process default 时，超限 frame 不发生 host copy。

## 5. Queue 与输出

- queue/collector/C pending input/output 同时按 packets 和 owned bytes 计数；
- shared payload 的物理内存与 queue slot 分别核算，规则保持一致；
- algorithm result、metrics JSON、error chain 和 C output capacity 受相同预算框架约束；
- 超限返回 typed `ResourceLimit` 并增加有限 label 的 reject counter。

## 6. 测试

- model/tensor/frame/buffer 所有限制覆盖 `limit-1/limit/limit+1`；
- 计数 reader 证明超限模型只读取至 `limit+1`；
- OpenVINO XML/BIN 单项合规但累计超限；
- 文件 metadata 合规、实际读取超限和 rename race；
- 计数 allocator/SDK shim 证明 device output 超限时未调用分配；
- graph effective frame limit 小于 default，bridge 在 copy count 仍为零时拒绝；
- padded/packed/dynamic output metadata overflow；
- external import 失败不转移 callback/FD 所有权。

## 7. 完成条件

- [ ] vendor backend 不存在无界模型读取。
- [ ] device/tensor output 在 SDK allocation 前受 effective policy 控制。
- [ ] stream/media frame 在第一份 host copy 前检查 graph effective limit。
- [ ] 所有超限错误 typed、可计数且不泄漏资源。
- [ ] 后置校验与 pre-consumption enforcement 均有测试。

