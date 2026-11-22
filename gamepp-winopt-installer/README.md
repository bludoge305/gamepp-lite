# 安装程序（gamepp-lite-setup）

`gamepp-lite` 的自包含安装 / 卸载器。**单个 exe、零外部依赖**（不依赖 Inno Setup / NSIS / .NET），
主程序与文档在构建时用 `include_bytes!` 嵌入。

产物：`dist\gamepp-lite-setup-0.2.0.exe`（约 16 MB，其中 15 MB 是主程序载荷）

> 包名是 `gamepp-lite-installer`、目录名却还叫 `gamepp-winopt-installer` —— 历史遗留，
> 它最初打包的是命令行版 `gamepp-winopt`。

## 构建

```cmd
build-installer.cmd
```

它会依次构建 `gamepp-lite`（必须先有产物，安装包要嵌入它）→ 构建本工程 → 复制到 `dist\`。
也可以手动：

```cmd
cargo build --release --manifest-path gamepp-lite/Cargo.toml
cargo build --release --manifest-path gamepp-winopt-installer/Cargo.toml
```

`build.rs` 从 `gamepp-lite/target/release/` 取载荷；缺文件时直接报错并提示先构建主程序。

## 用法

| 命令 | 说明 |
|---|---|
| `gamepp-lite-setup.exe` | 交互式向导（安装目录、快捷方式、是否立即启动） |
| `gamepp-lite-setup.exe /S` | 静默安装到 `%ProgramFiles%\GamePP-Lite` |
| `gamepp-lite-setup.exe /S /D=D:\Tools\GamePP-Lite` | 静默安装到指定目录（`/D` 必须放最后） |
| `gamepp-lite-setup.exe /S /desktop` | 静默安装并创建桌面快捷方式 |
| `gamepp-lite-setup.exe /run` | 安装完成后立即启动（配合 `/S`） |
| `gamepp-lite-setup.exe /uninstall` | 卸载 |
| `gamepp-lite-setup.exe --dry-run` | 只打印将要做的操作，不改动系统 |

其他开关：`/noshortcuts`（不建快捷方式）、`/noregistry`（不写控制面板项）、`/y`（全部用默认值）。

## 装了什么

```
<安装目录>\
├── gamepp-lite.exe          主程序
├── uninstall.exe            卸载器（安装包自身副本）
├── README.md                gamepp-lite 的说明
└── docs\
    ├── REVERSE-REPORT.md    四项功能逆向报告
    └── ARCHITECTURE.md      架构设计文档
```

此外：

* 注册表 `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\GamePP-Lite`
  （DisplayName / DisplayVersion / Publisher / InstallLocation / DisplayIcon /
  UninstallString / QuietUninstallString / EstimatedSize / NoModify / NoRepair）
* 开始菜单：`%ProgramData%\Microsoft\Windows\Start Menu\Programs\GamePP-Lite\GamePP-Lite.lnk`
* 桌面（可选）：`%PUBLIC%\Desktop\GamePP-Lite.lnk`

## 实现要点

* **提权**：默认装到 `Program Files`，写入前先做一次"真实可写性探测"（在目标目录建临时文件再删除）。
  不可写且当前不是管理员时，用 `ShellExecuteW(..., "runas", ...)` 拉起自身副本并带上 `--elevated`，
  避免重复提权循环。
* **快捷方式**：手写 COM 调用 `IShellLinkW` + `IPersistFile`（vtable 索引：`SetPath`=20、
  `SetDescription`=7、`SetWorkingDirectory`=9、`SetIconLocation`=17、`IPersistFile::Save`=6），
  不依赖 PowerShell / WScript。
* **卸载**：优先用 `--dir`，其次读注册表 `InstallLocation`，最后退回自身所在目录；
  删除除"正在运行的卸载器"之外的所有文件，卸载器自身用
  `MoveFileExW(..., MOVEFILE_DELAY_UNTIL_REBOOT)` 登记为重启后删除。
* **图标与版本信息**：`build.rs` 生成 `.rc` 交给 `windres`（`--preprocessor` 指向一个
  "原样输出"的 `rc-cpp.cmd`，因为 binutils 里没有 gcc）；找不到 `windres` 只 warning，不阻断构建。
* **诊断**：`--test-shortcut <path>` 单独验证快捷方式创建链路是否可用。

> 在 Git Bash 里测试时 `/S`、`/D=…` 会被 MSYS 当路径改写，请用 `--silent`、`--dir`。

## 已验证

在 Windows 11 + rustc 1.99.1（x86_64-pc-windows-gnu）上实测通过：

* 静默安装到临时目录 → 文件齐全、`docs\REVERSE-REPORT.md` 与 `docs\ARCHITECTURE.md` 正确落盘；
* 已安装的 `gamepp-lite.exe` 能正常启动，三页界面与托盘图标正常；
* 快捷方式自检生成有效 .lnk，目标/工作目录/描述正确；
* 卸载后目录内只剩卸载器本身（已登记重启后删除）。
