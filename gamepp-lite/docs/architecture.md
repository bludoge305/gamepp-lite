# GamePP-Lite 架构设计文档

> 用 **Rust + egui** 实现一个"结构上与 GamePP 对得上、行为上可解释"的独立程序。
> 覆盖：**游戏识别**、**Windows 优化**（由 `gamepp-winopt` 合并）、**托盘常驻**（配置收进设置页）。
> 反向依据见 [`../../report/gamepp-four-features-reverse-report.md`](../../report/gamepp-four-features-reverse-report.md)。

---

## 1. 设计原则

1. **边界对齐，实现降级。** 逆向出来的是"Electron 外壳 + 原生执行体 + 注入模块"。本项目保留同样的边界
   （UI 线程 / 服务线程 / 原生层），但**不做注入**：需要进入游戏进程的能力（真实 FPS、IGO 覆盖层）显式缺席，
   界面与文档都标注来源，不伪造数据。
2. **零框架 FFI。** GamePP 自己直接调 Win32；本项目同样手写 `extern "system"`，只依赖
   `eframe`（窗口/绘制）与 `serde`（配置）。
3. **一份状态、多处只读。** 服务线程写 `Shared`，UI 每帧取快照；UI 不参与判定。
4. **破坏性动作必须显式。** 结束进程、停服务、清剪贴板默认全关，执行前必须先"预演"，并且要管理员权限。

---

## 2. 工程结构

```
gamepp-lite/
├── Cargo.toml                  eframe 0.31 / serde / serde_json
├── build.cmd                   构建脚本（自动前置 binutils，见 §10）
├── opt-data/                   优化模块的数据表
│   ├── game_patterns.txt       371 条游戏/启动器进程特征（原版 .data 转储）
│   └── kill_list.txt           48 条后台进程黑名单
├── docs/architecture.md        本文
└── src/
    ├── main.rs                 ① 单实例 → ② 托盘 → ③ 唤醒监听 → ④ 窗口
    ├── app.rs                  状态机、导航（3 页）、事件分发、优化调度
    ├── win/                    原生层（薄封装）
    │   ├── ffi.rs              Win32 声明 + 结构体
    │   ├── foreground.rs       前台窗口 → PID → 映像路径（识别入口，含优先级/图形运行时采样）
    │   ├── process.rs          进程路径 / 存活 / 优先级 / 模块枚举
    │   ├── single.rs           命名互斥体 + 命名事件（单实例与唤醒）
    │   ├── autostart.rs        HKCU\...\Run 读写
    │   ├── icon.rs             .ico 字节 → HICON
    │   ├── time.rs             epoch → 本地时间（FileTimeToLocalFileTime）
    │   └── msgbox.rs           MessageBoxW（单实例提示）
    ├── services/               服务层（每线程一个文件）
    │   ├── mod.rs              Shared / Config / Session / 日志
    │   ├── watcher.rs          识别状态机 + 进入/退出事件
    │   └── tray.rs             托盘消息窗口 + 图标 + 菜单 + 气泡
    ├── domain/                 领域模型
    │   ├── settings.rs         配置（JSON，含 OptPlan）
    │   └── games.rs            游戏库（exe → 名称/标签，可编辑）
    ├── opt/                    Windows 优化（自 gamepp-winopt 合并）
    │   ├── ffi.rs              优化模块专用 Win32 声明
    │   ├── plan.rs             OptPlan：可勾选清单 + 服务映射
    │   ├── exec.rs             run / restore 流程（保留原版动作顺序）
    │   ├── process.rs          进程枚举 / 特征匹配 / 终止
    │   ├── service.rs          SCM 停启服务
    │   ├── power.rs            电源方案（POWRPROF 动态加载）
    │   ├── state.rs            优化状态落盘（只重启自己停过的服务 / 重启自己结束的 Wallpaper Engine）
    │   └── elevate.rs          管理员检测 / runas 重启
    └── ui/                     三个入口
        ├── detect.rs           ① 游戏识别（含游戏库增删改）
        ├── winopt.rs           ② Windows 优化
        ├── settings.rs         ③ 设置（含托盘常驻配置）
        └── mod.rs              卡片 / 键值 / 胶囊 / 左对齐文本单元
```

约 5,000 行 Rust；`unsafe` 仅出现在 `win/`、`opt/` 两个原生目录内。

---

## 3. 模块职责（与逆向结果对照）

