# RustVsZombies (rsvz)

临时 Windows 使用快照的安装、AvZ 迁移、ThinLTO 测算和 1051 录像流程见
[快照使用指南](docs/snapshot-quickstart.md)。本页下方的早期阶段记录并非当前能力清单；
API、示例源码和 CLI 帮助为准。

**RustVsZombies** 是一个用 Rust 重写的植物大战僵尸（Plants vs. Zombies）游戏自动操作库，
由 GPL-3.0 项目 [AsmVsZombies](https://github.com/vector-wlc/AsmVsZombies)（avz2）重构而来。

> ⚠️ **当前项目处于早期开发阶段，尚未发布稳定版本。API 和行为随时可能变化，请谨慎使用。**

---

## 项目目标

最终目标是让用户只需编写一份 Rust 脚本，就能通过**编译时切换后端**，在三种运行环境中执行：

1. **注入后端（1051）**：将脚本编译为 32 位 DLL，注入到原版 PvZ 1.0.0.1051 游戏进程中运行，可以实时观看操作效果。
2. **PE 模拟器后端（第一波已落地）**：将同一份脚本编译为独立可执行文件，连接无头 PvZ 模拟器（[PvZ-Emulator](https://github.com/PvZ-Unofficial/PvZ-Emulator)），以远超实时的速度进行批量模拟和数据测算。当前 PE 路径已经有 backend/tooling 与 `run-pe`；全部 10 个 lowdsl dev script 均已通过 generated-runner build-only，PE24 与 `re20_p` 还通过了固定 seed 单关 runtime。Bench/Measure 现由脚本声明并通过通用 artifact 输出；`qed16luo`、roof/moon-night 炮时机和更广泛 parity 仍待验证。
3. **PvZ-Portable 后端**：把相邻的 64 位 PvZ-Portable 构建为通用插件宿主，再把每份脚本编译为运行时加载的 DLL；可选择 MSVC 或 UCRT64/MinGW 工具链。启用接入的游戏不绑定具体脚本，不设置插件环境变量时仍可独立运行；Portable 默认构建则完全不含 rsvz。

用户脚本只依赖 backend-neutral 的 `rsvz` API，不关心底层到底是真实游戏进程还是无头模拟器。
切换后端只需要使用不同的构建命令（`inject-1051`、`run-pe` 或 `run-portable`），无需修改脚本源码。

---

## 当前状态

### 已完成

项目当前已经实现新一版**低层 DSL 框架**与 **1051 注入后端**；迁移后的 1051 PE24 人工 smoke 和三后端 Witness 基准验收已经完成：

| 功能域 | 状态 | 说明 |
|--------|------|------|
| **后端能力 trait 体系** | ✅ 完成 | 细粒度 safe 原子 trait（`Backend`、`BattleStatusBackend`、`PlantCreateBackend`、`ZombieCreateBackend`、`CobFireBackend`、`WaveTimingBackend` 等）；cards/contact/cob/motion 等组合留在 `rsvz-game`，core 层不含 1051 地址、裸指针或 ABI 细节 |
| **1051 注入后端** | ✅ 完成 | `Pvz1051Backend` 实现 1051 适用的原子能力，尚未验证的能力保持 typed unsupported；支持 32 位 DLL 注入、raw address hook、patch、typed pointer layout |
| **低层 DSL** | ✅ 已通过 1051 smoke | 低层 DSL 直接位于 `rsvz::dsl`：`wave()`/`waves()` 选择波次，`time << expr` 绑定时间，`+` 与 `d(...)` 组合操作；支持植物 callable alias、`keep`/`to`、`p`/`pp`、效果时刻卡、铲除、类型化移除、setup facade 与注册错误聚合；迁移后的 PE24 已完成真实 1051 运行验收 |
| **自动布阵** | ✅ 完成 | 支持 pvztoolkit 新格式（Base64 + Base64Url）和 pvztools 旧格式解析；场景检测、植物/南瓜/咖啡豆/墓碑/梯子/钉耙行列处理；模仿者目标、蘑菇清醒/睡眠、土豆地雷/阳光菇生长状态；1051 raw mode 11~15 |
| **Contact Geometry** | ✅ MVP 完成 | 植物/僵尸接触判定、攻击范围、防御边界、爆炸范围查询；Tall-nut/Pumpkin/Cob Cannon 覆盖；DriveOver/GargantuarSmash/JackExplosion 威胁候选 |
| **普通函数与功能自持状态** | ✅ 完成 | core 提供当前操作和功能 TLS，scripting 保留重导出、输入适配、必要重载和 DSL；`CobManager` 不再含 runtime 泛型；时间注册使用同一实现的顶层／timeline `at(wave, time, callback)` 和 `try_at`；独立 runtime 包与旧 Runtime 绑定已删除 |
| **Plant Fixer** | ✅ 完成 | 自动南瓜修补：HP 阈值监控、破损检测与替换种植 |
| **Ice Filler** | ✅ 完成 | 自动寒冰菇/仿寒冰菇填补 |
| **Item Collector** | ✅ 完成 | 1051 环境下自动拾取掉落物 |
| **Smart Remove** | ✅ 完成 | 1051 环境下智能铲除逻辑 |
| **Zombie Motion** | ✅ MVP 完成 | 僵尸横坐标预测（支持 Normal/Zomboni/Football/Pogo/Ladder/Gargantuar 等）；后台探针已 build-only 验证 |
| **PE 后端第一波** | 🚧 基准 runtime/Witness 已通过 | `crates/backends/pvz_emulator/{backend,tooling}` 已接入 `pe-rs` 与 `run-pe`；全部 10 个 lowdsl dev script 已通过 PE generated-runner build-only，PE24 与 `re20_p` 已通过固定 seed 单关 runtime，三轮 Witness acceptance 已与真实 1051 golden 匹配；`qed16luo`、roof/moon-night 炮时机和更广泛 parity 仍待验证 |
| **PvZ-Portable 后端** | 🚧 基准 runtime/Witness 已通过 | `crates/backends/pvz_portable/{pvzp-rs,backend,tooling}` 已接入原生对象池借用、窄 C ABI、modifier/event 落点与 `run-portable`；游戏按工具链/配置固定构建，脚本作为 DLL 动态加载。MSVC 和 UCRT64/MinGW 插件 build-only、真实宿主运行、Stop/detach 及三轮 Witness acceptance 已验证；更广泛的真实脚本 runtime 仍待验证 |
| **CLI 工具链** | ✅ 完成 | `inject-1051`、`run-pe`、`run-portable`、裸 `.rs`、`rsvz.toml` 与 versioned `prepare-script --json`；三端生成只转发 typed dispatch 的 manifest-only final runner，Bench/Measure 业务参数已从 CLI/wire 删除 |

### 真实 1051 注入状态

迁移后的 **`lowdsl/pe24`** 已在真实 PvZ 1.0.0.1051 中完成运行验收。`witness_acceptance`
还以真实英文 1.0.0.1051 生成了三轮、47,136 个 canonical frame 的 Digest golden；
PE 与 PvZ-Portable 已分别运行同一场景并通过比较。固定场景、seed、停止条件、
复跑命令与证据边界见 [`tests/witness/README.md`](tests/witness/README.md)。

这些结果证明已记录的 PE24/Witness 基准链路，不自动外推到所有脚本、所有后端能力或
尚未覆盖的 roof/moon-night 场景。**`qed16luo`** 当前仍只要求 generated-runner，
不借用 PE24 的 live 结果背书。

此外，`dev-scripts/lowdsl/` 下还有多个 1051 dev script probe（`zombie_motion_probe`、`contact_geometry_probe`、`ice_motion_probe`、
`plant_fixer_plus_probe`、`smart_remove_probe`、`stop_spawning_probe` 等），
连同 `pe24`、`qed16luo`、`re20_direct` 和 `re20_p`，全部 10 个 lowdsl dev script 均已分别通过 1051 与 PE generated final runner build-only。build-only 只证明真实 user package 的 source/link 集成；其中 PE24 与 `re20_p` 进一步通过了固定 seed 单关 runtime，PE24 还通过了吞吐 bench、粗粒度 profile、真实 1051 smoke 与 Witness acceptance。

### 脚本编写示例

用户只需要依赖 `rsvz`。普通即时 API 使用 `rsvz::prelude::*`；完整低层 DSL 使用
`rsvz::dsl::prelude::*`。`#[rsvz::script]` 会在函数体内自动导入后者；如果要在 DSL
脚本里立即种卡，应显式调用 `rsvz::cards::card(...)`，避免与 DSL 的 `card(...)` 混淆。

以下是一个最小脚本结构示例：

```rust
use rsvz::dsl::prelude::*;

fn choose(use_cob: bool) -> LowExpr {
    if use_cob {
        pp() + d(107) + p(15, 7.8)
    } else {
        fume(2, 9)
    }
}

const SHORT: Retention = keep(266);

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    skip_seed_chooser();
    select_cards("IINAJ", [pot, split, fume, star, grave]);
    lineup("LI43bJyUlNTYBS00RdPXWnxsNHHS3FlXRbJUVHQGQ8pW");
    set_zombies("普杆车豚丑矿梯偷跳舞白红");

    waves([1, 4, 7, 11, 14, 17]);
    let op = choose(true);
    359 << &op;
    wave(4);
    824 << pot(to(1292), 5, 9) + star(to(1133), 2, 9)
        + d(751) + star(to(1715), 2, 9);
    (20, -599) << auto_cobs() + set_ice([(4, 9)]);
}
```

详细 DSL 语法参考 `crates/scripting/api/src/dsl/` 下的模块文档。

---

## 项目架构

固定 current 链、crate 职责、依赖门禁、backend token、Opening、
Bench/Measure 和极小 composition root 统一记录在
[`docs/architecture.md`](docs/architecture.md)。代码约定、构建命令和三后端
验证等级见 [`docs/development.md`](docs/development.md)；已完成计划和审计的
阶段性证据保存在 [`docs/history/`](docs/history/) 中。

当前操作通过编译期 current 选择访问具体 backend，功能依赖如下：

```text
rsvz → game → schedule
          各层按需 → current → selected concrete backend
```

1051 injected、PE 和 PvZ-Portable backend 各自拥有物理 host；共享游戏语义位于 core，
用户表达位于顶层 `rsvz`，generated final root 只转发 typed dispatch。

---

## 未完成 / 规划中

以下内容按照 `todos/项目总计划.md` 中的阶段划分排列：

### 短期：PE 第一波收尾与 DSL 语料库

- **PE runtime smoke 扩展**：PE24 已有单关 runtime/关卡完成验证；下一步把同等 runtime/profile 验证扩展到 `qed16luo`，并补更小的回归 gate。
- **PE benchmark stats 扩展**：Bench 现由脚本调用 `rsvz::bench::start(...)` 声明，CLI 只接收通用 `--output`；后续在专门的性能 goal 中补 `qed16luo` 与稳定性能记录。
- **PE 多线程回归**：PE worker-local world/TLS runtime 已能跑 12 worker 2 分钟吞吐 bench；后续继续做固定 seed replay、默认 worker 数、跨脚本 profile 和全局/static 回归审计。
- **PE roof/moon-night 炮时机**：验证 PE cob command-to-impact / flight-time 与 LowDSL roof scheduling 的关系，不从当前 PE24 build-only 结果外推屋顶阵型 parity。
- **高层 DSL**（`pvz_script!` / seml-like 语法）：“定态操作”的简洁声明式语法，类似 `P 225 19 59 | 249 2 8.2+0.8` 这样的单行操作表达。
- **重复波次组 helper**：更多简化标准节奏（PP、PPDD、PPSSDD 等）的语法糖。
- **脚本元数据与 trace 系统**：脚本名称、场景、卡片、波长假设；debug 构建输出时机诊断。
- **compile-only 语料库**：批量编译验证大量脚本，硬化 public API。
- **纯 core 冒烟 harness**：不注入游戏的情况下抽取调度计划、检查时序逻辑。

### 中期：安全审计与 PE 能力补齐

- **1051 安全契约台账**：盘点所有 ABI 路径，对 public scripting API 可触达的每个函数记录前置条件、模式要求、非法状态；将裸指针写入转化为已校验 safe 调用。
- **PE 未实现 capability 台账**：补齐仍 typed unsupported 的 ContactGeometry、SmartRemove、cob delay/recharge rule、kernel-pult projectile policy、plant damage rule、jack/pepper/special-event rule hook 等能力。
- **多线程 PE 基准测试**：在完成全局/static 审计后，并行运行多个独立模拟世界，测量吞吐量并验证脚本跨后端一致性。

### 长期（阶段 6-7）：复杂脚本能力与产品化

- **无炮/键控设施**：僵尸状态与威胁窗口 snapshot、可复用防守 planner、条件动作与状态机 helper。
- **修改器后端**：patch toggle、内存写入、guard/恢复、进程 attach/detach、版本检查。
- **Tauri GUI 应用**：精美 UI、脚本 help、DLL 管理、后端选择、TAS 工具、录制辅助。

---

## 构建与使用

### 环境要求

- 仓库根 `rust-toolchain.toml` 固定的 `nightly-2026-06-04`（含 rustfmt、clippy、i686/x86_64 MSVC 与 x86_64 GNU target）
- 32 位 MSVC 工具链（用于 1051 backend 编译和测试）
- PvZ 1.0.0.1051 原版（用于真实注入测试）

### 基本命令

```powershell
# 根目录会自动使用精确 pin；不要用滚动的 `+nightly`
cargo check -p rsvz
cargo test -p rsvz --no-default-features --features pvz-emulator dsl

# 三个真实后端编译检查
cargo check -p rsvz --features pvz-1-0-0-1051 --no-default-features --target i686-pc-windows-msvc
cargo check -p rsvz --features pvz-emulator --no-default-features
cargo check -p rsvz --features pvz-portable --no-default-features

# 通过三个 final runner 检查 backend-neutral dev script
cargo run -p rsvz-cli -- inject-1051 --dev-script lowdsl\pe24 --build-only
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\pe24 --build-only
cargo run -p rsvz-cli -- run-portable --dev-script witness_baseline_empty --portable-toolchain msvc --build-only --portable-root ..\PvZ-Portable

# 注入 1051 脚本（需要 PvZ 在运行）
cargo run -p rsvz-cli -- inject-1051 --dev-script lowdsl\pe24 --no-wait

# PE generated-runner build-only（不推进模拟）
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\pe24 --build-only
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\qed16luo --build-only

# PE24 runtime smoke
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\pe24 --threads 1 --base-seed 100 --max-levels 1

# 脚本声明的 Refresh Measure；--output 是完整根 JSON
cargo run -p rsvz-cli -- run-pe --dev-script plan10_measure_smoke --threads 1 --base-seed 100 --output target\measure.json

# 收尾检查
cargo fmt --check
git diff --check
```

### 持续集成

仓库内的 [`.github/workflows/ci.yml`](.github/workflows/ci.yml) 会在推送、Pull Request
和手动触发时运行 Windows CI。workflow 文件可以先在本地创建和提交，但只有推送到
GitHub 后，GitHub Actions 才会执行并显示结果。

CI 会并排检出当前仓库、`PvZ-Emulator/pe-bugfix` 和
`PvZ-Portable/restore-1051-gameplay`。如果这些相邻仓库是私有仓库，需要在当前
仓库的 Actions secrets 中配置具有只读 Contents 权限的 `PVZ_REPOS_TOKEN`；公开仓库
则会回退使用内置 `github.token`。真实 1051 注入和 Portable Witness 需要用户持有的
原版游戏资源，在本地自动验证；自动证据不足时补充人工观察，不进入云端 CI。

### 注入说明

`inject-1051` 会在 `target/rsvz/generated/` 下生成 manifest-only final
runner crate；tooling 只生成 `Cargo.toml`（Cargo 可自行生成
`Cargo.lock`），target 的外部 `path` 绑定仓库内唯一固定 final entry，
且 package 不含 `src/` 或 generated `.rs`。runner 直接依赖真实 user package、
顶层 `rsvz[pvz-1-0-0-1051]` 和 injected；固定 root 只转发
`user_script::__rsvz_dispatch`，编译为 32 位 DLL 后注入到游戏进程。
该命令必须提供脚本来源或显式的 opaque `--dll <PATH>`，不再生成无脚本的
standalone backend DLL。
真实注入前先完成与改动相称的非 live 验证。允许自动操作明确识别的已有
目标实例；执行与恢复边界见 [1051 自动实验](docs/development.md#1051-自动实验)。

`run-pe` 会生成直接依赖真实 user package 与 `rsvz[pvz-emulator]` 的
manifest-only PE final runner，外部 `path` 绑定固定 PE final entry；该 root
只把 `user_script::__rsvz_dispatch` 交给预编译 PE host，再编译为原生可执行文件。
`--build-only` 仍只是 source/link gate；当前全部 10 个 lowdsl dev script
均已通过此 gate。PE24 与 `re20_p` 已完成固定 seed 单关 runtime，PE24
还可用于 frame/lifecycle profile。Bench/Measure 的 duration、trial、保护
等业务配置必须写在脚本中；CLI 只保留 `--output`、threads、seed、watchdog、
profile 等运行环境选项。没有 artifact 时不创建 output。
`qed16luo`、roof/moon-night 炮时机和更广泛 capability parity 仍不能
从已有结果外推。

---

## 相关项目

| 项目 | 说明 |
|------|------|
| [AsmVsZombies](https://github.com/vector-wlc/AsmVsZombies) | avz2：本项目重构来源，C++ 编写 |
| AvZ1 | avz 旧版本，语法与 avz2 不兼容 |
| [PvZ-Emulator](https://github.com/PvZ-Unofficial/PvZ-Emulator) | 无头 PvZ 模拟器（C++），PE 后端的底层引擎 |
| [PvZ-Portable](https://github.com/PvZ-Unofficial/PvZ-Portable) | 64 位源码移植，Portable 后端的原生宿主 |
| [PlantsVsZombies-decompilation](https://github.com/PvZ-Unofficial/PlantsVsZombies-decompilation) | 1051 反汇编参考 |
| AvZLib | 第三方 avz 插件库，部分兼容 avz1 和 avz2 |
| seml | 基于 PE 的定态操作脚本语言 + vscode 插件 |
| pvztools / pvztoolkit | 游戏修改器，阵型数据格式来源 |

---

## 开发原则

- **单线程约束**：1051 注入 DLL 运行在游戏主线程中，禁止引入 `Mutex`、`async`、worker thread。
- **后端中立**：core split crates 和 `crates/scripting/api` 绝不包含具体后端地址、ABI、裸指针或内存布局。
- **ABI 验证**：修改游戏 ABI 后对照原始 EXE 和真实调用点复核；DLL 入口检查按 [1051 验证](docs/development.md#pvz-1001051-后端)中的变更范围触发。
- **低层 DSL 语法兼容**：低层 DSL 语法是硬约束；按 [开发与验证](docs/development.md#通用检查)选择受影响后端的编译、generated-runner 与 runtime 检查，并区分各等级证据。
- **脚本只调用 safe API**：脚本不允许越过抽象层直接调用 1051 raw API。

---

## 许可证

RustVsZombies 以 [GNU General Public License v3.0 only](LICENSE) 发布。仓库中单独
标明许可证的第三方组件继续适用其各自的许可证。
