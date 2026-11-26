//! 运行状态持久化。
//!
//! 原版的这部分状态全在进程内（GamePPOptimize.node 里的全局结构体 0x1005a878、
//! 服务标记表 0x10059348），由一个常驻的 Electron 主进程持有，
//! 所以同一进程内 begin → restore 天然成对。
//!
//! 独立可执行程序没有这个生命周期，于是把同样几项状态落盘到
//! `%LOCALAPPDATA%\GamePP-Lite\opt-state.ini`。语义保持等价：
//!   - 状态文件不存在 ⇒ 视为"没优化过" ⇒ restore 什么都不做（与原版 +0x4D == 0 一致）；
//!   - 只有自己停过的服务才会被重新拉起（对应标记表里的 "1"）；
//!   - 只有自己结束过的 Wallpaper Engine 进程才会被重新启动（原版不做这一步，是本工程的加强）。
//!
//! **落盘 ≠ 仍然生效**。原版靠"进程活着"这点保证状态不会比系统更长寿；独立程序一旦
//! 重启（或被用户直接关掉再开），文件里的 `optimized=1` 会比实际效果活得更久：
//! 系统重启后服务早已被拉起、进程优先级早已复位、被结束的进程也早被开机自启拉回来，
//! 界面却仍然显示「已优化」——这就是启动后状态与实际不符的根因。
//! 所以启动时必须走一次 [`reconcile_at_startup`]：读盘之后**逐项对账**（开机标记 +
//! 服务真实状态 + 进程是否还在 + 电源方案是否仍是我们切的那个），对不上的直接划掉，
//! 一项都不剩就清空状态文件。界面的「已优化 / 已还原」永远由对账结果决定。

use super::power;
use super::process;
use super::service;
use crate::win::ffi;
use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
    /// 原版结构体 +0x4D：本次优化是否真的执行过。
    pub optimized: bool,
    /// 原版 0x10059348 标记表：被我们停掉的服务名。
    pub stopped_services: Vec<String>,
    /// 被「关闭 Wallpaper Engine」结束的进程完整路径（还原时逐个重新启动）。
    /// 记录的是 exe 路径而不是 pid：pid 在还原时几乎必然已被系统复用。
    pub killed_wallpaper: Vec<String>,
    /// 电源方案切换前的 GUID（原版记在 Power.cfg 的 LastPlan）。
    pub prev_plan: Option<String>,
    /// 本次优化切换过电源方案。
    pub plan_switched: bool,
    /// 本次切换到的方案 GUID。**必须记**：启动对账要拿它和当前活动方案比对，
    /// 只记 prev_plan 的话无法判断"我们切过去的那个方案现在还在不在"。
    pub plan_target: Option<String>,
    /// 被我们改过优先级的进程（pid + 映像名）。
    ///
    /// 优先级是进程级属性，进程一退出就没了，重启后 pid 也必然换人；所以启动对账时
    /// 拿它逐个核实（进程还在 + 名字还对得上 + 优先级仍非 NORMAL），而不是靠猜。
    pub priority_targets: Vec<(u32, String)>,
    /// 被提权的游戏 pid（便于诊断）。
    pub game_pid: u32,
    /// 写盘时所在的这次开机标记（见 [`boot_stamp`]）。只由 `load` 填充，
    /// `save` 一律按"当前这次开机"重写，避免旧值被带进新文件。
    pub boot_stamp: Option<u64>,
}

pub fn state_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("GamePP-Lite").join("opt-state.ini")
}

impl State {
    pub fn load() -> State {
        match fs::read_to_string(state_path()) {
            Ok(t) => State::parse(&t),
            Err(_) => State::default(),
        }
    }