| 本项目 | 逆向对应物 | 说明 |
|---|---|---|
| `win::foreground` | `GPPPlatform.exe` 的 `GetForegroundWindow hWnd=… processId=… process=…` | 轮询版；未接 `SetWinEventHook` |
| `win::process` | `processhelper`、`K32EnumProcessModules` | 取路径/存活/优先级/图形运行时 |
| `services::watcher` | `game listener`、`CGameWatcher` | Idle → Candidate → InGame，带防抖 |
| `domain::games` | `GamePP_BaseInfo.processpath` + `common\Icon\<exe>.png` | exe 为键；支持编辑/删除/恢复默认 |
| `opt::plan` | `GamePPOpt.ini [SYS]` 22 个开关 | 收敛为 13 个有动作的项 + 2 个自动化开关 |
| `opt::exec` | `GamePPOptimize.node!sub_10013f70` / `sub_100177f0` | 保留原版动作**顺序**与判据 |
| `opt::service` | `sub_100141e0` / `sub_10016d40` | 8 个服务；SysMain 恒不停（照抄原版） |
| `opt::power` | `Power.dll` 的 `PowerImportPowerScheme` / `PowerSetActiveScheme` | 运行时动态取 POWRPROF 导出 |
| `opt::state` | 原版进程内全局结构体 + 服务标记表 | 落盘成 `opt-state.ini`，语义等价；另记录被结束的 Wallpaper Engine（还原时重启） |
| `services::tray` | `wapTray` / `createAppTray` / `Game_Traymenu` / `displayBalloon` | 原生 `HMENU` 菜单 |
| `win::single` | `requestSingleInstanceLock` + `second-instance` | 互斥体 + 命名事件 |
| `win::autostart` | `isStartedWithOs` | `HKCU\...\Run` |

---

## 4. 线程与数据流

```
        ┌──────────────── UI 线程（eframe）────────────────┐
        │ App::update                                     │
        │  ├─ 托盘命令 tray_rx（切页/自启/气泡/退出）        │
        │  ├─ 识别通知 notices（GameStarted / GameEnded）   │
        │  ├─ 优化结果 opt_rx（逐行日志）                   │
        │  └─ 画 3 个页面 + 读写 Config                     │
        └──────┬──────────────────────────────┬───────────┘
               │ Arc<Mutex<Shared>>（只读快照）  │ Arc<Mutex<Config>>（读写）
   ┌───────────┴────────────┐        ┌────────┴──────────────────┐
   │ gpp-watcher 线程        │        │ gpp-winopt 线程（按需创建）│
   │ 每 poll_ms：            │        │ opt::run_plan / restore    │
   │  foreground::current()  │        │  → Vec<String> 日志        │
   │  → classify → 状态机     │        │  → State 落盘              │
   │  → 发 GameStarted/Ended │        └───────────────────────────┘
   └─────────────────────────┘
   ┌─────────────────────────┐        ┌───────────────────────────┐
   │ gpp-tray 线程            │        │ gpp-ui-tick 线程           │
   │ CreateWindowExW(HWND_    │        │ 每 400 ms request_repaint  │
   │  MESSAGE) + GetMessage   │        │ （最小化后仍响应托盘）      │
   └─────────────────────────┘        └───────────────────────────┘
```

要点：

- 锁顺序恒为 `Config → Shared`，且不跨 Win32 调用持锁；
- 所有 `.lock()` 都做"中毒容错"（`unwrap_or_else(|e| e.into_inner())`）；
- **Windows 优化跑在独立线程**：它会停服务、遍历进程，放在 UI 线程会卡死界面；
- **UI 不做系统调用**：前台窗口、进程优先级、图形运行时都由 `gpp-watcher` 按识别节拍
  （默认 1 秒）采样一次放进 `Shared`，`ui/` 每帧只读快照。模块枚举按 `(pid, exe)` 缓存，
  否则 60 fps 下会每秒枚举上百次模块。

---

## 5. 识别状态机

```
   ┌──────┐ 命中游戏库 / 全屏+图形运行时
   │ Idle │──────────────────────────────┐
   └──────┘                              ▼
      ▲                        ┌─────────────────────┐
      │ pid 变 / 不再命中        │ Candidate           │ ticks += 1
      └────────────────────────│ (pid, exe, ticks)   │
                               └──────────┬──────────┘
                                          │ ticks ≥ stable_polls
                                          ▼
                               ┌─────────────────────┐
                               │ InGame              │ 建 Session → GameStarted
                               └──────────┬──────────┘
                                          │ 进程消失（GetExitCodeProcess ≠ 259）
                                          ▼
                                   收尾会话 → 历史 → GameEnded → Idle
```

`InGame` 对 alt-tab 粘性（时长按进程生命周期算），退出只认进程消失。

---

## 6. 主要流程

### 6.1 启动

