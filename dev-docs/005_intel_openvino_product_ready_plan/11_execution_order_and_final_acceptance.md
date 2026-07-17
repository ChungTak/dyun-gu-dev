# 11. 实施顺序与最终验收

## Phase 0：事实基线

执行 INT5-01/02：保存审计证据，修复 fmt，统一工具链和 required checks。退出条件是 clean 默认基线全绿。

## Phase 1：长运行内核

执行 INT5-03/04：生命周期、supervisor、信号和事务式热更新。使用无限 mock 流验证，不提前引入硬件变量。

## Phase 2：真实协议与运维

执行 INT5-05/08：接通 Cheetah、typed retry、readiness、metrics、日志脱敏和 limits。退出条件是真协议故障注入通过。

## Phase 3：OpenVINO 产品路径

执行 INT5-06/07：设备配置收敛、live probe、CPU+iGPU regression、异步 request pool、背压和 copy指标。
CPU先绿，再在 iGPU runner关闭硬件门禁。

## Phase 4：ABI 与制品

执行 INT5-09/10：冻结 C ABI v1，构建最终 OCI，完成 E2E、性能、soak、SBOM、签名和回滚。

## Phase 5：收敛

执行 INT5-11：更新 README/user guide/design/remaining tasks/support matrix，填写 acceptance和 status。
只有所有证据引用同一源码 SHA/OCI digest才可标 Plan完成。

## 最小软件命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p dg-media --locked --features avcodec-profile-software
cargo test -p dg-stream --locked --features cheetah
cargo test -p dg-openvino --locked --features backend
cargo build -p dg-cli --locked --no-default-features --features product-intel
git diff --exit-code Cargo.lock
```

CPU/iGPU/OCI 具体命令在 release workflow冻结并写入 `RELEASE_EVIDENCE_TEMPLATE.md`，不得由验收者临时缩减。

## 最终清单

- [ ] INT5-01～11 Done。
- [ ] 长流 stop/reload/reconnect/metrics通过。
- [ ] OpenVINO CPU+iGPU真实回归与 E2E通过。
- [ ] C ABI v1、limits和安全测试通过。
- [ ] 最终 OCI完成24h soak、性能、SBOM、签名和回滚。
- [ ] 文档 capability与真实 release evidence一致。

