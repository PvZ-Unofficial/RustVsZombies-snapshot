# FE 后退无炮：选卡阶段筛除蹦极

这是 `../fe_houtui` 的独立副本，原脚本保留。

每轮 Opening 完成自然出怪初始化后，检查全部波次的出怪列表。如果有蹦极，
调用现有 `pick_spawn_list()` 原生加权生成器重抽，直到列表中的蹦极数量为零。
允许出怪的种类开关保持不变，包括蹦极开关；没有直接删除槽位、禁止蹦极类型、
重置场地或恢复阳光。已有列表没有蹦极时不额外重抽，也不消耗对应的随机数。
检查器核对前后类型开关，记录初始蹦极数与重抽次数，并刷新原生选卡预览。
战斗随机流锁成固定值时，若仍需重抽则报告错误，避免重复同一列表而无法退出。

这是调用原生列表生成函数的筛选策略，不模拟完整的主菜单/读档流程。
筛选会改变其他僵尸的数量和后续随机流；成绩要作为“允许选卡重抽”的独立口径，
不能直接当成原版未筛选脚本的自然出怪成绩。真实退进菜单的耗时没有被模拟。

删除了 `deal_bungee` 水路辣椒补救；W10/W20 的小偷专用 421cs 冰改为普通
280cs 控丑冰（-140cs 调用普通用冰函数）。旗帜波仍需要控丑，因此保留这两波
普通冰安排。其余修补、垫材、灰烬、主库智能铲除和布阵沿用原副本。
20波水路补南瓜及红眼减压不属于小偷处理，继续保留。

仍使用 pe96 式连续冲关测算：正常过关保持场地和阳光，只有自然失败才开始新样本。
默认初始8000阳光、成熟阶段500轮、卡片Ready；一轮为20波/2面旗帜，初始500轮
不计入通过成绩。测试设置、种植修正和原版差异详见 `../fe_houtui/README.md`。

- `HOUTUI_SL_SECONDS`：测量秒数，默认600；0采用host边界。
- `HOUTUI_SL_FINISH_ACTIVE`：默认到期后完成存活样本；0改为到期截尾。
- `HOUTUI_SL_TRACE_LOSS`：默认启用植物损失；0关闭。
- `HOUTUI_SL_TRACE_ROUNDS`：默认启用库级轮末阳光摘要；0关闭。
- `HOUTUI_SL_TRACE_ACTIONS`：默认启用实际修补结果、灰烬/冰/曾种植记录，以及按轮汇总的脚本检查/拒种原因；0关闭。
- `HOUTUI_SL_TRACE_WAVES`：1额外启用逐波阳光/CD采样，默认关闭。
- `HOUTUI_SL_TRACE_DIR`：已存在的空目录，每worker新建日志，拒绝覆盖。
- `HOUTUI_SL_SUN` / `HOUTUI_SL_ROUNDS`：初始条件，默认8000 / 500。

原脚本的 `HOUTUI_*` 环境变量不影响本副本。重抽结果日志始终输出为
`houtui_sl_spawn`，包括 `rerolls`、`bungees_before/after` 和前后出怪类型位图。

60秒有界试跑：

```powershell
$env:HOUTUI_SL_SECONDS = '60'
$env:HOUTUI_SL_FINISH_ACTIVE = '0'
$env:HOUTUI_SL_TRACE_DIR = '<existing empty trace directory>'
target/debug/rsvz.exe run-pe --dev-script lowdsl/fe_houtui_sl --release --pe-toolchain clang-thin-lto --threads 12 --base-seed 100 --json --output '<new artifact.json>'
```

若要获得完整寿命样本，将 `HOUTUI_SL_FINISH_ACTIVE` 设为1；尾部可能运行很久。
本副本未对真实1051/Portable执行主菜单退进或跨后端轨迹对照。

诊断不改变种卡决策。`action_summary`只汇总脚本手动用卡路径，主库修补另记
`plant_repair`，不得重复计账。冷却和危险检查按轮汇总，`gloom_summary`的前三项
是尚未搜索缺口前被跳过的检查；不能当作已知缺口修补失败。两个格子数组按
`GLOOMS`配置顺序记录实际发现缺口后的危险/不可种跳过次数。
`last_rejection`只保留该卡本轮最后一次拒绝原因，不是全部拒绝原因的分布。
灰烬/冰记录的是种下时刻，不伪装成真实生效时刻。未完成的失败轮在退出时输出
剩余动作汇总；只有真正完成轮次才输出`round_end`。
