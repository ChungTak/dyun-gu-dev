# 08. CI、Miri、Sanitizer、并发模型与 Fuzz

> 需求 ID：CORE7-08

## 1. PR 门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
git diff --exit-code Cargo.lock
```

另执行：

- product feature matrix，不使用不可构建的 `--all-features` 混淆 SDK；
- C11/C++17 ABI package smoke；
- fuzz check 全 target；
- x86_64、aarch64 GNU、Android、riscv64 和 32-bit policy/type check；
- generated header/schema/lockfile 无漂移。

## 2. Miri

固定 nightly 版本并至少运行：

- `dg-core` Buffer/Tensor/ExternalDropGuard/MemoryPool；
- `dg-capi` view、owned handle、callback helper 的纯 Rust 测试；
- scheduler lease/affinity；
- graph queue/stop ownership 的无真实 FFI 子集。

Miri 不支持的 FFI test 通过明确 test target 排除，不能把整个 crate success skip。

## 3. Sanitizer

- ASan/LSan：C harness、external callback、model/view parsing、create/run/destroy 循环；
- TSan：engine status/metrics/stop/reload、backend handle、scheduler、stream close/recv；
- UBSan-equivalent：C/C++ harness 的整数、alignment 和 enum wire 输入；
- sanitizer artifact 保存完整命令、compiler、features、exit code 和 report。

任何 report 都阻塞 core release。工具/SDK 不兼容写最小复现到 upstream issue；纯 core/C ABI 子集仍必须运行。

## 4. 并发模型

使用 loom/shuttle 类工具覆盖小状态空间：

- queue send/recv/close/depth/bytes；
- stop flag 与 worker join；
- external callback exactly-once；
- engine handle acquire/destroy；
- scheduler acquire/release/poison；
- stream close/recv wakeup。

模型测试使用较小独立 module，不把真实 SDK 或网络放入 state-space exploration。

## 5. Fuzz

保留并更新：

- GraphSpec string/file/include；
- C ABI views/runtime options/external descriptor；
- runtime backend/model metadata；
- tensor shape/stride/quant；
- media metadata/track/config；
- reload state transitions。

修复 `reload-transitions`：

- 保存 nightly 失败 corpus；
- fuzzer cleanup 使用有限非零 destroy deadline，并在 Busy 时 stop/retry；
- 不在每轮遗留 engine/worker；
- crash corpus 进入 PR regression；
- 当前候选每个 target 先 corpus regression，再至少 15 分钟 nightly。

artifact 保存 corpus、crash、minimized、target revision 和 libFuzzer 参数；无 artifact 不等于自动成功。

## 6. Workflow

- PR：快速 gates 与 fuzz check；
- nightly：Miri、sanitizer、model tests、每 target fuzz、2h core soak；
- release candidate：24h soak、性能、C package smoke、rollback；
- hardware/protocol：按 runner label 单独 workflow，缺设备为 Blocked。

workflow job 必须把源码 SHA、Cargo.lock、toolchain 和 feature 写入 artifact manifest。

## 7. 完成条件

- [ ] PR required checks 覆盖 C ABI 和核心合同。
- [ ] Miri/sanitizer/并发模型实际运行且无报告。
- [ ] `reload-transitions` 失败 corpus 已回归，全部 fuzz target 在候选 SHA 通过。
- [ ] workflow 不把 skip/缺 runner 当 Passed。
- [ ] evidence manifest 可追溯到唯一源码和工具链。

