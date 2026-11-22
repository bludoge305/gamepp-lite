# 游戏加加（GamePP）四项功能逆向分析报告

> 目标：`<GamePP 安装目录>\GamePP.exe`
> 范围：**识别游戏 / 游戏监控 / 自动触发 / 托盘常驻**
> 方法：静态分析为主（PE 结构、字符串、导出表、导入表、内嵌 ASAR 包、本地 SQLite 数据），
> 佐以只读的运行态观测（进程列表、文件时间戳）。**不注入、不读写目标进程内存、不破解授权。**

---

## 0. 一句话结论

GamePP 不是"一个程序"，而是**一个 Electron 外壳 + 一组原生执行体**的组合：

| 功能 | 实现主体 | 关键机制 |
|---|---|---|
| 识别游戏 | `GPPPlatform.exe`（常驻原生进程）+ 游戏库（SQLite）+ 注入式 `GameTracker64.dll` | 轮询 `GetForegroundWindow` → `GetWindowThreadProcessId` → 映像路径 → 查库判定；`SetWinEventHook` 做事件补充；注入型插件用 `GPPI_D3DPresent` 回报游戏帧 |
| 游戏监控 | 主进程硬件采集模块 + `GamePP5.dll`（**其实是 SQLite 库**）+ `OverlayClient64.node` | 1 Hz 采样写入"每会话一张表"；渲染侧通过命名共享内存 + `GAMEPP-IPC-OVERLAY` 把数据推给游戏内覆盖层 |
| 自动触发 | 主进程 `gameppoptimizesdk` + `hotkeysdk` / `iohook` | 识别事件（`OnPlatformActivedProcess` 等）→ `beginGameOptimization` → `SetPriorityClass` / `TerminateProcess` / `lockFPS` |
| 托盘常驻 | Electron `Tray`（原生托盘）+ 单实例锁 + 开机自启 | `createAppTray` / `TrayTitle` / `displayBalloon` / `Game_Traymenu` 页面；`requestSingleInstanceLock` + `second-instance` |

---

## 1. 目标与文件清单

安装目录 `<GamePP 安装目录>\` 只有 21 个条目，真正的业务件分三处：

```
<GamePP 安装目录>\
  GamePP.exe                104,020,184 B   sha256 1f8e81d6f9fc2f7c38e5ad2160d18e90004d039b363e54643b719d1609d2d570
  GamePPLauncher.exe            106,712 B
  chrome_100_percent.pak / icudtl.dat / ffmpeg.dll / libEGL.dll / libGLESv2.dll / v8_context_snapshot.bin
  res\desk.ico                  370,070 B    ← 托盘图标
  locales\  resources.pak  snapshot_blob.bin  swiftshader\  ...

%LOCALAPPDATA%\GamePP\                       ← 用户态数据
  GamePPConfig.ini   GamePPOpt.ini   *.cert  UserData\  common\{Data,Icon,quality,res}\  Apps\<hash>\<ver>\
%LOCALAPPDATA%\GamePPSdk\<ver>\              ← 原生执行体（按版本热更）
  GPPPlatform.exe  GPP64.dll  GPP32.dll  InjectHelper(_x64).exe
  OverlayClient64.node  Update64.node  crashDumpClient.exe
  Apps\GameTracker\{GameTracker.ini, GameTracker32/64.dll, 1.dat, 1_x64.dat}
  Apps\shade\{shade64.dll, Shade\Shaders\*.fx}   ← ReShade 派生的后处理器
