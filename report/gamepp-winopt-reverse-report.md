# GamePP「游戏优化 / Windows 优化」模块逆向分析报告

> 分析对象：本机安装的 GamePP（版本 5.6.99.701）
> 分析方式：纯静态分析（未运行目标程序）
> 分析工具：craft-anything（Deep 模式，Ghidra 级反编译 + 反汇编）
> 交付物：本报告 + Rust 复刻工程 `gamepp-winopt/`

---

## 0. 结论摘要

1. **Windows 优化的全部实现不在 `GamePP.exe` 里**，而在一个 Node 原生插件：
   `%LOCALAPPDATA%\GamePP\Apps\8fb708e5972662062da26a2bb4753a23\5.6.99.701\win32\common\GamePPOptimize.node`
   （PE32 x86，393,672 字节，SHA-256 `359b7dfa…93500`，自带 PDB 路径
   `C:\Build\Project\Wap-Ingame-Project\Src\GamePPOpti\GameOptimizaion\GamePPOptimize.pdb`）。

2. 插件对外只导出 10 个 N-API 函数，其中与 Windows 优化相关的是
   `beginGameOptimization(gameId)` / `restoreGameOptimization(gameId)`；
   真正的执行体是 `sub_10013f70`（开始）与 `sub_100141e0(0)` + `sub_100177f0`（还原）。

3. 界面上 9 个勾选项全部对应 `GamePPOpt.ini` 的 `[SYS]` 节里的开关，
   插件每次 begin/restore 都会用 `GetPrivateProfileIntW` 一次性读入一个 0x50 字节的结构体。

4. 实际动作只有 6 类系统调用：`WinExec` 命令行、`ControlService`/`StartServiceW` 服务控制、
   `TerminateProcess` 进程终止、`SetProcessWorkingSetSize`+`K32EmptyWorkingSet` 内存回收、
   `SetPriorityClass`/`SetProcessAffinityMask` 调度调整、`OpenClipboard/EmptyClipboard` 剪贴板清空。
   **没有任何注册表写入**（`RegSetValueExW` 只被"游戏设置优化"分支的 AppCompat 层用到）。

5. 电源方案一项（UI 第 3 项）**不在这个 .node 内**，由同级 `Power.dll` 完成
   （`PowerImportPowerScheme` + `PowerSetActiveScheme`，数据是 `PowerData\GPP_*.pow`）。

6. 还原是**不完整**的：服务能拉回来、优先级能复位，但被结束的进程、DNS 缓存、剪贴板、
   已释放的内存都不可撤销；而且还原阶段会把**当前会话里每一个能打开的进程**的优先级
   强制设回 `NORMAL_PRIORITY_CLASS`。

---

## 1. 证据链与文件清单

| 文件 | 作用 | 关键事实 |
|---|---|---|
| `GamePP.exe`（104 MB） | Electron/CEF 主程序 | 只是宿主，优化逻辑不在其中 |
| `win32\common\GamePPOptimize.node` | **本模块的实现主体** | 1149 个函数、2337 个字符串、152 个导入；`.text` 0x10001000–0x1003CFC5 |
| `win32\common\Power.dll` | 电源方案 | 导出 `PP_Initialization / PP_UnInitialization / PP_Active / PP_UnActive`；字符串里有 `PowerImportPowerScheme`、`PowerSetActiveScheme`、`POWRPROF.dll` |
| `win32\PowerData\GPP_Maxed.pow` 等 | 电源方案数据 | 文件头 `regf`（注册表 hive），内含方案 GUID，如 `GPP_Maxed = 9fda562c-d377-43d5-bb2c-afcb0a22c547` |
| `%LOCALAPPDATA%\GamePP\GamePPOpt.ini` | 配置 | `[SYS]` 节 22 个开关 + 5 个整型值 |
| `%LOCALAPPDATA%\GamePP\Power.cfg` | 还原电源方案 | `[General] LastPlan={381B4222-F694-41F0-9685-FF5BB260DF2E}` |
| `webapp.gpk`（main.jsc） | JS 侧编排 | 只调用 `beginGameOptimization` / `restoreGameOptimization`，并靠 `setDefaultValueTypeInt` 写 INI |

`GamePPOptimize.node` 导入表（与本模块相关的部分，全部来自静态导入表）：

```
KERNEL32: CreateMutexW WaitForSingleObject ReleaseMutex CloseHandle OpenProcess
          TerminateProcess SetPriorityClass GetPriorityClass SetProcessWorkingSetSize
          K32EmptyWorkingSet CreateToolhelp32Snapshot Process32FirstW Process32NextW
          ProcessIdToSessionId WinExec Sleep GetPrivateProfileIntW WritePrivateProfileStringW
ADVAPI32: OpenSCManagerW OpenServiceW QueryServiceStatusEx ControlService StartServiceW CloseServiceHandle
USER32  : OpenClipboard EmptyClipboard CloseClipboard
```

