# gamepp-winopt

GamePP「游戏优化 → Windows 优化」模块的独立 Rust 复刻（零第三方依赖）。

行为依据见 `../report/gamepp-winopt-reverse-report.md`，全部结论来自对
`GamePPOptimize.node`（PE32 x86，SHA-256 `359b7dfa…93500`）与 `Power.dll` 的静态逆向。

## 获取

本工程**没有安装包**：`cargo build --release` 后把 `target\release\gamepp-winopt.exe` 拷走即可
（单文件、零依赖）。仓库里的 `installer/`（NSIS）打包的是图形版 `gamepp-lite`，与本工程无关。

## 从源码编译

```bash
cargo build --release
```

目标平台：`x86_64-pc-windows-gnu`（已在 rustc 1.99.1 上验证通过）。

## 使用

```bash
gamepp-winopt.exe status                          # 打印当前 INI 开关与运行状态
gamepp-winopt.exe start --dry-run                 # 只打印将要执行的动作，不产生副作用
gamepp-winopt.exe start --game-id 500 --cores 8    # 执行优化
gamepp-winopt.exe stop                            # 还原
gamepp-winopt.exe stop --dry-run                  # 只打印还原计划（含亲和性掩码）
```

参数：

| 参数 | 说明 |
|---|---|
| `--ini PATH` | 指定配置文件，默认 `%LOCALAPPDATA%\GamePP\GamePPOpt.ini` |
| `--game-id N` | 标题 id，对应原版 `beginGameOptimization(gameId)`；`0x1F5/0x1FC/0x1FD/0x20B` 会走"降优先级"分支 |
| `--pid N` | 游戏进程 pid（仅记录到状态，便于诊断） |
| `--cores N` | 原版 `setCPUValue` 设定的核心数；`0` = 不做亲和性改动 |
| `--pow PATH` | 显式指定电源方案 `.pow`；**只要传了就按"切换电源方案"执行**（覆盖 INI 里的 `SystemAIPower`）。不传时仅在 `SystemAIPower=1` 时才自动在程序目录、`%LOCALAPPDATA%\GamePP\Apps\*\<版本>\win32\PowerData`、`%ProgramFiles%\GamePP` 下找 `GPP_Maxed.pow` |
| `--dry-run` | 只打印，不执行任何系统调用（`start` / `stop` 都支持） |
| `--no-elevate` | 不自动提权，以当前权限直接执行（诊断用，多数步骤会失败） |

**自动提权**：`start` / `stop` 在执行前会检查当前令牌；不是管理员时用
`ShellExecuteExW(..., "runas", ...)` 拉起自身副本，父进程等待子进程结束并把退出码透传回来。
提权后的新控制台会自动加 `--pause`，避免输出一闪而过。`--dry-run` 与 `status` 不需要提权，不会弹 UAC。

`start` 需要管理员权限（停服务、改其他进程的优先级），程序会自动请求。

**电源方案**：找不到 `.pow` 时会退而按 GUID `9fda562c-d377-43d5-bb2c-afcb0a22c547`
直接激活（前提是该方案此前已被游戏加加导入过）；两者都不可用就跳过并给出提示，不会误改电源计划。

## 与原模块的对应

| 原实现 | 本工程 |
|---|---|
| `sub_10013BD0` | `config::Config::load` |
| `sub_10013F70` | `optimize::begin` |
| `sub_10014440` / `sub_10016D40` | `service::services` / `stop_service` / `start_service_if_stopped` |
| `sub_10016B20` | `process::match_any` + `data/kill_list.txt` |
| `sub_10016720` | `process::list_processes` |
| `sub_10016580` / `sub_100174A0` / `sub_10017AA0` | `optimize::apply_priority` |
| `sub_100177F0` | `optimize::restore` |
| `Power.dll!PP_Active` / `PP_UnActive` | `power::switch_to_pow` / `power::set_active` |
| 进程内结构体 `0x1005A878` | `state::State`（`%LOCALAPPDATA%\GamePPWinOpt\state.ini`） |

## 已知偏差

原版把运行状态放在常驻 Electron 进程的内存里。独立可执行程序没有这个生命周期，
因此状态落盘到 `%LOCALAPPDATA%\GamePPWinOpt\state.ini`；语义保持一致：
状态文件不存在时 `stop` 什么都不做，只有自己停过的服务才会被重新拉起。

电源方案一节原版由 `Power.dll` 完成（`PowerImportPowerScheme` + `PowerSetActiveScheme`），
本工程用相同的 powrprof 调用，文件路径改为自动发现。

## 安全提示

`start` 会真实结束进程、停系统服务、清空剪贴板、抽走工作集。**先跑 `--dry-run` 看清单**；
`stop` 会把当前会话所有进程的优先级复位为 NORMAL（原版行为），也可以先 `stop --dry-run` 看范围。
