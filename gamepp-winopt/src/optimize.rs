//! Windows 优化模块的主流程。
//!
//! `begin` 严格对应 GamePPOptimize.node!sub_10013f70（逐条指令校对过的顺序），
//! `restore` 对应 sub_1000d920 里 [重新读配置 → sub_100141e0(0) → sub_100177f0] 那一段。

use crate::config::Config;
use crate::ffi;
use crate::power;
use crate::process;
use crate::protect;
use crate::service::{self, StopOutcome};
use crate::state::State;

/// 标题 id（原版 sub_10016580 用到的 4 个常量）：对这些进程不升优先级，
/// 反而先降到 BELOW_NORMAL，避免与反作弊/启动器抢调度。
pub const SPECIAL_GAME_IDS: [u32; 4] = [0x1F5, 0x1FC, 0x1FD, 0x20B];

/// 优化过程中自检用的两个特征串（原版 sub_10016580 检测 "cpucores" / "cpucoresspecs"）。
const SELF_HELPER_A: &str = "cpucores";
const SELF_HELPER_B: &str = "cpucoresspecs";

pub struct BeginOptions {
    pub game_id: u32,
    pub game_pid: u32,
    /// 用户设定的核心数（原版 0x1005a87c）。0 = 未设定，不做亲和性改动。
    pub cores: u32,
    /// 电源方案 .pow 文件路径；None 表示不切换。
    pub pow_path: Option<String>,
    pub dry_run: bool,
}

fn winexec(cmd: &str) {
    let mut b = cmd.as_bytes().to_vec();
    b.push(0);
    unsafe {
        ffi::WinExec(b.as_ptr(), 0);
    }
}