---

## 2. 导出表与调用链

`sub_1000dce0`（模块注册函数）用 `napi_define_properties` 注册了 10 个属性，名字直接躺在
`.rdata 0x10048EC4..0x10048F68`：

| 导出名 | 实现地址 | 作用 |
|---|---|---|
| `getDefaultValueTypeInt` | `0x1000C680` | 读某个内部默认值 |
| `setDefaultValueTypeInt` | `0x1000CC90` | 写 INI 开关（UI 勾选/取消） |
| `setCPUValue` | `0x1000D150` | 记录游戏 exe 名 + 核心数（全局 `0x1005A87C`） |
| `setProOpt` | `0x1000D3B0` → `sub_10016FC0` | 按 pid 做核心限制/亲和性 |
| `restoreProOpt` | `0x1000D450` | 只还原优先级/亲和性 |
| `stopMidwayOpt` | `0x1000D520` | 置中止标志（结构体 `+0x4C = 1`） |
| **`beginGameOptimization`** | **`0x1000D5F0`** | **Windows 优化 + 游戏设置优化** |
| **`restoreGameOptimization`** | **`0x1000D920`** | **还原** |
| `tarkovOpt` / `tarkovRestore` | `0x1000DBE0` / `0x1000DC90` | 塔科夫专用配置改写 |

`beginGameOptimization` 的骨架（`sub_1000D5F0`）：

```
CreateMutexW(NULL, TRUE, L"gamePP_opti")        ; 0x10048E54
if (WaitForSingleObject(h, 1000) == WAIT_TIMEOUT) return true;   // 拿不到锁就"假装成功"
sub_10013BD0()          // 读 GamePPOpt.ini → 0x1005A878 结构体
st->byte_0x4D = 1;      // "本次已优化" 标志
sub_10013F70(gameId)    // ★ Windows 优化的全部动作
if (st->byte_0x4C) return true;   // stopMidwayOpt 置过中止标志
sub_10016580(gameId)    // 游戏设置优化 + 优先级调整（内部调 sub_100174A0）
```

---

## 3. 配置映射（`[SYS]` 节 → 结构体字节偏移）

`sub_10013BD0` 里 26 次 `GetPrivateProfileIntW` 的顺序即字段顺序（`sete` 语义为 `值 != 0`）：

| 偏移 | 键名 | 缺省 | 用途 |
|---|---|---|---|
| +0x00 | `SystemAIPower` | 0 | 电源方案（由 Power.dll 执行） |
| +0x01 | `SystemLockFPS` | 0 | 锁帧（本模块内无消费点） |
| +0x02 | `SystemCleanClipBoard` | 0 | 清空剪贴板 |
| +0x03 | `SystemLimitCore` | 0 | 限制核心（亲和性） |
| +0x04 | `SystemFlushDns` | 0 | `ipconfig /flushdns` |
| +0x05 | `SystemWallPaperEngnine` | 0 | 结束 wallpaper* 进程 |
| +0x06 | `SystemDiagsvc` | 0/1 | 停 `Diagsvc` |
| +0x07 | `SystemDiagtrack` | 0/1 | 停 `DiagTrack` |
| **+0x08** | （无） | — | **对应 `SysMain`，但读取函数从不写这个字节 → 恒为 0，永远不停 SysMain** |
| +0x09 | `SystemWindowsUpdate` | 0/1 | 停 `wuauserv` |
| +0x0A | `SystemXblAuthManager` | 0/1 | 停 `XblAuthManager` |
| +0x0B | `SystemWindwosSearch` | 0/1 | 停 `WSearch` |
| +0x0C | `SystemHomeGroupProvider` | 0/1 | 停 `HomeGroupProvider` |
| +0x0D | `SystemHomeGroupListener` | 0/1 | 停 `HomeGroupListener` |
| +0x0E | `SystemWindowsAero` | 0 | `net stop uxsms` → 3s → `net start uxsms` |
| +0x0F | `SystemPowerOnOff` | 0 | 电源相关（非本路径） |
| +0x10 | `SystemNVIDIAGPUs` | 0 | 显卡（非本路径） |
| +0x11 | `SystemGameProPriority` | 0 | 游戏进程优先级 |
| +0x12 | `SystemKillBackPro` | 0 | 杀后台黑名单 |
| +0x13 | `SystemAIResGeLi` | 0 | 资源隔离（非本路径） |
| +0x14 | `SystemAIMemory` | 0 | 释放物理内存 |
| +0x15 | `SystemAIInfrastructureRes` | 0/1 | （非本路径） |
| +0x18 / +0x1C | `SystemGameClosePower` / `SystemGameingPower` | 0 | 电源策略编号 |
| +0x20 | `SystemNVIDIASets` | 0 | NVIDIA 设置档位 |
| +0x24 | `SystemLockFPSValue` | 0 | 锁帧值 |
| +0x28 | `SystemAMDOnlyMoveGameToCCD0` | 0 | AMD CCD0 绑定 |

