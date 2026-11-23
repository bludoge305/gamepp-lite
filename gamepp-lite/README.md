# GamePP-Lite · 游戏识别 + Windows 优化（Rust + egui）

对 GamePP（游戏加加）的**结构复刻**：读它怎么做的，然后用自己实现把同样的边界与交互走一遍。
不做注入、不破解、不伪造数据。

- 逆向依据：[`../report/gamepp-four-features-reverse-report.md`](../report/gamepp-four-features-reverse-report.md)
- 架构说明：[`docs/architecture.md`](docs/architecture.md)
- 界面截图：`qa/`（`shot-detect.png` / `shot-winopt.png` / `shot-settings.png`）

---

## 三个入口

| 入口 | 做什么 |
|---|---|
| ① 游戏识别 | 前台窗口 → PID → 映像路径 → 游戏库/启发式判定；**游戏库可编辑/删除/恢复默认**；手工登记；最近会话 |
| ② Windows 优化 | 进程优先级、后台进程、系统服务、电源方案、内存回收、剪贴板；预演 / 执行 / 还原；**可设为"识别到游戏自动执行"** |
| ③ 设置 | 识别节拍、数据保留、**受保护白名单**（只读）、**托盘常驻**（收托盘/气泡/开机自启/退出）、关于 |

### 受保护白名单（安全边界）

原版没有任何"不要碰系统"的概念：特征表命中就杀/改优先级，释放内存与还原优先级更是
全进程遍历。本工程在 `src/domain/protect.rs` 里维护一张白名单，**扫描、优化、清理全部
先过它**，命中即跳过，并在执行日志里写明跳过数量与原因：

- PID 0 / 4（Idle / System）永远受保护；
- 107 条系统组件映像名（按 `.exe` 归一后 106 个唯一名），分 7 组（内核与会话、系统管理工具、外壳与输入、安全与防护、
  安装与更新、虚拟化、本程序自身）——任务管理器、MMC 各管理器、注册表、资源监视器、
  Explorer、Defender、spoolsv、WMI 宿主…；
- 系统目录前缀（`%SystemRoot%`、`%ProgramFiles%\WindowsApps`、Windows Defender 等）；
- 46 个核心服务（RpcSs / DcomLaunch / Winmgmt / EventLog / Power / BFE / MpsSvc / WinDefend…）。

匹配是**大小写不敏感的精确名匹配**（去掉 `.exe` 再比，不做子串）+ 路径前缀（要求分隔符
边界），所以 `C:\Windows10Games` 不会被当成 `C:\Windows`。`OptPlan` 里可停的 7 个服务
刻意不在白名单内，单元测试锁死（见 `docs/architecture.md` §5.1）。

原来的监控面板、自动触发规则页、独立托盘页已按需求移除：
监控（逐秒遥测）不再采集，规则机制改成 ② 页里的两个自动化开关，托盘配置并入 ③ 页。

## 跑起来

```cmd
build.cmd                  :: 或 cargo build --release
target\release\gamepp-lite.exe
```

用安装包装（`dist\` 在仓库根目录）：

```cmd
build-installer.cmd                                 :: 在仓库根目录执行
dist\gamepp-lite-setup-0.2.7.exe                    :: 交互式
dist\gamepp-lite-setup-0.2.1.exe /S /D=D:\Tools\Lite :: 静默
dist\gamepp-lite-setup-0.2.7.exe /uninstall          :: 卸载
```

> Windows 优化需要管理员权限：② 页顶部有「以管理员身份重启」，未提权时只会记录"已跳过"。

## 图标与托盘状态

| 位置 | 图标 |
|---|---|
| `gamepp-lite.exe` / 安装包 | 石板灰圆角方 + 白色闪电（`assets/app.ico`，16~128 六个尺寸） |
| 托盘 · 未优化 | 同上一套：灰 + 闪电 |
| 托盘 · 已优化 | 绿色 + 白色对勾（`assets/app-opt.ico`） |

托盘图标不是静态的：执行/还原 Windows 优化后会即时换图标，提示文本也会带上
`已优化 / 未优化` 与当前游戏名。这样不开窗口就能看出优化状态。
图标由 `analysis/make_icons.py` 生成（纯 Python 画像素 + 4× 超采样），
`build.rs` 调 `windres` 把图标与版本信息编进 exe。

## 数据与日志

```
%LOCALAPPDATA%\GamePP-Lite\
  config.json    gamelib.json    opt-state.ini
  runtime.log    启动 / 托盘 / 优化逐行流水
  crash.log      panic 现场（含 backtrace）
```

**遇到"闪退"，先看 `runtime.log` 最后几行和 `crash.log`。**

## 修过的几个典型问题（都留了注释）

1. **中文显示成 □**：egui 自带字体只有拉丁字形。启动读系统字体
   （`msyh.ttc` → `Deng.ttf` → `simhei.ttf` → `simsun.ttc`）插到字体表首位。
2. **右栏被裁切 / 页面横向溢出**：手算 `available_width*0.55` 分栏不可靠；
   改用 `ui::split_row`（`ui::column` 定宽子 Ui + 黄金分割比例），右栏吃剩余宽度，
   行内先给右侧按钮留固定宽度。`ui.columns` 会均分且拿不到侧栏宽度，行内两列
   再做自适应子分栏时同样会溢出，所以最终没有采用。
3. **条目文字不对齐**：`add_sized` 会把控件居中，`Label::halign` 也救不回来；
   改用固定宽度 + `left_to_right` 子布局（`ui::text_cell`），所有条目从同一列开始。
4. **点关闭/收托盘像闪退**：`ViewportCommand::Visible(false)` 之后 winit 不再派发 redraw，
   `update` 停摆、托盘菜单点了没反应。改为最小化；第二个实例被单实例锁拦下时会唤醒
   常驻实例，唤醒失败才弹 MessageBox。
5. **单实例误判**：`GetLastError` 不会因调用成功而清零，前面任何失败的文件调用留下的 183
   会让 `CreateMutexW` 之后被误判成"已有实例"。修法是调用前 `SetLastError(0)`。

## 开发期调试开关

这些环境变量只用于本地核验排版 / 自动截图 / 托盘自检，正式使用不需要设置：

| 变量 | 作用 |
|---|---|
| `GPP_SIZE=1040x720` | 指定逻辑窗口尺寸，便于在小窗口下检查布局 |
| `GPP_TAB=1\|2\|3` | 启动时直接停在某一页（1 识别 / 2 优化 / 3 设置） |
| `GPP_SHOT=<path.bmp>` | 在第 N 帧后把窗口存成 24 位 BMP（`qa/*.png` 就是这么来的） |
| `GPP_SHOT_FRAME=<n>` | 截图帧号，默认 6；等高布局要等几帧收敛 |
| `GPP_PROBE=1` | 第 5 帧最小化到托盘、第 40 帧记一条"事件循环仍在跑" |
| `GPP_TRAY_TOGGLE=1` | 第 8/16 帧切换托盘"已优化/未优化"图标，验证换图标链路 |
| `GPP_OPT_DRY=1` | 启动即跑一次 Windows 优化**预演**（只读，但会写 runtime.log） |
| `GPP_INSTANCE_SUFFIX=xxx` | 让测试实例与正式实例并存（**仅 debug 构建生效**） |

## 明确没有做的事

注入游戏进程、Present hook、覆盖层绘制、GPU/温度/功耗采集、会话落 SQLite。
② 页里唯一"偏离原版"的地方是：每个动作前先过受保护白名单（svchost/explorer/lsass… 等系统组件），
因为实测原版的无差别匹配会命中它们，提优先级有害无益。
