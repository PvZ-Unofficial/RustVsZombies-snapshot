# RustVsZombies 开发与验证

本文保存代码约定和验证流程，按本次修改涉及的章节阅读。用相关源码和测试
确认行为；依赖问题查 Cargo metadata，命令问题查 CLI `--help`，不要求每次
执行全部发现步骤。按实际影响选择最小但足以覆盖风险的检查。

架构与 crate 归属见 [`architecture.md`](architecture.md)。

## 代码与修改原则

- 以 `rustfmt.toml`、`.editorconfig`、Cargo workspace lint 和相邻模块风格为准。
- 使用自然的 Rust 命名，保留有意义的游戏和逆向工程术语，但不复制 AvZ
  为避免命名冲突使用的历史 `A` 前缀。
- 添加抽象或依赖前，优先使用现有类型、helper、宏、模块、标准库或已安装依赖。
- 保持修改局部化，不把功能修改与无关重命名、格式化、模块移动或清理混合。
- 把代码放进拥有其语义的模块；避免万能 helper 和集中式外观层实现。
- 保持公开脚本模块精简。返回强类型错误或强类型“不支持”，不要吞掉失败。
- 局部隔离 `unsafe`、FFI、指针运算和 ABI 操作；安全性注释说明已验证的
  不变量、地址来源或调用约定。
- 注释说明机制来源、不变量、边界和原因，不复述显而易见的代码。
- 沿用现有 handle 和 ID 类型，不向公共层暴露 backend raw pointer。
- 对 nightly `Fn` trait 用户 API，先检查
  `crates/scripting/api/src/callable.rs`；现有声明宏能表达时，不手写第二套
  ZST/`FnOnce`/`FnMut`/`Fn` 样板或新增同用途过程宏。
- 在职责所属行为旁或现有集成测试入口添加测试，不为一次修改新建测试框架。
- 不自行加入通用重构、兼容层或推测性性能流程；等重复的真实任务证明需要。

## 验证等级

必须明确区分：

1. 单元测试或 `cargo check`；
2. generated-runner build-only；
3. 有边界的 PE runtime；
4. 真实 1051 注入；
5. 同一场景的跨后端一致性。

低等级结果不能证明更高等级。benchmark、LTO/PGO、多 worker 吞吐和 profile
是独立性能任务，不属于普通功能 gate。

纯文档修改只核对表述、链接与 `git diff --check`，不触发编译或真实游戏验证。
所选检查通过后，仅在新增改动、失败或尚未覆盖的风险需要时扩大或重复验证。
整条业务链重构的检查清单只在执行该类重构时适用。

## 通用检查

仓库根 `rust-toolchain.toml` 固定工具链；使用普通 `cargo`，不要用滚动的
`cargo +nightly` 绕过版本。

修改单个 core/support 包时，优先运行其测试：

```powershell
cargo test -p <package>
```

无原生后端检查和纯逻辑测试：

```powershell
cargo check -p rsvz --no-default-features
cargo test -p rsvz-game -p rsvz-schedule --no-default-features --lib
```

未选择 `pvz-*` 时使用 NoBackend，不加载游戏能力；需要访问游戏的测试选择
真实 PE。生产条件编译门禁解析 core/API 的 Rust AST，并扫描宏 token；只允许
测试专用条件，后端选择与原生配置条件只能位于 current。

架构门禁沿用 API 的测试依赖，因此需要 PE 构建环境；实际无原生依赖图和
可运行正例由下述独立 current-capabilities package 验证。

```powershell
cargo test -p rsvz --no-default-features
```

game 中确实访问所选后端的单元测试使用测试专用 feature：

```powershell
cargo test -p rsvz-game --no-default-features --features backend-tests,rsvz-current/pvz-emulator --lib
cargo test -p rsvz-game --no-default-features --features backend-tests,rsvz-current/pvz-emulator --doc
```

`backend-tests` 只控制测试代码，不能用于生产条件编译。
实体借用、生命周期和跨线程的 `compile_fail` 文档测试必须选择真实 PE；
NoBackend 下的编译失败可能仅是缺少游戏能力，不能作为借用安全证据。

修改 low DSL：

```powershell
cargo test -p rsvz --no-default-features --features pvz-emulator
```

修改共享 core、runtime、scripting、公开 API 或 backend 选择时，检查三个
最终 feature 表面：

