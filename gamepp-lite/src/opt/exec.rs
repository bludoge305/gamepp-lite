//! 优化执行流程。
//!
//! `run` 严格保留原版 `GamePPOptimize.node!sub_10013f70` 的动作顺序（逐条比对过）：
//!   ① 清 DNS → ② 关 Wallpaper Engine → ③ 停后台进程 → ④ 释放内存 →
//!   ⑤ 停服务 → ⑥ 电源方案 → ⑦ 无条件 lodctr /R → ⑧ 清剪贴板 → ⑨ 优先级
//!
//! `restore` 对应 `sub_1000d920` 里的 `[重读配置 → sub_100141e0(0) → sub_100177f0]`：
//!   只重启自己停过的服务 → 全会话优先级复位 NORMAL → 电源方案还原。
//!   在原版之外补了一步：重新启动被「关闭 Wallpaper Engine」结束的进程。
//!
//! 与原版的差异只有一处：所有输出走调用方给的 `log`（Vec<String>），因为 GUI 没有控制台。
//!
//! **安全边界（原版没有）**：每个会碰到"别人的进程/服务"的动作都先过
//! `domain::protect` 白名单（内核、系统管理工具、外壳、安全组件、系统目录、核心服务），
//! 命中即跳过，并在日志里写明跳过了多少个、因为什么。见 `note_skipped`。

use super::ffi;
use super::plan::OptPlan;
use super::power;
use super::process;
use super::service::{self, StopOutcome};
use super::state::State;
use crate::domain::protect;

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

