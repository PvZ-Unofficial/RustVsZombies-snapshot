# 仅凭 PE 判断超多炮阵解/脚本缺陷：信息需求与 Trace 设计

> 状态：设计研究稿；不包含生产代码修改。
>
> 目标：让 AI 不看 1.0.0.1051 画面，仅凭 PvZ Emulator（PE）一次或多次可复现运行的结构化输出，就能判断一份超多炮阵解/脚本是否有缺陷、最早在哪里偏离、为什么偏离，以及应当回到哪条脚本操作或哪项阵解假设修改。

## 1. 结论先行

最合适的方案不是在“基于现象的通用 trace”和“脚本预期标注”之间二选一，而是采用下面的混合结构：

1. **通用现象 trace 是不可替代的底座**：无论 AI 是否事先想到某种失败，都必须能报告进家、关键植物受损/消失、巨人砸击、长时间啃食、特殊僵尸越线、脚本操作失败等事实，并保留足以倒推原因的对象谱系、动作阶段、命中与位置历史。
2. **脚本标注只作为第二个判定器**：它表达阵解已经承诺的少量可观察结果，例如“W5 红眼所投小鬼应在 W6:225 前全部消失”“W4 撑杆不得落到 C8 炮前”“本时间窗 R4C7 炮允许承受至多 75 点篮球伤害”。标注失败时输出反例对象及对应 trace；没有标注的现象仍由通用规则发现。
3. **不要把历史数组塞进每个 PE 僵尸对象，也不要新增 `get_garg_history()` 一类上层用途 API**：PE 只在无法事后重建的瞬时节点发出窄事件；`rsvz-game` 在逐帧原子事实之上维护外置、定容、可压缩的诊断 tracker。小鬼父子关系在生成瞬间记录，巨人/撑杆/舞王/跳跳的行走区间主要由现有状态按段重建。
4. **采用“广测 + 失败种子精查”的两遍流程**：第一遍多 worker 只保留统计、异常摘要和少量失败 seed；第二遍单 worker 用同一 PE seed 重放，在相关波次、对象和脚本操作附近开启详细 trace。这样既能覆盖随机分支，又不会为每一轮保留海量逐帧日志。
5. **输出必须是可连接的结构化因果图，而非纯文本刷屏**：至少能连接 `脚本语义操作 -> 实际调度/等待 -> 炮/卡片实体 -> 作用事件 -> 僵尸状态变化 -> 植物后果 -> 违反的通用规则或预期`。

用户提出的“允许单只小鬼啃食伤害”思路是可行的，但只适合作为一个触发阈值，不能作为漏小鬼的完整定义。更稳妥的规则是同时累计“每只小鬼”“每个受保护植物实例/角色”“每个时间窗”的实际伤害，并在第一次啃食、超过预算、植物死亡/被替换、进家或预期截止时间仍存活时分别留下事实。

## 2. “超多炮脚本”实际上在做什么

参考 `dev-scripts/lowdsl/re20_p/RE20逐波逐行阵解.md` 和脚本 `dev-scripts/lowdsl/re20_p/src/lib.rs`，超多炮脚本并不是简单地“每波发若干炮”。它是在维护一份跨越 20 波的资源与僵尸状态账本：

- 宏观层先决定冰波/加速波、波长、炮的跨波复用和孤立冰波诱导的映射；`日常坐卧.md` 所描述的正是这种先构造轨道骨架、再填入操作的过程。局部可复用不等于全局最优，不能用贪心局部修补代替整轮炮序检查。
- 每一枚炮有三个不同的时间：脚本写下的**语义命中时间**、时间轴真正执行选炮/预约的**命令时间**、以及恢复等待和屋顶飞行修正后的**实际发射/命中时间**。`recover_p` 明确允许实际命中晚于语义时间。
- 每一波不是独立结算。上一波的冻结撑杆、残血巨人、尚未生成或仍在飞行的小鬼、已经离开发射者的篮球，都可能成为下一波的输入。
- 对每一行、每一种僵尸，阵解都隐含了一条处理链：在哪一炮进入可伤域、哪一伤跨过巨人投掷阈值、哪一枚 D/尾炸负责拦鬼、撑杆是否已起跳、冰三是取消当次生成还是永久消除投掷资格，以及残血对象最终在哪一波销账。
- 许多成立条件是分支而非单一路径。例如同一炮可能因出生横坐标/行走相位命中或未命中父体，但两支都应在后续闭合；正确 trace 不能把这种条件分支误报成脚本错误。
- 有些伤害是阵解主动接受的，例如已经在空中的篮球继续造成 75 点伤害、短暂梯啃、前置花盆被压但炮体仍安全。只看“炮伤总量”无法区分受控代价与失控泄漏。

因此，第 4 步诊断系统的职责不是重新发明宏观轨道，而是把游戏执行变成一份机器可审计的逐波账本：它要证明每项计划是否兑现，并在没有计划覆盖的地方仍能发现异常。

## 3. AI 能判断“阵解缺陷”所需回答的七个问题

一份诊断输出只有在 AI 能稳定回答下面问题时才算信息充足：

1. **最早偏离是什么？** 是脚本操作没有执行、实际命中延迟、目标未进入爆区、巨人动作被错误理解、还是预期本身写错？
2. **偏离发生在谁身上？** 要有稳定对象 ID、种类、行、出生波/生成时刻，以及小鬼父体、伴舞领舞、篮球投手等谱系。
3. **当时游戏状态是什么？** 至少包括绝对时钟、波相对时间、坐标/碰撞箱、血量与饰品、动作阶段、动画相位、冰冻/减速/黄油、正在啃食或已经动作承诺等。
4. **原本由哪项操作处理？** 要能追到脚本语义操作、实际选择的炮/卡、预约延迟、炮弹实体、实际落点和命中集合。
5. **为何命中或未命中？** 不能只写“撑杆漏了”；应能说明是爆炸时仍在右界外、处于不可伤阶段、跨行/圆形碰撞不成立、实际炮晚/早了，还是该炮本就不是负责它。
6. **造成了什么后果？** 要区分炮体、底盆、南瓜、战术垫材等同格层；给出实际伤害、持续啃食、砸击、跳越、碾压、进家，以及是否超过允许预算。
7. **如何复现和缩小？** 要有 PE seed、trial/shard、脚本与阵图版本、精查时间窗和 trace 完整性/截断信息。

