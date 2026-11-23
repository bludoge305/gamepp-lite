//! 领域模型：设置、游戏库、优化清单、受保护白名单。
//!
//! 这一层对应逆向到的三处"数据面"：
//! - 设置：`%LOCALAPPDATA%\GamePP\GamePPConfig.ini` + `GamePPOpt.ini`（我们换 JSON）
//! - 游戏库：`GamePP5.dll`（SQLite）里的游戏/会话记录 + `common\Icon\<exe>.png`
//! - 优化清单：`GamePPOpt.ini [SYS]` 的开关组（见 `crate::opt::OptPlan`）
//!
//! `protect` 是本工程自己加的安全边界（原版没有）：系统组件白名单，
//! 扫描 / 优化 / 清理所有动作都先过它。

pub mod games;
pub mod protect;
pub mod settings;
