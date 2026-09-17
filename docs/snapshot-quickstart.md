# Windows 临时使用快照

此快照用于让 AI 协助迁移 AvZ 脚本、在 PE 测算并注入英文 1.0.0.1051。
API 仍可能变化。不要把一个脚本的验证结果推广到全部后端和阵型。
发布包的 `SNAPSHOT.json` 记录源码提交与工具版本。

## 安装

从 Release 下载 `rsvz-windows-tools.zip`，解压到自己的目录。
最新完整 ZIP 包含本次发布的全部源码、群曾脚本、README、AGENTS.md、使用指南和固定工具链，
无需另行下载文档、源码或独立脚本仓库。后续更新也以重新打包的完整 ZIP 交付；
已下载的旧包不会自动更新。更新已有安装时保留个人脚本和 `LOCAL.md`，先合并个人修改。
群曾脚本使用库内 `--dev-script lowdsl/mge_qunzeng`，无需独立脚本仓库。

工具包自带固定版本 Rust（x64/x86 MSVC 标准库）、LLVM、CMake、Ninja、RSVZ CLI，
以及 `vendor/pvz-emulator`；不依赖相邻开发仓库。GitHub 自动生成的 Source ZIP
和 `rsvz-source.zip` 不包含工具链；后者包含同版 PE。

**仍需 Microsoft C++ Build Tools（x86/x64）和 Windows SDK，包内不附带它们。**
可从 https://visualstudio.microsoft.com/visual-cpp-build-tools/ 安装，选择“使用 C++ 的桌面开发”。
若当前任务不允许使用目录外的工具，不能通过调用本机 VS/SDK 绕过限制：报告这一缺口。
游戏和用户存档也不随包分发。首次 Cargo 构建会从 crates.io 下载依赖，工具包并非完全离线。

安装时先检查现有环境，只补缺失项：

- 包内 Rust、LLVM、CMake、Ninja 缺失或损坏时，从对应 Release 补齐，保留固定版本；
  不改用系统里的其他版本或随意安装最新版 LLVM。
- 缺少 MSVC x86/x64 C++ 工具或 Windows SDK 时，使用上面的微软官方安装入口。
  已有 Visual Studio / Build Tools 时，在安装器中修改现有安装，补齐“使用 C++ 的桌面开发”
  所需的 MSVC x86/x64 工具与 Windows SDK，无须安装完整 Visual Studio IDE。
- 完成后用下面的入口检查包内工具与 MSVC 环境；需要 1051 时也检查 x86 环境。
  工具版本检查和 CLI 启动成功不等于脚本编译成功，首次使用仍需针对所选后端构建验证。
- “安装好 RSVZ”包含补齐必要依赖。安装器要求提权或重启、或者当前访问限制不允许
  这些操作时，说明具体障碍，不擅自重启或绕过限制。

在解压目录运行（PowerShell）：

```powershell
.\scripts\rsvz-snapshot.ps1 --help
.\scripts\rsvz-snapshot.ps1 run-pe --help
```

包装脚本仅为本进程设置包内工具路径并加载 MSVC 环境，不修改系统 PATH。
直接使用 Cargo 时先执行 `. .\scripts\snapshot-env.ps1`；1051 编译用 `-Arch x86`。
Rust 固定为 nightly-2026-06-04，LLVM 固定使用包内版本，不自行升级其中一方。
源代码构建可使用 `PE_RS_SOURCE_DIR` 指定 PE，否则依次查找包内 vendor 和同级 PE。

## 找到和迁移脚本

无炮示例：`dev-scripts/lowdsl/mge_qunzeng`（独立仓库的固定四喷版，不偷阳光菇）、
`dev-scripts/lowdsl/pe96`。定时炮阵示例：`dev-scripts/lowdsl/pe24`。
MGE 示例基于 https://github.com/xqbzd/xqbzd1AVZ 中的群曾脚本，另含独立仓库中的安全复制冰及固定四喷打法；具体来源与有意差异见示例 README。
比较时记录具体 AvZ 文件与提交，先列出打法差异，再按用户选择移植。

新脚本可放在独立目录，CLI 支持裸 `.rs`（含相邻模块及可选 `rsvz.toml`）和
`--script-crate <目录或Cargo.toml>`。复制 crate 后修正其 `rsvz` 路径依赖，
指向本包的 `crates/scripting/api`；不要改到其他后端或另一个版本的库。

- 即时操作查 `rsvz::prelude`，定时操作查 `rsvz::dsl`；不要混淆注册操作和当帧操作。
- AvZ 全局状态要按 PE worker 隔离；参考示例的 `thread_local!` 与 world epoch，
  区分正常续关和开始新的独立游戏，不能每关重置本该保留的状态。