如果某个 schema 只能告诉 AI“本轮总炮伤 2400”，却不能回答上述问题，就仍然只是统计报表，不是阵解调试器。

### 3.1 最终结论还必须区分五类“缺陷”

相同的坏结果可能来自不同层，修复方向完全不同。finding 汇总后，AI 应把根因归到下列一类，并给出证据而非只给标签：

1. **阵解/策略缺陷**：脚本按计划准时执行，PE 现象也符合机制，但计划的处理链本身不能闭合，例如指定拦截炮的几何窗口根本覆盖不到目标。
2. **脚本实现缺陷**：宏观处理链可成立，但时间写错、炮序/manager 选错、预约冲突、种铲顺序错或 operation 失败。
3. **可接受随机分支或统计尾部**：实际路径属于阵解明确允许的 alternatives，且最终闭合；或者失败率未超过事先声明的容忍度。不能把“和主分支不同”自动判错。
4. **PE/机制模型疑点**：观测与已核验的 PUC/Wiki/1051 机制证据冲突，且脚本与预期本身没有先行偏离。此时应转入 PE parity/机制审查，不能靠改脚本掩盖。
5. **诊断/预期缺陷**：trace 截断、谱系断裂、时钟无法对齐，或标注自身与可观察事实矛盾。此时结论应是“证据不足/预期错误”，而不是武断归罪阵解。

推荐的归因顺序是：先找最早客观偏离，再检查 operation，随后验证机制/几何，最后才拿预期作解释。预期不能覆盖更早的客观证据。

## 4. 当前链路已经具备的能力与真正缺口

### 4.1 已经存在的精确现象事件

当前链路并非只有统计量。PE 的 `system/event.h/.cpp` 与 rsvz 的 `rsvz-model::model::event` 已经能在原生作用点记录：

- 小丑爆炸对植物的尝试与实际结果；
- 每次僵尸啃食造成的植物 HP 变化；
- 巨人砸击/地刺王反伤所涉及的植物效果；
- 篮球命中植物；
- 僵尸进家。

事件包含作用来源对象 ID、植物 ID/种类/格子、作用前 HP、请求效果、测算决策和原生返回结果、`main_counter`。DamageNarrow 使用 `SuppressByMeasurement` 时，报告的是被抑制的请求/推算伤害，不能表述为实际施加的 HP 变化；其他观察路径才按原生结果记录实际变化或死亡/砸扁。公开 `GameEvent` 目前只有 `PlantEffect` 和 `HomeEntry` 两类；`event_measure.rs` 在其上做的 `DamageNarrow`、`BroadPass`、`Smash`、`Pogo` 报告才是统计聚合层。

因此，应复用这些原始事实，不应重新做第二套啃食/砸击检测。当前不足是统计 reducer 丢弃了对象时间线、脚本操作关联和失败见证。

### 4.2 已经存在的逐帧原子状态

`ZombieRawFactsBackend`、`ZombieStateBackend` 和 projectile/plant state API 已经能读取大部分当前状态：

- 僵尸 ID、种类、行、出生波、年龄、HP/最大 HP、饰品 HP；
- phase/action、整数与浮点坐标、碰撞箱、速度、动画进度/速率/循环信息；
- 冰冻、减速、黄油、是否啃食、是否持有道具、是否死亡/消失；
- 舞王/伴舞的 master/partners 关系；
- 植物/炮弹的 ID、种类、位置、状态、倒计时与生命周期位。

`rsvz-game::logic::zombie_motion_state` 还已经把这些原子事实组合为运动模型。这意味着“每个历史行走区间”无需成为 PE 僵尸成员：逐帧比较状态键，就能把连续运动压成少量 segment。

### 4.3 当前无法可靠重建的瞬时因果

下面的信息若不在发生点记录，帧末轮询可能永久丢失：

- 小鬼是哪只巨人投出的。PE 小鬼对象没有父巨人字段，现有 `master_id` 已有舞王/伴舞语义，不应复用。
- 巨人/撑杆/跳跳在开始砸、跳时锁定了哪株植物。PE 的通用攻击目标是动态 `find_target`，当前 `zombie_target_plant` 的 PE 实现读取的是 `bungee_target`，不是通用啃食/跳跃目标。
- 一枚炮在爆炸瞬间准确命中了哪些僵尸；尤其是某只僵尸为何在像素边界上未命中。炮弹会在作用后销毁，帧末坐标最多只能近似。
- 篮球炮弹对应哪只投篮车、起手时选择了哪株目标。当前 projectile 对象不保存投手 ID；投手死亡后篮球仍独立存在。
- 脚本的语义操作最终选择了哪门炮、预约了多久、实际何时创建炮弹和命中。Timeline 有稳定 `TimeOpId`，炮管理器也知道 selected cob/delay/target，但这些信息尚未形成诊断事件。
- 同一帧内生成又消失的对象及其消失原因，单纯的帧末 pool 差分可能看不到完整生命周期。

这些才是需要新增“窄瞬时事件”的核心范围。

### 4.4 重点需求与现有/新增信息对照

