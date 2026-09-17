# Runtime state hooks 与裸脚本

## State hooks

脚本可用独立的 `#[rsvz::state_hooks]` 标记安装函数，并保持脚本入口为裸
`#[rsvz::script]`。两者必须位于同一模块；每个脚本模块最多只能标记一个
installer，重复标记会在编译期报错。安装函数只在每个 runtime session
初始化时执行一次；普通 script body 中尝试注册 session hook 会返回明确错误。

```rust
use rsvz::prelude::*;

#[rsvz::state_hooks]
fn install_hooks() {
    on_after_attach(|| set_auto_enter(false));
    on_before_script(-100, || {});
    on_before_exit(|| {});
}

#[rsvz::script]
fn script() {}
```

1051 host 默认从主菜单进入 Pool Endless。上例在 `AfterAttach` 中调用
`set_auto_enter(false)` 后，host 会保留主菜单供用户手动选择真实游戏模式；
普通 `script()` 到 LevelIntro/Playing 才注册，不能用于关闭首次自动进入。
默认自动进入在一个 session 中只请求一次；用户返回主界面后 host 会停留，
`reload(...)` 只使脚本在用户下次手动进入选卡或战斗界面时重新注册。
若关卡先进入 `ZombiesWon`/Award/Credit，支持 MainUi 的 reload 模式会保留
runner：终止状态只派发一次，真正回到主界面的第一帧完成 generation 清理，
随后 1051 host 继续原生菜单更新而不再次自动入关。

事件依次覆盖 attach、每次 script generation、fight attempt、每个 logic
tick 与最终退出。回调按 order 从小到大执行，同 order 保持注册顺序。
回调 error/panic 会中断该次 event 并走 runner fatal cleanup；`BeforeExit`
即使失败也不会重试。

Tick task 的 availability（AnyDispatch/Active/Playing）与 lifetime
（Session/Script/Fight）彼此独立。Script reload 只清除 Script/Fight，
fight close 只清除 Fight，session finalize 清除全部。
内置 manager 首次使用时各自惰性安装 `BeforeScript` 清理 hook；不存在
维护所有可选 manager 的中央 reset 名单。

## 裸 `.rs`

CLI 可直接准备和运行单个 Rust 文件：

```text
rsvz run-pe path/to/foo.rs
rsvz inject-1051 path/to/foo.rs
rsvz prepare-script path/to/foo.rs --backend pvz-emulator --json
```

裸路径与 `--script-crate`、`--dev-script` 互斥；`--script-name` 可覆盖默认
文件 stem。CLI 规范化真实源文件路径，在
`target/rsvz/scripts/<stable-path-hash>/<backend>/Cargo.toml` 创建
manifest-only hidden crate，并以 `[lib] path` 直接引用原文件。源码不会被
复制或包进 `include!`，因此 sibling `mod` 仍相对真实脚本目录解析。

hidden crate 通过独立 `[workspace]` 退出父 workspace，并直接依赖只启用当前
backend feature 的顶层 `rsvz`。`prepare-script --json` 返回 hidden manifest、
实际 final-runner analysis manifest、Cargo target 和结构化 program/args/cwd；
编辑器和 Run 使用同一份 prepared manifest。

注意：脚本内的 `env!("CARGO_MANIFEST_DIR")` 指向 hidden crate 目录，而不是
`.rs` 所在目录。需要脚本目录时应使用显式配置或运行期路径；本实现不会为
改变该 Cargo 语义复制源码或生成 build script。

## `rsvz.toml`

CLI 从 canonical 脚本目录向上只读取最近的一份 `rsvz.toml`：

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
my_lib = { path = "../my-rsvz-lib" }

[backend.pvz-emulator.dependencies]
pe_helper = { path = "../pe-helper" }

[backend.pvz-1-0-0-1051.dependencies]
native_helper = { path = "../native-helper", default-features = false }
```

相对 dependency path 以该配置目录解析。公共依赖只与当前 backend section
组合；两个位置重名会报错。`rsvz`、`user_script`、backend label 和 concrete
backend package key 是保留名称。

## VS Code

最小插件位于 `editors/vscode-rsvz`。它要求受信任的单 root 本地 workspace
和官方 rust-analyzer，并通过 `rsvz.cliPath` 或 PATH 找到 CLI。插件只：

1. 为当前 `.rs` 选择一个 backend；
2. 调用 `prepare-script --json`；
3. 保留用户已有 `linkedProjects`，只维护一个 plugin-owned analysis entry；
4. 设置 `cargo.target` 与 `cargo.targetDir = true`；
5. 通过 `ProcessExecution` 使用结构化参数运行 prepared command。

若 rust-analyzer auto-discovery 存在多个可能的 Cargo root，插件会拒绝猜测。
Detach 仅恢复仍等于插件最后写入值的 workspace setting，用户在 attached
期间的手工修改会保留。