/// 执行一次 Windows 优化。返回新的运行状态（调用方负责落盘）。
pub fn begin(cfg: &Config, opts: &BeginOptions) -> State {
    let mut st = State {
        optimized: false,
        cores: opts.cores,
        game_pid: opts.game_pid,
        ..Default::default()
    };

    // 上一次 start 还没 stop 就重复执行时，把旧记录并进来：那些服务/电源方案仍是
    // 我们改的，覆盖掉就再也还原不回来。
    if !opts.dry_run {
        let prev = State::load();
        if prev.optimized {
            st.stopped_services = prev.stopped_services;
            st.prev_plan = prev.prev_plan;
            st.plan_switched = prev.plan_switched;
            println!("  - 检测到上一次优化尚未还原，本次在原记录上继续累加");
        }
    }

    println!("[begin] game_id=0x{:x} pid={} cores={} dry_run={}", opts.game_id, opts.game_pid, opts.cores, opts.dry_run);

    // ① [cfg+0x04] 清理 DNS
    if cfg.flush_dns {
        println!("  - 清理 DNS: ipconfig /flushdns");
        if !opts.dry_run {
            winexec("ipconfig /flushdns");
        }
    }

    // ② 原版此处 sub_100145e0(1, gameId) 会改写 PUBG/SCUM/武侠乂 等具体游戏的配置文件，
    //    属于"游戏设置优化"而非 Windows 优化，本实现不包含（见报告 4.3 节）。

    // ③ [cfg+0x0E] 关/开 uxsms（Aero 主题服务），中间固定等待 3 秒
    if cfg.windows_aero {
        println!("  - 关闭 Aero: net stop uxsms → sleep 3000 → net start uxsms");
        if !opts.dry_run {
            winexec("net stop uxsms");
            unsafe { ffi::Sleep(3000) };
            winexec("net start uxsms");
        }
    }

    // ④ [cfg+0x05] 关闭 Wallpaper Engine：先刷新进程表，再按子串 "wallpaper" 终止
    if cfg.wallpaper_engine {
        if opts.dry_run {
            println!("  - 关闭 Wallpaper Engine（dry-run 不执行）");
        } else {
            let killed = process::kill_matching(&["wallpaper".to_string()], true);
            for (name, pid) in &killed {
                println!("  - 终止 Wallpaper Engine 进程: {} ({})", name, pid);
            }
            if killed.is_empty() {
                println!("  - 未发现 Wallpaper Engine 进程");
            }
        }
    }

    // ⑤ [cfg+0x12] 关闭影响流畅性的后台进程（重新刷新进程表，与原版一致）
    if cfg.kill_background {
        if opts.dry_run {
            println!("  - 关闭后台进程（dry-run 不执行）");
        } else {
            let patterns = process::kill_list();
            let killed = process::kill_matching(&patterns, true);
            for (name, pid) in &killed {
                println!("  - 终止后台进程: {} ({})", name, pid);
            }
            if killed.is_empty() {
                println!("  - 无可终止的后台进程");
            }
        }
    }

    // ⑥ [cfg+0x14] 释放物理内存：对**全部**进程（不按会话过滤）+ EmptyWorkingSet
    //
    // 安全边界：受保护白名单内的系统组件不裁剪工作集（原版无差别处理，
    // 会让 lsass/Defender 等随后大量缺页重入，反而更卡）。
    if cfg.ai_memory {
        println!("  - 释放内存: SetProcessWorkingSetSize(-1,-1) + EmptyWorkingSet");
        if !opts.dry_run {
            let mut ok = 0usize;
            let mut skipped = 0usize;
            for p in process::list_processes(false) {
                if process::protected_reason(p.pid, &p.name).is_some() {
                    skipped += 1;
                    continue;
                }
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
            println!(
                "    已处理 {} 个进程（跳过 {} 个受保护组件）",
                ok, skipped
            );
        }
    }

    // ⑦ 停服务（sub_100141e0(1)）
    for (name, enabled) in service::services(cfg) {
        if !enabled {
            continue;
        }
        if protect::service_protected(name) {
            println!("  - 跳过受保护服务: {}（系统组件白名单）", name);
            continue;
        }
        if opts.dry_run {
            println!("  - 停止服务（dry-run）: {}", name);
            st.stopped_services.push(name.to_string());
            continue;
        }
        match service::stop_service(name) {
            StopOutcome::Stopped => {
                println!("  - 已停止服务: {}", name);
                // 只有真正下发过 STOP 的才写标记，还原时也只拉这些
                st.stopped_services.push(name.to_string());
            }
            StopOutcome::NotRunning => println!("  - 服务本来就没运行: {}", name),
            StopOutcome::Missing => println!("  - 服务不存在: {}", name),
            StopOutcome::Failed => println!("  - 停止服务失败（可能权限不足）: {}", name),
        }
    }

    // ⑧ 电源方案（原版由 Power.dll 完成，不在 .node 内；见 power.rs 注释）
    if cfg.ai_power {
        match power::resolve_pow(opts.pow_path.as_deref()) {
            Some(pow) => {
                if opts.dry_run {
                    println!("  - 切换电源方案（dry-run）: {}", pow.display());
                } else if let Some((prev, new_guid)) = power::switch_to_pow(&pow) {
                    println!("  - 电源方案: {} → {}", prev, new_guid);
                    st.prev_plan = Some(prev);
                    st.plan_switched = true;
                } else {
                    println!("  - 切换电源方案失败: {}", pow.display());
                }
            }
            None => {
                // 找不到 .pow 时，若方案此前已被游戏加加导入过，直接按 GUID 激活
                let hint = match &opts.pow_path {
                    Some(p) => format!("指定的 .pow 不存在: {}", p),
                    None => "未在 GamePP 目录下找到 GPP_*.pow".to_string(),
                };
                if opts.dry_run {
                    println!(
                        "  - {}；将尝试直接激活已导入的方案 {}（dry-run）",
                        hint,
                        power::GPP_MAXED_GUID
                    );
                } else if let Some((prev, new_guid)) = power::switch_to_guid(power::GPP_MAXED_GUID) {
                    println!("  - 电源方案（已导入方案直连）: {} → {}", prev, new_guid);
                    st.prev_plan = Some(prev);
                    st.plan_switched = true;
                } else if opts.pow_path.is_some() {
                    println!(
                        "  - {}，且系统里也没有已导入的 GPP 方案；跳过电源切换",
                        hint
                    );
                } else {
                    println!(
                        "  - {}，且系统里也没有已导入的 GPP 方案；跳过电源切换\n    \
                         （用 --pow 指定 GPP_Maxed.pow 的路径，或先由游戏加加导入一次）",
                        hint
                    );
                }
            }
        }
    }

    // ⑨ 无条件重建性能计数器（原版这条 WinExec 没有任何条件保护）
    println!("  - 重建性能计数器: lodctr /R");
    if !opts.dry_run {
        winexec("lodctr /R");
    }

    // ⑩ [cfg+0x02] 清空剪贴板
    if cfg.clean_clipboard {
        println!("  - 清空剪贴板");
        if !opts.dry_run {
            unsafe {
                if ffi::OpenClipboard(std::ptr::null_mut()) != 0 {
                    ffi::EmptyClipboard();
                    ffi::CloseClipboard();
                }
            }
        }
    }

    // ⑪ 优先级 / 亲和性（原版 sub_10016580 → sub_100174a0）
    let priority_changed = apply_priority(cfg, opts);

    // 只有确实留下可还原改动才标记为"已优化"：否则 stop 会退化成把全会话
    // 进程优先级压回 NORMAL（会连带改掉用户自己设成 HIGH 的进程）。
    st.optimized = !opts.dry_run
        && (!st.stopped_services.is_empty() || st.plan_switched || priority_changed > 0);
    st
}

/// 对应 sub_10016580 + sub_100174a0 + sub_10017aa0。
///
/// 关键点（均已从反汇编确认）：
///   * 只看"当前会话"的进程；
///   * 进程名命中 372 条特征表之一才处理；
///   * 若发现自己的 cpucores 辅助进程在跑，本次完全不改优先级；
///   * 当前优先级是 HIGH/REALTIME 的进程不动；
///   * gameId ∈ SPECIAL_GAME_IDS 的标题：改设 BELOW_NORMAL(0x4000)，不设目标优先级；
///   * 其余：SystemGameProPriority=1 → ABOVE_NORMAL(0x8000)，=0 → NORMAL(0x20)。
fn apply_priority(cfg: &Config, opts: &BeginOptions) -> usize {
    let procs = process::list_processes(true);

    let helper_running = procs.iter().any(|p| {
        process::contains_ci(&p.name, SELF_HELPER_A) || process::contains_ci(&p.name, SELF_HELPER_B)
    });
    let special = SPECIAL_GAME_IDS.contains(&opts.game_id);

    let patterns = process::game_patterns();
    let target = if cfg.game_priority {
        ffi::ABOVE_NORMAL_PRIORITY_CLASS
    } else {
        ffi::NORMAL_PRIORITY_CLASS
    };

    // 亲和性掩码（SystemLimitCore）
    let total = total_cores().min(64);
    let limit_mask: Option<usize> = if cfg.limit_core && opts.cores >= 4 && total > 0 && !helper_running
    {
        let active = if opts.cores >= total { total / 2 } else { opts.cores };
        Some(mask_msb_first(total, active) as usize)
    } else {
        None
    };

    let mut changed = 0usize;
    for p in &procs {
        if process::match_any(&p.name, &patterns).is_none() {
            continue;
        }
        // 白名单：系统组件不改优先级、不改亲和性（特征表里有通用短子串，会命中系统进程）。
        if let Some(reason) = process::protected_reason(p.pid, &p.name) {
            println!("  - 跳过优先级调整（受保护组件）: {} ({}) — {}", p.name, p.pid, reason);
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
                    if !opts.dry_run
                        && ffi::SetPriorityClass(h, ffi::BELOW_NORMAL_PRIORITY_CLASS) != 0
                    {
                        changed += 1;
                    }
                    println!("  - 优先级（特殊标题，降为 BELOW_NORMAL）: {} ({})", p.name, p.pid);
                } else if !helper_running {
                    if !opts.dry_run && ffi::SetPriorityClass(h, target) != 0 {
                        changed += 1;
                    }
                    println!(
                        "  - 优先级: {} ({}) → 0x{:x}",
                        p.name, p.pid, target
                    );
                }
            }
            if let Some(mask) = limit_mask {
                if !opts.dry_run && ffi::SetProcessAffinityMask(h, mask) != 0 {
                    changed += 1;
                }
            }
            ffi::CloseHandle(h);
        }
    }
    changed
}