> 旁证：现场机器的 `GamePPOpt.ini` 与截图 9 个勾选项完全一致（含 `SystemWindowsAero=1`，
> 该键在当前 UI 里已无对应复选框）。

---

## 4. 逐项行为（含客观证据）

### 4.1 主调度函数 `sub_10013F70` 的原始指令序

以下是反汇编（`native_function_disassemble(0x10013F70)`，186 条指令）里决定行为的 12 条关键指令：

```
0x10013F8E  80 7B 04 00        cmp byte ptr [ebx+4], 0          ; SystemFlushDns
0x10013F9C  68 14 DC 04 10     push 0x1004DC14                  ; "ipconfig /flushdns"
0x10013FA8  6A 01              push 1
0x10013FAA  E8 31 06 00 00     call 0x100145E0                  ; 游戏配置文件改写（非 Windows 优化）
0x10013FAF  80 7B 0E 00        cmp byte ptr [ebx+0xE], 0        ; SystemWindowsAero
0x10013FB7  68 28 DC 04 10     push 0x1004DC28                  ; "net stop uxsms"
0x10013FBE  68 B8 0B 00 00     push 0xBB8                       ; Sleep(3000)
0x10013FCB  68 38 DC 04 10     push 0x1004DC38                  ; "net start uxsms"
0x10013FD2  80 7B 05 00        cmp byte ptr [ebx+5], 0          ; SystemWallPaperEngnine
0x10014016  68 80 0B 05 10     push 0x10050B80                  ; L"wallpaper" (len=9)
0x10014097  80 7F 12 00        cmp byte ptr [edi+0x12], 0       ; SystemKillBackPro
0x100140EA  80 7F 14 00        cmp byte ptr [edi+0x14], 0       ; SystemAIMemory
0x1001418A  E8 51 00 00 00     call 0x100141E0                  ; 服务控制（enable=1）
0x10014191  68 B0 DC 04 10     push 0x1004DCB0                  ; "lodctr /R"（无任何条件保护！）
0x1001419C  80 7B 02 00        cmp byte ptr [ebx+2], 0          ; SystemCleanClipBoard
```

按执行顺序还原成伪代码：

```c
// ① 清理 DNS
if (cfg->SystemFlushDns) WinExec("ipconfig /flushdns", 0);

// ② 游戏设置优化（PUBG/SCUM/武侠乂/Tarkov 的 ini 改写）—— 不属于 Windows 优化
sub_100145E0(1, gameId);

// ③ Aero 主题服务：停 → 等 3 秒 → 起
if (cfg->SystemWindowsAero) {
    WinExec("net stop uxsms", 0);
    Sleep(3000);
    WinExec("net start uxsms", 0);
}

// ④ 关闭 Wallpaper Engine：刷新进程表 → 名字含 "wallpaper" 就杀
if (cfg->SystemWallPaperEngnine) {
    sub_10016720();                        // 重建进程表（当前会话）
    for (p in session_procs)
        if (find_ci(p.name, L"wallpaper", 9) != -1) {
            h = OpenProcess(PROCESS_TERMINATE, 0, p.pid);
            if (h) TerminateProcess(h, 0);
        }
}

// ⑤ 关闭后台进程：再刷新一次进程表 → 命中 48 条黑名单就杀
if (cfg->SystemKillBackPro) {
    sub_10016720();
    for (p in session_procs)
        if (sub_10016B20(p.name)) {        // 48 条子串匹配
            h = OpenProcess(PROCESS_TERMINATE, 0, p.pid);
            if (h) TerminateProcess(h, 0);
        }
}

// ⑥ 释放物理内存：**全部进程**（不按会话过滤）
if (cfg->SystemAIMemory) {
    snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    for (e in snap) {
        h = OpenProcess(PROCESS_ALL_ACCESS, TRUE, e.th32ProcessID);
        if (h) { SetProcessWorkingSetSize(h, -1, -1); K32EmptyWorkingSet(h); CloseHandle(h); }
    }
}

// ⑦ 停服务
sub_100141E0(1);

// ⑧ 重建性能计数器（无条件执行）
WinExec("lodctr /R", 0);

// ⑨ 清空剪贴板
if (cfg->SystemCleanClipBoard) {
    if (OpenClipboard(NULL)) { EmptyClipboard(); CloseClipboard(); }
}
```

### 4.2 服务控制（UI：关闭 Xbox Live 身份验证管理器 / 停止 Windows Search）

`sub_100141E0(enable)` 里 8 条宽字符串按固定顺序构造（`.rdata 0x1004DC48..0x1004DCA8`），
并用 `sub_10014440(name)` 把"服务名 → 配置字节"映射回去：