- 保留原始阵型、选卡时机、阳光、出怪、舞王选项及卡序。显示功能可不移植；
  原脚本问题与功能变化应单独说明，不把新打法冒充等价迁移。
- 使用公开 API。遇到能力不足先报告，不能靠修改游戏结果伪造成功。
- 游戏机制先查 https://wiki.pvz1.com/；AvZ API 查其官方源码或相应插件。
  Wiki 无法代替本包当前 API。必要时按用户授权下载资料，不依赖作者本机资料目录。
- 冷却、已发生事件和可观察状态可以辅助分析；不要用未来随机结果作为脚本决策依据。

## MGE 十分钟测算

```powershell
New-Item -ItemType Directory -Force results\mge | Out-Null
$env:MGE_TRACE_DIR = (Resolve-Path results\mge).Path
$env:MGE_SECONDS = '600'
$env:MGE_TRACE_LOSS = '1'
$env:MGE_TRACE_DECISIONS = '1'
.\scripts\rsvz-snapshot.ps1 run-pe --dev-script lowdsl/mge_qunzeng `
  --release --pe-toolchain clang-thin-lto --threads 12 `
  --performance-window-secs 600 --output results\mge\summary.json --json
```

先做短测试（如 `MGE_SECONDS=10`），确认后换新输出目录跑正式测算。
logger 使用 create_new，不能复用已有 trace 文件。线程数按机器能力选择。
默认初始阳光 8000、已完成轮数 1500；分别可用 `MGE_SUN`、`MGE_ROUNDS` 设置。
它们是初始条件，不代表本次实测已经通过这么多轮。

MGE 示例使用 `ExpectedPassesEnd::FinishActive`：十分钟后停止接纳新样本，
存活样本继续到失败；`--performance-window-secs 600` 单独统计前十分钟性能。
不要用 `--max-wall-secs 600` 代替它，否则会截断样本。启动后检查一次，随后长时间
阻塞等待，避免频繁轮询。若平台限制单次等待时间，在客户端安静等待，不反复读取结果。

记录吞吐（所有 worker 总帧/秒）、样本数、平均通过轮数及统计窗口。一轮是
20 波、2 面旗帜。不同打法采用独立随机种子，不比较相同种子的通关数；固定种子
只用于复现失败。没有基线时不能声称成绩有所提高。

## 分析失败

先读 `summary.json`，再按 worker/epoch/rounds 关联 trace。植物死亡可能来自多个
事件，按植物身份与时间核对，不能简单数行。关注失败前最后一轮和修补经过，
不能把很早已修复的损失当成最后破阵的原因，也不能只按进家僵尸归因。

库已有 `trace_plant_losses`、`trace_round_end_sun`、`PlantFixer::set_trace(true)`。
它们分别给出损失、轮末阳光、实际修补尝试。脚本决策原因需要脚本按需记录：
关键种卡失败时的阳光、卡片、格子和拒种原因；不要每帧完整输出场面。
区分正常维护耗尽阳光与先破阵后大量补种耗尽阳光。缺少事件时明确说明，
可加针对性 trace 再复现，不能输出没有证据的原因百分比。

## 注入录像

先用同一脚本完成 1051 build-only：

```powershell
.\scripts\rsvz-snapshot.ps1 inject-1051 --dev-script lowdsl/mge_qunzeng --build-only
```

用户提供英文 PvZ 1.0.0.1051 游戏，准备可进入无尽的存档并启动游戏。
录像配置不启用测算限时、批量重开或跳帧；MGE 示例用 `MGE_SECONDS=0` 关闭测算。
初始阳光/轮数的测算设置属于 WorldResetConfig，关闭测算后不会自动沿用，
录像前需通过现有 setup API 或用户游戏设置核对这些初始条件。

```powershell
$env:MGE_SECONDS = '0'
Remove-Item Env:MGE_TRACE_DIR -ErrorAction SilentlyContinue
.\scripts\rsvz-snapshot.ps1 inject-1051 --dev-script lowdsl/mge_qunzeng --no-wait
```

多个游戏实例时明确指定 `--pid`。由用户开启录屏软件。`run-1051` 是会结束并
回收专用游戏实例的实验命令，不是录像默认入口。注入前按任务授权确认目标实例；
停止可通过注入器的 `--unload --pid <PID>`（先查看其 `--help`）完成。

## 范围

本次快照交付 PE 和 1051 路径。Portable 源码接口保留，但其宿主与资源不在包内，
不运行依赖 Portable 的整工作区测试。历史报告不是新脚本正确性的保证。
测试脚本、正式用户脚本和测算结果分别保存；不要把测算产物提交进上游 AvZ PR。