```powershell
cargo check -p rsvz --features pvz-1-0-0-1051 --no-default-features --target i686-pc-windows-msvc
cargo check -p rsvz --features pvz-emulator --no-default-features
cargo check -p rsvz --features pvz-portable --no-default-features --target x86_64-pc-windows-msvc
cargo check -p rsvz --features pvz-portable --no-default-features --target x86_64-pc-windows-gnu
```

原 runtime 回归测试现位于 API 的 runtime_regression 集成测试，调用实际所属的
core 模块；独立 runtime 包已删除：

```powershell
cargo test -p rsvz --test runtime_regression --no-default-features --features pvz-emulator
```

当前普通函数与 callable 能力边界使用独立 Cargo 图检查，包含 NoBackend
纯操作/实体构造正例、真实游戏操作的调用/函数值/实体/callable 负例，以及四种
真实目标的能力、炮、卡和 DSL 重载表面：

```powershell
& ./crates/scripting/api/tests/current-capabilities/verify.ps1
```

该命令包含能力/选择编译检查、实际 Cargo 图检查，以及 `smart_remove_pe` 的
默认视觉与显式无高亮入口实际运行和 `key_registry_1051` 的登记/句柄测试。键盘例子
不读取游戏或键盘，全部属于非 live 验证。脚本临时使用仓库 target 复用构建产物。

图检查使用目标过滤的 Cargo metadata，按实际包身份验证依赖别名、target 和
host feature 下的边界；Witness tooling 使用排除 dev-dependency 的 Cargo tree，
避免其 PE 测试依赖污染 feature 选择。无原生图必须包含 current/NoBackend，
并排除真实 backend、pe-rs 和 pvzp-rs；源码文本门禁作为补充。
常用能力门禁也实际编译 Witness tooling-only，不能只用 dev feature 合并后的
capture 测试或 Cargo 图证明 backend-free 构建。

图检查也可单独执行，或在 build-only 后检查一个实际 generated runner：

```powershell
python crates/scripting/api/tests/check_dependency_graphs.py
python crates/scripting/api/tests/check_dependency_graphs.py --manifest <generated-runner/Cargo.toml> --target <target>
```

修改过程宏或 support 宏时运行对应现有测试：

```powershell
cargo test -p rsvz-macros
cargo test -p rsvz-abi-macros --lib
cargo test -p rsvz-abi-macros --target i686-pc-windows-msvc --test ui
cargo test -p rsvz-abi-macros --target i686-pc-windows-msvc --features verify-exe --test verify_ui
```

按原版 EXE 静态复核 1051 ABI wrapper 时，显式启用审计 feature：

```powershell
$env:RSVZ_PVZ1051_EXE = '<PlantsVsZombies.exe>'
cargo check -p rsvz-pvz1051-injected --no-default-features --features verify-abi --target i686-pc-windows-msvc
```

该检查只把可证明的矛盾视为错误；无法从静态代码证明的属性以 warning 汇总，
不能替代下文要求的 objdump 和真实调用点复核。

修改可选 Witness 扩展时使用短 target 目录避免 Windows 原生构建路径超限：

```powershell
$env:CARGO_TARGET_DIR = 'C:\rsvz-wt-target'
cargo check --manifest-path extensions/rsvz-witness/Cargo.toml --no-default-features --features tooling --bin witness-diff
cargo test --manifest-path extensions/rsvz-witness/Cargo.toml --no-default-features --features capture,tooling
```

capture 测试的 PE 选择由 dev-dependency 提供；正常 tooling 图使用 NoBackend，
不得选择真实 backend。capture 仅启用扩展的采集模块，不代替后端选择。
随后用 `witness_baseline_empty` 分别构建 PE 与 1051 generated runner，证明
外部依赖能从最终 Cargo graph 取得所选 current backend；单独检查扩展 crate
不能替代这一步。

## Generated runner

dev script 保持 backend-neutral，不用直接检查 `dev-scripts/...` 代替最终
Cargo graph。修改脚本、公开 API、runtime、backend host 或 tooling 时，对
实际受影响脚本分别构建对应 final runner：

```powershell
cargo run -p rsvz-cli -- inject-1051 --dev-script lowdsl\<name> --build-only
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\<name> --build-only
cargo run -p rsvz-cli -- run-portable --dev-script lowdsl\<name> --portable-toolchain msvc --build-only
cargo run -p rsvz-cli -- run-portable --dev-script lowdsl\<name> --portable-toolchain mingw-ucrt64 --build-only
```

