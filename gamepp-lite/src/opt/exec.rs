//! 优化执行流程。
//!
//! `run` 严格保留原版 `GamePPOptimize.node!sub_10013f70` 的动作顺序（逐条比对过）：
//!   ① 清 DNS → ② 关 Wallpaper Engine → ③ 停后台进程 → ④ 释放内存 →
//!   ⑤ 停服务 → ⑥ 电源方案 → ⑦ 无条件 lodctr /R → ⑧ 清剪贴板 → ⑨ 优先级/亲和性
//!
//! `restore` 对应 `sub_1000d920` 里的 `[重读配置 → sub_100141e0(0) → sub_100177f0]`：
//!   只重启自己停过的服务 → 全会话优先级复位 NORMAL → 电源方案还原。
//!
//! 与原版的差异只有一处：所有输出走调用方给的 `log`（Vec<String>），因为 GUI 没有控制台。

use super::ffi;
use super::plan::OptPlan;
use super::power;
use super::process;
use super::service::{self, StopOutcome};
use super::state::State;

/// 原版 `sub_10016580` 的 4 个特殊标题 id：不升优先级，反而降到 BELOW_NORMAL。
pub const SPECIAL_GAME_IDS: [u32; 4] = [0x1F5, 0x1FC, 0x1FD, 0x20B];

const SELF_HELPER_A: &str = "cpucores";
const SELF_HELPER_B: &str = "cpucoresspecs";

fn winexec(cmd: &str) {
    let mut b = cmd.as_bytes().to_vec();
    b.push(0);
    unsafe {
        ffi::WinExec(b.as_ptr(), 0);
    }
}

fn note(log: &mut Vec<String>, s: impl Into<String>) {
    log.push(s.into());
}

/// 执行一次优化，返回新的运行状态（调用方负责落盘）。
pub fn run(plan: &OptPlan, game_pid: u32, dry_run: bool, log: &mut Vec<String>) -> State {
    let mut st = State {
        optimized: !dry_run,
        cores: if plan.limit_cores { plan.cores } else { 0 },
        game_pid,
        ..Default::default()
    };
    note(
        log,
        if dry_run {
            "开始预演（不会修改系统）"
        } else {
            "开始执行优化"
        },
    );

    // ① 清理 DNS
    if plan.flush_dns {
        note(log, "· 清理 DNS：ipconfig /flushdns");
        if !dry_run {
            winexec("ipconfig /flushdns");
        }
    }

    // ② 关闭 Wallpaper Engine（按子串 "wallpaper" 终止）
    if plan.wallpaper_engine {
        if dry_run {
            note(log, "· 关闭 Wallpaper Engine（预演不执行）");
        } else {
            let killed = process::kill_matching(&["wallpaper".to_string()], true);
            if killed.is_empty() {
                note(log, "· 未发现 Wallpaper Engine 进程");
            }
            for (name, pid) in killed {
                note(log, format!("· 已结束 {name}（{pid}）"));
            }
        }
    }

    // ③ 关闭影响流畅性的后台进程（重新刷新进程表，与原版一致）
    if plan.kill_background {
        if dry_run {
            note(log, "· 关闭后台进程（预演不执行，黑名单 48 条）");
        } else {
            let patterns = process::kill_list();
            let killed = process::kill_matching(&patterns, true);
            if killed.is_empty() {
                note(log, "· 没有命中黑名单的后台进程");
            }
            for (name, pid) in killed {
                note(log, format!("· 已结束后台进程 {name}（{pid}）"));
            }
        }
    }

    // ④ 释放物理内存：对全部进程 EmptyWorkingSet（原版不过滤会话）
    if plan.free_memory {
        if dry_run {
            note(log, "· 释放内存（预演不执行）");
        } else {
            let mut ok = 0usize;
            for p in process::list_processes(false) {
                unsafe {
                    let h = ffi::OpenProcess(ffi::PROCESS_ALL_ACCESS, 1, p.pid);
                    if h.is_null() {
                        continue;
                    }
                    ffi::SetProcessWorkingSetSize(h, usize::MAX, usize::MAX);
                    ffi::K32EmptyWorkingSet(h);
                    ffi::CloseHandle(h);
                    ok += 1;
                }
            }
            note(log, format!("· 已回收 {ok} 个进程的工作集"));
        }
    }

    // ⑤ 停服务
    for (name, enabled) in plan.services() {
        if !enabled {
            continue;
        }
        if dry_run {
            note(log, format!("· 将停止服务 {name}"));
            st.stopped_services.push(name.to_string());
            continue;
        }
        match service::stop_service(name) {
            StopOutcome::Stopped => {
                note(log, format!("· 已停止服务 {name}"));
                st.stopped_services.push(name.to_string());
            }
            StopOutcome::NotRunning => note(log, format!("· 服务本来就没运行 {name}")),
            StopOutcome::Missing => note(log, format!("· 服务不存在 {name}")),
            StopOutcome::Failed => {
                note(log, format!("· 停止服务失败（权限不足？）{name}"))
            }
        }
    }

    // ⑥ 电源方案
    if plan.power_high_perf {
        match power::resolve_pow(None) {
            Some(pow) => {
                if dry_run {
                    note(log, format!("· 将切换电源方案：{}", pow.display()));
                } else if let Some((prev, new_guid)) = power::switch_to_pow(&pow) {
                    note(log, format!("· 电源方案已切换：{prev} → {new_guid}"));
                    st.prev_plan = Some(prev);
                    st.plan_switched = true;
                } else {
                    note(log, "· 切换电源方案失败（可能未导入过 GPP 方案）");
                }
            }
            None => {
                if dry_run {
                    note(
                        log,
                        format!("· 将尝试直接激活已导入方案 {}", power::GPP_MAXED_GUID),
                    );
                } else if let Some((prev, new_guid)) = power::switch_to_guid(power::GPP_MAXED_GUID)
                {
                    note(log, format!("· 电源方案（已导入直连）：{prev} → {new_guid}"));
                    st.prev_plan = Some(prev);
                    st.plan_switched = true;
                } else {
                    note(log, "· 未找到 GPP_*.pow，系统里也没有已导入的 GPP 方案，跳过电源切换");
                }
            }
        }
    }

    // ⑦ 无条件重建性能计数器（原版这条没有任何条件保护）
    note(log, "· 重建性能计数器 lodctr /R");
    if !dry_run {
        winexec("lodctr /R");
    }

    // ⑧ 清空剪贴板
    if plan.clean_clipboard {
        note(log, "· 清空剪贴板");
        if !dry_run {
            unsafe {
                if ffi::OpenClipboard(std::ptr::null_mut()) != 0 {
                    ffi::EmptyClipboard();
                    ffi::CloseClipboard();
                }
            }
        }
    }

    // ⑨ 优先级 / 亲和性
    apply_priority(plan, game_pid, dry_run, log);

    if !dry_run {
        note(
            log,
            format!("完成：共处理 {} 项动作", plan.enabled_count()),
        );
    } else {
        note(log, "预演结束（未做任何修改）");
    }
    st
}