%PROGRAMDATA%\GamePPSdk\<ver>\vulkan\
```

**关键观察：所谓 `GamePP5.dll` 其实是一个 229 MB 的 SQLite 数据库**（`%LOCALAPPDATA%\GamePP\common\Data\GamePP5.dll`）。
工具侧对它的判定是 "A SQLite database (how an app stores its data)"，这不是命名失误就是故意的伪装。

> 证据：`ev_6c4db140…`（SQLite 结构读取，89 张表）；`ev_ae6cd4b1…`（GamePP.exe 节表）。

---

## 2. 容器层：Electron 13.6.9 + 加壳 + 自有包格式

### 2.1 是 Electron，不是 CEF

`.rdata` 里保留着 Electron 的构建路径与运行时字符串：

```
../../electron/shell/app/electron_main_delegate.cc
Electron 13.6.9
app.disableHardwareAcceleration() can only be called before app is ready
second-instance        ← 单实例事件名
```

> 证据：`ev_85173af8…`（在 `GamePP.exe` 中命中 `electron_main_delegate.cc` 与 `second-instance`）。

### 2.2 主程序被加壳

节表异常，多出两个非标准节：

| 节 | 文件偏移 | 大小 | 熵 | 属性 |
|---|---|---|---|---|
| `.text` | 0x400 | 83,870,832 | 6.76 | X |
| `.rdata` | 0x4FFC000 | 18,326,228 | 6.74 | R |
| `.oAY0` | 0x61C6000 | 1,107,536 | **7.11** | X |
| `.rsrc` | 0x62D5000 | 385,600 | 4.37 | R |
| `CPADinfo` | 0x61D0000 | 40 | 1.00 | RW |

`.oAY0` 这种"随机五字符 + 高熵 + 可执行"的节，配合 `CPADinfo`，是商业壳的典型特征。
后果很直接：**主进程 JavaScript 无法从磁盘静态提取**，必须走运行态或从它自己的包格式里拿
（见 2.3）。

> 证据：`ev_ae6cd4b1…`（10 个节的地址/大小/熵）。

### 2.3 界面与主进程逻辑放在 ASAR（本地扩展名 `.gpk`）里

`%LOCALAPPDATA%\GamePP\Apps\<hash>\<ver>\webapp.gpk` 的头部是
`04 00 00 00 | <header> | {"files":{...}}`，即标准 ASAR pickles 结构，只是扩展名换成了 `.gpk`。

两个包的角色不同：

| 包 | 规模 | 内容 |
|---|---|---|
| `Apps\f647a66b…\3.2.105.701\webapp.gpk` | 454 条目 / 361 文件 / 21 MB | Vue 页面：`windows/pages/{AILab,AIMaster,AimTrainer,…}`、`windows/background/background.js`（108 KB）、`receiveGame.js`、`game_client.js`、`assets\*.png` |
| `Apps\8fb708e5…\5.6.99.701\webapp.gpk` | 5 条目 / 3 文件 / 4.7 MB | **主进程包**：`app/core/main.jsc`（4,625,684 B，V8 code cache，**未压缩存储**）、`app/core/preload.jsc`、`app/package.json` |

> 证据：`ev_5e3197e1…`（454 条目清单）、`ev_2218d2b5…`（解包 361 文件）、`ev_8ab0bffe…`（主进程包 5 条目）。

**文本类文件逐文件加密**：解包后 `assets\*.png`、`*.ico` 魔数正常（`89 50 4E 47` / `00 00 01 00`），
而 `*.html / *.css / *.js / *.json` 全是高熵字节 —— 说明 gpk 里 `"zipped":1` 的文件不只是压缩，
还叠了一层按文件的加密。图像不动、代码加密，是很典型的"资源要能直接给渲染进程、代码要防抄"的取舍。

`main.jsc` 的 `"zipped":0` 因此是个缺口：它是 V8 代码缓存，只存字节码，但**字符串常量仍在**。
从它的字符串表可以恢复出整个主进程的模块名、IPC 名、错误文案（下文的 `WebApiImpl_*` 就来自这里）。

---

## 3. 运行态：它不是单进程

实测（只读 `tasklist`）：

```
GamePP.exe        21476   145,788 K   ← 主进程
GamePP.exe        12772   109,736 K   ← 渲染进程
GamePP.exe        32432    38,884 K   ← GPU 进程
GamePP.exe        22160    83,828 K
GamePP.exe        32704    91,608 K
GamePP.exe        22048   116,228 K
GPPPlatform.exe   27956    18,060 K   ← 常驻原生识别器
```

也就是说：**Electron 的进程模型（主 + 渲染 + GPU）之上，再挂一个独立的原生进程
`GPPPlatform.exe` 专门干"识别"，游戏内再注入 IGO 模块**。这种"外壳/执行体分离"让识别与采集可以
脱离 UI 存活，也让主程序更新时不必重启原生部分。

---

## 4. 功能一：识别游戏

### 4.1 判据是"映像名"，不是"窗口标题"

`GPPPlatform.exe` 里有一条格式化串，直接暴露了它的主循环：

```
GetForegroundWindow hWnd = {} processId = {} process = {}
```

配套导入：`GetForegroundWindow`、`GetActiveWindow`、`GetWindowThreadProcessId`、
`K32GetProcessImageFileNameW`、`IsWow64Process`；同时带 `CProcessNotifyWrapper`（`SetWinEventHook` /
`SetWindowsHookEx`）。

> 证据：`ev_511f9f3c…`（该日志串 + 两处 xref）。

### 4.2 "这是不是游戏"：本地游戏库 + 图标目录

`GamePP5.dll`（SQLite）的会话主表直接以**映像名**为键：

```sql
CREATE TABLE "GamePP_BaseInfo"(
  "id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "processpath" TEXT,        -- 实测存的是 exe 名，如 Winhance.exe / deadcells.exe
  "exec_path"   TEXT,        -- 百分号编码的 JSON：磁盘型号/序列号/分区
  "starttime" INTEGER, "endtime" INTEGER, "gametime" INTEGER,
  "icon" TEXT,               -- 图标文件名，如 "Winhance.png"
  "city","wea","wea_img","tem","hd_info","data_type","refresh_time",
  "province","hd_list_data","resolutions","dx_version","is_check")
