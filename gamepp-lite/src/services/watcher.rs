//! 识别服务：前台窗口 → 进程 → 游戏库/启发式 → 状态机。
//!
//! 逆向对照 `GPPPlatform.exe`：它就是"每秒取一次前台窗口，拿到 pid 和映像名，
//! 再去平台上问这是不是游戏"。本工程把"问平台"换成"查本地游戏库 + 图形运行时启发式"，
//! 判定同样带 `stable_polls` 次防抖（对应 `onGameWindowStateChanged` 的稳定态）。
//!
//! 本模块只负责"识别"，不再执行任何优化动作 —— Windows 优化由 ② 页显式触发，
//! 或由 ② 页里的两个自动化开关驱动。

use super::{now_epoch, Config, DetectState, Level, Session, SessionSummary, Shared, UiNotice};
use crate::domain::games::{self, Confidence, GameLibrary};
use crate::domain::protect;
use crate::win::{foreground, process};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 判定"这是不是游戏"。
///
/// 图形运行时由 `foreground::current()` 在采样时一并取好（带缓存），这里只读结果，
/// 不再重复枚举模块。
///
/// 判定顺序（先排除、后识别）：
///   1. 受保护白名单：系统组件（内核/系统管理工具/外壳/安全组件/系统目录里的程序）
///      永远不判为游戏 —— 否则任务管理器最大化、设置全屏之类都会被当成"游戏"，
///      进而触发优化动作；
///   2. 识别黑名单 `domain::games::DENY_LIST`（截图工具等）；
///   3. 游戏库命中；
///   4. 启发式（全屏 + 图形运行时）。
fn classify(
    lib: &GameLibrary,
    fg: &foreground::Foreground,
) -> Option<(String, Confidence, Option<u32>)> {
    if protect::check(fg.pid, &fg.exe, &fg.path).is_some() {
        return None;
    }
    if games::is_denied(&fg.exe) {
        return None;
    }
    if let Some(entry) = lib.lookup(&fg.exe) {
        return Some((entry.name.clone(), Confidence::Matched, entry.title_id));
    }
    // 启发式：全屏 + 加载了图形运行时。对应 GetGameProcessType 的兜底分支。
    if fg.fullscreen && !fg.runtimes.is_empty() {
        let stem = fg
            .exe
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(&fg.exe)
            .to_string();
        return Some((format!("{stem}（启发式识别）"), Confidence::Heuristic, None));
    }
    None
}

pub fn run(shared: Arc<Mutex<Shared>>, cfg: Arc<Mutex<Config>>, tx: Sender<UiNotice>) {
    {
        if let Ok(mut s) = shared.lock() {
            s.running = true;
            s.push_log(Level::Info, "识别服务已启动（前台窗口轮询）", 300);
        }
    }

    loop {
        let (poll_ms, stable, log_cap, history_limit, library) = {
            let c = cfg.lock().unwrap_or_else(|e| e.into_inner());
            (
                c.poll_ms.max(150),
                c.stable_polls.max(1),
                c.log_limit.max(20),
                c.history_limit.max(1),
                c.library.clone(),
            )
        };
        std::thread::sleep(Duration::from_millis(poll_ms));

        let fg = foreground::current();
        let mut entered: Option<(u32, String)> = None;
        let mut exited = false;

        {
            // 锁中毒（某个持锁线程 panic）不应该让识别线程永久退出：
            // 取回内部数据继续跑，比"UI 显示运行中、实际早已停摆"要好。
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.polls += 1;
            s.foreground = fg.clone();
            // 受保护判定放在状态机之前：命中即"这条前台窗口不参与识别"，
            // 同时把原因留给 UI 展示（用户能看到"为什么没识别成游戏"）。
            let guard = fg
                .as_ref()
                .and_then(|f| protect::check(f.pid, &f.exe, &f.path));
            let prev_guard = s.protected;
            s.protected = guard;
            if guard.is_some() && guard != prev_guard {
                if let Some(f) = &fg {
                    s.push_log(
                        Level::Info,
                        format!(
                            "前台窗口 {} 属于受保护系统组件（{}），已跳过识别与优化",
                            f.exe,
                            guard.unwrap_or_default()
                        ),
                        log_cap,
                    );
                }
            }
            let state = s.detect.clone();

            match state {
                DetectState::Idle => {
                    if let Some(f) = &fg {
                        if classify(&library, f).is_some() {
                            s.detect = DetectState::Candidate {
                                pid: f.pid,
                                exe: f.exe.clone(),
                                ticks: 1,
                            };
                        }
                    }
                }
                DetectState::Candidate { pid, exe, ticks } => {
                    let same = fg.as_ref().map(|f| f.pid) == Some(pid);
                    let hit = if same {
                        fg.as_ref().and_then(|f| classify(&library, f))
                    } else {
                        None
                    };
                    match (same, hit) {
                        (true, Some((name, conf, game_id))) if ticks + 1 >= stable => {
                            let started = now_epoch();
                            s.detect = DetectState::InGame {
                                pid,
                                exe: exe.clone(),
                                name: name.clone(),
                                confidence: conf.clone(),
                                since: started,
                                game_id,
                            };
                            s.session = Some(Session::new(exe.clone(), name.clone(), started));
                            if conf == Confidence::Heuristic {
                                s.push_log(
                                    Level::Warn,
                                    format!(
                                        "{exe} 未在游戏库中，按「全屏 + 图形运行时」判定为疑似游戏"
                                    ),
                                    log_cap,
                                );
                            }
                            s.push_log(
                                Level::Info,
                                format!("识别到游戏：{name}（{exe} · PID {pid}）"),
                                log_cap,
                            );
                            entered = Some((pid, exe));
                        }
                        (true, Some(_)) => {
                            s.detect = DetectState::Candidate {
                                pid,
                                exe,
                                ticks: ticks + 1,
                            };
                        }
                        _ => s.detect = DetectState::Idle,
                    }
                }
                DetectState::InGame { pid, name, .. } => {
                    if !process::is_alive(pid) {
                        if let Some(mut sess) = s.session.take() {
                            sess.ended = Some(now_epoch());
                            s.history.push_front(SessionSummary {
                                exe: sess.exe.clone(),
                                name: sess.name.clone(),
                                started: sess.started,
                                duration_secs: sess.duration_secs(),
                            });
                            while s.history.len() > history_limit {
                                s.history.pop_back();
                            }
                        }
                        s.detect = DetectState::Idle;
                        s.push_log(Level::Info, format!("退出游戏：{name}"), log_cap);
                        exited = true;
                    }
                }
            }
        }

        if let Some((pid, exe)) = entered {
            let _ = tx.send(UiNotice::GameStarted { pid, exe });
        }
        if exited {
            let _ = tx.send(UiNotice::GameEnded);
        }
    }
}