| 要解释的现象 | 现有信息可直接提供 | 仍缺的不可重建信息 | 推荐处理 |
| --- | --- | --- | --- |
| 巨人砸炮 | plant-effect 的巨人/植物 ID、格、时刻、结果；逐帧 HP/phase/x/from_wave | 砸击开始锁定的目标；每次炮击精确命中历史 | action-commit + cob-hit 窄事件；外置 MotionSegment |
| 小鬼漏出/啃食 | bite 明细、进家、当前小鬼状态 | 父巨人 ID、投掷时父体状态、同帧生成关系 | 小鬼生成点写 `ZombieParented`；伤害 episode/预算在 game 层聚合 |
| Ice3 取消后重投 | freeze/phase/reanimation 可逐帧观察 | 当次投掷是否已经真正生成小鬼 | 投掷开始 + child spawn；无 child 的阶段转移由 tracker 判为 reset/cancel，不把资格永久销账 |
| 撑杆早炮未伤/跳炮 | 当前 phase/x/hitbox、炮落点；后续 bite | 爆炸瞬间的精确 miss 原因；起跳锁定目标 | cob candidate resolution（精查）+ vault commit；其余阶段压缩采样 |
| 舞王/伴舞 | master/partners、phase、row/x | 精确召唤/生成时刻（帧末可能近似） | 统一 parent relation + group 派生视图，不另造专用历史 API |
| 长时间炮伤 | 每次 bite/jack/basketball/砸击结果 | 阵解允许的 role/source/window 预算 | episode 聚合 + 稀疏 expectation；按植物实例/层记账 |
| 跳跳进家 | home-entry、phase/action/has_object/x | 每次跨越目标与失杆原因（必要时） | 第一遍越线检测；精查时 action commit/失杆事件 |
| 篮球延迟命中 | projectile ID 的 plant-effect、projectile 当前状态 | 投手 ID、发射时目标 | 创建篮球时记录 source relation；独立追踪至命中 |
| `recover_p` 晚炮 | Timeline 时钟、炮管理器恢复/选择逻辑 | 语义操作到预约/炮弹/命中的稳定关联 | 在炮管理器记录 OperationRecord，并由 projectile spawn 接链 |
| 炮击边界 miss | 爆炸几何规则、上一/下一帧对象状态 | 爆炸同一瞬间的 eligibility/overlap | 广测只记 hits；失败 seed 精查关注候选的 exact resolution |

### 4.5 对象 ID 已有 generation，但仍要带 trial/world 边界

PE 的 `obj_list` ID 已包含 generation，槽位复用后新对象不会沿用旧 ID。诊断报告仍应把 `(world_epoch, trial_sequence, object_id)` 作为完整引用，因为 world reset 后同一位型可以再次出现，合并多个 trial 时不能只看裸 ID。

## 5. 建议的统一信息模型

诊断信息按下列九层组织。它们不是九套 API，而是最终 artifact 中九类可连接记录。

### 5.1 运行与复现信封 `RunEnvelope`

必须包含：

- trace schema 版本；
- rsvz、PE、脚本构建标识/commit（至少能唯一定位二进制与脚本内容）；
- 脚本名和内容 hash；阵图编码/hash；场景、选卡、出怪集合及每波覆盖；
- worker/shard、全局 trial sequence、PE battle/level seed、world epoch；
- 会影响结果的 PE 规则开关，如 cob drift/fixed delay、植物伤害规则、舞王时基策略、随机锁定；
- 运行窗口、停止原因、总帧数；
- 每类记录的容量、实际条数、丢弃条数与 `trace_complete`。

没有版本和 seed 的漂亮时间线不能作为可复现证据。

### 5.2 三套时钟 `ClockStamp`

每个重要记录都应同时携带或可反查：

- `main_counter`/logical frame；
- `Wave n : t`，以及该波真实 refresh clock；
- 若来自脚本操作：声明的语义时间、预定 command time、实际 dispatch time、实际 effect/impact time。

这能直接解释跨波炮和 `recover_p`。报告不应把“源码写在 W19:1887”直接等同于物理命中时刻。

### 5.3 脚本操作账本 `OperationRecord`

每个会改变游戏的叶操作应有稳定 `op_id`（可连接 Timeline 的 `TimeOpId`），至少记录：

- 操作种类、源码/DSL 标签（若可得）、语义波时；
- due clock、dispatch clock、结果或错误；
- 输入目标、选择器/manager 标识；
- 对发炮：selected cob ID/格、恢复值、屋顶提前量、预约 delay、实际发射时刻、炮弹 ID、实际爆炸时刻；
- 对种植/铲除：卡片、目标格、创建/移除的植物 ID；
- 对冰/灰烬：效果实体或生效时刻；
- 若 operation 产生后续预约，父 `op_id` 与子 `TimeOpId` 都要保留。

建议允许 AI 给操作附加一个简短的稳定标签，如 `w6_intercept_lower_imps`，但标签只帮助阅读，不能成为唯一关联键。

### 5.4 实体生命周期与谱系 `EntityLifecycle`

每个相关实体需要：

- 完整实体引用、kind、row、出生波、生成绝对时刻与初始坐标；
- 生成原因：自然出怪、巨人投掷、舞王召唤、脚本直接创建等；
- `parent_entity` 与关系种类：`imp_of_gargantuar`、`backup_of_dancer`、`basketball_of_catapult`、`projectile_of_cob`；
- 死亡/消失时刻和原因；若原因未知必须显式写 `unknown`，不能凭空推断；
- 若跨波存活，列出进入/离开每个波边界时的简要状态。

对小鬼，只需记录父子边和投掷时父体事实，不必把父体全部历史复制进小鬼对象。

### 5.5 战斗/作用事件 `EffectRecord`

最少应覆盖：

- 炮弹生成、爆炸中心/行/半径/flags；
- 每个实际炮击目标的作用前后 HP、饰品、坐标、phase、hitbox 与来源 `op_id/projectile_id`；
- 精查模式下，对被关注但未命中的候选记录排除原因：行距、圆形重叠失败、不可伤 phase、已死亡/魅惑/潜水等；
- 冰冻/减速/黄油的施加、解除，以及是否导致动作阶段中止；
- 巨人投掷开始、小鬼生成、投掷被重置/结束后重投；
- 巨人砸击开始时的目标和真正砸中/取消；
- 撑杆/跳跳起跳时目标、起跳/落地、失去道具；
- 舞王召唤开始、伴舞生成及 leader/slot；
- 篮球生成、投手、初始目标、命中目标；
- 已有的植物 effect 与进家事件。

“未命中”不是天然事件，因此不能为所有僵尸无差别刷一条记录。第一遍只记录实际命中；第二遍针对失败对象/预期 selector 在每次相关炮爆炸时记录 candidate resolution。

### 5.6 压缩运动区间 `MotionSegment`

segment 的状态键建议包含：

- phase/action、运动模型、方向与速度参数；
- reanimation 资源/进度/速率/循环计数；
- frozen/chilled/buttered、is_eating、has_object；
- 已知 target/动作承诺；
- row。

