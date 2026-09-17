# FE 后退无炮

移植自用户提供的 `FE后退无炮浅梦解 20250601.cpp`，使用更新后的布阵码。
六路两格普通南瓜不搭梯；其余布局保留。无显示、回放、热键和 500F 停止限制。

Opening 在生成当轮自然出怪后选择策略；三类关卡均携带双冰、荷叶、核武、
辣椒、大喷、南瓜、曾、小喷、樱桃。快速关将原本未使用的阳光菇改为樱桃。
红眼、单白、快速关保留各自波次安排和 10/20 波冰杀小偷。

明确修正：模仿冰匹配；恢复冰改用 (4,5)，避免原 (3,5) 铲掉永久香蒲；
垫材的同行统计、最前红眼和冰车关系；补曾的小丑行号；删除不可达的 need_fix
半场分支；五路小丑改用 (4,8) 樱桃；增加六路辣椒；应急灰烬校验实际命中与
种植结果，只有成功种下才占用本次救场；取消无阳光时按错误 CD 预约。
补曾沿用原位置优先级。普通修补使用三个独立的主库 PlantFixer 实例。
智能铲除使用主库当前规则和 After 调度阶段，关闭高亮。

定时用卡仅延迟尝试一次，延迟任务属于当前脚本代，换轮自动清理。
第 20 波 [320,6320) 的五路红眼减压按仍有效的需求重试，不积累重复请求；
开盒小丑只在当前仍有超过 100cs 的拦截时间时救场。

测试采用与 pe96 相同的连续冲关测算：正常过关保持场地、阳光和续局状态；
只有自然失败才重建样本。初始 8000 阳光、500 个已完成轮次、卡片 Ready，
这是成熟无尽测试条件，不声称恢复原存档。报告轮次不包含起始的 500 轮。
一轮是 20 波/2 面旗帜。PE 阳光直接入账，与真实游戏收集过程不完全等价。

- `HOUTUI_SECONDS`：测量秒数，默认 600；0 禁用测算，采用 host 边界。
- `HOUTUI_FINISH_ACTIVE`：默认到期后完成存活样本；0 改为到期截尾。
- `HOUTUI_TRACE_LOSS`：默认启用植物损失及逐波阳光/CD 信息；0 关闭。
- `HOUTUI_TRACE_DIR`：已存在的输出目录，每 worker 新建独立日志，拒绝覆盖。
- `HOUTUI_SUN` / `HOUTUI_ROUNDS`：新样本初始条件，默认 8000 / 500。

```powershell
$env:HOUTUI_SECONDS = '600'
$env:HOUTUI_FINISH_ACTIVE = '1'
$env:HOUTUI_TRACE_DIR = '<existing empty trace directory>'
target/debug/rsvz.exe run-pe --dev-script lowdsl/fe_houtui --release --pe-toolchain clang-thin-lto --threads 12 --base-seed 100 --performance-window-secs 600 --json --output '<new artifact.json>'
```

性能使用 host 的前 600 秒窗口；尾部样本的额外时间不计入该性能窗口，
但自然结束后进入最终寿命统计。植物损失不直接判失败，脚本错误不当作自然死亡。
