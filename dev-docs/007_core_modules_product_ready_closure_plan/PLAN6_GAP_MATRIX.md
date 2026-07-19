# Plan 6 完成度复核矩阵

## 1. 分类规则

| 分类 | 含义 |
|---|---|
| Verified Done | 当前 main 有实现、自动测试及该合同所需的运行证据 |
| Carry Forward | 实现缺失、只完成一部分、接口偏离或证据基础设施不足 |
| Capability Qualification | 软件入口存在，但真实协议/设备资格尚未取得 |

本表基于 `main@feddd3add23ec8647f91b61fd3c15837342b790a`。执行 Plan 7 时以新的实际 HEAD 复核。

## 2. CORE6 需求复核

| Plan 6 ID | 分类 | 当前证据 | 缺口/Plan 7 去向 |
|---|---|---|---|
| CORE6-01 | Carry Forward | baseline、风险台账和 main CI 已有 | status/acceptance 候选 SHA 过期，Done 与 Pending 混用；CORE7-01 |
| CORE6-02 | Carry Forward | Graph/Runtime ResourcePolicy、部分 boundary tests | CLI/C bootstrap、RuntimeOption/backend、bounded model、effective pre-copy 未贯通；CORE7-02/03 |
| CORE6-03 | Carry Forward | fallible host alloc、physical stride、external guard、MemoryPool unit tests | device consumption 与 pool/process policy/ops evidence 不完整；CORE7-03/05 |
| CORE6-04 | Carry Forward | shared backend metrics、bounded histogram、affinity capacity/TTL | vendor cancel/capability 和真实 contract 不完整；CORE7-04 |
| CORE6-05 | Verified Done（软件） | bounded queue、shutdown retry、hot-update phase faults/tests | 真实长队列/SIGTERM 进入 soak，不重做 lifecycle 设计；CORE7-09 |
| CORE6-06 | Carry Forward + Capability | mock/hub timeout、bridge typed conversion、registry reap | Cheetah native timeout、detached timer、effective pre-copy、真实网络；CORE7-06 |
| CORE6-07 | Carry Forward | 算法复杂度/NaN/external tensor 上限 | 通用 frame-local 错误隔离和指标仍缺；CORE7-05 |
| CORE6-08 | Carry Forward | enum i32、owned handles、external callback、engine destroy | views 未接入、runtime options 空、ABI version/string/package 不完整；CORE7-07 |
| CORE6-09 | Carry Forward | 部分 typed error、ops snapshot、reconnect readiness | taxonomy/metrics/required readiness 未全部兑现；CORE7-05 |
| CORE6-10 | Carry Forward | fuzz target、property tests、nightly/soak 骨架 | nightly fuzz 失败、无 Miri/sanitizer/model、soak 非长流；CORE7-08/09 |
| CORE6-11 | Carry Forward | Pending acceptance 与模板 | 候选身份旧、无 artifact/24h/performance/rollback；CORE7-10/11 |

## 3. 总完成定义复核

| Plan 6 总完成条件 | 结论 | 依据 |
|---|---|---|
| CORE6-01～11 Done、P0/P1 Closed | 未完成 | acceptance Pending；R6-002/R6-003 仍 Mitigated |
| limit 在真实消费前执行并有边界测试 | 部分 | vendor `fs::read`、effective frame pre-copy、device output 未闭环 |
| stream/runtime/graph pending 可 deadline shutdown | 部分 | mock/graph 有测试；Cheetah adapter 无 native deadline |
| external/C owned/error 所有权明确 | 大部分完成 | callback/owned handle 已有；view/wire 产品合同未完成 |
| metrics/cache/affinity/sink 有界 | 部分 | 内部结构有界；ops 必需指标和长稳证据不足 |
| PR/nightly/sanitizer/Miri/fuzz/24h 证据 | 未完成 | 无 sanitizer/Miri；nightly fuzz 失败；无真实 24h |
| C ABI/header/schema 同候选 | 未完成 | acceptance 指向旧 SHA，无 artifact digest |

## 4. 不再重复实现的基线

Plan 7 应回归但不重新设计：

- Buffer fallible reads、external guard exactly-once；
- checked shape/stride/physical bytes；
- fixed backend latency histogram 与 pool shared metrics；
- bounded graph queue/collector、shutdown retry 与 hot-update phase injection；
- element NMS/top-k/OCR/track/OSD 基础复杂度限制；
- C owned bytes/error、external memory callback、engine destroy Busy/retry；
- hub registry reap 和 graph frame-local stream conversion drop。

若回归审计发现上述事实不成立，必须重开相应风险，而不是依赖本表豁免。

## 5. Capability Qualification

| Capability | 当前状态 | 关闭证据 |
|---|---|---|
| Cheetah RTSP/HTTP-FLV/RTMP/WebRTC | Blocked | native deadline revision + real network fault/soak |
| OpenVINO CPU | SoftwareVerified（历史） | 候选 SHA 重新执行 CPU load/infer/regression |
| OpenVINO GPU/NPU | Blocked | device allocator/cancel/precision/soak |
| TensorRT/RKNN/Sophon | Blocked | 各自硬件 contract、精度、资源和 shutdown |
| 硬件 avcodec profiles | Blocked/按既有计划 | 实机 decode/encode/frame-limit/long soak |

