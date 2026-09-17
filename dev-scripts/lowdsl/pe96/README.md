# PE 九六

移植自用户提供的 `PE 九六 202609131832.cpp`，使用用户提供的布阵码。
不包含 ShowWavelength、图鉴点击补丁、调速/暂停热键或未启用的气球/垫材代码。

补曾优先级和 299cs 冰的晚归一化保留；有无查询命中即停，植物查询复用原生按格接口。
智能铲除改用主库 `smart_remove`，关闭高亮，替代原脚本逐南瓜全量扫描僵尸的循环。
现在采用主库的动画、冻结/冰菇、容器和右格植物规则，不再与旧手写智能铲逐帧等价。
普通通关延续卡片时钟，独立冲关样本从零开始。
PE 没有 AvZ 选卡/暂停界面的 GLOBAL 回调，因此计时使用每个战斗逻辑帧一次。
管理器在选卡完成后的 EnterFight 启动，执行顺序是 Logic → 存冰 → 修补 → After 阶段智能铲除。

默认每条样本以 1128 个已完成轮次、9990 阳光、fresh reset 的默认卡 CD 开始；
1128 取自原文件的周目注释，是本次实验条件，不声称恢复了原存档。
初阵只在 fresh reset 应用，普通通关不补满植物、不恢复阳光。
一轮为 20 波/2 旗。PE 的直接入账阳光模型不等于原版阳光收集过程。

两个独立选项：

- `PE96_SECONDS`：连续冲关测量秒数，默认 1800；0 禁用测量并采用 host 运行边界。
- `PE96_FINISH_ACTIVE`：1 表示到时停止开始新样本，当前样本继续到进家；默认0仍立即结束并记录截尾。
- `PE96_TRACE_LOSS`：默认启用忧郁菇、大喷菇、冰瓜的原生战斗损失 trace；0 禁用。

`PE96_TRACE_DIR` 指向已存在目录时，每 worker 创建独立 trace 文件（拒绝覆盖），
否则日志写 stderr。损失 trace 不保护植物、不改变伤害，也不判定失败。
正常进家才结束样本；回调/时序错误停止该 worker 并报告，避免把错误当作正常寿命。
到时仍存活的样本单独记录为截尾，不混入已失败样本均值。

```powershell
$env:PE96_SECONDS = '600'
$env:PE96_FINISH_ACTIVE = '1'
$env:PE96_TRACE_DIR = '<existing empty output directory>'
target/debug/rsvz.exe run-pe --dev-script lowdsl\pe96 --release --pe-toolchain clang-thin-lto --threads 12 --base-seed 100 --performance-window-secs 600 --json --output '<new artifact.json>'
```

通用声明为 `measure::expected_passes_for(duration, reset)`、
`measure::expected_passes_trials(count, reset)` 和 `measure::trace_plant_losses(kinds)`。
前两项使用独立 Session 测量任务；后者是普通 Script observer，可单独启用。

布阵中的模仿植物按 PT/PTK 流程立即完成原生变身，再布置梯子；正常种卡仍按原生时间变身。

`expected_passes_for_with_end(duration, reset, ExpectedPassesEnd::FinishActive)` 提供相同的通用结束策略。
收尾模式不要使用相同时间的 `--max-wall-secs` 硬截止。
`--performance-window-secs` 是独立的 PE host 性能窗口：到时固定原生更新帧数、墙钟耗时和轮数，输出一次 `PE_PERFORMANCE_WINDOW` JSON，并在最终 worker result 的 `performance_window` 中保留同一记录；后续尾段不改变它。