键不变时只更新 `end_clock/end_x/max|min_x`；任一键变化时结束旧段并写出 transition reason。这样一只巨人的“正常行走 -> 投掷停步 -> Ice3 重置 -> 冻结 -> 解冻收手 -> 重新行走/重投”会得到数个 segment，而不是数千个逐帧点。

运动 segment 对巨人尤其重要，因为 Wiki 归档指出啃食、投掷、砸击会重置非匀速行走动画相位；只保存平均速度无法复原后续位置。撑杆、舞王/伴舞、跳跳同样依赖动作相位，不能只看起点和终点。

### 5.7 植物实例、层级与战术角色 `PlantRoleState`

同一格可能同时有底盆、内容植物、南瓜；炮还占两格。诊断必须按植物实例而非“格子总 HP”记账：

- plant ID、raw/effective kind、锚点格、占用格、创建/移除时间；
- 当前层级（base/content/pumpkin/coffee 等）；
- 可选的战术 role，例如 `cob:R4C7`、`stored_ice:R1C9`、`fodder:R5C9`、`front_pot:R4C8`；
- 伤害来源、每次 applied damage、连续啃食 episode、死亡/砸扁/被铲/被替换原因；
- role 在重种后映射到新 plant ID 的生命周期。

这能区分“前盆被车压掉但炮体没事”和“炮体真正受损/消失”，也允许声明某些垫材本来就是消耗品。

### 5.8 波边界账本 `WaveLedger`

在每个 refresh 前后生成简要清单：

- 仍存活的对象按 `(from_wave, kind, row, parent)` 聚合；
- 每个对象的 HP 档、phase、坐标范围、冰/慢状态；
- 已开始但未结束的投掷/砸击/跳跃/召唤；
- 已生成但未命中的炮弹/篮球；
- 本波声明的预期中尚未闭合的项。

这正对应 RE20 逐波阵解中的“跨波”章节。没有这层，AI 很容易把 W3 的遗留红眼误算成 W4 新生红眼，或把 Ice3 暂时重置误写成永久消鬼。

### 5.9 异常与证据连接 `DiagnosticFinding`

每项 finding 不只是消息字符串，而应包含：

- `finding_id`、严重度、规则/预期 ID；
- 首次违反时刻和受影响实体/植物 role；
- `observed` 与 `expected/budget`；
- 直接证据 event IDs；
- 相关 operation IDs、projectile IDs、父子实体；
- 自动选取的前置/后置 context window；
- 判定类型：`observed_fact`、`derived_inference` 或 `expectation_violation`；
- 若 trace 不完整或原因只能推断，显式给出置信/缺失字段。

## 6. 各类重点现象应怎样处理

### 6.1 巨人砸炮

**触发条件**：巨人开始砸击关键 role、真正产生 squish/HP damage、或砸击动作进入后未按预期取消。

**报告至少包含**：

- 巨人 ID、红/白眼、row、from_wave、出生/砸击时刻；
- 砸击开始和实际命中的 x、HP/最大 HP、冰慢/黄油状态；
- 锁定的 plant ID/role，以及同格所有植物层；
- 从出生到砸击前所有炮击命中（来源炮/op、命中时 x、前后 HP）；
- 压缩行走/投掷/冻结/砸击 segment；
- 最早进入炮防线的原因链，以及哪项计划本应杀死/阻塞它。

现有 garg plant-effect 已能证明“谁砸了哪株植物”，但不能单独解释它如何走到这里。应由外置 tracker 补历史；砸击开始时的目标属于瞬时动作承诺，宜新增窄事件。

### 6.2 漏小鬼与“允许单只小鬼啃食伤害”

该阈值应定义为**实际 applied damage**，而不是简单的“啃食时间”，因为减速、动画相位、伤害被规则阻止、植物被铲走/替换都会让时长与后果不等价。

建议同时维护：

- `per_imp_damage[imp, protected_role]`；
- `per_role_damage[role, window, source_kind]`；
- 每段连续 bite episode 的 start/end、首次/末次 bite、总伤害、目标实例变化；
- 第一次 bite 即保留轻量记录；超过预算、杀死植物、进家或截止时间仍存活时升级为 finding。

输出需带父巨人：父 ID、红/白眼、from_wave、投掷开始与真正生成时刻、投掷时 x/HP/冰慢状态，以及父体后来是否死亡/重投。父子关系必须在 `gargantuar.cpp` 创建小鬼后立即记录；不建议给小鬼对象增加整套父体快照。

阈值规则不能替代“预期截止时间”。一只小鬼可能尚未啃食却已经越过阵解允许的位置，或本应在空中被尾炸而仍在飞行；通用越线/存活规则和可选预期仍要工作。

### 6.3 撑杆

撑杆需要重点 trace 的不是“这种僵尸出现了”，而是以下状态链：

- 出生 x/row/from_wave、跑动 phase 和动画相位；
- 每次炮爆炸时是否可伤、精确 hitbox 与命中/未命中原因；
- 锁定哪株植物并开始起跳；
- 跳跃中、落地、失杆后行走；
- 是否越过 C9 垫材进入 C8 炮区域，是否产生 bite。

Wiki 归档记录撑杆起跳后会持续检查目标并在约 1.8 秒完成跳跃；PE 代码也在起跳时通过 `find_target` 计算跳跃速度，但不把目标保存在通用字段里。因此“开始跳哪株植物”应是窄事件；其余阶段可由 phase/reanimation tracker 压缩重建。

通用 finding 可定义为：撑杆对关键 role 起跳、落地点越过防线 x、首次啃食关键 role、或进家。是否允许跳一个战术花盆由 role/budget 配置，而不是硬编码“跳跃总是失败”。

### 6.4 舞王与伴舞

舞王问题由领舞全局时基、月步、召唤动作、伴舞生成位置和主从关系共同决定。当前 PE 已保存 master/partners，适合直接构建谱系，但仍应记录：

- 领舞出生与月步结束、召唤开始/完成；
- 每个伴舞的 leader ID、slot、row/x 与生成时刻；
- 领舞/伴舞的冰冻、黄油和脱离/死亡；
- 对关键 role 的首次接触/啃食和越线。

