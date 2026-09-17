# MGE 群曾：固定四喷版

同步自独立 MGE 仓库提交 `5fb8b9c6ea729e6a6296c7be22a66ebae15bc65f`。
采用“补五列喷、不偷阳光菇”的已测版本，不是发布验收会话重新移植的脚本。
源码与该提交一致，只调整 Cargo 依赖路径以适配库内示例。

- 阵型为边路四、五列双大喷，四列带南瓜；三路七列没有误放的梯子。
- 四列喷正常维护；前场六曾和四列喷齐全时，按五路、一路顺序尝试补五列喷。
  使用原有局部威胁条件，红眼关也允许补，不采用“不补五列喷”实验。
- 不将四、五列大喷替换为阳光菇。卡组中的阳光菇仍可作为普通垫材。
- 模仿冰使用主库 `is_safe_imitator_ice`，候选顺序为
  `(1,4) → (5,4) → (1,6) → (5,6) → (2,7) → (4,7) → (1,5) → (5,5)`。
  先空格、普通垫材、五列喷，等待后才允许牺牲四列喷；冰冻未结束时延后请求。
- 普通三叶草使用主库 `is_safe_blover`；智能铲除使用主库。
- 保留六列普通垫材保护、冰车/小丑选曾、前场补曾优先级及该版本的南瓜维护。
- 自然随机出怪后移除蹦极；女仆召唤伴舞、逐帧原生 Dance、5001cs 卡序和 W20 禁冰不变。
  正常续关保留时钟，新独立样本按 world epoch 重置。

## 测算

默认接纳新样本 600 秒，之后不启动新样本，已有样本继续到自然失败。
一轮为 20 波、两面旗帜，初始成熟阶段不计入本次成绩。

| 环境变量 | 默认 | 用途 |
|---|---|---|
| MGE_SECONDS | 600 | 测算接纳窗口；0 关闭测算 |
| MGE_SUN | 8000 | 测算新样本初始阳光 |
| MGE_ROUNDS | 1500 | 测算新样本初始成熟阶段 |
| MGE_TRACE_DIR | 未设置 | 每 worker 日志目录，运行前创建且不复用旧文件 |
| MGE_TRACE_LOSS | 1 | 植物损失日志 |
| MGE_TRACE_ACTIONS | 0 | 全部用卡结果详细日志，按需启用 |
| MGE_TRACE_DECISIONS | 0 | 决策、防空和成功用卡日志，定向诊断时启用 |
| MGE_SMOKE_FRAMES | 0 | 非零时短测指定帧数并检查场景，配合 MGE_SECONDS=0 |

在 RSVZ 根目录运行：

```powershell
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl/mge_qunzeng --release --pe-toolchain clang-thin-lto --threads 12 --performance-window-secs 600 --output results/mge.json
cargo test --manifest-path dev-scripts/lowdsl/mge_qunzeng/Cargo.toml --lib
```

发布工具包可用 `scripts/rsvz-snapshot.ps1` 代替上面的 `cargo run -p rsvz-cli --`。
1051 录像前先做 build-only，关闭测算并核对游戏初始条件；关闭测算后不自动应用
`MGE_SUN` / `MGE_ROUNDS` 的 WorldResetConfig。细节见 [快照使用指南](../../../docs/snapshot-quickstart.md)。

不同打法的正式效果比较使用独立随机种子；固定种子只用于复现失败。
本次同步不借用其他脚本或历史样本的均值作为新验证结果。

2026-09-18 同步验证：9 项原有测试、格式检查、PE ThinLTO 2,000 帧短模拟、
1051 DLL 与注入器 build-only 通过。本次没有重跑十分钟测算或进行实机注入。