/// 对应 sub_100177f0：还原优先级（并恢复全核心亲和性）。
///
/// 注意这里是**当前会话中每一个能打开的进程**，不只是游戏进程 —— 这是原版行为，
/// 报告 4.4 节有说明与风险评估。
pub fn restore(cfg: &Config, st: &mut State, dry_run: bool) {
    println!("[restore]{}", if dry_run { " (dry-run)" } else { "" });

    // 1) 只重启自己停过的服务
    for (name, _enabled) in service::services(cfg) {
        if !st.stopped_services.iter().any(|s| s == name) {
            continue;
        }
        if dry_run {
            println!("  - 将恢复服务: {}", name);
        } else if service::start_service_if_stopped(name, true) {
            println!("  - 已恢复服务: {}", name);
        }
    }

    // 2) 优先级 / 亲和性复位
    //
    // 对应 sub_100177f0：掩码字符串的长度是**总核心数**（0x1005a87c），全 1 → 恢复全部核心。
    // 早期版本这里误用了"设定核心数"当作宽度，会在核数多于设定值时把进程钉在少数核上。
    let cores = st.cores;
    let total = total_cores().min(64);
    let all_mask = if cores > 0 && total > 0 {
        Some(mask_msb_first(total, total) as usize)
    } else {
        None
    };

    if dry_run {
        match all_mask {
            Some(m) => println!(
                "  - 将把当前会话所有进程的优先级复位为 NORMAL，亲和性掩码 = 0x{:x}（全 {} 核）",
                m, total
            ),
            None => println!("  - 将把当前会话所有进程的优先级复位为 NORMAL（不改亲和性：未记录核心数）"),
        }
        // 3) 电源方案
        if st.plan_switched {
            if let Some(prev) = st.prev_plan.clone() {
                println!("  - 将还原电源方案: {}", prev);
            }
        }
        return;
    }

    let mut touched = 0usize;
    let mut skipped = 0usize;
    for p in process::list_processes(true) {
        // 复位也是"改动别人的进程"：系统组件一律不碰。
        if process::protected_reason(p.pid, &p.name).is_some() {
            skipped += 1;
            continue;
        }
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
    println!(
        "  - 已把 {} 个进程的优先级复位为 NORMAL（跳过 {} 个受保护组件）",
        touched, skipped
    );

    // 3) 电源方案还原
    if st.plan_switched {
        if let Some(prev) = st.prev_plan.clone() {
            if power::set_active(&prev) {
                println!("  - 电源方案已还原: {}", prev);
            } else {
                println!("  - 电源方案还原失败: {}", prev);
            }
        }
    }
}

fn total_cores() -> u32 {
    unsafe {
        let n = ffi::GetActiveProcessorCount(0xFFFF);
        if n == 0 {
            1
        } else {
            n
        }
    }
}

/// 按 sub_10017980 / sub_10017fe0 的语义构造掩码：
/// 生成 length 个字符，前 active 个为 '1'，再按"首字符为最高位"解释成整数。
fn mask_msb_first(length: u32, active: u32) -> u64 {
    let mut mask: u64 = 0;
    for i in 0..length {
        let bit = if i < active { 1u64 } else { 0u64 };
        mask = (mask << 1) | bit;
    }
    mask
}
