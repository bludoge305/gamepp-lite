# 安装程序（NSIS）

`gamepp-lite` 的 Windows 安装包用 **NSIS 3** 打包：`installer/gamepp-lite.nsi` 是唯一的事实来源，
`build-installer.cmd` 在构建主程序后调用 `makensis` 生成 `dist\gamepp-lite-setup-<版本>.exe`。
（曾用零依赖 Rust 自研安装器，2026-11 按"主流打包方式"决策替换为 NSIS。）

产物大小约 4MB（主程序 15MB，lzma 固实压缩后 27%）。

## 本机构建

```cmd
winget install NSIS.NSIS     :: 一次性安装 NSIS 3（默认装到 %ProgramFiles(x86)%\NSIS）
build-installer.cmd          :: 构建主程序 + 打包 → dist\
```

版本号自动取自 `gamepp-lite/Cargo.toml`；也可用 `NSIS_HOME` 指定自定义 NSIS 目录。

## 用法

| 命令 | 说明 |
|---|---|
| `gamepp-lite-setup.exe` | 交互式向导（许可协议 → 目录 → 安装 → 完成页可勾选"立即运行"） |
| `gamepp-lite-setup.exe /S` | 静默安装到 `%ProgramFiles%\GamePP-Lite` |
| `gamepp-lite-setup.exe /S /D=D:\Tools\GamePP-Lite` | 静默安装到指定目录（`/D` 必须放最后） |
| `gamepp-lite-setup.exe /S /desktop` | 同时创建桌面快捷方式 |
| `gamepp-lite-setup.exe /S /run` | 装完立即启动 |
| `gamepp-lite-setup.exe /S /noshortcuts` | 不创建快捷方式 |
| `gamepp-lite-setup.exe /S /noregistry` | 不写控制面板卸载项 |
| `gamepp-lite-setup.exe /uninstall` | 卸载（复用安装包本体，读取注册表里的 InstallLocation） |
| `uninstall.exe /S` | 静默卸载 |

## 装了什么

```
<安装目录>\
├── gamepp-lite.exe          主程序
├── uninstall.exe            NSIS 卸载器
├── README.md                gamepp-lite 的说明
└── docs\
    ├── REVERSE-REPORT 相关两份逆向报告
    └── ARCHITECTURE.md      架构设计文档
```

* 注册表（64 位视图）：`HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\GamePP-Lite`
* 开始菜单（公共）：`%ProgramData%\Microsoft\Windows\Start Menu\Programs\GamePP-Lite\`
* 桌面（可选，`/desktop`）：`%PUBLIC%\Desktop\GamePP-Lite.lnk`
* 卸载时会顺带清掉应用写入的 `HKCU\...\Run\GamePPLite`（开机自启）

## 实现/踩坑备忘

* **`SetRegView 64`**：NSIS 本体是 32 位进程，不设这句 HKLM 卸载项会写进
  `WOW6432Node`，"程序和功能"里看不到；安装与卸载两侧都要设。
* **脚本带 UTF-8 BOM**：`Unicode true` 下无 BOM 的脚本按 ANSI 解析，中文全部乱码。
* `RequestExecutionLevel admin`：装 Program Files 必然提权；`/D=` 指向用户目录也一样（与 Inno 默认一致）。
* 卸载只删除安装过的文件，目录里有用户自己的文件时 `RMDir` 安全失败。
* 快捷方式用 `SetShellVarContext all` 写入公共位置。

> 在 Git Bash 里测试时 `/S`、`/D=…` 会被 MSYS 当路径改写，请改用 cmd 调用。

## 已验证

Windows 11 + NSIS 3.13：

* `/S /D=<临时目录>` 静默安装 → 6 个文件齐全 → 64 位卸载项可见 → 开始菜单快捷方式正确；
* `uninstall.exe /S` 静默卸载 → 安装目录 / 卸载项 / 快捷方式全部清除。