```
main → 安装 crash log → acquire(mutex)
     ├─ 失败 → SetEvent(awake) 成功则静默退出；失败才 MessageBox（避免"闪退"错觉）
     └─ 成功 → spawn(tray) → spawn(awake_listener) → Services::start(watcher) → run_native
```

### 6.2 识别 → 自动优化（原"自动触发规则"的替代）

```
watcher: InGame → UiNotice::GameStarted
App:     若 settings.opt.auto_on_game_start 且已提权 → start_opt(dry_run=false)
        未提权 → 只记一条 Warn 日志，不静默失败
退出:    UiNotice::GameEnded → 若 auto_restore_on_exit 且当前处于"已优化" → start_opt(restore=true)
```

### 6.3 Windows 优化执行

```
UI 按钮 → App::start_opt(dry_run, restore)
        → 起 gpp-winopt 线程 → opt::run_plan(&plan, game_pid, dry_run)
            ① 清 DNS  ② 关 Wallpaper Engine  ③ 停后台进程  ④ 释放内存
            ⑤ 停服务 ⑥ 电源方案              ⑦ lodctr /R   ⑧ 清剪贴板
            ⑨ 优先级（跳过系统关键进程与自身）
        → 逐行日志回传 UI；非预演时 State::save()（还原则 State::clear()）
          「关闭 Wallpaper Engine」终止前记录 exe 路径，还原时逐个重启
          （文件已删除 / 同名进程已在运行则跳过）
```

### 6.4 托盘图标与状态

两套图标随状态切换：

```
assets/app.ico      灰底 + 闪电  ← 未优化（State::load().optimized == false）
assets/app-opt.ico  绿底 + 对勾  ← 已优化
```

- 启动时 `main` 读 `opt-state.ini` 决定初始图标；
- 优化/还原结束后 `TrayHandle::set_optimized(bool)` 发 `WM_APP+2`；
- 托盘线程在 `WM_APP+2` 里重建 HICON（`CreateIconFromResourceEx` 从 ICO 字节解析）
  并 `Shell_NotifyIconW(NIM_MODIFY, NIF_ICON|NIF_TIP)` 一次改完图标与提示；
- 提示文本 = `GamePP-Lite · 已优化/未优化 · 游戏中：xxx`。

### 6.5 托盘菜单 / 收托盘 / 退出

- 右键图标：`WM_APP+1` → `CreatePopupMenu` + `TrackPopupMenu(TPM_RETURNCMD)` → `TrayCmd` → UI 线程切页；
- 点 × 或"立即最小化"：`CancelClose` + `Minimized(true)`；
  **不用 `Visible(false)`** —— 实测窗口真正隐藏后 winit 不再派发 redraw，`update` 停摆，
  托盘菜单点了没反应，用户看到的就是"闪退"；
- 退出：`quitting = true` → 存盘 → `NIM_DELETE` → 关闭窗口。

### 6.6 以管理员身份重启

```
② 页按钮 → single::release()（让出单实例互斥体）
        → ShellExecuteExW("runas", 自身)   不等待
        → 成功：本实例存盘退出；被取消：重新 acquire，继续以普通权限运行
```

**必须先让出互斥体**：提权副本是同一会话的新进程，会看到我们持有的同名互斥体，把自己判成
"第二个实例"→ 唤醒我们 → 直接退出。那样点完按钮只会看到窗口消失（这个坑踩过）。

---

## 7. 配置与持久化

