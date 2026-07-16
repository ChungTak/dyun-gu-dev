# RC2 pin 回滚与 stable 升级演练

> INT4-10 部分交付：在上游 `0.2.0` stable 发布前，生产候选为 `0.2.0-rc.2`。
> 本文件记录可重放的 pin 回滚步骤；stable 升级在 tag 出现后按同一模板执行。

## 当前生产候选

| 项 | 值 |
|---|---|
| SDK tag | `0.2.0-rc.2` |
| SDK commit | `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| dyun pin 提交（基线） | `4b7a0e4` / merge `14e2b6e` |
| 前一 pin（RC1 时代 main） | 见 git history：`7faba6fe…`（Plan 3） |

## 回滚到 pin 前状态

同时恢复以下文件（不得只改 manifest）：

1. `crates/dg-media-avcodec/Cargo.toml` 中 `avcodec` `rev`
2. `Cargo.lock` 全部 avcodec workspace git packages
3. `crates/dg-media/tests/dependency_contract.rs` 预期 SHA
4. 相关示例 / capability 文档（若 pin 期间改过）

```bash
# 示例：回退到 RC2 pin 合并前
git checkout 137b7b80896395cf8164e8c2172a345d9bc857fd -- \
  crates/dg-media-avcodec/Cargo.toml \
  Cargo.lock \
  crates/dg-media/tests/dependency_contract.rs

cargo fetch --locked
cargo test -p dg-media --locked --features avcodec-profile-native-free
```

禁止：运行期切换低层 backend、隐式改变 Profile 语义、跳过 lock。

## 升级到 stable（待 tag）

1. `git ls-remote --tags … | grep '0.2.0$'` 确认 annotated tag 与解引用 commit
2. 原子更新 manifest `rev` / lock / `dependency_contract`
3. 重跑 plan4 最小命令矩阵（见 `11_execution_order_and_final_acceptance.md`）
4. NV：`DYUN_NV_HW=1` Host + device-frame
5. 更新 `AVCODEC_RC2_ACCEPTANCE.md` 为 stable 接纳记录并回传上游

## 验证回滚成功

- `dependency_contract` 与 lock 中 git rev 一致
- NativeFree 测试通过
- 无未提交的 `Cargo.lock` 漂移