    /// 解析状态文件正文。单独抽出来是为了能对"旧版本文件"和"新字段"直接写测试，
    /// 不必真的去动 `%LOCALAPPDATA%`。
    fn parse(text: &str) -> State {
        let mut st = State::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            let (k, v) = match line.split_once('=') {
                Some(kv) => kv,
                None => continue,
            };
            let v = v.trim();
            match k.trim() {
                "optimized" => st.optimized = v == "1",
                "game_pid" => st.game_pid = v.parse().unwrap_or(0),
                "plan_switched" => st.plan_switched = v == "1",
                "prev_plan" => st.prev_plan = if v.is_empty() { None } else { Some(v.to_string()) },
                "plan_target" => {
                    st.plan_target = if v.is_empty() { None } else { Some(v.to_string()) }
                }
                "boot_stamp" => st.boot_stamp = v.parse().ok(),
                // 服务名不含分隔符，逗号/分号都容忍。
                "stopped_services" => st.stopped_services = split_list(v, &[',', ';']),
                // 路径里可能有逗号（"C:\Games, Inc\wp.exe"），只按分号切。
                "killed_wallpaper" => st.killed_wallpaper = split_list(v, &[';']),
                // pid|名字，条目之间用分号（| 与 ; 在 Windows 文件名里都是非法的，不会撞）。
                "priority_targets" => st.priority_targets = parse_targets(v),
                _ => {}
            }
        }
        st
    }

    /// 生成状态文件正文。`boot_stamp` 一律取"当前这次开机"：状态是本次会话里生效的，
    /// 不能沿用 load 进来的旧标记，否则重启后会被误认为仍属同一次开机。
    fn render(&self) -> String {
        let targets: Vec<String> = self
            .priority_targets
            .iter()
            .map(|(pid, name)| format!("{pid}|{name}"))
            .collect();
        format!(
            "; GamePP-Lite Windows-optimize state (generated)\n\
             optimized={}\n\
             game_pid={}\n\
             plan_switched={}\n\
             prev_plan={}\n\
             plan_target={}\n\
             stopped_services={}\n\
             killed_wallpaper={}\n\
             priority_targets={}\n\
             boot_stamp={}\n",
            if self.optimized { 1 } else { 0 },
            self.game_pid,
            if self.plan_switched { 1 } else { 0 },
            self.prev_plan.clone().unwrap_or_default(),
            self.plan_target.clone().unwrap_or_default(),
            self.stopped_services.join(","),
            self.killed_wallpaper.join(";"),
            targets.join(";"),
            boot_stamp(),
        )
    }

    pub fn save(&self) {
        let path = state_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = crate::domain::settings::write_atomic(&path, &self.render());
    }

    pub fn clear() {
        if let Some(dir) = state_path().parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::remove_file(state_path());
    }

    /// 状态文件是否属于"当前这次开机"。
    ///
    /// 旧版本文件里没有 `boot_stamp`，无从证明，按"不是本次开机"处理 —— 后面会走
    /// 逐项对账兜底，宁可少报「已优化」也不谎报。
    pub fn same_boot(&self) -> bool {
        match self.boot_stamp {
            Some(saved) => saved.abs_diff(boot_stamp()) <= BOOT_TOLERANCE_SECS,
            None => false,
        }
    }

    /// 还有多少"可还原"的内容（用于判断对账后是否还该显示「已优化」）。
    pub fn effective_items(&self) -> usize {
        self.stopped_services.len()
            + self.killed_wallpaper.len()
            + self.priority_targets.len()
            + usize::from(self.plan_switched)
    }
}

/// 启动对账结果。
pub struct Startup {
    /// 对账之后仍然成立的优化状态（一项都不剩时已清空并 `optimized=false`）。
    pub state: State,
    /// 给人看的说明（无变化时为空）。会写进 runtime.log，并显示在 ② 页「执行日志」。
    pub lines: Vec<String>,
    /// 读盘时磁盘上的状态是否已经过期（发生过重置/裁剪）。诊断用。
    pub reconciled: bool,
}

