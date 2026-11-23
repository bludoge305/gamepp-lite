//! 游戏库：exe 名 → 游戏档案。
//!
//! 逆向对照：
//! - `GamePP5.dll`（SQLite）的 `GamePP_BaseInfo` 以 `processpath`（实为 exe 名）为主键，
//!   并带 `icon`、`resolutions`、`dx_version` 等字段；
//! - `%LOCALAPPDATA%\GamePP\common\Icon\` 里是一堆以 exe 名命名的 PNG
//!   （本机实测有 `Client-Win64-Shipping.png`、`HTGame.png`、`deadcells.png`、
//!   `Endfield.png`、`PlatformProcess.png` …），说明"识别"这件事的判据就是映像名；
//! - `WebApiImpl_GetGame` / `StartGetGame` / `SaveProcessIconToFile` 是这条链路的 JS 侧入口。
//!
//! 因此本工程的识别分两档：
//!   1. 命中游戏库 → `Matched`（信度高）；
//!   2. 未命中但前台全屏且加载了 d3d11/d3d12/vulkan → `Heuristic`（信度低，可手工登记）。

use serde::{Deserialize, Serialize};

/// 识别黑名单：命中这些映像名的进程**永远不判为游戏**，在识别阶段直接排除。
///
/// 用途：一些常用工具会同时满足"全屏 + 加载图形运行时"的启发式条件（或碰巧与
/// 游戏库条目同名），把它们过滤在这里，其余应用的识别不受影响。
/// 匹配规则：映像名大小写不敏感的**精确**匹配（不是子串，避免误伤）。
pub const DENY_LIST: &[&str] = &[
    "SnowAir-Lite.exe", // 截图工具
];

/// 映像名是否命中识别黑名单。
pub fn is_denied(exe: &str) -> bool {
    DENY_LIST.iter().any(|d| d.eq_ignore_ascii_case(exe))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Confidence {
    /// 命中游戏库。
    Matched,
    /// 启发式：全屏 + 图形运行时。
    Heuristic,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameEntry {
    /// 映像文件名（大小写不敏感匹配）。
    pub exe: String,
    pub name: String,
    /// 标签，仅用于展示与规则筛选。
    pub tags: Vec<String>,
    /// 是否由用户手工登记。
    pub custom: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GameLibrary {
    pub entries: Vec<GameEntry>,
}

impl GameLibrary {
    /// 内置示例库：exe 名取自本机 `common\Icon\*.png` 与常见引擎默认名。
    pub fn builtin() -> Self {
        let rows: &[(&str, &str, &[&str])] = &[
            ("Client-Win64-Shipping.exe", "Unreal 项目（通用 Shipping）", &["虚幻"]),
            ("HTGame.exe", "环塔（HTGame）", &["网游"]),
            ("deadcells.exe", "死亡细胞", &["单机"]),
            ("Endfield.exe", "明日方舟：终末地", &["二次元"]),
            ("GenshinImpact.exe", "原神", &["二次元"]),
            ("YuanShen.exe", "原神（国服）", &["二次元"]),
            ("StarRail.exe", "崩坏：星穹铁道", &["二次元"]),
            ("VALORANT-Win64-Shipping.exe", "无畏契约", &["竞技"]),
            ("r5apex.exe", "Apex 英雄", &["竞技"]),
            ("cs2.exe", "反恐精英 2", &["竞技"]),
            ("dota2.exe", "Dota 2", &["竞技"]),
            ("eldenring.exe", "艾尔登法环", &["单机"]),
            ("Cyberpunk2077.exe", "赛博朋克 2077", &["单机"]),
            ("GTA5.exe", "侠盗猎车手 V", &["单机"]),
            ("BaldursGate3.exe", "博德之门 3", &["单机"]),
            ("NarakaBladepoint.exe", "永劫无间", &["竞技"]),
        ];
        Self {
            entries: rows
                .iter()
                .map(|(exe, name, tags)| GameEntry {
                    exe: (*exe).to_string(),
                    name: (*name).to_string(),
                    tags: tags.iter().map(|t| (*t).to_string()).collect(),
                    custom: false,
                })
                .collect(),
        }
    }

    pub fn lookup(&self, exe: &str) -> Option<&GameEntry> {
        self.entries
            .iter()
            .find(|e| e.exe.eq_ignore_ascii_case(exe))
    }

    pub fn add_custom(&mut self, exe: &str, name: &str) {
        if self.lookup(exe).is_some() {
            return;
        }
        self.entries.push(GameEntry {
            exe: exe.to_string(),
            name: name.to_string(),
            tags: vec!["手工登记".into()],
            custom: true,
        });
    }

    pub fn remove(&mut self, exe: &str) {
        self.entries.retain(|e| !e.exe.eq_ignore_ascii_case(exe));
    }

    /// 编辑一条记录（改 exe 名或显示名）。返回 false 表示新 exe 与其它记录重名。
    pub fn update(&mut self, orig_exe: &str, new_exe: &str, new_name: &str) -> bool {
        let new_exe = new_exe.trim();
        let new_name = new_name.trim();
        if new_exe.is_empty() {
            return false;
        }
        let clash = self
            .entries
            .iter()
            .any(|e| !e.exe.eq_ignore_ascii_case(orig_exe) && e.exe.eq_ignore_ascii_case(new_exe));
        if clash {
            return false;
        }
        match self
            .entries
            .iter_mut()
            .find(|e| e.exe.eq_ignore_ascii_case(orig_exe))
        {
            Some(e) => {
                e.exe = new_exe.to_string();
                e.name = if new_name.is_empty() {
                    new_exe.to_string()
                } else {
                    new_name.to_string()
                };
                e.custom = true;
                true
            }
            None => false,
        }
    }

    /// 恢复内置库（丢弃所有手工修改）。
    pub fn restore_default(&mut self) {
        self.entries = Self::builtin().entries;
    }

    pub fn path() -> std::path::PathBuf {
        super::settings::data_dir().join("gamelib.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::builtin();
        };
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                // 同 config：损坏的库先留档，再回落内置列表，别让它被静默覆盖。
                let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                crate::log_line("gamelib", &format!("gamelib.json 解析失败，已另存为 .corrupt：{e}"));
                Self::builtin()
            }
        }
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = super::settings::write_atomic(&Self::path(), &text);
        }
    }
}