不建议为舞王建立一套完全独立的日志协议；应使用统一的 EntityLifecycle、MotionSegment、EffectRecord，只增加 `backup_of_dancer` 关系和派生的“召唤组”视图。RE 屋顶不出舞王，但通用系统不能据此省略。

### 6.5 长时间啃食与炮伤定位

现有每次 bite 事件已经是最佳事实源。诊断层应把同一攻击者、同一植物实例、连续时间内的 bites 聚合为 episode，避免每 4 点伤害刷一行。

建议触发：

- 第一次啃食关键 role：info；
- 单攻击者/单 role 超过预算：warning/error；
- 连续啃食超过时长阈值，即使伤害因 invincible 规则未落地：warning；
- 关键植物死亡/被砸扁：fatal；
- 同一格植物被替换后攻击延续：保留两个 plant instance，并连接同一 role。

“可接受炮伤”必须能按 source、role、wave window 分开配置。例如篮球 75 可以允许，撑杆啃炮 4 点也许就不能允许；只设一个全局总炮伤上限会掩盖错误。

### 6.6 跳跳

进家事件已能确定最终失败，但定位需要更早的里程碑：

- 有无跳杆、被磁力菇/高坚果移除的时刻；
- bounce phase、每次开始跨越的目标、x 区间；
- 越过预设防线、首次接触关键 role；
- 进家前最后若干 motion segments 和相关炮爆炸。

PE 的跳跳 phase、action countdown、has_object 都能逐帧读取；起跳目标与失杆原因若不能稳定由状态差分区分，再补窄事件。不要一开始就记录每次 0.8 秒弹跳的所有帧。

### 6.7 投篮车与离体篮球

篮球必须作为独立实体追踪，因为投篮车死亡不会删除已生成篮球。至少记录：

- 篮球 projectile ID、投手 zombie ID/from_wave/row；
- 发射时投手 x、初选 plant ID/role；
- 飞行区间、最终实际命中 plant ID/role、75 点作用结果；
- 命中时投手是否已经死亡。

当前植物命中事件只有 projectile ID，而 projectile 不保存投手关系；故应在 `catapult.cpp` 创建篮球时发出父子关系事件。它比给所有 projectile 增加永久 source 字段更窄。

### 6.8 其他通用失败

为了让系统不只会诊断 RE20，还应有少量通用规则：

- 任意僵尸进家、GameOver；
- 关键植物实例死亡、砸扁、被碾/偷走或意外消失；
- 脚本 operation 失败、panic、timing violation、预约到点炮不可用；
- 关键炮实际发射/命中相对语义时间偏差超过预算；
- 波边界存在未闭合的关键动作/实体；
- trace 容量溢出或对象谱系断裂。

冰车、小丑、潜水、海豚、矿工、蹦极等以后可在统一词汇上增加派生规则，不应每种僵尸都再造一条 backend API。

## 7. 两种实现思路的比较

### 7.1 方案一：基于现象的通用 trace

优点：

- 能发现 AI 事前没有想到的失败；
- 同一套事实可被新的诊断规则重复利用；
- 适合脚本生成早期，因为此时“计划”本身可能不完整或错误；
- 能通过失败对象时间线定位实际原因。

缺点：

- 仅靠“异常阈值”难以知道某些受控伤害是否符合阵解；
- 如果无差别记录全部帧/全部对象，数据和热路径成本不可接受；
- “本应被哪一炮杀”属于意图信息，纯现象无法唯一推断。

### 7.2 方案二：脚本预期标注

优点：

- 能直接比较实际与阵解承诺，报告更接近人类看录像时的思路；
- 对随机分支可声明可接受集合，而不是用一个粗糙阈值；
- 能表达受控炮伤、特定截止时间和跨波闭合。

缺点：

- 标注只覆盖 AI 已经考虑到的事情，无法成为完备 oracle；
- 过度细化会把 RE20 逐行阵解重新手写一遍，生成成本高且容易自证循环；
- 若标注绑定具体对象 ID/精确随机路径，会非常脆弱；
- 错误预期可能把正确执行误报，必须把“预期违反”和“客观失败”分开。

### 7.3 推荐：现象优先、稀疏预期、自动派生

最终判定来源分三类：

1. **客观致命不变量**：进家、关键炮体消失、operation 失败、trace 不完整等，无需脚本标注。
2. **通用风险规则**：长啃、特殊僵尸越线、关键 role 受损、实际命中严重延迟等，使用默认或阵型级预算。
3. **脚本预期**：只写阵解真正关心、通用规则无法知道的处理承诺。

三类 finding 必须在输出中标明来源，AI 才能区分“游戏已经失败”“高度可疑”和“与作者声明不一致”。

## 8. 预期标注应表达什么

标注应面向**可观察结果**，而不是把实现过程逐帧复述。建议只有四类基础构件：

### 8.1 实体选择器

可按以下条件组合：

- kind/种类集合；
- `from_wave`；
- row 集合；
- 生成原因与 ancestry，例如“W5 红眼所投的小鬼”；
- 出生/生成时间窗；
- 可选的脚本 operation/effect 关联。

### 8.2 时间窗

同时支持：

- 绝对 `Wave:t`；
- 相对某个事件，例如“父体过投掷阈值后 100..260cs”；
- 波边界前后。

### 8.3 谓词和量词

最小谓词集合可包括：

- `all dead_or_absent by T`；
- `none enters/bites/smashes role`；
- `cumulative_damage(role, source) <= N`；
- `all hit_by one_of(op/effect set)`；
- `count in range`；
- `no unresolved action/projectile at boundary`。

量词支持 all/none/any/count，允许 alternatives，避免把随机命中分支强行压成单一路径。

### 8.4 战术 role 与预算

预算绑定 role/来源/时间窗，而不是只绑格子：

```text
expect damage(role = "cob:R2C1", source = basketball,
              during = W5:0..W6:225) <= 75

expect all(selector = imp(from_wave = 5, parent_kind = giga_gargantuar,
                          rows = [1,2,3]))
       dead_or_absent by W6:225
       handled_by one_of ["w6_upper_intercept", "w6_lower_intercept"]

expect none(selector = pole(from_wave = 4, rows = [4,5]))
       lands_left_of role("front_of_cob:R4C7")
```

