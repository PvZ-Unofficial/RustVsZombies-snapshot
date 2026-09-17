# rsvz-witness

`rsvz-witness` 是可选的编译时脚本扩展，不属于 `rsvz` 主库或 host 协议。它用
公开的原子 backend capability、fallible lifecycle hook、session control 和
typed artifact API 组合确定性跨后端采集。

脚本 crate 添加依赖：

```toml
[dependencies]
rsvz = { path = "../../crates/scripting/api" }
rsvz-witness = { path = "../../extensions/rsvz-witness" }
```

裸 `.rs` 脚本则把同一 `rsvz-witness` path dependency 放进最近的
`rsvz.toml`。脚本显式安装扩展：

```rust,ignore
#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::prelude::*;

    // Witness 要求显式波表；战斗中的自然刷怪仍照常进行。
    set_zombies("普");
    rsvz_witness::start(rsvz_witness::WitnessOptions {
        locked_random: 7,
        ..rsvz_witness::WitnessOptions::default()
    })?;
    Ok(())
}
```

Witness 的生成 API 只有 `locked_random`：Battle 与 Level 随机流在每次
`EnterFight` 都重新锁为该固定返回值。它不提供 seeded 模式，也不主动停止自然
刷怪。脚本仍须在 `start` 前给出显式 spawn list，避免两端各自生成不同波表；是否
调用 `set_zombie_spawn_stopped` 由具体测试场景决定。artifact reader 继续接受旧的
seeded artifact，便于读取历史结果。

跨生存轮比较跳过白字/选卡传输，只归一舞王时基，不主动回收对象或重置对象池。
Schema v6 比较存活玩法对象，排除纯尸体、失去有效目标的追踪刺、对象池布局和
植物查找缓存。实体按逻辑创建顺序编号，记录按逻辑 ID 排序，不要求物理槽位相同。
旧 schema 的字段含义不同，需要重新采集，不能与 v6 混合比较。

正常帧在公开的 `AfterTick(order = 0)` 封存；fatal tick 不伪造 `AfterTick`，
受控 teardown 时只会从 `BeforeExit` 发布 incomplete artifact。负 order 的
`AfterTick` 变化计入当前帧，正 order 的变化发生在封存之后。

比较工具由扩展自己提供：

```powershell
cargo run --manifest-path extensions/rsvz-witness/Cargo.toml --features tooling --bin witness-diff -- left.json right.json
```
