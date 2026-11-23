# GamePP Windows 优化模块：逆向分析与 Rust 复刻

对 GamePP（游戏加加）「游戏优化 → Windows 优化」模块的静态逆向分析，以及一份行为对齐的
Rust 复刻：带图形界面的 `GamePP-Lite`、零依赖命令行版 `gamepp-winopt`，和打包用的安装器。

只做分析与重写，不注入、不破解、不伪造数据。

```
.
├── report/
│   ├── gamepp-winopt-reverse-report.md      Windows 优化模块逆向报告（反汇编/字符串证据）
│   └── gamepp-four-features-reverse-report.md  四项功能整体逆向报告
├── gamepp-lite/                主程序：游戏识别 + Windows 优化（Rust + egui）
│   ├── src/                    win/ 原生层 · services/ 识别与托盘 · opt/ 优化 · ui/ 三页
│   ├── opt-data/               48 条后台进程黑名单、371 条游戏特征表
│   ├── docs/architecture.md    设计文档
│   └── qa/                     界面截图
├── gamepp-winopt/              命令行版（零依赖，只做 Windows 优化）
├── installer/                  NSIS 安装脚本（打包 gamepp-lite → dist/setup exe）
├── analysis/                   逆向过程中用的脚本与转储
├── .github/workflows/          push tag 自动构建 + 发 GitHub Release
├── dist/                       构建产物（安装包，不入库）
└── build-installer.cmd         一键构建（主程序 → NSIS 安装包 → dist）
```

## 快速开始

用安装包（推荐）：

```
build-installer.cmd                              :: 先构建
dist\gamepp-lite-setup-0.2.7.exe                 :: 交互式安装
dist\gamepp-lite-setup-0.2.1.exe /S /D=D:\Tools\GamePP-Lite
```

只用命令行版：

```bash
cargo build --release --manifest-path gamepp-winopt/Cargo.toml
gamepp-winopt.exe status
gamepp-winopt.exe start --dry-run        # 先看清单，不产生副作用
gamepp-winopt.exe start --game-id 500 --cores 8
gamepp-winopt.exe stop --dry-run         # 先看还原范围
gamepp-winopt.exe stop
```

两者都需要管理员权限才能实际生效；命令行版会自动弹 UAC 拉起提权副本，图形版在 ② 页有
「以管理员身份重启」。

## 这个模块做了什么

读 `%LOCALAPPDATA%\GamePP\GamePPOpt.ini` 的 `[SYS]` 节，然后按开关执行 6 类系统调用：

1. `WinExec("ipconfig /flushdns")` — 清 DNS 缓存
2. `ControlService(SERVICE_CONTROL_STOP)` — 停 WSearch / XblAuthManager / DiagTrack / Diagsvc / wuauserv / HomeGroup*（SysMain 因配置字节从不赋值而永远不停）
3. `TerminateProcess` — 结束名字含 `wallpaper` 的进程、以及 48 条后台进程黑名单（鲁大师/360 系后台）
4. `SetProcessWorkingSetSize` + `K32EmptyWorkingSet` — 全系统进程内存回收
5. `SetPriorityClass` / `SetProcessAffinityMask` — 命中 371 条特征表的进程调优先级（ABOVE_NORMAL；特殊标题反而降为 BELOW_NORMAL）
6. `OpenClipboard` + `EmptyClipboard` — 清空剪贴板

外加无条件执行的 `WinExec("lodctr /R")`，和由 `Power.dll` 完成的电源方案切换
（`PowerImportPowerScheme` + `PowerSetActiveScheme`，数据为 `PowerData\GPP_*.pow`）。

细节、调用链和证据见 [`report/gamepp-winopt-reverse-report.md`](report/gamepp-winopt-reverse-report.md)。

## 注意事项

`start` 会真实结束进程、停系统服务、清空剪贴板、抽走工作集；`stop` 的还原是**不完整**的
（被杀的进程、DNS、剪贴板不可恢复），并且会把当前会话所有进程的优先级复位为 NORMAL
（受保护白名单内的系统组件除外）。建议先跑 `--dry-run`（图形版是「预演」按钮）。

两个程序共有两处**刻意偏离原版**：

1. **受保护白名单**（`gamepp-lite/src/domain/protect.rs`，命令行版用 `#[path]` 共用同一份）：
   PID 0/4、107 条系统组件映像名（按 `.exe` 归一后 106 个唯一名；内核与会话、任务管理器/MMC/注册表等系统管理工具、
   外壳与输入、安全与防护、安装与更新、虚拟化、本程序自身）、系统目录前缀
   （`%SystemRoot%`、`%ProgramFiles%\WindowsApps`、Windows Defender…）、46 个核心服务。
   扫描（识别）、终止进程、改/复位优先级、释放工作集、停服务全部先过它，命中即跳过，
   并在日志里写出"跳过了多少个、因为什么"。
   原版的无差别子串匹配实测会命中 svchost / explorer / Defender，而「释放内存」与
   「优先级复位」本身就是全进程遍历，不设边界会直接动到系统组件。
2. **Wallpaper Engine 可还原**：终止前记录 exe 路径，还原时重新启动（原版杀掉就不管了）。

## 版权与声明

本项目是对第三方产品的独立研究性复刻，与 GamePP（游戏加加）厂商无任何关联；
"GamePP / 游戏加加"及相关名称归其权利人所有。仓库有意不包含从被分析产品中
提取的二进制与批量字符串转储；代码许可见 [LICENSE](LICENSE)（MIT）。

---

## GamePP-Lite（图形界面版）

三个入口（游戏识别 / Windows 优化 / 设置），Rust + egui，单文件约 15 MB。

- 逆向报告：`report/gamepp-four-features-reverse-report.md`
- 架构设计：`gamepp-lite/docs/architecture.md`
- 一键构建（主程序 + 安装包）：`build-installer.cmd` → `dist\gamepp-lite-setup-0.2.7.exe`
- 只构建主程序：`gamepp-lite\build.cmd`

## 两个程序的关系

`gamepp-winopt/` 是零依赖命令行版，`gamepp-lite/` 是图形版（优化逻辑是它的
`src/opt/` 子集）。两者各自可单独构建，但**共用同一份**受保护白名单与数据表
（命令行版用 `#[path]` / `include_str!` 直接引用图形版的 `protect.rs` 与
`opt-data/*.txt`），避免两份副本各改各的。安装器只打包图形版。