这只是语义示例，不是拟定的 Rust API。实现时应先稳定数据模型，再决定 DSL 表面形式。

### 8.5 不应要求标注的内容

- 每只普通僵尸的每次坐标；
- 所有炮的精确命中对象列表；
- 通用“不能进家/不能砸掉关键炮”不变量；
- 已可从脚本操作与事件自动推导的选炮、发射和伤害事实。

标注越像逐波阵解全文，越说明自动 trace/派生层做得不够。

## 9. 推荐的分层实现位置

### 9.1 PE 原生层：只记录不可重建的瞬时事实

建议的最小事件候选集：

1. `ZombieParented`：child ID、parent ID、relation、生成时刻；用于小鬼/伴舞。
2. `ZombieActionCommit`：actor、action kind、target plant（若有）、x/HP/phase；用于巨砸、投掷、撑杆/跳跳起跳。
3. `ProjectileSpawnedFrom`：projectile、source plant/zombie、目标；用于炮弹和篮球。
4. `CobDetonation`：projectile、中心、行、半径、flags、时刻。
5. `CobTargetHit`：projectile、zombie、命中前后 HP/饰品、x/phase/hitbox。
6. 精查模式可选 `CobCandidateResolution`：被关注对象在爆炸瞬间的 eligibility/overlap 与排除原因。
7. 必要时 `EntityRemoved`：只补同帧生成后消失、或无法由 occupied pool 看出原因的生命周期边界。

这些事件应是 copy-only、无分配、无用户回调的窄事实。PE backend/bridge 负责把发生点事实送入诊断器，不负责解释“这是 W6 拦截炮失误”。

如果通过 `NativeEventSink` 扩展 FFI，应该增加少量**固定、类型化函数指针**，并同步更新架构文档；不要做一个携带任意 tag/`void*` payload 的通用事件协议。callback 只复制事实到预先准备的内部状态，不查询 backend、不分配、不写文件、不执行脚本。

### 9.2 `rsvz-game`：组合事实、维护 tracker、生成 finding

这里负责：

- 每逻辑帧采样已有原子状态；
- 建立 EntityLifecycle、MotionSegment、WaveLedger；
- 聚合 bite episode 与伤害预算；
- 连接原生事件、脚本 operation、对象谱系；
- 运行通用规则和预期；
- 选择异常上下文并编码 JSON artifact。

这符合现有架构：backend API 提供原子事实，游戏规则/测算/reducer 位于 `rsvz-game`。不要新增按诊断用途打包的 `GargantuarHistoryFacts`、snapshot 或 geometry bundle。

### 9.3 Timeline/炮管理器：记录意图到实际操作

炮管理器的 `select_cob_for_fire`、`delay_fire`、`schedule_cob_fire` 和 `fire_cob_attempt` 已经同时掌握 selected cob、recover wait、roof lead、due clock 与 fire target，是建立 OperationRecord 的正确位置。PE 原生 projectile spawn 事件再把实际 projectile ID 接上。

不要靠比较“场上突然多了一枚炮弹”猜 selected cob；`recover_p` 的预约和同帧多炮会让这种猜测不可靠。

### 9.4 不使用公共 `GameEvent` 队列承载详细 trace

当前公共队列默认容量 256、`GameEvent` 为 60 字节，并且架构明确规定热路径不扩容。详细炮击/动作/segment 若全部进入该队列，很快溢出，也会把内部测算事实暴露成不稳定的公开 API。

可以复用现有 `NativeEventSink -> 内部同步 interceptor -> 帧后 task` 的分层模式，但详细诊断应有自己的受控内部 reducer/缓冲，不等于再做一条用户回调总线。公开 observer 继续只面向稳定、低量的 resolved events。

### 9.5 PE-only 能力与 1051 行为

用户提出“只有 PE 实现，1051 不实现”是合理的，但应做到**显式不可用**：

- 数据 schema、规则和预期类型可以保持 backend-neutral；
- 安装完整诊断 task 需要一个 PE 才提供的窄 capability/feature；
- 1051 在请求该模式时编译期拒绝或返回 typed unsupported，不能静默产出缺字段的“成功报告”；
- 不需要为了 API 对称给 1051 增加对象历史字段或 native hook。

具体采用 trait bound、feature 还是 setup-time unsupported，应在实现时结合现有脚本生成方式决定；关键约束是“不伪造支持”和“不把两个物理 host 抽象成统一 trace host”。

## 10. 为什么不应给 PE 僵尸对象直接增加历史字段

用户担心对象本身没有炮击历史、行走历史和父体历史，这个观察正确，但解决方案不应是把所有历史都放进 `object::zombie`：

- PE 的 zombie pool 上限是 1024，给每个对象增加 vector/history 会扩大每个 world 的常驻体积，复制/reset 语义也更复杂；
- `obj_list::alloc()` 通过 `T{}` 重置槽位，带资源字段会触及项目明确警告的对象池约束；
- 历史是诊断用途，不是游戏状态，放进对象会污染 PE 的原子模拟层；
- 多数历史可由外置 tracker 从现有状态重建；只有父子关系和精确命中等瞬时事实必须在发生点发出。

推荐外置结构：

- 以 generation-aware ID 为键的当前对象 tracker；
- 每对象只保存当前 segment 和少量已闭合 segment；
- 炮击/动作事件用 ID 连接，不复制整条父时间线；
- 只为异常相关对象保留详细上下文，正常对象在波边界聚合后可丢弃细节。

如果实现确实需要新的大块定容存储，必须先给出每 worker 的字节预算和最大 worker 放大值，并按 `RustVsZombies/AGENTS.md` 把它列为待用户批准的热路径分配例外；本设计稿不预先批准该开销。

## 11. 两遍运行与数据保留策略

### 11.1 第一遍：广测 `screening`

目的：从大量随机 trial 中找到失败分布和代表 seed。

只保留：

- 现有统计报表；
- 通用 fatal/serious finding 的计数与最早时刻；
- 每种 finding 最先/最典型的有限个 `(trial_sequence, seed, wave, entity summary)`；
- 标注违反率；
- trace overflow 计数。

