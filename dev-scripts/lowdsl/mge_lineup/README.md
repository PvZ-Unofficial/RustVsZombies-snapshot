# MGE 群曾布阵

使用 `mge_qunzeng` 的布阵码，包含植物、模仿者南瓜和梯子；场景设为蘑菇园。
仅首次布阵，不运行群曾自动战斗、修补或测算，不自动选卡或跳过选卡。
阳光使用当前游戏值，不额外设置8000阳光或成熟轮次。

在 RustVsZombies 目录中执行，向已运行的1051游戏注入：

```powershell
cargo run -p rsvz-cli -- inject-1051 --dev-script lowdsl/mge_lineup
```

建议在生存无尽选卡界面使用；布阵操作会替换当前场上的阵容。
仅编译DLL时，在上述命令末尾追加 `--build-only`。
