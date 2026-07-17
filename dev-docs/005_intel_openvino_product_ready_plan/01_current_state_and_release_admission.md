# 01. 当前状态与产品发布接纳门禁

## 1. 基线采集

执行前记录源码、dirty 状态、工具链、lock、OpenVINO pin、CI 状态与宿主设备。不得直接复制本计划中的审计 SHA。

```bash
git status --short
git rev-parse HEAD
git log -5 --oneline
rustup show
rustc --version --verbose
cargo --version --verbose
rg -n 'rust-version|toolchain|openvino' Cargo.toml rust-toolchain.toml Cargo.lock .github/workflows
```

CPU/iGPU runner 额外记录 `uname`、CPU 型号、`/dev/dri`、PCI ID、内核、Intel 驱动、
OpenVINO runtime/plugin 版本及容器 runtime。敏感宿主信息进入受控 artifact，不写入普通日志。

## 2. 当前能力与已证实缺口

- 默认 workspace test 和 clippy 可通过，fmt 在 `dg-media/src/avcodec.rs` 有格式差异。
- OpenVINO CI 只验证 CPU；backend `probe_capabilities` 仍返回静态 CPU/GPU/NPU 列表。
- Graph 顶层 `device` 与 OpenVINO `options.device` 是两条路径，可能出现调度 GPU、实际编译 CPU。
- OpenVINO run 使用 Host tensor 拷入/拷出；不得声明 remote tensor 或 zero-copy。
- `RunningGraph` 不能由产品进程主动停止，也不能在线导出 metrics。
- `dg run --watch` 对无限流先阻塞于 `graph.run()`；有限图结束后只打印 diff。
- `dg-cli`/`dg-capi` 没有 `cheetah` feature 和 embedded connector 初始化入口。

## 3. 首发支持矩阵

| 能力 | Production | 说明 |
|---|---|---|
| OpenVINO CPU | 是 | 通用 CI + release OCI |
| OpenVINO Intel iGPU | 是 | 自托管 runner 强制门禁 |
| OpenVINO NPU/remote tensor | 否 | experimental/unverified |
| Software H.264 | 是 | avcodec software profile |
| OneVPL | 否 | 后续独立硬件验收 |
| Cheetah 四协议方向 | 是 | 实际 connector，不是 mock hub |
| NVIDIA/RKNN/Sophon | 否 | 保留现有能力，不阻塞本轮 |

## 4. 接纳规则

发布候选必须同时具备：clean tree、完整 required checks、CPU+iGPU 实机通过、24h soak、
性能基线不过阈值、OCI 签名/SBOM、回滚演练。任一硬门禁 skip 都使状态保持 Partial。

## 5. 完成条件

- [ ] 审计基线与 runner 环境已保存。
- [ ] production/experimental 支持矩阵经代码、文档和 CLI 输出统一。
- [ ] required checks 与硬件资源负责人明确。
- [ ] 接纳决定写入 `OPENVINO_PRODUCT_ACCEPTANCE.md`。

