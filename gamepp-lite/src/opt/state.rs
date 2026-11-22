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

use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
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
    /// 被提权的游戏 pid（便于诊断）。
    pub game_pid: u32,
}

pub fn state_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("GamePP-Lite").join("opt-state.ini")
}

impl State {
    pub fn load() -> State {
        let mut st = State::default();
        let path = state_path();
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return st,
        };
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
                "stopped_services" => st.stopped_services = split_list(v),
                // 记录用 ';' 分隔（Windows 路径基本不含 ';'）；也容忍 ','。
                "killed_wallpaper" => st.killed_wallpaper = split_list(v),
                _ => {}
            }
        }
        st
    }

    pub fn save(&self) {
        let path = state_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let body = format!(
            "; GamePP-Lite Windows-optimize state (generated)\n\
             optimized={}\n\
             game_pid={}\n\
             plan_switched={}\n\
             prev_plan={}\n\
             stopped_services={}\n\
             killed_wallpaper={}\n",
            if self.optimized { 1 } else { 0 },
            self.game_pid,
            if self.plan_switched { 1 } else { 0 },
            self.prev_plan.clone().unwrap_or_default(),
            self.stopped_services.join(","),
            self.killed_wallpaper.join(";")
        );
        let _ = fs::write(&path, body);
    }

    pub fn clear() {
        if let Some(dir) = state_path().parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::remove_file(state_path());
    }
}

fn split_list(v: &str) -> Vec<String> {
    v.split([',', ';'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
