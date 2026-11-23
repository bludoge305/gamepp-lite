# 安全说明

## 这个程序会真的改系统

`GamePP-Lite` / `gamepp-winopt` 不是演示程序。勾选并执行「Windows 优化」后，它会：

- 结束进程（后台黑名单、Wallpaper Engine，不可恢复）；
- 停止系统服务（Windows Search / Windows Update / DiagTrack / Xbox 认证 / 家庭组等）；
- 修改其他进程的优先级与 CPU 亲和性；
- 调整全系统进程的工作集（EmptyWorkingSet）；
- 切换电源方案；
- 清空剪贴板（不可恢复）；
- 执行 `ipconfig /flushdns` 与 `lodctr /R`。

**还原是不完整的**：被杀掉的进程、清掉的 DNS 缓存与剪贴板无法恢复；还原阶段还会把
当前会话里能打开的进程优先级统一复位为 NORMAL（受保护白名单内的组件除外）。

建议先点「预演」看清要做什么，再决定是否执行。不要在服务器、远程桌面宿主或他人机器上
直接执行优化。

## 受保护白名单

为避免误伤系统，所有会碰到"别人的进程/服务"的动作都先过 `gamepp-lite/src/domain/protect.rs`
的白名单（PID 0/4、系统组件映像名、系统目录前缀、核心服务），命中即跳过并写日志。
白名单是安全边界，不是免责声明：它降低误伤概率，不保证系统一定不出问题。

## 开发期调试开关

以下环境变量面向排版核验 / 自动截图 / 托盘自检，正式使用不需要设置。其中两个有副作用，
从带残留变量的 shell 启动时会表现异常：

| 变量 | 副作用 |
|---|---|
| `GPP_SHOT=<path>` | 把当前窗口以**未压缩 24 位 BMP** 写到该路径 |
| `GPP_OPT_DRY` | **只要设置就生效（值任意，含 `0`）**：启动即跑一次优化预演，不改系统，但会写 `runtime.log`、填执行日志、弹提示 |
| `GPP_INSTANCE_SUFFIX=xxx` | 仅 debug 构建生效；release 忽略（避免双实例并发写状态文件） |

其余（`GPP_SHOT_FRAME` / `GPP_PROBE` / `GPP_TRAY_TOGGLE` / `GPP_TAB` / `GPP_SIZE`）无持久副作用。

## 报告问题

发现安全问题（例如某个动作绕过了白名单、越权写入、状态文件损坏导致错误还原），请开
Issue 并附上 `%LOCALAPPDATA%\GamePP-Lite\runtime.log` 的相关片段。日志里不要贴个人路径之外
的敏感信息。本项目不收集任何遥测，也不联网。
