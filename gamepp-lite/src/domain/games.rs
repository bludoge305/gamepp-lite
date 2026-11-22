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
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(Self::builtin)
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), text);
        }
    }
}