/// 把「被白名单挡下的进程」压成一行日志：数量 + 原因 + 前几个名字。
///
/// 这一步不是废话：用户勾了"关闭后台进程"却看不到某进程被结束，必须能在日志里
/// 追到"它被安全策略跳过了"，否则会被当成 bug 或误以为动作生效。
fn note_skipped(log: &mut Vec<String>, scope: &str, skipped: &[(String, &'static str)]) {
    if skipped.is_empty() {
        return;
    }
    let mut reasons: Vec<&str> = Vec::new();
    // 同名进程（几十个 svchost）合并成 "svchost.exe×52"，否则一行日志全是重复名字。
    let mut rows: Vec<(String, usize)> = Vec::new();
    for (name, reason) in skipped {
        if !reasons.contains(reason) {
            reasons.push(reason);
        }
        match rows.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(r) => r.1 += 1,
            None => rows.push((name.clone(), 1)),
        }
    }
    let shown: Vec<String> = rows
        .iter()
        .take(3)
        .map(|(n, c)| if *c > 1 { format!("{n}×{c}") } else { n.clone() })
        .collect();
    let tail = if rows.len() > shown.len() { "…" } else { "" };
    note(
        log,
        format!(
            "· {scope}：跳过 {} 个受保护组件（{}）{}{}",
            skipped.len(),
            reasons.join("、"),
            shown.join("、"),
            tail
        ),
    );
}

/// 执行一次优化，返回新的运行状态（调用方负责落盘）。
pub fn run(plan: &OptPlan, game_pid: u32, dry_run: bool, log: &mut Vec<String>) -> State {
    let mut st = State {
        optimized: !dry_run,
        game_pid,
        ..Default::default()
    };

    // 上一次优化还没还原就再次执行时，**不能**把旧状态丢掉：那些服务/进程/电源方案
    // 仍然是我们改的，丢了就再也还原不回来。这里把旧记录并进来再做增量。
    if !dry_run {
        let prev = State::load();
        if prev.optimized {
            st.stopped_services = prev.stopped_services;
            st.killed_wallpaper = prev.killed_wallpaper;
            st.plan_switched = prev.plan_switched;
            st.prev_plan = prev.prev_plan;
            note(log, "· 检测到上一次优化尚未还原，本次在原记录上继续累加");
        }
    }
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

    // ② 关闭 Wallpaper Engine（按子串 "wallpaper" 终止；终止前记录 exe 路径，还原时重启）
    if plan.wallpaper_engine {
        if dry_run {
            note(log, "· 关闭 Wallpaper Engine（预演不执行；还原时会重新启动它们）");
        } else {
            let outcome = process::kill_matching_recording(&["wallpaper".to_string()], true);
            if outcome.killed.is_empty() && outcome.skipped.is_empty() {
                note(log, "· 未发现 Wallpaper Engine 进程");
            }
            for (name, pid, path) in outcome.killed {
                note(log, format!("· 已结束 {name}（{pid}）"));
                // 只记录拿得到完整路径的进程，否则还原时无从启动。
                if !path.is_empty() {
                    st.killed_wallpaper.push(path);
                }
            }
            // 子串匹配天然会扫到系统组件（名字里带 wallpaper 的驱动/服务宿主也可能命中），
            // 所以这里同样要报出被白名单挡下的对象。
            note_skipped(log, "关闭 Wallpaper Engine", &outcome.skipped);
        }
    }

    // ③ 关闭影响流畅性的后台进程（重新刷新进程表，与原版一致）
    if plan.kill_background {
        if dry_run {
            // 预演也真的扫一遍（只读）：用户能提前看到"会杀几个、跳几个"。
            let patterns = process::kill_list();
            let hits: Vec<_> = process::list_processes(true)
                .into_iter()
                .filter(|p| process::match_any(&p.name, &patterns).is_some())
                .collect();
            let guarded = hits
                .iter()
                .filter(|p| process::protected_reason(p.pid, &p.name).is_some())
                .count();
            note(
                log,
                format!(
                    "· 关闭后台进程（预演不执行，黑名单 48 条）：命中 {} 个，其中 {} 个受保护组件将跳过",
                    hits.len(),
                    guarded
                ),
            );
        } else {
            let patterns = process::kill_list();
            let outcome = process::kill_matching(&patterns, true);
            if outcome.killed.is_empty() {
                note(log, "· 没有命中黑名单的后台进程");
            }
            for (name, pid, _) in outcome.killed {
                note(log, format!("· 已结束后台进程 {name}（{pid}）"));
            }
            note_skipped(log, "关闭后台进程", &outcome.skipped);
        }
    }

    // ④ 释放物理内存：对全部进程 EmptyWorkingSet（原版不过滤会话）
    //
    // 白名单在这里尤其关键：原版对**所有**进程（含 lsass/svchost/Defender）裁剪工作集，
    // 系统组件随后要重新缺页调入，反而更卡。现在只处理非系统进程。
    if plan.free_memory {
        if dry_run {
            // 预演只读扫描：统计"将处理多少、跳过多少"，不做任何裁剪。
            let mut would = 0usize;
            let mut skipped = 0usize;
            for p in process::list_processes(false) {
                if protect::name_group(&p.name).is_some()
                    || protect::path_reason(&process::exe_path(p.pid).unwrap_or_default()).is_some()
                {
                    skipped += 1;
                } else {
                    would += 1;
                }
            }
            note(
                log,
                format!("· 释放内存（预演不执行）：将处理 {would} 个进程，跳过 {skipped} 个受保护组件"),
            );
        } else {
            let mut ok = 0usize;
            let mut skipped: Vec<(String, &'static str)> = Vec::new();
            for p in process::list_processes(false) {
                // 名字命中白名单：连句柄都不用开。
                if let Some(g) = protect::name_group(&p.name) {
                    skipped.push((p.name, g.label));
                    continue;
                }
                unsafe {
                    let h = ffi::OpenProcess(ffi::PROCESS_ALL_ACCESS, 1, p.pid);
                    if h.is_null() {
                        continue;
                    }
                    // 名字没命中，再按安装路径判一次（System32 / WindowsApps 里的组件）。
                    let path = process::exe_path_of_handle(h).unwrap_or_default();
                    if let Some(label) = protect::path_reason(&path) {
                        skipped.push((p.name, label));
                        ffi::CloseHandle(h);
                        continue;
                    }
                    ffi::SetProcessWorkingSetSize(h, usize::MAX, usize::MAX);
                    ffi::K32EmptyWorkingSet(h);
                    ffi::CloseHandle(h);
                    ok += 1;
                }
            }
            note(log, format!("· 已回收 {ok} 个进程的工作集"));
            note_skipped(log, "释放内存", &skipped);
        }
    }

    // ⑤ 停服务
    for (name, enabled) in plan.services() {
        if !enabled {
            continue;
        }
        // 服务白名单：核心子系统/网络/安全类服务停掉会破坏系统，直接跳过。
        if protect::service_protected(name) {
            note(log, format!("· 跳过受保护服务 {name}（系统组件白名单）"));
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

    // ⑨ 优先级
    let priority_changed = apply_priority(plan, game_pid, dry_run, log);

    // 只有"确实留下了可还原的改动"才算已优化。否则全不勾选也会把状态标成已优化，
    // 游戏退出时触发自动还原 → 还原会把全会话进程优先级压回 NORMAL（有外部副作用）。
    let need_restore = !st.stopped_services.is_empty()
        || st.plan_switched
        || !st.killed_wallpaper.is_empty()
        || priority_changed > 0;
    st.optimized = !dry_run && need_restore;

    if !dry_run {
        if plan.enabled_count() == 0 {
            note(log, "· 没有可执行的动作项，未改动可还原状态");
        } else {
            note(
                log,
                format!(
                    "完成：共处理 {} 项动作，其中 {} 个进程优先级已调整",
                    plan.enabled_count(),
                    priority_changed
                ),
            );
        }
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
///
/// **本工程刻意加的安全边界**：命中特征表还要再过 `domain::protect` 白名单。
/// 原版把"名字命中即提优先级"无差别套用到当前会话，而特征表里有很短的通用子串，
/// 实测会命中 `svchost.exe` / `explorer.exe` 一类系统进程 —— 给它们提优先级没有收益，
/// 反而可能拖慢整机，所以白名单内的一律跳过并计数。
fn apply_priority(plan: &OptPlan, game_pid: u32, dry_run: bool, log: &mut Vec<String>) -> usize {
    let procs = process::list_processes(true);
    let mut skipped: Vec<(String, &'static str)> = Vec::new();
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

    let mut touched = 0usize;
    let mut changed = 0usize;
    for p in &procs {
        if process::match_any(&p.name, &patterns).is_none() {
            continue;
        }
        // 白名单：PID 0/4、系统关键进程、系统目录下的组件、本程序自身。
        if let Some(reason) = process::protected_reason(p.pid, &p.name) {
            skipped.push((p.name.clone(), reason));
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
                    if !dry_run && ffi::SetPriorityClass(h, ffi::BELOW_NORMAL_PRIORITY_CLASS) != 0 {
                        changed += 1;
                    }
                    note(log, format!("· 特殊标题，降为 BELOW_NORMAL：{}（{}）", p.name, p.pid));
                } else if !helper_running {
                    if !dry_run && ffi::SetPriorityClass(h, target) != 0 {
                        changed += 1;
                    }
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
            ffi::CloseHandle(h);
            touched += 1;
        }
    }
    if helper_running {
        note(log, "· 检测到 cpucores 辅助进程在运行，本次不修改优先级（原版行为）");
    }
    note_skipped(log, "调整优先级", &skipped);
    if touched == 0 {
        note(log, "· 没有可处理的游戏进程，优先级未改动");
    }
    changed
}

/// 还原：只重启自己停过的服务 → 重启被结束的 Wallpaper Engine → 全会话优先级复位 → 电源方案还原。
///
/// 返回 `true` 表示还原完整成功。只有全部成功才清空状态文件 —— 否则保留"仍未完成"
/// 的那部分（服务名、电源方案），让用户再点一次「还原」时只重试剩下的，而不是
/// 报一句"还原完成"把失败掩盖掉。
pub fn restore(st: &mut State, dry_run: bool, log: &mut Vec<String>) -> bool {
    note(
        log,
        if dry_run {
            "开始预演还原（不会修改系统）"
        } else {
            "开始还原"
        },
    );

    // 没有可还原内容时直接返回：否则「还原」会退化成"把全会话进程优先级压回 NORMAL"，
    // 把用户自己设成 HIGH 的进程（OBS、渲染器等）一起改掉。
    if !st.optimized {
        note(log, "没有可还原的记录（状态文件为空或已还原）");
        return true;
    }

    // 以状态文件记录的「我们停过谁」为准，而不是当前清单：用户可能在优化之后
    // 取消勾选了某个服务，若按清单遍历，那个服务就永远起不来了。
    let mut still_stopped: Vec<String> = Vec::new();

    for name in st.stopped_services.clone() {
        if dry_run {
            note(log, format!("· 将恢复服务 {name}"));
            continue;
        }
        if service::ensure_running(&name) {
            note(log, format!("· 已恢复服务 {name}"));
        } else {
            note(log, format!("· 恢复服务失败（权限不足或服务被禁用）{name}"));
            still_stopped.push(name);
        }
    }

    // 重新启动被「关闭 Wallpaper Engine」结束的进程（原版不做这一步，本工程补上）。
    restore_wallpaper(st, dry_run, log);

    if dry_run {
        note(log, "· 将把当前会话所有进程优先级复位 NORMAL");
        if let Some(prev) = st.prev_plan.clone() {
            note(log, format!("· 将还原电源方案 {prev}"));
        }
        note(log, "预演结束（未做任何修改）");
        return true;
    }

    let mut touched = 0usize;
    let mut skipped: Vec<(String, &'static str)> = Vec::new();
    for p in process::list_processes(true) {
        // 复位同样属于"改动别人"，系统组件一律不碰：系统进程的优先级由系统自己调度。
        if let Some(reason) = process::protected_reason(p.pid, &p.name) {
            skipped.push((p.name, reason));
            continue;
        }
        unsafe {
            let h = ffi::OpenProcess(ffi::PROCESS_ALL_ACCESS, 0, p.pid);
            if h.is_null() {
                continue;
            }
            ffi::SetPriorityClass(h, ffi::NORMAL_PRIORITY_CLASS);
            ffi::CloseHandle(h);
            touched += 1;
        }
    }
    note(log, format!("· 已把 {touched} 个进程的优先级复位为 NORMAL"));
    note_skipped(log, "优先级复位", &skipped);

    let mut power_ok = true;
    if st.plan_switched {
        if let Some(prev) = st.prev_plan.clone() {
            if power::set_active(&prev) {
                note(log, format!("· 电源方案已还原 {prev}"));
            } else {
                note(log, format!("· 电源方案还原失败 {prev}"));
                power_ok = false;
            }
        }
    }

    if still_stopped.is_empty() && power_ok {
        st.optimized = false;
        st.stopped_services.clear();
        st.killed_wallpaper.clear();
        st.plan_switched = false;
        st.prev_plan = None;
        note(log, "还原完成");
        true
    } else {
        // 保留未完成的部分，供下次重试；optimized 仍为 true，界面会继续显示「已优化」。
        st.stopped_services = still_stopped;
        note(
            log,
            "还原未完全成功：仍显示「已优化」，可再点一次「还原」重试剩余项",
        );
        false
    }
}

/// 重启被结束的 Wallpaper Engine 进程。
///
/// 规则：
/// - 记录的是 exe 完整路径（终止前用 QueryFullProcessImageNameW 取的）；
/// - 文件已不存在（比如游戏期间被卸载/移动）→ 跳过；
/// - 同名进程已在运行（用户自己又开了，或它有守护进程）→ 不再重复启动；
/// - 启动走 ShellExecuteW("open")，不等待、不提权。
///
/// 返回"仍未恢复成功"的路径，调用方在还原不完整时写回状态文件，供下次重试。
fn restore_wallpaper(st: &State, dry_run: bool, log: &mut Vec<String>) -> Vec<String> {
    if st.killed_wallpaper.is_empty() {
        return Vec::new();
    }
    if dry_run {
        note(
            log,
            format!(
                "· 将尝试重启 {} 个被结束的 Wallpaper Engine 进程",
                st.killed_wallpaper.len()
            ),
        );
        return Vec::new();
    }

    let mut ok = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for path in &st.killed_wallpaper {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() || !std::path::Path::new(path).is_file() {
            // 文件已经不存在，重试也没有意义：直接丢弃这条记录。
            note(log, format!("· 无法恢复（文件不存在）：{path}"));
            continue;
        }
        if process::is_running(&name) {
            note(log, format!("· 已在运行，无需恢复：{name}"));
            ok += 1;
            continue;
        }
        if process::launch_detached(path) {
            note(log, format!("· 已重新启动 {name}（{path}）"));
            ok += 1;
        } else {
            note(log, format!("· 启动失败：{path}"));
            failed.push(path.clone());
        }
    }
    if ok + failed.len() > 0 {
        note(
            log,
            format!(
                "· Wallpaper Engine 恢复：成功 {ok}，待重试 {}",
                failed.len()
            ),
        );
    }
    failed
}