/// 对应 `sub_10016580 + sub_100174a0 + sub_10017aa0`。
///
/// 要点（均从反汇编确认）：
/// - 只看当前会话的进程；
/// - 进程名命中 371 条特征表之一才处理；
/// - 若发现自己的 cpucores 辅助进程在跑，本次完全不改优先级；
/// - 当前已是 HIGH/REALTIME 的不动；
/// - `SystemGameProPriority=1` → ABOVE_NORMAL，否则 NORMAL。
/// 无论特征表怎么命中，都不动的系统关键进程。
///
/// 这是本工程**刻意**加的安全边界：原版把"名字命中即提优先级"无差别套用到当前会话，
/// 实测会命中 svchost.exe / explorer.exe 一类系统进程（特征表里有很短的通用子串）。
/// 给这些进程提优先级没有收益、反而可能拖慢整机，因此在提权前先排除。
const PROTECTED: [&str; 12] = [
    "system",
    "registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "svchost.exe",
    "explorer.exe",
    "dwm.exe",
    "audiodg.exe",
];

fn is_protected(name: &str) -> bool {
    let lower = name.to_lowercase();
    PROTECTED.iter().any(|p| lower == *p)
}

fn apply_priority(plan: &OptPlan, game_pid: u32, dry_run: bool, log: &mut Vec<String>) {
    let procs = process::list_processes(true);
    let self_pid = unsafe { ffi::GetCurrentProcessId() };
    let mut skipped = 0usize;
    let helper_running = procs.iter().any(|p| {
        process::contains_ci(&p.name, SELF_HELPER_A) || process::contains_ci(&p.name, SELF_HELPER_B)
    });
    let special = SPECIAL_GAME_IDS.contains(&game_pid);
    let patterns = process::game_patterns();
    let target = if plan.game_priority {
        ffi::ABOVE_NORMAL_PRIORITY_CLASS
    } else {
        ffi::NORMAL_PRIORITY_CLASS
    };

    let total = total_cores().min(64);
    let limit_mask: Option<usize> = if plan.limit_cores && plan.cores >= 4 && total > 0 && !helper_running
    {
        let active = if plan.cores >= total { total / 2 } else { plan.cores };
        Some(mask_msb_first(total, active) as usize)
    } else {
        None
    };

    let mut touched = 0usize;
    for p in &procs {
        if process::match_any(&p.name, &patterns).is_none() {
            continue;
        }
        if p.pid == self_pid || is_protected(&p.name) {
            skipped += 1;
            continue;
        }
        unsafe {
            let h = ffi::OpenProcess(ffi::PROCESS_ALL_ACCESS, 0, p.pid);
            if h.is_null() {
                continue;
            }
            let cur = ffi::GetPriorityClass(h);
            if cur != ffi::HIGH_PRIORITY_CLASS && cur != ffi::REALTIME_PRIORITY_CLASS {
                if special {
                    if !dry_run {
                        ffi::SetPriorityClass(h, ffi::BELOW_NORMAL_PRIORITY_CLASS);
                    }
                    note(log, format!("· 特殊标题，降为 BELOW_NORMAL：{}（{}）", p.name, p.pid));
                } else if !helper_running {
                    if !dry_run {
                        ffi::SetPriorityClass(h, target);
                    }
                    // game_priority 关闭时目标也是 NORMAL（原版语义），别一律写成"提"
                    note(
                        log,
                        format!(
                            "· {} {:#x}：{}（{}）",
                            if plan.game_priority {
                                "提优先级"
                            } else {
                                "设优先级"
                            },
                            target,
                            p.name,
                            p.pid
                        ),
                    );
                }
            }
            if let Some(mask) = limit_mask {
                if !dry_run {
                    ffi::SetProcessAffinityMask(h, mask);
                }
            }
            ffi::CloseHandle(h);
            touched += 1;
        }
    }
    if helper_running {
        note(log, "· 检测到 cpucores 辅助进程在运行，本次不修改优先级（原版行为）");
    }
    if skipped > 0 {
        note(
            log,
            format!("· 跳过 {skipped} 个系统关键进程（安全策略：不提系统进程优先级）"),
        );
    }
    if touched == 0 {
        note(log, "· 没有可处理的游戏进程，优先级未改动");
    }
}