| 序号 | 服务名 | 地址 | 配置字节 |
|---|---|---|---|
| 0 | `SysMain` | 0x1004DC48 | +0x08（**恒 0**） |
| 1 | `HomeGroupListener` | 0x1004DC50 | +0x0D |
| 2 | `HomeGroupProvider` | 0x1004DC64 | +0x0C |
| 3 | `wuauserv` | 0x1004DC78 | +0x09 |
| 4 | `WSearch` | 0x1004DC84 | +0x0B |
| 5 | `XblAuthManager` | 0x1004DC8C | +0x0A |
| 6 | `DiagTrack` | 0x1004DC9C | +0x07 |
| 7 | `Diagsvc` | 0x1004DCA8 | +0x06 |

`sub_10016D40(index, enable)` 的实测语义（反汇编 214 条，关键片段如下）：

```
0x10016D70  push 0xF003F                 ; OpenSCManagerW(., ., SC_MANAGER_ALL_ACCESS)
0x10016DD3  push 0xF01FF                 ; OpenServiceW(., name, SERVICE_ALL_ACCESS)
0x10016E03  call QueryServiceStatusEx    ; level=0, buf=0x24 字节
0x10016E11  mov eax,[ebp-0x48]           ; dwCurrentState
0x10016E1E  jmp [eax*4+0x10016FA4]       ; 状态分派
; state == SERVICE_RUNNING(4):
0x10016F51  push 0x10050B94               ; L"1"  ——写入并行标记表 0x10059348
0x10016F62  call ControlService(., 1, .) ; SERVICE_CONTROL_STOP
; state == SERVICE_STOPPED(1):
0x10016EA7  cmp dword ptr [ebp-0x74],1   ; 标记长度 == 1（即标记为 "1"）
0x10016EAF  cmp [ebp-0x69], al           ; enable == 0
0x10016EB7  call StartServiceW(., 0, 0)
```

即：

* **开始**：服务在跑 → `ControlService(SERVICE_CONTROL_STOP)`，并在**进程内**标记表写 `"1"`；
* **还原**：服务已停 **且** 标记表里是 `"1"`（说明是自己停的）→ `StartServiceW`；
* 处于 `START_PENDING`/`STOP_PENDING` 等中间态 → 什么都不做。

`SysMain` 永远不会被停：它对应的配置字节 +0x08 在 `sub_10013BD0` 里根本没有被赋值。

### 4.3 关闭后台进程（UI 第 2 项）

`sub_10016B20(name)` 遍历 `.data 0x10056AF8` 起的 **48 条** `std::wstring`（每条 0x18 字节，
由 `sub_100010A0` 用宽字面量初始化），把进程名和模式**都转小写**后做**子串**匹配
（`sub_1000B520` 返回 -1 表示不含）。

48 条黑名单（`.rdata 0x1004DDFC … 0x1004E478`，按初始化顺序）：

```
ComputerZService.exe  MedalWall.exe        ZMQLaunch.exe         Tracehelper.exe
LdsMMFeedback.exe     ModuleUpdate.exe     ComputerZ_CN.exe      ScriptExecute.exe
TslXETE.exe          Display3DEx.exe      LDSGameCenter.exe     iosdb64.exe
SoftMgrInst.exe      LDSGameMasterLoader.exe  ComputerZ14.exe    Launcher.del.exe
NavPlugin.exe        DrvMgr.exe           LiveUpdate360.exe     aapt.exe
DrvMgrFeedBack.exe   ComputerZHelper_x64.exe  AssociationSuggest.exe  ComputerZTray.exe
LuDaShiFeedBack.exe  MobileDeviceSrv.exe  LdsDrvInst64.exe      ComputerZTinyTray.exe
Ldshelper.exe        BenchmarkLauncher.exe  LdsIeView.exe        LuDaShiPreview.exe
LdsMobile.exe        LdsLite.exe          mininews.exe          DumpUper.exe
LiteUninst.exe       NavProxy64.exe       360ScreenCapture.exe  ComputerZBrowser.exe
BenchmarkLauncher_x64.exe  CleanHelper64.exe  DrvInst64.exe      LdsDrvInst32.exe
gameuninst.exe       guardhp.exe          MemoryOptimizer.exe   TrayHelper.exe
```

观察：这批名字几乎全是**鲁大师（LDS / ComputerZ / LuDaShi）自家后台**与 360、驱动安装器等
工具链进程 —— 也就是说这一项的真实目标是清掉"同类工具的后台常驻/悬浮窗"。

### 4.4 释放电脑内存（UI 第 4 项）

`CreateToolhelp32Snapshot` 遍历**全部进程**（不做会话过滤），对每个能 `OpenProcess(0x1FFFFF, TRUE, pid)`
成功的进程调用：