```

而 `%LOCALAPPDATA%\GamePP\common\Icon\` 里是一堆**以 exe 名命名的 PNG**：

```
Client-Win64-Shipping.png   HTGame.png   deadcells.png   Endfield.png
PlatformProcess.png         craft-anything.png   Plain Craft Launcher.png   …
```

"exe 名 → 图标 + 游戏名"这条映射同时被数据层和资源层使用，说明识别就是**按映像名查表**，
图标是按需从进程里抓取后落盘的（对应 JS 侧 `WebApiImpl_SaveProcessIconToFile`）。

### 4.3 JS 侧接口（从主进程包字符串表恢复）

```
WebApiImpl_GetForegroundProcess      WebApiImpl_GetGame
WebApiImpl_StartGetGame              WebApiImpl_UploadGameInfo
WebApiImpl_RefreshActiveProcess      WebApiImpl_UpdateProcessAccess
WebApiImpl_GetProcessFilePathByProcessId   WebApiImpl_SaveProcessIconToFile
WebApiImpl_GetGameProcessType        WebApiImpl_GetProcessALLList
```

> 证据：`ev_8ab0bffe…`（主进程包）；`ev_a7110d3f…`（`GamePPSdk.dll` 导出
> `GamePP_Sdk_CreateInstance / Read / Write / Shutdown`、`MainServiceProcessDetection`）。

### 4.4 注入型插件：`IGPlugin` ABI + Overwolf 游戏事件

`Apps\GameTracker\GameTracker.ini`：

```ini
[gamein]
appid=400000000
appname=GameTracker
x86=GameTracker32.dll
x64=GameTracker64.dll
```

`GameTracker64.dll` 的 RTTI 与字符串：

```
CreatePluginInstance                       ← 唯一导出
.?AVIGPluginInterface@IGPlugin@@           ← 插件接口
.?AVCGameWatcher@@  / CSingletonT<CGameWatcher>
CGameWatcher::GPPI_Init
CGameWatcher::GPPI_Unload
CGameWatcher::GPPI_D3DPresent              ← 挂钩游戏 Present
CGameWatcher::SendDataToServer / SendDataToGame
owgame_events_create / owgame_events_trigger_event / owgame_events_commit_info_transaction …
PDB: C:\Build\Project\Wap-Ingame-Project\Src\Bin\GameTracker64.pdb
```

三点信息量很大：

1. **插件 ABI 是 `GPPI_Init / GPPI_Unload / GPPI_D3DPresent` 三件套**，由 `CreatePluginInstance` 造对象；
2. `GPPI_D3DPresent` 说明它靠**替换/挂钩 Present** 拿到每帧时机（这也是 FPS 数据的唯一来源）；
3. `owgame_events_*` 是 **Overwolf Game Events** 的 C 接口 —— 游戏开始/结束/击杀等对局事件走的是
   第三方事件总线，而不是自己解析内存。

> 证据：`ev_2655a8c0…`（`GPPI_` 三处）、`ev_d60f489d…`（`CGameWatcher::*` 七处）、`ev_a2fc9b16…`（`owgame_events_create`）。

### 4.5 注入链

```
InjectHelper_x64.exe   802? B  导入 EnumWindows / IsWindowVisible / K32EnumProcessModules / OpenMutexW / SetEvent
GPPHost64.dat（实为 PE）      F:\works\GamePPElectron\projects\WAP-Ingame\Src\Bin\GPPHost64.pdb
                             导入 WriteProcessMemory / OpenProcess / GetWindowThreadProcessId；导出 gameppSelfDetection