第一遍不保存所有 MotionSegment 或每次炮击候选。

第一遍若要检测撑杆/跳跳越线，只维护重点种类的“当前最左位置、phase、是否已告警”等定长状态，不保留完整历史；现有 plant-effect/home-entry 继续承担低成本的客观失败检测。

### 11.2 第二遍：失败 seed 精查 `focused replay`

用同一 PE seed 单 worker 重放，配置：

- 关注波次/绝对时间窗；
- 关注 entity selector、其父/子、目标 plant role；
- 关注 operation IDs 与相关炮弹；
- 异常前预卷和后卷，例如前 300cs、后 200cs；
- 开启 CobCandidateResolution 等高精度记录。

PE 同 seed 可用于 PE 内部可复现重放；这不意味着同 seed 会和 1051 产生相同随机对象。报告必须明确 backend 和版本。

### 11.3 定容与截断

每个缓冲都要有独立上限：

- 每 trial finding 数；
- 保留失败 seeds 数；
- 关注实体数；
- segments/events/candidate resolutions 数；
- 上下文时间窗。

溢出时不能悄悄丢数据，应设置 `trace_complete=false`、分类 dropped count，并把“诊断证据不完整”本身作为 finding。优先丢低严重度重复项，保留最早因果链。

所有 Vec/表/ring 的容量都应在脚本冻结后的冷路径一次性确定；战斗热路径只复用并在满时报告截断，不能自动扩容。详细精查默认单 worker，也不能借“仅用于诊断”绕过每 worker 内存预算审查。

## 12. 建议的结构化输出形状

下面是缩减示例，字段名仅用于说明关系：

```json
{
  "schema": "pe-diagnostic-trace/v1",
  "run": {
    "script": "re20_p",
    "script_hash": "...",
    "backend": "pe",
    "trial_sequence": 1842,
    "seed": 1842,
    "world_epoch": 1843,
    "trace_complete": true
  },
  "finding": {
    "id": 7,
    "severity": "fatal",
    "rule": "protected_role_squished",
    "at": { "main_counter": 14321, "wave": 14, "time": 612 },
    "entity": "z:1843:00af0042",
    "role": "cob:R4C7",
    "evidence": [902, 903, 1041],
    "related_operations": [117]
  },
  "entities": [
    {
      "id": "z:1843:00af0042",
      "kind": "giga_gargantuar",
      "row": 4,
      "from_wave": 13,
      "spawn_at": 12890,
      "segments": [301, 302, 303, 304]
    }
  ],
  "events": [
    {
      "id": 902,
      "kind": "cob_target_hit",
      "operation": 117,
      "projectile": "p:1843:00110009",
      "zombie": "z:1843:00af0042",
      "at": { "wave": 13, "time": 950 },
      "x": 801.25,
      "hp_before": 5980,
      "hp_after": 4180
    },
    {
      "id": 1041,
      "kind": "gargantuar_smash_hit",
      "zombie": "z:1843:00af0042",
      "plant": "plant:1843:002a0004",
      "role": "cob:R4C7",
      "hp": 2380,
      "x": 521.7
    }
  ],
  "operations": [
    {
      "id": 117,
      "label": "w14_lower_intercept",
      "semantic_time": { "wave": 14, "time": 304 },
      "selected_cob": "plant:1843:00080012",
      "scheduled_delay": 0,
      "actual_impact": { "wave": 14, "time": 304 }
    }
  ]
}
```

AI 首先读 findings 和 wave ledger，再按 ID 展开少量相关时间线，不必把整局所有对象一次塞进上下文。

## 13. 两条典型因果链应怎样呈现

### 13.1 漏小鬼

理想报告不是：

> W6 炮伤 800，小鬼啃食过久。

而是：

> W5:R4 红眼 `z42` 在 W5:951 吃第一炮、W6:12 吃第二炮并开始减速投掷；W6:119 生成小鬼 `z77`。计划标签 `w6_lower_intercept` 的炮语义命中为 W6:225，实际因恢复等待命中 W6:247。`z77` 在 W6:225 的精确位置本应位于爆区；W6:247 时已经落地越过右界而未命中。它于 W6:281 开始啃 `cob:R4C7` 的前盆，累计 84 点，超过该 role 的 8 点预算。证据：投掷/父子事件、operation delay、两次 cob resolution、bite episode。

AI 由此能判断修的是炮恢复/炮序，而不是误改小鬼父体处理。

### 13.2 巨人砸炮

理想报告应说明：

> W13:R3 红眼在 W13:950 后按位置分支未吃跨波 D；W14:304 的偏左炮也未命中父体，这是阵解允许分支。脚本本应由 W14:595 完成下路闭合，但该 operation 选择的炮已在前一 `recover_p` reservation 中，实际报 NoReadyCob。巨人从 W14:304 至 598 恢复行走，W14:602 锁定 `cob:R4C7`，W14:612 砸中。这里最早缺陷是 operation 失败，而不是“巨人随机走太快”。

这类因果链正是人类看录像时在脑中完成的工作。

## 14. 分阶段实施建议

### 阶段 0：用现有能力验证报告形状

不改 PE 核心机制，先在一个只用于研究的分支/原型中验证：

- 复用现有 plant-effect/home-entry；
- 每帧读取现有 zombie/plant/projectile 原子事实；
- 建立 generation-aware 生命周期、bite episode、基础 MotionSegment 和 WaveLedger；
- 从 Timeline/炮管理器记录 OperationRecord；
- 对 RE20 单 seed 输出结构化报告。

验收：已发生的砸击、啃食、篮球命中、进家和 `recover_p` 延迟都能追到脚本操作；报告明确标出仍缺 parent/cob-hit 精确因果的地方。

### 阶段 1：补最小瞬时事件

优先顺序：

1. 小鬼/伴舞/篮球的 parent/source relation；
2. 炮弹 spawn/detonation/实际 hit；
3. 巨砸、撑杆/跳跳起跳的 action target commit；
4. 必要的同帧 lifecycle end。

验收：针对一只砸炮巨人、一只漏鬼、一只早炮未伤撑杆、一枚父体死亡后的篮球，都能无画面回答第 3 节七个问题。