```
SetProcessWorkingSetSize(h, (SIZE_T)-1, (SIZE_T)-1);   // 让系统把工作集页换出
K32EmptyWorkingSet(h);                                  // 立即清空工作集
```

这是"一次性"的：不改变任何持久状态，还原时不会（也无法）恢复。

### 4.5 清理 DNS / 清理粘贴板（UI 第 8、9 项）

```
WinExec("ipconfig /flushdns", 0)        // 0x1004DC14，ShowWindow 参数 0
OpenClipboard(NULL) → EmptyClipboard() → CloseClipboard()   // 三个 USER32 导入直接调用
```

### 4.6 调整游戏进程优先级（UI 第 1 项）

调用链：`beginGameOptimization → sub_10016580 → sub_100174A0 → sub_10017AA0`。

`sub_10016580(gameId)`：

```
if (gameId ∈ {0x1FD, 0x1FC, 0x20B, 0x1F5}) global_0x1005A89C = 1;   // "特殊标题"
global_0x1005A8A0 = 0;                     // "自有 cpucores 辅助进程是否在跑"
sub_10016720();                             // 重建当前会话进程表
for (p in session_procs)
    if (p.name 含 "cpucores" || 含 "cpucoresspecs") global_0x1005A8A0 = 1;
sub_100174A0();                             // 优先级主循环
```

`sub_100174A0` 从 INI 直接再读一次 `SystemGameProPriority`，据此决定目标优先级：

```
target = (SystemGameProPriority == 1) ? 0x8000 /*ABOVE_NORMAL*/ : 0x20 /*NORMAL*/;
if (global_0x1005A89C == 0 && global_0x1005A8A0 == 0) should_set = 1; else should_set = 0;
```

`sub_10017AA0(h, entry, target, should_set, limit_core)` 对每个进程：

```
if (进程名 命中 372 条特征表之一) {
    if (当前优先级 != HIGH(0x80) && != REALTIME(0x100)) {
        if (global_0x1005A89C == 1) SetPriorityClass(h, 0x4000);   // BELOW_NORMAL
        if (should_set)             SetPriorityClass(h, target);
    }
} else {
    /* 不命中：只可能在 limit_core 时改亲和性 */
}
```

于是实际效果是：

* 普通标题、选项开 → 命中进程升到 **ABOVE_NORMAL**；选项关 → 设为 **NORMAL**；
* 特殊标题（0x1F5/0x1FC/0x1FD/0x20B）→ 命中进程降为 **BELOW_NORMAL**；
* 一旦发现自己的 `cpucores` 辅助进程在跑 → 本次一个进程都不动（避免与"限制核心"打架）；
* 已经是 `HIGH`/`REALTIME` 的进程不动（不降级高优先级进程）。

### 4.7 限制核心（`SystemLimitCore`，扩展项，当前 UI 已隐藏）

`sub_10017DC0` 在 `SystemLimitCore == 1 && 核心数 >= 4 && 没有 cpucores 进程` 时返回 true；
`sub_10017980(1)` 构造一个长度为"总核心数"的 `'0'/'1'` 字符串（前 N 个为 `'1'`，
N = 设定核心数；若设定值等于总数则取总数的一半），`sub_10017FE0` 把它按**首字符为最高位**
解析成整数掩码，最后 `SetProcessAffinityMask(h, mask)`。

---

## 5. 游戏识别机制

需要明确一点：**这个模块自己不认游戏**。

1. **标题 id 由外部传入**：`beginGameOptimization(gameId)` 的 `gameId` 是 Electron 主进程
   （`GamePP.exe` + 游戏库/进程扫描）算出来的整数。模块内只有 6 个 id 被硬编码：
   `0x1F4/0x1F5`（PUBG 系）、`0x1FC/0x1FD`、`0x205/0x206`、`0x20B`。
2. **进程级识别靠名字子串**：`.data 0x10056FA8` 起 372 个 `std::wstring`（每条 0x18 字节，
   由 `sub_10001C00` 用 `.rdata 0x1004E490…0x10050AD8` 的字面量逐条构造）。
   匹配方式同样是"转小写 + 子串"。
3. **会话隔离**：`sub_10016720` 建两张表，一张全系统进程，一张用
   `ProcessIdToSessionId(pid) == ProcessIdToSessionId(self)` 过滤出的**当前会话**进程；
   杀进程/改优先级都只用后者，内存回收用前者。
4. **没有窗口类名/标题匹配**：该 `.node` 的导入表里没有 `FindWindow`/`EnumWindows`/
   `GetWindowThreadProcessId`。窗口特征识别在 `GamePP.exe` 主程序侧（不在本模块范围）。

> 特征表的一个附带事实：表里除了游戏名（`TslGame`、`EscapeFromTarkov`、`VALORANT`、
> `FortniteClient-Win64-Shipping`、`csgo` …）还包含 `svchost`、`explorer`、`GamePP`、
> `wegame`、`steam` 等条目。因为系统进程通常拒绝 `SetPriorityClass`（权限不足），
> 实际生效的往往是用户态进程（explorer、启动器、GamePP 自身）。

