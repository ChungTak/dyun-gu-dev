# OpenVINO 本地调试约定

## 1. 原则

本地可使用系统/Python OpenVINO runtime验证，但 production CI和OCI不得依赖测试中的硬编码 `.so` 文件名或
开发者私有路径。所有 local override必须在提交前移除。

## 2. 环境采集

```bash
rustc --version --verbose
python3 -c 'import openvino as ov; print(ov.__version__)'
ls -l /dev/dri || true
lspci -nn | rg -i 'vga|display|intel' || true
```

通过 `dg doctor --format json` 保存 runtime/plugin/device probe；输出必须脱敏。

## 3. 本地命令

```bash
cargo test -p dg-openvino --locked --features backend
cargo test -p dg-openvino --locked --features backend --test openvino_runtime -- --ignored --nocapture
cargo run -p dg-cli --locked --no-default-features --features product-intel -- \
  doctor --format json
```

iGPU测试必须显式配置 `device: intel_gpu` 并断言 selected runtime device为 GPU。CPU成功不能替代 iGPU结果。

## 4. 上游调试

需要 patch `openvino` 或 Cheetah 时，在独立 checkout复现并记录完整 revision。临时 `[patch]`、path dependency、
`LD_LIBRARY_PATH` 或本地 symlink不得进入生产分支；修复合入上游后以固定版本/rev原子更新 lock并重跑矩阵。

## 5. 故障分类

- runtime/plugin/library不可发现：环境/镜像；
- `/dev/dri`或权限缺失：runner/部署；
- device配置映射错误：dyun；
- compile/infer返回错误：先生成最小 OpenVINO复现，再判断 dyun/upstream；
- 结果或metadata错误：dyun bridge/runtime；
- 只有静态 capability成功：不算硬件通过。