GPP64.dll / GPP32.dll         被注入的宿主模块
overlaydata_x64.data          内容是 "C:\Windows\system32\d3d9.dll"（挂钩目标模块）
shade64.dll                   ReShade 派生后处理器 + Shade\Shaders\*.fx
```

`InjectHelper` 的导入表组合（`EnumWindows` + `IsWindowVisible` + `EnumProcessModules` + 命名事件）
说明它的职责是"**等目标进程把窗口亮出来 → 确认加载完成 → 触发注入**"，而不是盲注。

---

## 5. 功能二：游戏监控

### 5.1 数据层：一会话一张表，按秒一行

同库 89 张表：`GamePP_BaseInfo` + 88 张以 **unix 时间戳**命名 的表。任取一张：

```sql
CREATE TABLE '1782813483'(
  id, fps, fps1, fps01, memory, memorytemperature,
  cpuload, cpu_load_core, cpuloadP, cpuloadE, cputemperature, cpu_temperature_core,
  cpuclock, cpuclockP, cpuclockE, cpu_clock_core, cpu_clock_effective_core, cpuclockAVG,
  cpupower, cpuvoltage,
  gpuload, gpuload1, gputemperature, gpuhotspottemp, gpuclock, gpupower,
  gpumemoryload, gpumemorytemp, gpumemoryclock, gpuvoltage,
  disk_temp, disk_load, frametime, performance, performance_gpu,
  amd_cpu_thermal, amd_gpu_thermalhotspot, amd_gpu_thermalmemory)
```

实测样本（1 Hz，14,510 行）：

```
fps=67  memory=81  cpuload='32|'  cpu_load_core='35|37|44|47|32|34|22|22|24|23|25|25|22|22'
cpuclockP=38 cpuclockE=23 cpuclock='2936|' cpuclockAVG=2936 cpupower=50 cpuvoltage=0.857
gputemperature=72 gpuhotspottemp=87 gpuclock=1785| gpupower=109 gpumemoryload=30
performance='[{"P-core 0 Thermal Throttling":1},{...}]'   ← 逐核热限判定
```

两个细节值得记：

- **per-core 是竖线分隔字符串**（`35|37|44|…`），不是子表 —— 说明写入端追求的是一次 INSERT 的吞吐；
- 表名用时间戳而非 game_id，**一张表 = 一次会话**；`GamePP_BaseInfo` 才是跨会话索引。

> 证据：`ev_6c4db140…`（schema + 行数 + 样本行）。

### 5.2 采集层：HWiNFO + 厂商 SDK + 自家 Node 扩展

`Apps\8fb708e5…\5.6.99.701\win32\`：

```
Hardware\HWiNFO64.dll / HWiNFO32.dll / HardwareAddon.node / HwMonitor64.exe   ← 全机传感器
common\GPPNVAPILib.node / GPPAMDADLLib.node                                   ← NVIDIA NVAPI / AMD ADL
common\BaseUtils.node   common\FFIHelper.node
NodeModules\iohook.node（全局输入钩子） / ffinapi.node / sqlite3.node / refnapi.node
GPPFeature_x64.exe / GPPHost64.dat
DeskBand\{32bit,64bit}\DeskBandRender*.exe   ← 桌面监控条（Shell_NotifyIcon 在这里才出现）
GPPMagicGlass\…                              ← 窗口透明/毛玻璃
```

也就是说"温度/频率/功耗"这类指标 GamePP 自己没有去读 MSR，而是**复用 HWiNFO 的库 + 厂商 API**。

### 5.3 呈现层：IGO（In-Game Overlay）

`OverlayClient64.node`（Node N-API 原生扩展）的 JS 可见函数名，就是主进程与原生层之间的完整契约：

```
InitProduct                StartServer / _IPC_StartServerThread      StopSever
RefreshActiveProcess       SetCurrentGameProcessID / GetCurrentGameProcessID
ActiveIgoModuleInTargetProcess          ← 在目标进程里激活 IGO
InsertWindowWithAbsolutePositionEx      ← 覆盖层窗口定位
InsertWindowWithRelationshipPositionEx  / UpdateIngameDraggableRegions
SetWindowProperties  ShowWindow  RemoveWindow  UpdateWindow  GetGameProcessType
```

命名 IPC 资源（`boost::interprocess`）：

```
GAMEPP-IPC-OVERLAY                      ← 共享内存/通道名
GAMEPP_PRODUCT_RUNNING_LIST             ← 运行中产品列表
GamePP-Product-{}-{}-Running            ← 单产品运行态事件
GAMEPP-INGAME-MUTEX-{}-{}               ← 游戏内互斥
gGPPInnerWindowResourceFileMap_{}_{}    ← 窗口资源映射
```

> 证据：`ev_60111f7e…`（`GAMEPP-IPC-OVERLAY` 在 `GPPPlatform.exe` 中的 xref）、
> `ev_ff16e974…`（`OverlayClient64.node` 的 8 个导出名及注册点 `sub_18002fee0`）。

**数据流**：采样线程写 SQLite → 主进程把最新一帧写进 `GAMEPP-IPC-OVERLAY` 共享内存 →
注入在游戏里的 IGO 模块在 `GPPI_D3DPresent` 里读出来画到 D3D 后缓冲上。UI 只是"另一个消费者"。

---

## 6. 功能三：自动触发

### 6.1 模块引导日志暴露了完整功能清单

`main.jsc` 字符串表里有一组成对的启动日志（`Starting <module> … running time : `）：

```
Starting game listener and IGO implementation   ← 识别 + 覆盖层
Starting hardware monitor system                ← 采集
Starting gameppoptimizesdk system               ← 优化动作
Starting hotkeysdk system                       ← 热键
Starting processhelper system                   ← 进程工具
Starting desktopmonitor system                  ← 桌面监控条
Starting ipchelper system                       ← IPC
Starting database implementation                ← SQLite
Starting setting system / wapTray features      ← 设置 / 托盘
Starting main services / core services
```

### 6.2 触发链：识别事件 → 优化引擎 → 动作

```
[原生] GPPPlatform.exe  ──GetForegroundWindow──▶ 判定游戏
        │
        └─ IgEventImpl_OnPlatformActivedProcess / OnSystemActiveWindowChanged
           IgEventImpl_onGameWindowStateChanged / onHotkeyTriggered
                    │