---

## 6. 并发与生命周期

| 机制 | 位置 | 行为 |
|---|---|---|
| 互斥体 | `GamePPOptimize.node!0x10048E54` = `L"gamePP_opti"` | `CreateMutexW(NULL, TRUE, ...)` 后 `WaitForSingleObject(1000)`；超时（0x102）→ **直接返回 true，什么都不做** |
| 中止标志 | 结构体 `+0x4C` | `stopMidwayOpt` 置 1；`begin` 读完配置后检查，置位则跳过游戏设置优化 |
| 已优化标志 | 结构体 `+0x4D` | `begin` 一开始置 1；`restore` 只有它非 0 才执行还原 |
| 服务标记表 | `.data 0x10059348`（8×0x18） | 记录"是我停的"，还原时只拉这些 |

**关键点：全部状态都在进程内存里。** 如果 GamePP 主进程退出，再启动后调用
`restoreGameOptimization` 会因为 `+0x4D == 0` 而**什么都不做** —— 被停的服务不会自动拉回来。

---

## 7. 还原路径

`restoreGameOptimization`（`sub_1000D920`）：

```c
if (st->byte_0x4D == 0) { ReleaseMutex(...); return true; }   // 没优化过 → 直接返回
sub_10013BD0();          // 重新读 INI
sub_100145E0(0, 0);      // 游戏配置文件还原
sub_100141E0(0);         // ★ 服务还原（只拉标记为 "1" 的）
sub_100177F0();          // ★ 优先级/亲和性复位
st->byte_0x4D = 0;
ReleaseMutex(...); CloseHandle(...);
```

`sub_100177F0` 的实际动作（注意粒度）：

```c
for (p in session_procs) {                       // 当前会话的每一个进程
    h = OpenProcess(0x1FFFFF, 0, p.pid);
    if (!h) continue;
    SetPriorityClass(h, NORMAL_PRIORITY_CLASS);  // ★ 无论是不是游戏进程
    mask = build_mask(total_cores);              // 全 1（核心数为 0 时跳过）
    SetProcessAffinityMask(h, mask);
    CloseHandle(h);
}
```

**不可还原项**：被 `TerminateProcess` 结束的进程、`ipconfig /flushdns`、`EmptyClipboard`、
`SetProcessWorkingSetSize` 释放的内存、`lodctr /R`。

---

## 8. 电源方案（UI 第 3 项）：由 `Power.dll` 完成

`GamePPOptimize.node` 里没有 `powrprof` 导入，`SystemAIPower`（+0x00）在 begin 路径上
也没有任何消费点。真正的实现在 `win32\common\Power.dll`：

* 导出：`PP_Initialization` / `PP_UnInitialization` / `PP_Active` / `PP_UnActive`；
* 字符串：`POWRPROF.dll`、`PowerImportPowerScheme`、`PowerSetActiveScheme`（运行时 `LoadLibrary`+`GetProcAddress`）；
* `PP_Initialization` 的反编译里可见 `PowerImportPowerScheme(0, &guid, &out)` →
  `PowerWriteFriendlyName` / `PowerWriteDescription` → `PowerWriteACValueIndex` /
  `PowerWriteDCValueIndex`，并按系统版本/架构在 `0x10016870` 的 GUID 表里选档；
* 数据：`PowerData\GPP_Maxed.pow`（高性能）、`GPP_Excellence.pow`（卓越性能）、
  `GPP_Ryzen.pow`（Ryzen 专用），文件头 `regf`；
* 上一个方案记在 `%LOCALAPPDATA%\GamePP\Power.cfg` 的 `LastPlan`，用于还原。

因此 Rust 版用同样的动态加载方式实现（`src/power.rs`）：`PowerImportPowerScheme(.pow)` →
`PowerSetActiveScheme(新 GUID)`；还原时 `PowerSetActiveScheme(旧 GUID)`。

---

## 9. 行为风险（客观陈述，非评价）

1. `SystemKillBackPro` 用的是**子串**匹配，任一名含 48 条之一的进程都会被 `TerminateProcess(0)` 结束，
   没有"用户已保存文档"之类的保护。
2. `SystemAIMemory` 对**全系统**进程（含其他用户会话）调用 `EmptyWorkingSet`，会瞬间抽走工作集，
   短时间内系统可能明显卡顿。
3. 还原阶段会把**当前会话所有能打开的进程**优先级改成 `NORMAL` —— 包括用户自己设过
   `ABOVE_NORMAL`/`BELOW_NORMAL` 的进程。