`--build-only` 只证明 manifest-only generated runner 与最终 Cargo graph
能够构建，不证明 runtime 语义。

修改 fixed final entry、PE host 或 PE tooling 时，使用上面的 `run-pe --build-only`
构建实际 shipping binary；它同时验证 manifest-only package、fixed entry 与最终
Cargo graph，不另建 test-harness runner。

## PvZ-Emulator 后端

支持常规六场景及 `SceneKind::MushroomGarden`（蘑菇园无尽）。MGE 使用夜间五行
陆地规则，不生成墓碑，允许冰车，舞王只在中三路自然出场。1051/Portable 的
Opening 场景切换使用原生蘑菇园背景资源；无需以 NE 贴图代替。

`pe-rs/build.rs` 优先使用 `PE_RS_SOURCE_DIR` 指定的源码；未设置时使用发布包的
`vendor/pvz-emulator`，不存在则从 RustVsZombies 的同级目录解析模拟器源码。
显式配置无效时直接报错，不回退。vendor 只在打包时导入，不作为主库维护副本：

```text
<workspace>/
├── RustVsZombies/
└── PvZ-Emulator/
```

PE 默认使用 `clang-cl + Ninja + lld-link` 构建原生模拟器与 bridge；
`--pe-toolchain msvc` 保留为显式回退。Thin/Full LTO 仍分别通过
`--pe-toolchain clang-thin-lto` 和 `--pe-toolchain clang-full-lto` 启用，
普通默认构建不启用 LTO。

脚本可用 `rsvz::setup::select_cards_with(fn() -> RuntimeResult<Vec<CardSelection>>)`
在 Opening 完成场景和出怪初始化后生成本轮卡组。回调每轮只执行一次，发生在实际
选卡之前；固定 `select_cards` 与动态入口以后一次配置为准。回调不持有 setup 借用，
可以读取当前出怪类型；返回的卡组沿用现有数量、重复卡和模仿卡校验。

PE 续关时保留旧卡槽作为冷却来源，新选择暂存在 backend 的十卡缓冲中；
`finish_card_selection` 使用 PE 原有 `world::select_plants` 一次提交，按卡种
继承冷却（包括模仿目标变化）。不改变 Board 的 UI，也不额外推进战斗帧。
原生图形选卡器逐次选卡已经提交，该完成动作无需再次修改卡槽。

`rsvz::core::modifier::set_dance_mode(bool)` 对应一次原生 `Board::SetDanceMode`，
会重设符合条件的普通/路障/铁桶行走动画。每次调用有实际效果；与持续的
`CommonZombieDanceBackend` 测算步态选项及舞王 `MaidCheats` 是不同能力。

`rsvz::measure::trace_round_end_sun()`每个脚本代注册一次，在真正的RoundComplete
结算阶段输出worker、world epoch、累计已通过轮数及阳光；不是每帧读取阳光。
`PlantFixer::set_trace(true)`按需记录实际修补尝试、结果与种卡前后阳光，默认关闭；
常规冷却轮询不输出记录。脚本决策解释应由脚本提供，不能从一次拒种推测其意图。

PE PGO 使用 `--pgo-generate <目录>` 构建训练程序，正常退出后用同版本 LLVM 的
`llvm-profdata merge` 合并该次训练的 `.profraw`，再以 `--pgo-use <文件>` 重编译。
训练和最终构建应保持同一脚本、后端与 LTO 模式；默认插桩计数不是原子的，可先
用单 worker 训练，性能评估再使用实际 worker 数。训练输出采用 `%m-%p` 命名。
tooling 把 profdata 按内容指纹保存到 `target/rsvz/pgo`，向 Rust 和 C++ 传入该副本，
使数据变化触发重编译；不得为此添加或改变 `-Cmetadata`，否则 Rust 训练符号会失配。
插桩依赖可以提前构建复用；新训练数据的使用阶段仍需重新编译受影响的依赖。
验证 PGO 时除了跑分，还应检查训练文件包含两种语言的数据，并检查 profile-use
产物中的热点函数计数/分支权重，不能仅凭构建成功认定数据已经命中。

诊断 native build 前先确认布局和当前 CMake/toolchain 配置。按影响选择：

```powershell
cargo test -p pe-rs
cargo check -p rsvz-pvz-emulator-backend
cargo test -p rsvz-pvz-emulator-tooling
cargo check -p rsvz --features pvz-emulator --no-default-features
```