### 阶段 2：通用 finding 与稀疏预期

- 加入 role、预算、selector/window/predicate；
- 先支持 4–6 个正交谓词，不做“每种僵尸一个 DSL”；
- 输出实际反例及上下文；
- 用 RE20 逐波阵解抽取少量关键预期作验收，不把全文人工翻译成标注。

### 阶段 3：多 trial 筛选与自动精查

- 第一遍保存代表失败 seed；
- 自动生成第二遍 focus 配置；
- 验证分片下 trial sequence/seed 稳定；
- 合并报告时保持 seed 与 world epoch，不跨 trial 混对象。

## 15. 验证矩阵

### 15.1 微场景

至少构造：

- 巨人连续吃 1–4 炮后砸不同植物层；
- Ice3 在投掷生成点重置，解冻后重投与父体提前死亡两支；
- 小鬼空炸、尾炸命中/漏出、落地后短啃/长啃；
- 撑杆在爆区右界两侧、起跳前/中/后被炮处理；
- 舞王召唤四伴舞、伴舞死亡/补召/黄油脱离；
- 跳跳失杆、越高坚果、进家；
- 投篮车死亡后篮球继续命中；
- `recover_p` 准时、延迟、预约炮失效。

### 15.2 RE20 回归

用逐波阵解中最能暴露错误理解的节点：

- W1 Ice3 只重置当次投掷，不永久删除资格；
- W2 的 B 命中/未命中父体分支都闭合；
- 早炮不应被误记为已经全伤撑杆；
- W3/W4/W5 跨波撑杆和残红要按来源波保留；
- 已生成篮球在父体死后仍造成 75 点受控伤害；
- W19/20 `recover_p` 的语义时间与实际命中时间分离。

### 15.3 系统性质

- 详细 trace 关闭时，不改变 PE 结果与现有统计；
- 开启 trace 只观察，不改变 RNG 调用数量和游戏状态；
- 相同 PE 版本/seed/config 的 focus replay 产生相同关键事件；
- 所有缓冲溢出都可见，绝不静默截断；
- 单/多 worker 的第一遍统计一致，第二遍强制单 worker；
- 报告 encoder 可验证 schema 与引用完整性；
- 与现有 DamageNarrow/Smash/Pogo 聚合对账，计数不凭空改变。

## 16. 不推荐的方案

1. **给每个 zombie 加 `vector<hit>`/`vector<walk>`**：污染游戏对象、内存不可控、复制/reset 复杂。
2. **只输出每帧完整 JSON snapshot**：数据巨大，关键因果反而淹没，边界事件仍可能丢失。
3. **只增加若干专用 getter**：历史在发生后无法从当前对象读取；还违反 backend 原子能力边界。
4. **只依赖脚本标注**：只能检查 AI 已想到的假设，无法发现盲区。
5. **只依赖通用炮伤阈值**：无法区分受控篮球/梯损与真正炮体泄漏。
6. **把详细 trace 塞进公开 GameEvent queue**：容量/稳定性/热路径都不合适。
7. **为 1051 做空实现**：会让 AI 把缺失证据当成“没有异常”；应明确 unsupported。
8. **一次广测保存所有对象全历史**：多 worker 放大后代价高，应先筛 seed 再精查。

## 17. 最小可用版本必须拥有的信息

如果需要把范围压到最小，第一版至少不能少于：

1. run/script/config/seed/trial/version；
2. 三套时钟和 Timeline operation ID；
3. selected cob、预约 delay、实际发射/爆炸；
4. 每个炮实际命中的 zombie ID、from_wave、row、x、phase、前后 HP；
5. 小鬼父巨人、篮球投手、伴舞领舞；
6. 现有 plant-effect/home-entry 的完整明细；
7. 重点对象的压缩 MotionSegment；
8. 植物实例/层级/role 与伤害预算；
9. 波边界 unresolved ledger；
10. finding 的证据 ID、失败 seed 和截断状态。

只有这十项齐全，AI 才能从“看到失败概率”迈到“解释是哪条阵解假设或哪项脚本操作有缺陷”。

## 18. 资料与代码依据

本设计查阅了以下本地资料，没有打开 `日常坐卧.md` 中的外部网站：

- `C:\Users\123\Desktop\rust vs zombies开发\RE20逐波逐行阵解.md`
- `C:\Users\123\Desktop\rust vs zombies开发\日常坐卧.md`
- `dev-scripts/lowdsl/re20_p/src/lib.rs`
- `crates/core/model/src/model/event.rs`
- `crates/core/backend-api/src/backend/contact.rs`
- `crates/core/backend-api/src/backend/state.rs`
- `crates/core/game/src/event_measure.rs`
- `crates/core/game/src/logic/zombie_motion.rs`
- `crates/core/game/src/logic/cob/manager.rs`
- `crates/core/schedule/src/event.rs`
- `../PvZ-Emulator/system/event.h/.cpp`
- `../PvZ-Emulator/system/zombie/gargantuar.cpp`
- `../PvZ-Emulator/system/zombie/pole_vaulting.cpp`
- `../PvZ-Emulator/system/zombie/pogo.cpp`
- `../PvZ-Emulator/system/zombie/dancing.cpp`
- `../PvZ-Emulator/system/zombie/catapult.cpp`
- `../PvZ-Emulator/system/projectile/projectile_system.cpp`
- `../PvZ-Emulator/system/damage.cpp`
- `../references/pvz-wiki-slim/pages/教程/僵尸图鉴.txt`
- `../references/pvz-wiki-slim/pages/攻略/僵尸速度.txt`
- `../references/pvz-wiki-slim/pages/技术/精确数据.txt`
- `../references/pvz-wiki-slim/pages/技术/生存无尽入门.txt`
- `../puc/target/release/puc.exe docs intercept` 与 `puc.exe docs seml` 的本地帮助。

PUC/SEML 适合在阵解阶段计算命中边界、拦截窗和概率旁证；它们不替代运行时 trace。运行时诊断的价值正是告诉 AI“这一个失败 seed 中，哪个实际对象沿哪条分支走到了什么结果”。