/// 读盘 + 逐项对账，得到"启动这一刻"的真实优化状态。
///
/// 这是修复"重启后仍显示已优化"的入口：界面与托盘都必须以它的结果为准，
/// 不能直接拿 `State::load().optimized` 当显示值。
pub fn reconcile_at_startup() -> Startup {
    let saved = State::load();
    if !saved.optimized {
        return Startup { state: saved, lines: Vec::new(), reconciled: false };
    }

    let rebooted = !saved.same_boot();
    let mut lost: Vec<String> = Vec::new();
    let mut next = State {
        optimized: true,
        game_pid: saved.game_pid,
        ..Default::default()
    };

    // ① 被停掉的服务：重启后系统会把该起的服务重新拉起来，原来的"停过"就不作数了。
    //    即使没重启，也要以服务当前状态为准（可能被用户手动启动、或被触发器拉起）。
    for name in &saved.stopped_services {
        let still_ours = !rebooted && service::is_running(name) == Some(false);
        if still_ours {
            next.stopped_services.push(name.clone());
        } else if rebooted {
            // 重启后"当前是停着的"不再能证明是我们停的：可能是它本来就没启动。
            lost.push(format!("服务 {name} 随关机重启已失效"));
        } else {
            lost.push(format!("服务已恢复运行 {name}"));
        }
    }

    // ② 被结束的 Wallpaper Engine：进程已经又跑起来了（开机自启/用户手动开）就没必要还原。
    let procs = process::list_processes(false);
    for path in &saved.killed_wallpaper {
        let name = file_name_of(path);
        let file_exists = !name.is_empty() && Path::new(path).is_file();
        if !file_exists {
            // 程序被卸载/移动了，重启也没有意义。
            lost.push(format!("记录的程序已不存在 {path}"));
        } else if running(&procs, &name) {
            lost.push(format!("Wallpaper Engine 已恢复运行 {name}"));
        } else {
            next.killed_wallpaper.push(path.clone());
        }
    }

    // ③ 电源方案：重启不会把它改回去，所以不看开机标记，只看"当前活动方案是否还是
    //    我们切过去的那个"。用户手动换过方案，就等于我们的改动已经被覆盖。
    if saved.plan_switched {
        // 旧文件没记 plan_target，退回用 GPP 方案 GUID 判断（历史上只会切到它）。
        let target = saved
            .plan_target
            .clone()
            .unwrap_or_else(|| power::GPP_MAXED_GUID.to_string());
        match power::active_scheme() {
            Some(cur) if cur.eq_ignore_ascii_case(&target) => {
                next.plan_switched = true;
                next.plan_target = Some(cur);
                next.prev_plan = saved.prev_plan.clone();
            }
            other => lost.push(format!(
                "电源方案已不是优化时切换的 {}（当前 {}）",
                target,
                other.unwrap_or_else(|| "查询失败".to_string())
            )),
        }
    }

    // ④ 进程优先级：pid 变了 / 进程没了 / 优先级已回落 ⇒ 我们没有可还原的内容。
    //    重启后 pid 必然换人，所以这一条同时也是"重启检测"的兜底。
    for (pid, name) in &saved.priority_targets {
        if priority_still_set(&procs, *pid, name) {
            next.priority_targets.push((*pid, name.clone()));
        } else {
            lost.push(format!("进程优先级已复位 {name}（{pid}）"));
        }
    }

    // 旧版本状态文件没有 priority_targets（也就无从核对），这里不额外报警：
    // 只要别的项还成立就继续算已优化，一项都不剩下面自然会被清掉。

    let mut lines = Vec::new();
    if next.effective_items() == 0 {
        // 一项都不剩：磁盘上的"已优化"已经纯属历史残留，连文件一起清掉，
        // 免得下次启动又把同一份过期状态读回来（也免得 restore 去碰不该碰的东西）。
        State::clear();
        next = State { optimized: false, ..Default::default() };
        lines.push(format!(
            "启动检查：上次的优化已不再生效（{}），状态已重置为「未优化」",
            if rebooted { "系统已重新启动" } else { "相关项目已自动恢复" }
        ));
    } else if rebooted || !lost.is_empty() {
        // 还剩下真的生效的部分：按对账后的子集重写，让「还原」只处理还能处理的那几项。
        next.save();
        lines.push(format!(
            "启动检查：上次的优化仍有 {} 项在生效，已按当前实际状态校正",
            next.effective_items()
        ));
    }
    // 逐项列出被划掉的原因：用户看到"怎么变未优化了"时，日志里能直接对上号。
    for l in &lost {
        lines.push(format!("· 已失效：{l}"));
    }
    Startup { state: next, lines, reconciled: rebooted || !lost.is_empty() }
}