`%LOCALAPPDATA%\GamePP-Lite\`

| 文件 | 内容 |
|---|---|
| `config.json` | 识别节拍、稳定轮数、托盘开关、自启、数据保留、`opt`（优化清单 + 两个自动化开关） |
| `gamelib.json` | 游戏库（exe、显示名、标签） |
| `opt-state.ini` | 优化状态：是否已优化、停过哪些服务、被结束的 Wallpaper Engine、上一套电源方案 |
| `runtime.log` | 启动 / 托盘 / 优化逐行流水（排查"闪退"与优化失败） |
| `crash.log` | panic 现场 + backtrace |

---

## 8. 与 `gamepp-winopt` 合并的说明

原工程是**零依赖命令行程序**（`status / start / stop`），本次把它的 6 个能力文件整体搬进
`src/opt/`，只做了三处改动：

1. 输出从 `println!` 改成 `&mut Vec<String>`，由 GUI 展示 + 落 `runtime.log`；
2. 配置源从"读 GamePP 的 INI"改成"自己的 `OptPlan`"（不再提供 INI 导入）；
3. 增加一条**安全偏差**：提优先级前跳过系统关键进程
   （`svchost.exe / explorer.exe / services.exe / lsass.exe / dwm.exe` 等 12 个）与自身进程。
   原因：原版对"命中特征表"的进程无差别处理，实测会命中 svchost 一类系统进程；
   给它们提优先级没有收益、反而可能拖慢整机。日志里会写明"跳过 N 个系统关键进程"。

命令行版仍然保留在 `gamepp-winopt/`，两者互不依赖。

---

## 9. 安装器（NSIS）

`installer/gamepp-lite.nsi` 是 NSIS 3 脚本，`build-installer.cmd` 在构建主程序后调用
`makensis` 打包；产物从 15MB 压到约 4MB（lzma 固实压缩）。曾用零依赖 Rust 自研安装器，
已按"主流打包方式"决策替换为 NSIS。

```cmd
gamepp-lite-setup.exe                       交互式向导（许可 → 目录 → 安装 → 完成页可勾选运行）
gamepp-lite-setup.exe /S /D=D://Tools//Lite   静默安装（/D 必须放最后）
gamepp-lite-setup.exe /S /desktop           静默安装并创建桌面快捷方式
gamepp-lite-setup.exe /S /run               装完立即启动
gamepp-lite-setup.exe /S /noshortcuts       不创建快捷方式
gamepp-lite-setup.exe /S /noregistry        不写控制面板卸载项
gamepp-lite-setup.exe /uninstall            卸载（复用安装包本体）
```

实现要点（踩过的坑）：

- **`SetRegView 64`**：NSIS 本体是 32 位进程，不加这一句 HKLM 卸载项会写进
  `WOW6432Node`，"程序和功能"里看不到。安装（`.onInit`）与卸载（`un.onInit`）都要设。
- 卸载项写 `HKLM\...\Uninstall\GamePP-Lite`（DisplayName/DisplayVersion/InstallLocation/
  UninstallString/QuietUninstallString/EstimatedSize/NoModify/NoRepair）；
  卸载时顺带清掉应用自己写的 `HKCU\...\Run\GamePPLite`（开机自启）。
- 脚本文件必须带 **UTF-8 BOM**（`Unicode true` 下无 BOM 会按 ANSI 解析，中文乱码）。
- 快捷方式用 `SetShellVarContext all` 建到公共开始菜单（%ProgramData%）与公共桌面。
- `RequestExecutionLevel admin`：装到 Program Files 必然提权（与 Inno 默认行为一致）。
- 卸载只删除安装过的文件，目录里有用户自己的文件时 `RMDir` 安全失败。

已实测（Windows 11，NSIS 3.13）：静默安装到临时目录 → 文件齐全 → 64 位卸载项可见 →
`uninstall.exe /S` 静默卸载后目录/注册表/快捷方式全部清除。

> 注意：在 Git Bash 里测试时 `/S`、`/D=…` 会被 MSYS 当路径改写，请改用 cmd 调用。

---

## 10. 构建

```cmd
build-installer.cmd        :: 主程序 + NSIS 安装包 → dist\gamepp-lite-setup-<版本>.exe
gamepp-lite\build.cmd      :: 只构建主程序
```

> **GNU 工具链的坑**：`eframe → winit → windows-sys 0.61 → windows-link` 使用 `raw-dylib`，
> rustc 在 `windows-gnu` 目标下会调用 `dlltool`（它自己还要调 `as`）。
> rustup 自带的 `dlltool.exe` 旁边没有 `as.exe`，于是报
> `error calling dlltool 'dlltool.exe': program not found` / `CreateProcess`。
> 解决：准备完整 binutils（含 `as.exe`）并把 `bin` 目录前置到 `PATH`。
> 本机放在 `~/.cargo/mingw-binutils/bin`，两个 build 脚本都会自动探测。

> **图标/版本信息的坑**：`windres` 默认用 `gcc -E` 预处理 .rc，而 binutils 包里没有 gcc。
> 两个 `build.rs` 因此会生成一个"原样输出"的 `rc-cpp.cmd`（忽略 `-xc/-D`，只 `type` 最后一个参数），
> 用 `--preprocessor` 指过去；找不到 `windres` 时只打 warning，不影响构建。

---

## 11. 已知限制

| 限制 | 原因 | 影响 |
|---|---|---|
| 无注入 / 无 IGO 覆盖层 | 主动取舍 | 游戏内无浮层 |
| 真实 FPS 不可得 | 需要 Present hook | 已随监控面板一并移除 |
| GPU/温度/功耗不可得 | 需要 NVAPI/ADL/MSR 驱动 | 不再展示 |
| 游戏库为演示数据 | 原库在加密包/服务端 | 支持手工登记与编辑 |
| 会话不落 SQLite | 保持精简 | 历史仅存内存 |
| 托盘菜单为原生 `HMENU` | 免第二个渲染进程 | 不能放富内容 |