只有 build-only 通过后，才运行单 worker、固定 seed、有停止边界的功能 smoke：

```powershell
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\pe24 --threads 1 --base-seed 100 --max-levels 1
cargo run -p rsvz-cli -- run-pe --dev-script lowdsl\re20_p --threads 1 --base-seed 100 --max-levels 1
```

Measure 的 quota、reset 和 artifact 可使用 `plan10_measure_smoke` 做小规模
单/多 worker 验证。只运行与修改相关的脚本；不要把 PE24 外推到
qed16luo、roof/moon-night 或其他 capability。

DamageNarrow 的漏小鬼首败链路使用 `measure_imp_leak_smoke`：固定父巨投掷、
让小鬼累计啃食受保护坚果，并验证 200cs 阈值在 201cs 封存 `ImpLeak` 及其
结构化失败样本。

`measure_plant_loss_smoke` 连续运行十轮：受保护坚果被两种车辆压毁、模仿冰被
两种车辆压毁、正常变身反例，以及模仿冰被啃死、炸毁、砸毁、篮球击杀和偷走。
慢速攻击场景延长模仿者倒计时并将 HP 设为 1；普通变身轮使用原始倒计时。
公共 observer 逐轮校验破坏事件的来源；预期车压失败 2、模仿冰损失 7、成功 1、
invalid 0。PE 可用 `--threads 1 --base-seed 100`，原生端使用隔离实例及只读存档
模板。每次 `--output` 必须使用尚不存在的路径。

`--seed` 给所有 worker 同一 seed；`--base-seed` 派生不同但可复现的 seed。
每个 worker 独占一个 world 和一套 runtime TLS，不能共享 world，也不能调用
C++ `world::update_all`。

`--output` 写脚本发布的完整根 artifact JSON；`--stats`/`--json` 只控制展示。
`--profile`/`--profile-detail` 会增加开销，只用于回答相应 profile 问题，
不能和未开启 profile 的吞吐直接比较。

## PvZ-Portable 后端

游戏通过自身 CMake 构建原生 SDK 和导入库；`pvzp-rs` 使用配套 SDK，以 clang-cl
或 UCRT64 ABI Clang 编译 bridge 并静态链接进脚本 DLL。Release 以 Rust/Clang
bitcode 和 LLD 执行 DLL 内 ThinLTO，Debug 使用普通对象。游戏不编译 bridge，
不需要 Rust。Portable backend 的 host 实现及依赖常编译，没有 host 或
native-exports feature；PE/1051 的 feature 保持原样。

检查最终插件的跨语言 ThinLTO 时，先生成 runner 及配套 Release SDK，再运行
`python crates/backends/pvz_portable/pvzp-rs/tests/check_lto.py --manifest <runner/Cargo.toml> --sdk <sdk/Release> --abi <msvc|gnu> --output <report.json>`。
检查器自行在全新目录构建同一个 runner，只接受这次链接产生的 DLL 和 IR，保留
命令、日志及文件摘要；`--build-dir` 可指定尚不存在的短路径。不能用旧 DLL 和
deps 中按时间挑选的 bitcode 证明一次最终链接。检查器自身的负例由
`python crates/backends/pvz_portable/pvzp-rs/tests/test_check_lto.py` 验证。

```powershell
cargo test -p pvzp-rs -p rsvz-pvz-portable-backend -p rsvz-pvz-portable-tooling
cargo test -p pvzp-rs --target x86_64-pc-windows-gnu
cargo run -p rsvz-cli -- run-portable --dev-script lowdsl/pe24 --portable-toolchain msvc --release --build-only
cargo run -p rsvz-cli -- run-portable --dev-script lowdsl/pe24 --portable-toolchain mingw-ucrt64 --release --build-only
```

Tooling 先构建游戏及 `<native>/sdk/<config>`，再通过 `PVZP_SDK` 构建插件；
只修改脚本不会重新链接游戏。手工构建消费 `pvzp-rs` 的 DLL 时，同样设置
`PVZP_SDK` 指向同构建配套 SDK。游戏只读取 `PVZP_PLUGIN`，rsvz 插件读取
`RSVZ_PORTABLE_CONFIG`；资源仍由用户安排。布局在任何对象访问和回调登记前
完成 EXE/Clang 与 Clang/Rust 两级校验。SDK 契约及独立加载负例见
[Portable SDK](../../PvZ-Portable/docs/plugin-sdk.md)。

