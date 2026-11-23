//! gamepp-winopt — GamePP「游戏优化 / Windows 优化」模块的独立 Rust 复刻。
//!
//! 用法：
//!   gamepp-winopt start [--game-id N] [--pid N] [--cores N] [--pow PATH] [--ini PATH] [--dry-run]
//!   gamepp-winopt stop  [--ini PATH]
//!   gamepp-winopt status [--ini PATH]
//!
//! 与原始模块的对应关系见 report/gamepp-winopt-reverse-report.md。

mod config;
mod elevate;
mod ffi;
mod optimize;
mod power;
mod process;
mod service;
mod state;

/// 受保护白名单（系统组件 / 系统目录 / 核心服务）。
///
/// 与 GUI 版 `gamepp-lite/src/domain/protect.rs` **共用同一份源文件**（`#[path]` 引入），
/// 避免两份名单各改各的、逐渐漂移 —— 名单一旦不一致，安全边界就形同虚设。
/// 该文件只用标准库，无 crate 内部依赖，所以可以这样复用。
#[path = "../../gamepp-lite/src/domain/protect.rs"]
#[allow(dead_code)] // CLI 只用到其中一部分判定函数；同一份源文件在 GUI 里用得更多。
mod protect;

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

const MUTEX_NAME: &str = "gamePP_opti"; // 原版 0x10048E54 处的宽字符串

struct Args {
    cmd: String,
    ini: Option<String>,
    game_id: u32,
    pid: u32,
    cores: u32,
    pow_path: Option<String>,
    dry_run: bool,
    /// 由提权后的子进程携带，避免无限递归提权
    elevated: bool,
    /// 明确要求不提权（诊断用）
    no_elevate: bool,
    /// 结束前等一次回车（提权后的新窗口需要，否则输出一闪而过）
    pause: bool,
}

fn parse_args(argv: &[String]) -> Args {
    let mut a = Args {
        cmd: String::new(),
        ini: None,
        game_id: 0,
        pid: 0,
        cores: 0,
        pow_path: None,
        dry_run: false,
        elevated: false,
        no_elevate: false,
        pause: false,
    };
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let next = |i: &mut usize| -> String {
            *i += 1;
            argv.get(*i).cloned().unwrap_or_default()
        };
        match arg {
            "start" | "stop" | "status" => {
                if a.cmd.is_empty() {
                    a.cmd = arg.to_string();
                }
            }
            "help" | "-h" | "--help" | "/?" | "-?" => {
                if a.cmd.is_empty() {
                    a.cmd = "help".to_string();
                }
            }
            "--ini" => a.ini = Some(next(&mut i)),
            "--game-id" => a.game_id = next(&mut i).parse().unwrap_or(0),
            "--pid" => a.pid = next(&mut i).parse().unwrap_or(0),
            "--cores" => a.cores = next(&mut i).parse().unwrap_or(0),
            "--pow" => a.pow_path = Some(next(&mut i)),
            "--dry-run" => a.dry_run = true,
            "--elevated" => a.elevated = true,
            "--no-elevate" => a.no_elevate = true,
            "--pause" => a.pause = true,
            other => eprintln!("忽略未知参数: {}", other),
        }
        i += 1;
    }
    if a.cmd.is_empty() {
        a.cmd = "help".to_string();
    }
    a
}

fn usage() {
    println!(
        "gamepp-winopt — GamePP「Windows 优化」模块的 Rust 复刻\n\
         \n\
         用法:\n\
         \x20 gamepp-winopt start  [--game-id N] [--pid N] [--cores N] [--pow PATH] [--ini PATH] [--dry-run]\n\
         \x20 gamepp-winopt stop   [--ini PATH]\n\
         \x20 gamepp-winopt status [--ini PATH]\n\
         \n\
         说明:\n\
         \x20 配置读取 %LOCALAPPDATA%\\GamePP\\GamePPOpt.ini 的 [SYS] 节，与原模块一致。\n\
         \x20 start / stop 需要管理员权限：未提权时会自动弹 UAC 并拉起自身副本。\n\
         \x20 --no-elevate 跳过自动提权（诊断用）；--elevated 为内部标志，请勿手动指定。\n\
         \x20 --cores 对应原版 setCPUValue 设定的核心数；0 表示不做亲和性改动。"
    );
}

