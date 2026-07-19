# 01. Plan 6 缺口审计、风险重开与准入

> 需求 ID：CORE7-01

## 1. 基线采集

实施前和每个候选制品生成时保存：

```bash
git status --short
git rev-parse HEAD
git log -10 --oneline
rustup show
rustc --version --verbose
cargo --version --verbose
sha256sum Cargo.lock crates/dg-capi/include/dg_capi.h
cargo metadata --locked --no-deps --format-version 1
```

再记录当前 main CI、nightly、release workflow 的 run URL、结论和 head SHA。任何不属于候选 SHA 的
历史成功结果只能作为趋势，不能填写到候选 acceptance。

## 2. Plan 6 逐项复核

使用 `PLAN6_GAP_MATRIX.md` 对 Plan 6 README、01～11、风险台账和 acceptance 的每个完成条件分类：

- `Verified Done`：当前 main 的代码、自动测试和必要运行证据均存在；
- `Carry Forward`：软件合同未实现、实现偏离合同或验证基础设施不足；
- `Capability Qualification`：软件适配存在，但真实协议/设备资格未完成；
- `Obsolete`：仅当接口已被明确替代且迁移和测试完整时使用。

不能根据 `EXECUTION_STATUS.md` 中的 Done 直接判定。审计至少核对公开接口、真实调用路径、feature
组合、CI workflow、release package 和运行证据。

## 3. 已确认 Carry Forward

1. `DgStringView/DgByteView/DgShapeView` 已定义，但多数 C API 未使用。
2. `DgRuntimeInitOptions` 只有 struct prefix，不能配置 process policy。
3. CLI 通过 `Graph::new` 使用默认 policy，没有可信 runtime limits 输入。
4. `RuntimeOption` 不携带 policy；vendor backend 仍直接 `fs::read` 模型。
5. OpenVINO XML/BIN 没有按 artifact 总量统一限长读取。
6. Cheetah bridge 先按默认 frame limit 拷贝，再由 element 检查更小的 effective limit。
7. Cheetah `recv_timeout` 通过 timer thread 竞速；上游 `SubscriberSource` 没有原生 deadline。
8. vendor sync/cancel 能力未通过统一真实合同，不能自动进入 product support。
9. 算法 element 的 frame-local 数据错误仍可能返回并终止整个 graph。
10. MemoryPool/affinity/registry/resource reject/shutdown 等指标没有完整导出到 ops。
11. readiness 主要依赖 Running 与 reconnecting，未汇总必需 backend/source/sink live readiness。
12. 缺 Miri、sanitizer、并发模型和 C/C++ ABI runtime gate。
13. `reload-transitions` nightly fuzz 已失败，当前 SHA 未保存通过证据。
14. soak 仅重复执行 workspace tests，不运行真实长流，也不采集资源/性能曲线。
15. release 包没有严格验证/安装 `libdg_capi.so.2`、导出符号、C examples 与 header/library hash。

## 4. 风险重开

上述事实写入 `CORE7_RISK_REGISTER.md`，不得修改 Plan 6 的历史风险结论来掩盖后续发现。Plan 6 的
R6-002/R6-003 分解为：

- 核心软件风险：policy 传递、pre-consumption enforcement、native cancellation；
- capability qualification：真实 device allocator、Cheetah 网络和厂商 backend 实机证据。

核心软件部分修复后可以 Closed；capability 未通过时对应 support matrix 行保持 Blocked。

## 5. 失败基线

进入功能修复前提交最小失败测试：

- C header 签名断言证明 view 未接线、runtime options 缺字段；
- 计数 reader/allocator 证明 vendor model 或 effective frame limit 检查过晚；
- thread 计数证明 Cheetah timeout 创建额外线程；
- frame-local element 错误导致 graph 退出；
- metrics/readiness 缺少约定字段；
- `reload-transitions` crash/leak corpus 可重复；
- release package 缺 SONAME 文件或 C examples。

失败测试 PR 只保存复现，不顺带修复；若测试本身需要最小 instrumentation，instrumentation 不改变产品行为。

## 6. 准入门禁

- 当前工作树 clean，fmt/clippy/tests/deny/lockfile 无失败；
- 风险台账 owner 不得使用虚构姓名；初始 `Unassigned`，进入 In Progress 前填写真实 owner；
- 当前 nightly failure 已下载或以最小 corpus 重现；
- C ABI v2 尚无 Accepted 制品的事实由 release owner 确认；
- 24h fixed CPU runner、sanitizer runner 和 capability runner 的标签/负责人已登记；
- 不能取得的外部资源明确标 Capability Blocked，不影响不包含该 feature 的软件 PR。

## 7. 完成条件

- [ ] Plan 6 每个完成条件都有唯一分类和证据。
- [ ] 所有 Carry Forward 项进入 CORE7 requirement/risk。
- [ ] P0/P1 软件失败基线先于修复提交。
- [ ] 当前 CI/nightly/release 结果绑定准确 SHA。
- [ ] 核心与 capability acceptance 边界获得 reviewer 确认。