能力门禁中的 Portable refresh_trials/refresh_for 必须编译通过，1051 的缺失
能力负例保留。同步输入使用独占 BackendScope；实体/迭代器的借用负例及
共享入口冲突必须仍然失败。模态窗口内仅执行原生 UI，普通脚本暂停；关闭和
卸载等最外层回调退出后完成。输入返回后沿既有 epoch 重采样 phase/clock。

两种工具链都实际运行原有 `plan10_measure_smoke` 的两次 trial/reset；
`pvzp-rs/tests/run_native.py` 通过真实游戏验证新增能力和回调寿命，使用临时
档案。DisplayBackend 因缺少 paint/显示期契约继续不实现。构建检查、原生
运行、人工画面/声音验收与性能对比是不同证据等级，见
[本轮记录](history/plan24-portable-sdk.md)。

## PvZ 1.0.0.1051 后端

`rsvz-pvz1051-injected` 的库、单测和 doctest 均只支持
`i686-pc-windows-msvc`，相关 Cargo 命令必须显式指定该 target。

修改 1051 raw ABI、injected backend/host、hook、patch、injector、tooling、
fixed final entry 或 `pvz-1-0-0-1051` feature 时，按影响选择：

```powershell
cargo test -p rsvz-pvz1051-injected --target i686-pc-windows-msvc --no-default-features -- --test-threads=1
cargo check -p rsvz-pvz1051-injected --target i686-pc-windows-msvc
cargo check -p rsvz --features pvz-1-0-0-1051 --no-default-features --target i686-pc-windows-msvc
```

1051 host 及其 serde 依赖常编译，不再提供 host feature；平台 compile_error 门禁保留。
ABI 过程宏在编译主机运行，只生成 x86-32 调用；展开编译测试明确使用 i686 目标，
其他目标的默认测试运行会将这两项集成测试标为 ignored，解析/展开单元测试仍运行。

当前 `inject-1051` 必须接收裸 `.rs`、script crate、dev script 或
`--dll <PATH>`；`--dll` 是 opaque 外部产物入口，不是 standalone backend
构建模式。

函数地址、参数载体、寄存器、栈布局、清栈规则、返回寄存器或 clobber 有变化时：

1. 对原始 1.0.0.1051 EXE 检查该函数及真实调用点的 objdump/反汇编。
2. 记录实际寄存器、栈和分支证据，再核对 wrapper。
3. 运行相关 `rsvz-abi-macros` 测试和 i686 构建。

DLL 入口检查单独按影响范围触发：修改 `DllMain`/recorder、生命周期导出、
fixed entry 的链接接入，或影响它们的 target、工具链、CRT、链接及 LTO 配置时，
核对实际 DLL 的 PE entry、CRT 到 record-only `DllMain` 的调用链、
`extern "system"` 参数和返回清栈、export table。入口实现或链接链变化时覆盖
debug、release 和 fat-LTO；只调整一种构建配置时检查受影响配置。普通游戏
ABI wrapper 修改不自动触发整套 DLL 入口检查。

这些检查验证的是最终链接产物：加载必须经过正常 CRT 初始化并接通 recorder，
调用双方必须使用相同栈约定，正式导出必须存在且指向有效代码。源码中存在
某个函数或普通单元测试通过，不能单独证明这些性质。

不得用惯例性的 thiscall/stdcall/cdecl 猜测代替证据。静态检查、PE runtime
和 build-only 都不能证明真实 hook、reset、UI 恢复或卸载安全。

injected runtime 始终按严格单线程验证。

## 1051 自动实验

