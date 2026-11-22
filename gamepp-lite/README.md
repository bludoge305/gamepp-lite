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
| ③ 设置 | 识别节拍、数据保留、**托盘常驻**（收托盘/气泡/开机自启/退出）、关于 |

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
dist\gamepp-lite-setup-0.2.1.exe                    :: 交互式
dist\gamepp-lite-setup-0.2.1.exe /S /D=D:\Tools\Lite :: 静默
dist\gamepp-lite-setup-0.2.1.exe /uninstall          :: 卸载
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
   改用 `ui.columns(2, …)`，并在行内先给右侧按钮留固定宽度（滚动条宽度也要算进去）。
3. **条目文字不对齐**：`add_sized` 会把控件居中，`Label::halign` 也救不回来；
   改用固定宽度 + `left_to_right` 子布局（`ui::text_cell`），所有条目从同一列开始。
4. **点关闭/收托盘像闪退**：`ViewportCommand::Visible(false)` 之后 winit 不再派发 redraw，
   `update` 停摆、托盘菜单点了没反应。改为最小化；第二个实例被单实例锁拦下时会唤醒
   常驻实例，唤醒失败才弹 MessageBox。
5. **单实例误判**：`GetLastError` 不会因调用成功而清零，前面任何失败的文件调用留下的 183
   会让 `CreateMutexW` 之后被误判成"已有实例"。修法是调用前 `SetLastError(0)`。

## 明确没有做的事

注入游戏进程、Present hook、覆盖层绘制、GPU/温度/功耗采集、会话落 SQLite。
② 页里唯一"偏离原版"的地方是：提优先级前跳过 12 个系统关键进程（svchost/explorer/lsass…），
因为实测原版的无差别匹配会命中它们，提优先级有害无益。