fn pause_if_needed(force: bool) {
    if force || std::io::stdin().is_terminal() {
        print!("\n按回车键退出...");
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    // 诊断：用 "open" 谓词走一遍 ShellExecuteExW + 等待 + 取退出码，不弹 UAC
    if argv.iter().any(|a| a == "--self-test-elevate") {
        let cmd = std::path::PathBuf::from(r"C:\Windows\System32\cmd.exe");
        match elevate::launch_wait("open", &cmd, "/c exit 7") {
            elevate::Relaunch::Finished(c) => {
                println!("self-test: 子进程退出码 = {}（期望 7）", c);
                return if c == 7 { ExitCode::SUCCESS } else { ExitCode::FAILURE };
            }
            other => {
                println!("self-test 失败: {:?}", other);
                return ExitCode::FAILURE;
            }
        }
    }

    let args = parse_args(&argv);
    if args.cmd == "help" {
        usage();
        return ExitCode::SUCCESS;
    }

    // ---- 自动提权 ----------------------------------------------------------
    // start / stop 需要改服务与别的进程；--dry-run 只打印，不需要提权。
    let needs_admin = matches!(args.cmd.as_str(), "start" | "stop") && !args.dry_run;
    if needs_admin && !args.elevated && !args.no_elevate && !elevate::is_elevated() {
        // stop 且没有进行中的优化时，先别弹 UAC
        if args.cmd == "stop" && !state::State::load().optimized {
            println!("没有进行中的优化（状态文件不存在或已还原），无需操作。");
            return ExitCode::SUCCESS;
        }

        println!("需要管理员权限，正在请求提权...");
        let mut child: Vec<String> = argv.clone();
        child.push("--elevated".to_string());
        if std::io::stdin().is_terminal() {
            // 提权会开出新的控制台窗口，不暂停的话输出一闪而过
            child.push("--pause".to_string());
        }
        let code = match elevate::relaunch(&elevate::join_args(&child)) {
            elevate::Relaunch::Finished(c) => c,
            elevate::Relaunch::Cancelled => {
                eprintln!("已取消提权，未做任何操作。");
                return ExitCode::FAILURE;
            }
            elevate::Relaunch::Failed(e) => {
                eprintln!("提权失败: {}", e);
                eprintln!("可以右键「以管理员身份运行」，或加 --no-elevate 以当前权限执行（多数步骤会失败）。");
                return ExitCode::FAILURE;
            }
        };
        println!("管理员进程已结束，退出码 {}", code);
        return if code == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    let code = run(&args);
    pause_if_needed(args.pause);
    code
}

fn run(args: &Args) -> ExitCode {
    let ini = args.ini.clone().unwrap_or_else(config::default_ini_path);
    let mut cfg = config::Config::load(&ini);
    println!("配置文件: {}", ini);
    println!("权限: {}", if elevate::is_elevated() { "管理员" } else { "普通用户（部分操作会失败）" });

    match args.cmd.as_str() {
        "status" => {
            println!("\nWindows 优化开关:");
            for (name, on) in cfg.ui_items() {
                println!("  [{}] {}", if on { "x" } else { " " }, name);
            }
            println!("\n扩展开关:");
            println!("  [{}] SystemLimitCore / 限制核心", if cfg.limit_core { "x" } else { " " });
            println!("  [{}] SystemWindowsAero / uxsms", if cfg.windows_aero { "x" } else { " " });
            println!("  [{}] SystemWindowsUpdate / wuauserv", if cfg.windows_update { "x" } else { " " });
            println!("  [{}] SystemDiagtrack / DiagTrack", if cfg.diag_track { "x" } else { " " });
            println!("  [{}] SystemDiagsvc / Diagsvc", if cfg.diag_svc { "x" } else { " " });
            println!("  [{}] SystemHomeGroupProvider", if cfg.homegroup_provider { "x" } else { " " });
            println!("  [{}] SystemHomeGroupListener", if cfg.homegroup_listener { "x" } else { " " });
            println!("  [ ] SysMain（原版配置字节 +0x08 从不写入，恒为关）");

            let st = state::State::load();
            println!("\n运行状态:");
            println!("  optimized = {}", st.optimized);
            println!("  cores     = {}", st.cores);
            println!("  stopped   = {:?}", st.stopped_services);
            println!("  prev_plan = {:?}", st.prev_plan);
            if let Some(cur) = power::active_scheme() {
                println!("  当前电源方案 = {}", cur);
            }
            ExitCode::SUCCESS
        }
        "start" => {
            let m = lock_mutex();
            if m.is_none() {
                println!("另一个优化流程正在运行（等待 1 秒超时），本次不做任何操作。");
                return ExitCode::SUCCESS; // 原版同样返回 true
            }
            // 显式 --pow 优先：用户给了文件就按"要切电源"处理，不再被 INI 里的
            // SystemAIPower=0 静默吞掉（旧实现会忽略 --pow，且不给任何提示）。
            if args.pow_path.is_some() && !cfg.ai_power {
                cfg.ai_power = true;
                println!("提示：已指定 --pow，本次按「切换电源方案」处理（覆盖 INI 的 SystemAIPower=0）");
            }
            // 未指定 --pow 时交给 power::resolve_pow 自动定位游戏加加的 PowerData
            let pow = args.pow_path.clone();
            let opts = optimize::BeginOptions {
                game_id: args.game_id,
                game_pid: args.pid,
                cores: args.cores,
                pow_path: pow,
                dry_run: args.dry_run,
            };
            let st = optimize::begin(&cfg, &opts);
            if !args.dry_run {
                st.save();
                println!("状态已保存: {}", state::state_path().display());
            }
            unlock_mutex(m);
            ExitCode::SUCCESS
        }
        "stop" => {
            let mut st = state::State::load();
            if !st.optimized {
                println!("没有进行中的优化（状态文件不存在或已还原），无需操作。");
                return ExitCode::SUCCESS;
            }
            optimize::restore(&cfg, &mut st, args.dry_run);
            if args.dry_run {
                println!("[dry-run] 未修改任何进程/服务，状态文件保持不变。");
            } else {
                state::State::clear();
                println!("已还原，状态文件已删除。");
            }
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("未知子命令: {}", other);
            usage();
            ExitCode::FAILURE
        }
    }
}

/// 建立/获取 "gamePP_opti" 互斥体，1 秒超时；返回句柄用于最后释放。
fn lock_mutex() -> Option<ffi::HANDLE> {
    let name = ffi::wide(MUTEX_NAME);
    unsafe {
        let h = ffi::CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if h.is_null() {
            return None;
        }
        let r = ffi::WaitForSingleObject(h, 1000);
        if r == ffi::WAIT_TIMEOUT {
            ffi::CloseHandle(h);
            return None;
        }
        Some(h)
    }
}

fn unlock_mutex(h: Option<ffi::HANDLE>) {
    if let Some(h) = h {
        unsafe {
            ffi::ReleaseMutex(h);
            ffi::CloseHandle(h);
        }
    }
}