4. `lodctr /R` 是无条件执行的，它会重建性能计数器（耗时且偶发失败）。
5. `net stop uxsms` → 等 3 秒 → `net start uxsms` 会强制重启桌面窗口管理器合成，
   部分场景下会闪屏/黑屏一次。
6. `SysMain` 因配置字节从不赋值而永远不会被停（原版的死代码，本实现保留同样行为）。

---

## 10. Rust 复刻工程

```
gamepp-winopt/
├── Cargo.toml
├── data/
│   ├── game_patterns.txt   # 从 .rdata 0x1004E490..0x10050AD8 精确转储的 371 条进程特征
│   └── kill_list.txt       # 48 条后台进程黑名单
└── src/
    ├── main.rs      # CLI：start / stop / status，互斥体 gamePP_opti
    ├── ffi.rs       # 手写 Win32 FFI（无第三方 crate）
    ├── config.rs    # GamePPOpt.ini [SYS] 读取，字段偏移与原结构体对齐
    ├── process.rs   # 会话过滤的进程枚举、大小写不敏感子串匹配、TerminateProcess
    ├── service.rs   # 8 个服务的 STOP / 条件 START（带"是我停的"标记）
    ├── power.rs     # POWRPROF 动态加载：导入 .pow + 激活 / 还原
    ├── optimize.rs  # begin/restore 主流程，逐条对齐 sub_10013F70 与 sub_100177F0
    └── state.rs     # 跨进程状态（替代原版的进程内结构体）
```

原模块函数 ↔ Rust 函数对应表：

| 原实现 | Rust |
|---|---|
| `sub_10013BD0`（读 INI） | `config::Config::load` |
| `sub_10013F70`（Windows 优化主流程） | `optimize::begin` |
| `sub_10014440`（服务名→配置字节） | `service::services` |
| `sub_10016D40`（停/起服务） | `service::stop_service` / `start_service_if_stopped` |
| `sub_10016B20`（48 条黑名单匹配） | `process::match_any` + `data/kill_list.txt` |
| `sub_10016720`（进程表 + 会话过滤） | `process::list_processes` |
| `sub_10016580`/`sub_100174A0`/`sub_10017AA0`（优先级） | `optimize::apply_priority` |
| `sub_100177F0`（还原优先级/亲和性） | `optimize::restore` |
| `sub_10017980`/`sub_10017FE0`（亲和性掩码） | `optimize::mask_msb_first` |
| `Power.dll!PP_Active` / `PP_UnActive` | `power::switch_to_pow` / `power::set_active` |
| 进程内结构体 `0x1005A878` + 标记表 `0x10059348` | `state::State`（落盘到 `%LOCALAPPDATA%\GamePPWinOpt\state.ini`） |

### 唯一的结构性偏差（必须说明）

原版把"当前是否已优化、停过哪些服务、核心数、上一个电源方案"全部放在**常驻进程的内存**里，
靠 Electron 主进程的存活来保证 begin/restore 成对。独立可执行程序没有这个生命周期，
因此本实现把这些字段落盘到 `state.ini`，语义保持一致：

* 状态文件不存在 ⇒ `stop` 什么都不做（等价于原版 `+0x4D == 0`）；
* 只有自己停过的服务才会被重新拉起（等价于标记表 `"1"`）。

### 实现备注（两处易错点）

1. **还原时的亲和性掩码宽度**：原版 `sub_100177F0` 里掩码字符缓冲的长度是**总核心数**
   （全局 `0x1005A87C`），全填 `'1'` 后再由 `sub_10017FE0` 按"首字符为最高位"解析。
   等价实现必须是 `mask(total_cores, total_cores)`；若误用"设定核心数"当宽度，
   在核数多于设定值时会把进程钉死在少数核心上。
2. **电源方案文件定位**：原版从 GamePP 安装目录的 `win32\PowerData\` 取 `.pow`。
   独立安装后目录结构不同，因此按 `--pow` → 程序目录 → `%LOCALAPPDATA%\GamePP\Apps\*\<版本>\win32\PowerData`
   → `%ProgramFiles%\GamePP` 的顺序查找；都找不到时退化为按已知 GUID
   `9fda562c-d377-43d5-bb2c-afcb0a22c547` 直接激活已导入的方案，再不行就跳过（绝不乱改电源计划）。
3. **管理员权限**：原版由 Electron 主程序以管理员身份启动，`.node` 里没有任何自提权代码。
   独立 exe 在 `start` / `stop` 执行前检查令牌，非管理员时用 `ShellExecuteExW(..., "runas", ...)`
   拉起自身副本，父进程 `WaitForSingleObject` 等子进程结束并透传退出码；`--dry-run` 与 `status` 不提权。
   FFI 结构体 `SHELLEXECUTEINFOW` 在 x64 下必须为 112 字节，代码里有编译期断言。

### 编译与运行

```bash
cd gamepp-winopt
cargo build --release            # 已验证：rustc 1.99.1 / x86_64-pc-windows-gnu，零外部依赖