[主进程]            ▼
        beginGameOptimization ──▶ gameppoptimizesdk ──▶ SetPriorityClass / TerminateProcess / lockFPS
        restoreGameOptimization （退出时还原）
        startPlan / stopPlan   （"计划"型优化）
```

`main.jsc` 里的相关接口名：

```
WebApiImpl_beginGameOptimization        WebApiImpl_restoreGameOptimization
WebApiImpl_startPlan                    WebApiImpl_stopPlan
WebApiImpl_killProcess                  WebApiImpl_lockFPS / unlockFPS
WebApiImpl_SetProcessCpuSet             WebApiImpl_setGameWindowActiveState
WebApiImpl_startIohook / stopIohook     WebApiImpl_EnableHotKey
WebApiImpl_IsValidInWhiteListSpecialProcess   ← 黑/白名单
```

### 6.3 开关的物理形态：两个 INI

`%LOCALAPPDATA%\GamePP\GamePPOpt.ini`：

```ini
[SYS]
SystemGameProPriority=1     ← 游戏进程提优先级
SystemKillBackPro=1         ← 结束后台进程
SystemAIMemory=1            ← 内存回收
SystemLockFPS=0 / SystemLockFPSValue=0
SystemFlushDns=1  SystemCleanClipBoard=1  SystemWallPaperEngnine=1
SystemWindwosSearch=1  SystemXblAuthManager=1  SystemWindowsAero=0
```

`GamePPConfig.ini`（值多为 base64 编码的 UTF-16）：

```ini
[GPPSHOW]  CPUUseed=1  GPUUseed=1  SysMemoryUseed=1  FreReduceRange=220  FreReducePrecent=2
[general]  GameAutoORManRecevice=0        ← 自动识别 / 手动添加
[Notify]   UIOpen=0                        ← 通知开关
```

**结论**：GamePP 的"自动触发"本质是**"游戏状态变化 → 一组固定动作"**，动作集写死在
`gameppoptimizesdk` 里，用户能改的只是布尔开关与少量阈值（限帧值、降频区间）。
它不是通用规则引擎，是一个**带开关的策略集合**。

### 6.4 热键与输入

`iohook.node`（全局键鼠钩子）+ `hotkeysdk` + `common\Hotkey.dll`
（`ShieldingHotkey` / `SetGameActiveState` / `SetProcessThreadID`，PDB 路径
`HYY\Block_Hotkey\Block_Hotkey_DLL`）—— 后者用于**在覆盖层激活时屏蔽游戏自身热键**，
避免"按 F1 既开覆盖层又触发游戏技能"。

---

## 7. 功能四：托盘常驻

### 7.1 结构

```
Electron 主进程
  createAppTray / createTray        → Tray 实例（Windows 下最终是 Shell_NotifyIcon）
  TrayTitle / TrayToolTip           → 悬停文案
  displayBalloon → balloon-show/click/closed   → 气泡通知
  \windows\pages\game_traymenu\     → 右键菜单内容其实是一个独立渲染页面
  requestSingleInstanceLock / makeSingleInstance + second-instance  → 单实例
  isStartedWithOs / setStartWithWindowState → 开机自启（写注册表）