[AGENTS.md](../AGENTS.md#验证)允许任务范围内的自动实验，包含已有目标实例。
先完成所有适用的非 live 检查，再按实际实例选择入口：

| 实例 | 入口与收尾 |
| --- | --- |
| 本次新建的专用实例 | `run-1051` 或下节的 `scripts/check-live.py`；工具负责期限、安全卸载、模块消失与存活检查，以及回收本次进程 |
| 已运行且能明确识别的目标实例 | `inject-1051 --pid <PID>`；按当前 CLI 帮助运行脚本，记录目标与停止条件，需要卸载时使用既有 `injector --unload --pid <PID>` 安全握手 |

已有实例的 `--wait-timeout-secs` 仅限制等待报告，超时后脚本仍可能运行；
命令返回或脚本报告 success 不证明 DLL 已卸载。验证涉及卸载时，须确认模块
消失及卸载后进程继续存活。现有同步错误弹窗曾阻塞 1051 的正常清理，见
[实测记录](history/live-smoke-validation.md#最终结果)；不能把等待超时当作实验已停止。
自建实例由工具有界回收；未经授权不终止用户已有进程、不覆盖原存档，安全
卸载被拒绝后不得强制 FreeLibrary。已有实例的游戏动作不保证自动回滚。

记录脚本、提交、构建模式、PID、场景、停止条件及实际断言、日志和必要的
画面证据。出现异常时保留证据，先复现和定位；不凭猜测修改游戏机制，也不
改写验收预期来消除失败。只有自动证据不足以判断待验性质时才请求人工观察，
无须在每次实验前后固定插入人工确认。

## 基本真实游戏自动验证

`scripts/check-live.py` 串行运行同一普通 `live_smoke` 脚本，自动进入泳池战斗，
观察至少 30 次原生更新，检查阳光写读及恢复，并核对 DLL 已卸载、游戏仍存活
500 毫秒后才关闭本次实例。它不验证画面、听感、模态窗口或八项缺失能力。

```powershell
python scripts/check-live.py --backend all --check-timeout `
  --game-1051 C:\path\to\PlantsVsZombies.exe `
  --profile-1051 C:\path\to\1051\userdata `
  --resources C:\path\to\1051-resources `
  --profile-portable C:\path\to\portable-savedir
```

1051 模板目录内须有 `users.dat`；Portable 模板目录内须有 `userdata/users.dat`。
模板只读复制，入口保存前后文件内容摘要；默认输出 `target/live-tests/<运行时间>/`。
`--check-timeout` 另用同一脚本的 `never-stop` feature 验证受控超时与进程回收。
所有适用的非 live 检查应在第一次运行此命令前完成。

`run-1051 --game <EXE> --profile-template <userdata> --dev-script live_smoke`
复用既有构建和注入协议，但创建并拥有测试进程。启动前锁定原版 EXE SHA-256，
在暂停的子进程中校验并修改 `0x553B05` 的六字节跳转，让存档使用测试目录；
不修改磁盘 EXE，不改变玩法。工作目录及 `-changedir="路径"` 同时指定测试目录。
该机制隔离文件和存档，不隔离 Windows 注册表设置。已有 1051 实例会使预检失败。

Portable 使用 `run-portable --close-game-on-finish --profile-template <savedir>`；
普通 `run-portable` 仍让游戏在脚本结束后继续运行。测试命令禁止 `--no-wait`
及自行覆盖 `-savedir`。两端构建后开始计时，运行默认 60 秒，错误清理另有上限。
脚本报告不是卸载证明；工具必须完成模块枚举、存活观察和本次进程清理。

## 收尾

所有源码修改完成后运行：

```powershell
cargo fmt --check
git diff --check
```

runtime 或 live 验证记录应包含实际提交、脚本、场景、seed、停止条件、构建
配置和 worker 数。阶段性证据放入 `docs/history/`；它不取代当前源码和测试。

## 整条当前业务链重构验证

- 按功能审计公开入口、helper、扩展 trait、回调与状态，不能仅搜索 `<B>`；
  保留显式访问参数时记录其物理入口、独占失效或直接实体借用用途。
- 普通调度器以数据测试；当前 API 和实体使用真实 PE world。裁剪重复 fake
  测试，保留部分提交、预约清理等窄失败覆盖，不为测试保留第二套业务实现。
- 验证 Frame 只持有 token 借用、实际 Board 查询失败才中止；迭代器按需获取
  world/pool 且保持扫描上限与顺序。检查逃逸、独占冲突、同帧死亡和 unwind 恢复。
  性能对照使用相同 release 场景预热后五次 worker wall_ns 的中位数，排除编译时间。
- 对照架构中的错误策略验证局部中止、重复 tick 重试、注册事务回滚和
  显式 Stop/Pause/fatal 优先；error 日志不记执行失败，操作报告仅记账一次。
- 保存并对照 Witness 字段、缺失值和规范输出，不重新生成期望值掩盖变化。
- 交付前完成四目标与 generated runner build-only、固定 seed 有停止边界的
  PE24/re20_p、测算/reset，以及 Witness tooling 无 backend 构建。
