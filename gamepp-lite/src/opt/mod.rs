//! Windows 优化模块（由 `gamepp-winopt` 合并进来）。
//!
//! 结构沿用原工程：`ffi` 是最小 Win32 声明，`plan` 是用户可勾选的优化项，
//! `exec` 是执行/还原流程，其余（process/service/power/state/elevate）是它调用的底层能力。
//!
//! 与独立命令行版的唯一区别：日志不再 `println!`，而是回传给 GUI 展示。

pub mod elevate;
pub mod exec;
pub mod ffi;
pub mod plan;
pub mod power;
pub mod process;
pub mod service;
pub mod state;

pub use exec::{restore, run};
pub use plan::OptPlan;
pub use state::State;

/// 一次执行的结果。
pub struct OptReport {
    pub lines: Vec<String>,
    pub state: State,
}

/// 按当前清单执行一次优化（`dry_run` 只产出计划文本）。
pub fn run_plan(plan: &OptPlan, game_pid: u32, dry_run: bool) -> OptReport {
    let mut lines = Vec::new();
    let st = run(plan, game_pid, dry_run, &mut lines);
    OptReport { lines, state: st }
}

/// 按状态文件里的记录还原。
pub fn restore_plan(plan: &OptPlan, dry_run: bool) -> OptReport {
    let mut st = State::load();
    let mut lines = Vec::new();
    restore(plan, &mut st, dry_run, &mut lines);
    OptReport { lines, state: st }
}

pub fn is_elevated() -> bool {
    elevate::is_elevated()
}

/// 以管理员身份重启本程序（UAC）。
pub fn relaunch_as_admin() -> elevate::Relaunch {
    elevate::relaunch("")
}