```

`res\desk.ico`（370 KB，多层图标）与 `common\res\desk.ico` 就是托盘用的图标资源。

> 证据：`ev_8ab0bffe…`（`createAppTray` / `TrayTitle` / `displayBalloon` / `Game_Traymenu` / `requestSingleInstanceLock`）。

### 7.2 为什么"菜单是页面"

`Game_Traymenu` 与 `\windows\pages\game_traymenu\` 同时出现，说明右键菜单不是原生 `HMENU`，
而是**一个无边框小窗口**（渲染进程画的 HTML 菜单）。好处是能放图标、能跟随主题、能远程更新；
代价是菜单弹出慢、点到外面不一定收。这也是它能显示"当前游戏 + 一键优化"这种富内容的原因。

### 7.3 常驻的三道约束

1. **单实例**：`requestSingleInstanceLock` + `second-instance`，第二个实例不启动 UI，只把参数递给常驻实例；
2. **关窗不退**：Electron 侧 `window-all-closed` 不 `quit()`（`main.jsc` 里有 `before-quit` / `will-quit` 钩子用于清理）；
3. **开机自启**：`isStartedWithOs`，并在 `setStartWithWindowState` 里决定启动时是否带窗口。

---

## 8. 模块关系图（逆向结论）

```
                        ┌─────────────────────────────────────────┐
                        │ GamePP.exe (Electron 13.6.9, 加壳)       │
                        │  main.jsc: WebApiImpl_* (445 个)         │
                        │   ├─ game listener / IGO wrapper         │
                        │   ├─ hardware monitor system             │
                        │   ├─ gameppoptimizesdk                   │
                        │   ├─ hotkeysdk / ipchelper / database    │
                        │   └─ wapTray (createAppTray + 菜单页面)   │
                        └───────┬──────────────────┬──────────────┘
             N-API .node 扩展   │                  │  渲染进程（Vue 页面 / gpk 包）
        ┌───────────────────────┴──────┐           │
        │ OverlayClient64.node         │           ▼
        │ HardwareAddon.node           │      windows/pages/*.html
        │ iohook.node / ffinapi.node   │      windows/background/*.js
        │ GPPNVAPILib / GPPAMDADLLib   │
        └───────┬──────────────────────┘
                │ 命名共享内存 GAMEPP-IPC-OVERLAY
                ▼
    ┌───────────────────────────┐        ┌──────────────────────────────┐
    │ GPPPlatform.exe（常驻）    │        │ 游戏进程（被注入）             │
    │  前台窗口轮询 + 事件钩子    │──注入──▶│  GPP64.dll / GameTracker64.dll│
    │  GAMEPP_PRODUCT_RUNNING_  │        │   GPPI_Init / GPPI_D3DPresent │
    │  LIST                     │        │   IGO 覆盖层 + owgame_events  │
    └───────────┬───────────────┘        └──────────────────────────────┘
                │ SQLite（GamePP5.dll）
                ▼
    ┌───────────────────────────┐
    │ GamePP_BaseInfo + 会话表   │   ← 识别结果、时长、逐秒遥测
    └───────────────────────────┘
```

---

## 9. 证据索引

| id | 内容 |
|---|---|
| `ev_ae6cd4b1…` | `GamePP.exe` 10 个节：地址/大小/熵（发现 `.oAY0`、`CPADinfo`） |
| `ev_85173af8…` | `electron_main_delegate.cc`、`second-instance` |
| `ev_5e3197e1…` | `webapp.gpk`(f647…) 454 条目 / 361 文件，含 `background.js` 108 KB |
| `ev_2218d2b5…` | 该 gpk 解包成功；文本文件为密文、图片为原文 |
| `ev_8ab0bffe…` | 主进程包（`app/core/main.jsc` 4,625,684 B）结构 |
| `ev_2655a8c0…` | `CGameWatcher::GPPI_Init / GPPI_Unload / GPPI_D3DPresent` |
| `ev_d60f489d…` | `CGameWatcher::SendDataToServer / SendDataToGame` 等 7 处 |
| `ev_a2fc9b16…` | `owgame_events_create`（Overwolf Game Events） |
| `ev_511f9f3c…` | `GetForegroundWindow hWnd = {} processId = {} process = {}` |
| `ev_60111f7e…` | `GAMEPP-IPC-OVERLAY` |
| `ev_ff16e974…` | `OverlayClient64.node` 8 个 N-API 导出名 |
| `ev_a7110d3f…` | `GamePPSdk.dll`：`GamePP_Sdk_*` + `MainServiceProcessDetection` |
| `ev_6c4db140…` | `GamePP5.dll` = SQLite，89 表 / schema / 样本行 |

---

## 10. 未解与边界（诚实声明）

| 未解 | 影响 | 可能解法 |
|---|---|---|
| 主进程 JS 被壳保护 + gpk 逐文件加密 | 无法给出 UI 侧精确分支逻辑（只能给接口名与文案） | 运行态从渲染进程内存取解密后的脚本；成本高，本次不做 |
| "是不是游戏"的完整判定表 | 只能确定以映像名为主键，阈值/白名单内容拿不到（在加密 JS 或服务端） | 抓包看 `UploadGameInfo` / `GetGame` 的返回 |
| `1.dat` / `1_x64.dat`（GameTracker）内容 | 可能是对局事件映射表，未解 | 结构分析 + 与 owgame 事件名对齐 |
| FPS 采集精度、`frametime` 计算方式 | 只知在 `GPPI_D3DPresent` 里 | 反汇编 `sub_180005fb1` 等热点 |
| 未做任何注入 / 内存读取 | 所有"注入后行为"均为**推断**（已标注） | 需要时再单独授权 |

---

*报告基于 2026-11-22 的安装副本；文件哈希见第 1 节。分析全程只读，未修改目标程序与其数据。*

---

## 11. 工程落地时的取舍（GamePP-Lite 的差异）

逆向结论与最终实现并非一一照搬，下列差异是**主动取舍**，写在这里以免把"没做"当成"没查到"：

| 逆向到的能力 | 落地方式 | 原因 |
|---|---|---|
| 逐秒遥测（fps/cpuload/温度…） | **未实现**：监控面板与采样线程已移除 | 真实 FPS 需 Present hook；温度需驱动。只保留会话时长与历史 |
| 自动触发规则（`beginGameOptimization`） | 收敛成「Windows 优化」里的两个开关：识别到游戏自动执行 / 退出自动还原 | 规则引擎对单个用户没有增量价值，开关更直观 |
| 无差别给"命中特征表"的进程提优先级 | 跳过 12 个系统关键进程与自身 | 实测原版会命中 svchost/explorer，提优先级反而有害 |
| 托盘菜单是渲染页面（`Game_Traymenu`） | 原生 `HMENU` | 免第二个渲染进程，代价是不能放富内容 |
| `GAMEPP-IPC-OVERLAY` 共享内存 | 未实现 | 没有覆盖层，不需要跨进程通道 |
| 会话表写入 SQLite | 状态与历史写 JSON/INI | 工程精简；数据量级不同 |

Windows 优化部分的实现细节、动作顺序与原版地址对照，见
[`gamepp-winopt/README.md`](../gamepp-winopt/README.md) 与
[`report/gamepp-winopt-reverse-report.md`](gamepp-winopt-reverse-report.md)。