/// 还原：只重启自己停过的服务 → 全会话优先级复位 → 电源方案还原。
pub fn restore(plan: &OptPlan, st: &mut State, dry_run: bool, log: &mut Vec<String>) {
    note(
        log,
        if dry_run {
            "开始预演还原（不会修改系统）"
        } else {
            "开始还原"
        },
    );

    for (name, _enabled) in plan.services() {
        if !st.stopped_services.iter().any(|s| s == name) {
            continue;
        }
        if dry_run {
            note(log, format!("· 将恢复服务 {name}"));
        } else if service::start_service_if_stopped(name) {
            note(log, format!("· 已恢复服务 {name}"));
        }
    }

    let total = total_cores().min(64);
    let all_mask = if st.cores > 0 && total > 0 {
        Some(mask_msb_first(total, total) as usize)
    } else {
        None
    };

    if dry_run {
        match all_mask {
            Some(m) => note(
                log,
                format!("· 将把当前会话所有进程优先级复位 NORMAL，亲和性 = {m:#x}（全 {total} 核）"),
            ),
            None => note(log, "· 将把当前会话所有进程优先级复位 NORMAL"),
        }
        if let Some(prev) = st.prev_plan.clone() {
            note(log, format!("· 将还原电源方案 {prev}"));
        }
        note(log, "预演结束（未做任何修改）");
        return;
    }

    let mut touched = 0usize;
    for p in process::list_processes(true) {
        unsafe {
            let h = ffi::OpenProcess(ffi::PROCESS_ALL_ACCESS, 0, p.pid);
            if h.is_null() {
                continue;
            }
            ffi::SetPriorityClass(h, ffi::NORMAL_PRIORITY_CLASS);
            if let Some(m) = all_mask {
                ffi::SetProcessAffinityMask(h, m);
            }
            ffi::CloseHandle(h);
            touched += 1;
        }
    }
    note(log, format!("· 已把 {touched} 个进程的优先级复位为 NORMAL"));

    if st.plan_switched {
        if let Some(prev) = st.prev_plan.clone() {
            if power::set_active(&prev) {
                note(log, format!("· 电源方案已还原 {prev}"));
            } else {
                note(log, format!("· 电源方案还原失败 {prev}"));
            }
        }
    }

    st.optimized = false;
    st.stopped_services.clear();
    st.plan_switched = false;
    st.prev_plan = None;
    note(log, "还原完成");
}

pub fn total_cores() -> u32 {
    unsafe {
        let n = ffi::GetActiveProcessorCount(0xFFFF);
        if n == 0 {
            1
        } else {
            n
        }
    }
}

/// 生成 length 个字符、前 active 个为 1 的掩码，按"首字符为最高位"解释成整数。
fn mask_msb_first(length: u32, active: u32) -> u64 {
    let mut mask: u64 = 0;
    for i in 0..length {
        let bit = if i < active { 1u64 } else { 0u64 };
        mask = (mask << 1) | bit;
    }
    mask
}