fn running(procs: &[process::ProcEntry], name: &str) -> bool {
    procs.iter().any(|p| p.name.eq_ignore_ascii_case(name))
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 我们改过优先级的进程是否还活着、名字还对得上、优先级还非 NORMAL。
///
/// 只申请 `PROCESS_QUERY_LIMITED_INFORMATION`：启动时可能没提权，全权限句柄会直接失败，
/// 那样所有项都会被误判成"已失效"。
fn priority_still_set(procs: &[process::ProcEntry], pid: u32, name: &str) -> bool {
    let Some(p) = procs.iter().find(|p| p.pid == pid) else {
        return false;
    };
    if !p.name.eq_ignore_ascii_case(name) {
        return false;
    }
    unsafe {
        let h = ffi::OpenProcess(ffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let cur = ffi::GetPriorityClass(h);
        ffi::CloseHandle(h);
        cur != 0 && cur != ffi::NORMAL_PRIORITY_CLASS
    }
}

fn split_list(v: &str, seps: &[char]) -> Vec<String> {
    v.split(seps)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_targets(v: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for item in split_list(v, &[';']) {
        let Some((pid, name)) = item.split_once('|') else {
            continue;
        };
        let (Ok(pid), false) = (pid.trim().parse::<u32>(), name.trim().is_empty()) else {
            continue;
        };
        out.push((pid, name.trim().to_string()));
    }
    out
}

// ---------------------------------------------------------------- 开机标记

/// 两次判定之间允许的开机标记漂移（秒）。
///
/// 标记本身是"本次开机的绝对时刻"，同一会话内恒定；容差只是留给用户手工改系统时间、
/// 或 NTP 大幅校时的情况 —— 重启造成的差值远大于它。
const BOOT_TOLERANCE_SECS: u64 = 10;

/// 当前这次开机的时刻（Unix 秒）。重启后必然变化，同一次开机内恒定。
pub fn boot_stamp() -> u64 {
    if let Some(t) = nt_boot_time() {
        return t;
    }
    // 兜底：当前时间 − 系统运行时长。休眠时 GetTickCount64 的计法各版本不一致，
    // 可能引入偏差，所以只在 NtQuerySystemInformation 不可用时使用。
    let up = unsafe { ffi::GetTickCount64() } / 1000;
    now_secs().saturating_sub(up)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `NtQuerySystemInformation(SystemTimeOfDayInformation)` 的第一个字段是**开机时刻**。
///
/// 为什么不用 `SystemTime::now() - GetTickCount64()`：那套算法把"休眠时长"算不算进
/// 运行时长这件事交给了系统版本，同一会话内也可能漂移；这个字段是内核开机时从 RTC
/// 记下的绝对时间，只有系统时钟被修改时才跟着变，正好用来区分"重启过"和"只是又开了
/// 一次程序"。
fn nt_boot_time() -> Option<u64> {
    /// SystemTimeOfDayInformation
    const CLASS: u32 = 3;
    // SYSTEM_TIMEOFDAY_INFORMATION：BootTime / CurrentTime / TimeZoneBias /
    // TimeZoneId / Reserved / BootTimeBias / SleepTimeBias，旧系统 40 字节、新系统 48，
    // 只取前 8 字节（BootTime）即可，缓冲区按大的给。
    let mut buf = [0u8; 48];
    let mut ret: u32 = 0;
    let status = unsafe {
        ffi::NtQuerySystemInformation(
            CLASS,
            buf.as_mut_ptr() as *mut c_void,
            buf.len() as u32,
            &mut ret,
        )
    };
    if status < 0 {
        return None;
    }
    let ft = i64::from_le_bytes(buf[0..8].try_into().ok()?);
    if ft <= 0 {
        return None;
    }
    /// FILETIME 起点（1601-01-01）到 Unix 纪元（1970-01-01）的秒数。
    const FILETIME_EPOCH_DIFF: i64 = 11_644_473_600;
    u64::try_from(ft / 10_000_000 - FILETIME_EPOCH_DIFF).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧版本状态文件（没有 plan_target / priority_targets / boot_stamp）必须能读，
    /// 而且不能被当成"本次开机"写的 —— 否则重启后那份过期状态又会原样生效。
    #[test]
    fn legacy_file_parses_and_is_not_same_boot() {
        let text = "; GamePP-Lite Windows-optimize state (generated)\n\
                    optimized=1\n\
                    game_pid=0\n\
                    plan_switched=1\n\
                    prev_plan=381b4222-f694-41f0-9685-ff5bb260df2e\n\
                    stopped_services=WSearch,wuauserv\n\
                    killed_wallpaper=C://Program Files\\Steam\\wp64.exe\n";
        let st = State::parse(text);
        assert!(st.optimized);
        assert_eq!(st.stopped_services, vec!["WSearch", "wuauserv"]);
        assert_eq!(st.killed_wallpaper.len(), 1);
        assert!(st.plan_switched);
        assert!(st.prev_plan.is_some());
        // 新字段在旧文件里没有：plan_target 为空（对账会退回用 GPP GUID 判断）、
        // priority_targets 为空、boot_stamp 为空 ⇒ 不属于当前这次开机。
        assert_eq!(st.plan_target, None);
        assert!(st.priority_targets.is_empty());
        assert_eq!(st.boot_stamp, None);
        assert!(!st.same_boot());
    }

    /// 写盘 → 读回：所有字段都要能原样还原（boot_stamp 例外，它总按"当前开机"重写）。
    #[test]
    fn render_roundtrip_keeps_all_fields() {
        let st = State {
            optimized: true,
            stopped_services: vec!["WSearch".into(), "DiagTrack".into()],
            killed_wallpaper: vec![r"D:/Games, Inc/wallpaper64.exe".into()],
            prev_plan: Some("381b4222-f694-41f0-9685-ff5bb260df2e".into()),
            plan_switched: true,
            plan_target: Some("9fda562c-d377-43d5-bb2c-afcb0a22c547".into()),
            priority_targets: vec![(1234, "cs2.exe".into()), (5678, "PING.EXE".into())],
            game_pid: 999,
            boot_stamp: None,
        };
        let mut back = State::parse(&st.render());
        // 渲染时 boot_stamp 取的是"当前这次开机"，比对前先归一化。
        assert!(back.same_boot(), "刚写出的状态必须被判定为本次开机");
        back.boot_stamp = None;
        assert_eq!(back, st);
    }

    /// 优先级记录里出现的畸形条目直接忽略，不能让整份状态解析失败。
    #[test]
    fn priority_targets_ignore_junk() {
        let st = State::parse("optimized=1\npriority_targets=1234|cs2.exe;;bad|;nopipe;0|\n");
        assert_eq!(st.priority_targets, vec![(1234, "cs2.exe".to_string())]);
        // 一项有效目标 ⇒ 有可还原内容。
        assert_eq!(st.effective_items(), 1);
    }
}
