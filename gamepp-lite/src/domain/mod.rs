//! 领域模型：设置、游戏库、优化清单。
//!
//! 这一层对应逆向到的三处"数据面"：
//! - 设置：`%LOCALAPPDATA%\GamePP\GamePPConfig.ini` + `GamePPOpt.ini`（我们换 JSON）
//! - 游戏库：`GamePP5.dll`（SQLite）里的游戏/会话记录 + `common\Icon\<exe>.png`
//! - 优化清单：`GamePPOpt.ini [SYS]` 的开关组（见 `crate::opt::OptPlan`）

pub mod games;
pub mod settings;