gamepp-winopt.exe status
gamepp-winopt.exe start --dry-run          # 只打印将要执行的动作
gamepp-winopt.exe start --game-id 500 --pid 1234 --cores 8
gamepp-winopt.exe stop --dry-run           # 只打印还原计划（含亲和性掩码）
gamepp-winopt.exe stop
```

`--dry-run` 下的实测输出（本机）：

```
- 清理 DNS: ipconfig /flushdns
- 关闭 Aero: net stop uxsms → sleep 3000 → net start uxsms
- 停止服务（dry-run）: WSearch
- 停止服务（dry-run）: XblAuthManager
- 切换电源方案（dry-run）: win32\PowerData\GPP_Maxed.pow
- 重建性能计数器: lodctr /R
- 清空剪贴板
- 优先级: svchost.exe (...) → 0x8000 ...
```

---

## 11. 尚未解决的问题

| 编号 | 问题 | 影响 | 如何解决 |
|---|---|---|---|
| U1 | 特征表实际是 **372 个槽位**，按字面量地址区间精确转储得到 **371 条**（缺 1 条，疑为空串或重复项被折叠） | 极低（子串匹配下少一条几乎无感） | 在隔离环境里 `beginGameOptimization` 之后 dump `.data 0x10056FA8` 起 0x22E0 字节 |
| U2 | `sub_10017AA0` 内 `arg_c`（是否写优先级）在 `setProOpt` 路径上的实际传值未逐一追清 | 中（只影响"限制核心"独立入口） | 追踪 `sub_10016FC0` 的调用方参数 |
| U3 | `sub_100145E0`（游戏配置文件改写，PUBG/SCUM/武侠乂/Tarkov 的 ini 与 AppCompat 层）未展开 | 不影响 Windows 优化复刻 | 需要时单独出报告 |

---

## 12. 证据索引

| 证据 id | 内容 |
|---|---|
| `ev_cbcd8614…6ae1` | 目标总览：PE32 x86、1149 函数、PDB 路径、导入/导出计数 |
| `ev_2537df5c…abf8` | 26 个 `System*` 键的地址与引用点（`sub_10013BD0` / `sub_100151F0` / `sub_10015630`） |
| `ev_1c32097c…b578` | `sub_10013BD0` / `sub_100151F0` / `sub_10015630` 伪代码（配置结构体与默认值写入） |
| `ev_a8ab16ab…ba422` | `sub_10013F70` 完整反汇编（186 条指令，Windows 优化主流程） |
| `ev_7f6640bc…cdb11` | `sub_10013F70` 伪代码 |
| `ev_53d14366…7b254` | `"ipconfig /flushdns"` 字符串及其唯一引用点 `sub_10013F70` |
| `ev_96182110…e09a1` | `sub_10014440` / `sub_10016D40` / `sub_100145E0` 伪代码（服务映射与服务控制） |
| `ev_7a686837…faecf` | `sub_10016D40` 完整反汇编（OpenSCManagerW→QueryServiceStatusEx→ControlService/StartServiceW） |
| `ev_66cc19f8…8cbf` | `sub_100010A0`：48 条后台进程黑名单的字面量初始化 |
| `ev_a31e4a16…2d8` | `sub_10016B20` / `sub_10017FE0` / `sub_10016AC0` / `sub_10017980` 伪代码 |
| `ev_6b535d07…94145` | `sub_10001C00`：372 条进程特征表的字面量初始化（含每条 `(地址, 长度)`） |
| `ev_3a4e4a70…feed6` | `sub_10016580`：特殊标题 id 判定、cpucores 自检、调用优先级主循环 |
| `ev_735ba2c9…d5348` | `sub_10017AA0`：命中特征表后的 `SetPriorityClass` 逻辑 |
| `ev_d02c3a69…c269a` | `sub_100141E0`（服务遍历）与 `sub_100177F0`（全进程优先级复位） |
| `ev_d812701a…1b671` | `beginGameOptimization` / `restoreGameOptimization` 伪代码（互斥体、+0x4C/+0x4D） |
| `ev_b1842938…06c2` | `sub_1000DCE0` 导出表注册、`sub_100174A0`、`sub_10017DC0` |
| `ev_2c9f072d…be999` | `sub_10016720`（进程表 + `ProcessIdToSessionId` 会话过滤）、`sub_100187C0` |
| `ev_c17099ea…e3e` | `Power.dll` 中的 `PowerImportPowerScheme` / `PowerSetActiveScheme` / `POWRPROF.dll` |
| `ev_a7723369…bed6` | `Power.dll!PP_Initialization` 伪代码（导入 .pow 并写 AC/DC 值） |
| `ev_918f8577…d16f` | `AppCompatFlags\Layers` 字符串（游戏设置优化分支，非 Windows 优化） |
