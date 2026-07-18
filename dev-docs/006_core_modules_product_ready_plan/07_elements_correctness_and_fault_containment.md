# 07. Elements 正确性与故障隔离

> 需求 ID：CORE6-07

## 1. 统一 Element 合同

每个注册 element 必须声明并测试：

- 输入/输出 port、payload type 和 required/optional；
- 参数 schema、默认值、数值范围和未知字段策略；
- 最大输入 bytes、最大输出 item、时间/空间复杂度；
- 是否有状态、reload 时保留还是 reset；
- stop、EOS、空输入、错误输入和 backpressure 行为；
- 产生的 metrics 和可能的高基数字段。

schema 存在不代表安全；validator 必须把 ResourcePolicy 与参数组合后的实际内存/候选规模计算出来。

## 2. 算法边界

- YOLO/RetinaFace 在分配预处理 tensor、anchor 或 detection 前检查 width × height × channels、class count 和候选数。
- NMS 的输入候选数有硬上限；超限按配置选择 reject 或先做确定 top-k，默认 reject，不静默截断。
- PPOCR alphabet、rows、connected-component queue、输出字符串和 region 数量有上限。
- ByteTrack 的 active/lost track 数、history 和 `max_lost` 有上限；长流不能持续保留从不匹配的 state。
- OSD box、color、thickness、坐标和画布物理 bytes 有上限。
- distributor/converger 的 pending branch/sequence state 必须有 deadline 和容量。
- HTTP push 的 request body、响应、重试和连接等待受 policy；URL/sensitive header 脱敏。

所有 f32 参数和模型输出拒绝 NaN/±Inf；概率要求 `[0,1]`。`exp/sigmoid` 等运算对极端输入保持有限，
错误 tensor shape/dtype/layout 返回 typed error。

## 3. 外部内存与转换

算法 element 不再调用可能返回空数据的 `read_bytes()`。Host-only 算法遇到 device/external tensor：

1. 有显式 planner/mapper 时 stage 并记录 copy；
2. 无 mapper 时返回 Unsupported；
3. 不得把空 Vec 当作空检测结果。

预处理/后处理产生的 tensor 同样使用 effective tensor limit 和 fallible allocator。

## 4. 故障隔离

- 单 frame 数据错误默认 drop 当前 frame并计数；配置/模型/不变量错误使 node/graph Failed。
- error policy 明确列出 retry/drop/fail，不能用字符串匹配决定。
- element panic 由 graph 边界捕获，但库代码产品路径不得依赖 panic 表达输入错误。
- stateful element reload 若无法迁移，明确 reset 并增加 `state_reset_total`。
- 错误日志包含 graph/node/operation/error kind，不包含完整 payload、模型数据或敏感 URL。

## 5. 测试

- 每个 element 的 schema 与 validator 一致性测试；unknown/invalid/limit 参数。
- property test 生成 shape、阈值、bbox、stride、class count 和空/非有限 tensor。
- worst-case NMS、connected component、track churn、converger missing branch 均在预算内终止。
- external-only tensor 明确 Unsupported；staging copy metrics 准确。
- frame-local error 不终止其他 stream；config/invariant error 必须终止并保留根因。
- 100 次 reload 验证 state keep/reset、pending state 和内存回到稳定区间。

## 6. 完成条件

- [ ] 所有产品 element 有资源、复杂度和失败合同。
- [ ] NaN、极端 shape、候选爆炸和长期 state 不会造成 OOM/无限执行。
- [ ] Host-only element 不会静默消费 external 空数据。
- [ ] frame-local 与 graph-fatal 错误分类稳定。
- [ ] schema、validator、运行期和测试使用同一限制。
