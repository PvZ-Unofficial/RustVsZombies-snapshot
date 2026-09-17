# RustVsZombies 开发门禁

只需安装、迁移脚本、测算或注入时，先读 [快照使用指南](docs/snapshot-quickstart.md)。
下文为库开发门禁，不要求脚本使用者执行所有后端的开发测试。

## 脚本使用约定

- 安装或使用前，按 [使用指南的安装部分](docs/snapshot-quickstart.md#安装) 检查工具链。
  优先使用发布包内的固定版本 Rust、LLVM、CMake、Ninja；缺失时补齐对应版本，
  不擅自升级、替换已有工具链。MSVC x86/x64 C++ 工具或 Windows SDK 缺失时，
  按官方安装方式补齐缺少的组件，不重复安装整套环境。
  用户已要求“安装好 RSVZ”时，必要依赖安装属于任务范围，无须重复询问；需要提权、
  重启或与用户访问限制冲突时，说明具体障碍并按当前权限处理，不擅自重启或绕过限制。
- 请求运行 1051 时，先读取仓库根目录 `LOCAL.md` 中的游戏 EXE 路径并检查文件存在。
  尚未记录时，优先使用用户已经提供的路径；没有可用路径或路径失效时，再询问用户并记录。
  `LOCAL.md` 是 AI 使用的本机备忘，CLI 不会自动读取；不要提交个人路径或扫描其他项目来猜位置。
- 安全三叶草和安全模仿寒冰菇默认复用 `rsvz::is_safe_blover`、
  `rsvz::is_safe_imitator_ice`，不要为迁移重复实现。确有脚本特化逻辑、无法判断内置
  语义是否适用时，先说明差异并向用户确认。
- 群曾脚本的维护入口为 `dev-scripts/lowdsl/mge_qunzeng`，保持固定四喷、不偷阳光菇
  的既定打法；不要另建独立仓库作为维护副本，也不要未经当前任务要求切换实验策略。
- 长时间测算或上传在开始时检查一次，随后阻塞等待完成；不要频繁轮询或反复唤醒模型。
  正式打法比较使用独立随机种子，固定种子只用于复现失败。

## 库开发约定

本文保存开发硬门禁。按任务读取下列相关章节，无须每次通读所有文档。
当前 API、命令和进度以源码、Cargo 清单、测试及 CLI `--help` 为准。

| 本次任务 | 阅读入口 |
| --- | --- |
| 跨 crate 修改、新 API、能力归属或依赖变化 | [依赖边界](docs/architecture.md#依赖边界)、[Crate 职责](docs/architecture.md#crate-职责) |
| 实体借用、调度、hook 或生命周期变化 | [Board 访问](docs/architecture.md#board-访问和错误边界)、[Runtime 与 host](docs/architecture.md#runtime-与-host-职责)、[生命周期](docs/architecture.md#生命周期与-opening) 中受影响的章节 |
| Rust 源码或构建配置修改 | [代码约定](docs/development.md#代码与修改原则)、[通用检查](docs/development.md#通用检查)，再读受影响后端章节 |
| 1051 ABI、DLL 入口或真实实验 | [1051 验证](docs/development.md#pvz-1001051-后端)、[1051 自动实验](docs/development.md#1051-自动实验) 中适用的部分 |
| PE 或 Portable 接入 | [PE 后端](docs/development.md#pvz-emulator-后端)或 [Portable 后端](docs/development.md#pvz-portable-后端) |
| 纯文档修改 | 核对受影响表述、链接和差异，不触发代码构建或 runtime 检查 |

## 架构

- 目标依赖图为 `rsvz -> game -> schedule`，三者的当前操作可依赖
  `core/current -> selected concrete backend`；current 是唯一后端选择处。
  未选择真实后端时使用 `crates/backends/none` 的 `NoBackend`，只提供类型与
  访问边界，不实现游戏能力或伪造成功操作。
  `model`、`backend-api`、`profiling` 不依赖 current 或具体 backend。
- concrete backend（包括 `host` feature）在 backend 文件夹外只允许依赖
  backend-api、model 和实际需要的 profiling；不得依赖 game、schedule、
  current、顶层 rsvz、user package 或 tooling。
- 功能自持 TLS：玩法、诊断、注册和生命周期协调属于 game，调度实例属于
  schedule，短期 backend 访问作用域属于各具体 backend。backend-api 提供
  无全局实例的公共访问工具。不得创建统一 Session/God Object、中央资源表
  或通用 driver；cob、ice_filler 等组件并列且按需初始化。
- 独立 rsvz-runtime 包、Runtime／Current*Runtime 绑定和 rsvz::current
  整体门面已删除，不得重新引入。
- `rsvz-current` 只做编译期 backend 与 backend-local dispatch
  类型选择，不保存 runtime 状态或 policy。
- core（除 `crates/core/current`）及 `crates/scripting/api/src` 的生产代码禁止
  `cfg`、`cfg_attr` 和 `cfg!`，包括宏模板中的条件编译。仅测试代码可使用
  `cfg(test)` 或必需包含 test 的复合条件；`any(test, feature = ...)` 不属于
  测试专用条件。能力差异在实际使用处用 trait bound 表达，原生配置检查集中
  在 current；不得用空实现、常量假数据或新增 feature 恢复生产条件分支。
- 顶层 `crates/scripting/api/src/**` 只保留必要重载、植物简写、便捷别名、DSL、
  Rust 输入适配和 core 重导出；解析后的输入模型、时间
  计划和可复用语义属于 core。不得实现
  1051 ABI/hook、PE world/worker、backend wire、文件发布或物理 host。
- 1051 与 PE 各自拥有物理 host。不得创建统一 Host trait、driver、
  controller、callback table、event/action protocol 或共享循环。
- 两端 host 只接收一个非泛型、无捕获的 typed `DispatchEntry`；fixed
  final root 只转发 `user_script::__rsvz_dispatch`，不得加入第二 callback、
  循环、TLS、配置、I/O、状态机或业务分支。
- 唯一额外 native-to-Rust ingress 是 `NativeEventSink` 中已列明的固定函数
  指针，用于效果发生点的同步 instrumentation；它不是 host dispatch。该边界
  只能传递小型 `Copy` fact/token/decision，不得携带 closure、context/raw
  object reference，不得执行用户回调、推进 backend、分配、持锁或重入。
- tooling 不得进入最终 artifact 的 Cargo graph。

详细 crate 职责、代码放置规则和架构原理见
[`docs/architecture.md`](docs/architecture.md)，可执行门禁见
`crates/scripting/api/tests/architecture.rs`。

## 实现约束

- Portable 接入禁止为补偿 Rust/C++ 边界、减少 FFI 调用或绕过不合理绑定而新增
  派生地址缓存、查询结果缓存或镜像状态。原生 C++ 同样需要的回调登记、配置、
  对象所有权和恢复历史值按职责保存；不得按名称放行。若确实必须引入上述缓存，
  立即停止全部实施并报告必要性、原生对照和证据，不继续收尾、回滚或提交。

- `rsvz-backend-api` 提供原子事实和动作、共享错误/artifact/Opening 边界契约；
  backend-neutral 组合规则放进 `rsvz-game`。按操作语义区分正常实现、合法
  no-op 和真正缺失的能力；PE 自动收集是保留输入校验的成功 no-op，不安装
  逐帧空任务。完整能力缺失时不实现对应 trait，部分参数或状态不支持时保留
  typed unsupported；不能用 no-op 伪造要求真实效果的操作。
- backend API 不得按上层用途聚合原子事实，不得新增 `*Facts`、snapshot、
  bundle、geometry input 等状态打包接口。
- Bench 与 Measure 是普通脚本声明；CLI/wire 只提供运行环境和通用
  `--output`，不得恢复专用测算模式或参数。
- 新增测算不得增加中央 mode 分派或中央测算状态；使用 Session TickTask、
  可选 lifecycle hook 和 typed artifact 组合。
- 原生瞬时事件先以最窄 raw fact 进入 `EventDispatcher`，同步内部拦截与公开
  resolved event 分层；backend 不得知道具体测算模式，用户回调不得在原生
  hook/FFI 栈上执行。
- core 普通业务链只保留一份无 backend 参数实现，不保留泛型 backend 内层、
  对称显式门面或改名为 frame/context 的逐层转发。显式访问只用于 host 安装、
  实际独占失效操作及直接维护实体／池借用的少量私有 helper。
- backend 保持 ZST，不缓存 App、Board、world、pool 地址或验证结果。Frame 只持有
  共享 backend 借用；实体借用不能跨越回收、update 或场景替换，不得解除 guard
  制造重叠可变引用。池地址按需查询，已有 handle 字段不重新解析 ID 或检查 Board。
- 一次原生语义查询产生的完整值允许保留，不以此包装多个独立字段查询。
- 公开当前操作使用无 backend/runtime 参数的普通 core 函数；能力约束
  放在实际使用处。scripting 只为必要的输入/参数数量重载保留 callable_api!，
  适合直接使用的 core 接口直接重导出，不增加重复包装、第二套 Fn 样板或注册表。
- 将 AvZ 作为重要实现参考。接口形状可以不同，但不得用推测性抽象替换
  已验证的实现结构。游戏内容使用工作区 `research-pvz-gameplay` skill，资料
  项目定位和版本管理使用 `manage-pvz-sources` skill。独立发布快照未安装这些
  技能时，按 [快照使用指南](docs/snapshot-quickstart.md) 查询公开 Wiki 与原始资料。

## 热路径性能

PE 逐帧模拟、1051 逐 tick/paint/命令消费及其调用的 core 热路径：

- 不得新增 AvZ 对应实现中不存在的堆分配；
- 不得新增大型数据结构完整复制、重复中间集合或可避免的额外全量遍历；
- 标量、小型 `Copy` 值和所有权 move 不属于该禁令；
- 脚本注册、进程启动/退出、tooling 和外部 GUI 不按逐帧标准处理。

已批准例外的预算与理由见 [允许的额外大数据结构分配](docs/architecture.md#允许的额外大数据结构分配)。
确实无法避免的新开销必须先把
位置、频率、规模、AvZ 基线、替代方案和原因写入
`必须做的额外大数据结构分配`，然后停止并等待用户确认。

### 必须做的额外大数据结构分配

- 无。

## 1051 与生命周期安全

- 每项 1051 raw ABI、地址、参数载体、寄存器、栈布局和清栈假设都必须对照
  原始 EXE 的 objdump/反汇编及真实调用点；不得按常规 x86 调用约定猜测。
- 1051 injected runtime 严格单线程；不得加入 worker、async runtime、
  `Mutex` 或基于锁的通用命令系统。
- 同一帧内被杀死的对象池对象仍可访问；跨帧保存的指针/槽位必须重新查找并
  校验 ID 或代数。
- `DllMain` 保持 record-only；真实 hook/patch、I/O、用户脚本和卸载握手
  只能在 loader lock 之外执行。

## 验证

- 仓库根 `rust-toolchain.toml` 固定工具链；使用普通 `cargo`，不得用
  滚动 `cargo +nightly` 绕过版本。
- 按 [`docs/development.md`](docs/development.md) 的相关章节选择最小但相称的
  检查。Rust 源码修改运行 `cargo fmt --check`；文件修改运行 `git diff --check`。
- 单元/编译、generated-runner build-only、PE runtime、真实 1051 注入和
  跨后端一致性是不同证据等级，不得互相冒充。
- 允许在任务范围内自动进行真实 1051 实验，包括能明确识别的已有游戏实例，
  无须逐次确认或人工观察；注入前先完成所有适用的非 live 验证。实例选择、
  停止与恢复边界见 [1051 自动实验](docs/development.md#1051-自动实验)。
- 以实际观测证据判定结果。异常先复现、定位，不凭猜测修改游戏机制或验收
  预期；自动证据不足以判断时再与用户讨论。
