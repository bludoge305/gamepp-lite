//! 识别服务：前台窗口 → 进程 → 游戏库/启发式 → 状态机。
//!
//! 逆向对照 `GPPPlatform.exe`：它就是"每秒取一次前台窗口，拿到 pid 和映像名，
//! 再去平台上问这是不是游戏"。本工程把"问平台"换成"查本地游戏库 + 图形运行时启发式"，
//! 判定同样带 `stable_polls` 次防抖（对应 `onGameWindowStateChanged` 的稳定态）。
//!
//! 本模块只负责"识别"，不再执行任何优化动作 —— Windows 优化由 ② 页显式触发，
//! 或由 ② 页里的两个自动化开关驱动。

use super::{now_epoch, Config, DetectState, Level, Session, SessionSummary, Shared, UiNotice};
use crate::domain::games::{Confidence, GameLibrary};
use crate::win::{foreground, process};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 判定"这是不是游戏"。
///
/// 图形运行时由 `foreground::current()` 在采样时一并取好（带缓存），这里只读结果，
/// 不再重复枚举模块。
fn classify(lib: &GameLibrary, fg: &foreground::Foreground) -> Option<(String, Confidence)> {
    if let Some(entry) = lib.lookup(&fg.exe) {
        return Some((entry.name.clone(), Confidence::Matched));
    }
    // 启发式：全屏 + 加载了图形运行时。对应 GetGameProcessType 的兜底分支。
    if fg.fullscreen && !fg.runtimes.is_empty() {
        let stem = fg.exe.split('.').next().unwrap_or(&fg.exe).to_string();
        return Some((format!("{stem}（启发式识别）"), Confidence::Heuristic));
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
            let mut s = match shared.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            s.polls += 1;
            s.foreground = fg.clone();
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
                        (true, Some((name, conf))) if ticks + 1 >= stable => {
                            let started = now_epoch();
                            s.detect = DetectState::InGame {
                                pid,
                                exe: exe.clone(),
                                name: name.clone(),
                                confidence: conf.clone(),
                                since: started,
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
