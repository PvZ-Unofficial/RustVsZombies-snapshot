# RustVsZombies (rsvz)

**RustVsZombies** 是一个用 Rust 重写的植物大战僵尸（Plants vs. Zombies）游戏自动操作库，
由 GPL-3.0 项目 [AsmVsZombies](https://github.com/vector-wlc/AsmVsZombies)（avz2）重构而来。

> ⚠️ **当前项目处于早期开发阶段，尚未发布稳定版本。API 和行为随时可能变化，请谨慎使用。**

## 使用方式
当前版本唯一的推荐使用方式是让llm将avz的键控无炮脚本转化为rsvz，从而使用pe模拟器进行高性能模拟。使用方式示例：（下文是给llm的典型提示词）

- 帮我下载PvZ-Unofficial/RustVsZombies-snapshot
- 有一个avz的脚本xxx.cpp，我想要转化为rsvz的对应脚本，你看看是否有什么需要确认的问题
- 好的，请进行转换吧
- 进行10分钟的测量，开启thinlto
- 分析一下可能的失败原因
- 请开启1051游戏并完成注入运行

## 备注
- 请将1051的游戏位置记录在（占位），如果其中没有记录，请询问用户1051的游戏位置后进行记录
- 安全三叶草、安全模仿寒冰菇除非有特殊逻辑，否则请使用rsvz内置版本。如果有疑惑请向用户确认

## 许可证

RustVsZombies 以 [GNU General Public License v3.0 only](LICENSE) 发布。仓库中单独
标明许可证的第三方组件继续适用其各自的许可证。
