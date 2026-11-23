//! 电源方案切换（UI 第 3 项：Windows电源方案更改为游戏加加电源方案（高性能））。
//!
//! 原始产品里这一步**不在** GamePPOptimize.node 内，而在同目录 Power.dll：
//!   字符串表里有 "PowerImportPowerScheme" / "PowerSetActiveScheme" / "POWRPROF.dll"
//!   （Power.dll 0x10015668 / 0x100156e8 / 0x10015712），导出 PP_Initialization /
//!   PP_UnInitialization / PP_Active / PP_UnActive；配套数据是 win32\PowerData 下的
//!   GPP_Maxed.pow / GPP_Excellence.pow / GPP_Ryzen.pow（文件头 "regf"，注册表 hive 格式，
//!   内含方案 GUID，例如 GPP_Maxed = 9fda562c-d377-43d5-bb2c-afcb0a22c547）。
//!
//! 因此这里用同样的"运行时动态取 powrprof 导出"的方式实现：
//!   PP_Active   ≈ PowerImportPowerScheme(.pow) 然后 PowerSetActiveScheme(新 GUID)
//!   PP_UnActive ≈ PowerSetActiveScheme(上一个方案 GUID)
//!
//! 上一个方案 GUID 原版记在 %LOCALAPPDATA%\GamePP\Power.cfg 的 [General] LastPlan，
//! 本实现记在状态文件里（见 state.rs）。

use super::ffi;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

// `PowerImportPowerScheme(HKEY RootPowerKey, LPCWSTR ImportFileNamePath,
// GUID **DestinationSchemeGuid)`：第三个参数是 **GUID 的指针**，语义为「*传入值为 NULL
// 时由函数自行分配一个 GUID 并把地址写回，调用方用 LocalFree 释放」。
// （对应 Power.dll!PP_Initialization 的反编译调用 `PowerImportPowerScheme(0, &guid, &out)`。）
// 注意不能写成 `*mut GUID`：那样 callee 会把 &g 当 GUID** 解引用，把堆指针写进 g，
// 拿到的是假 GUID —— 不崩溃但永远切不了电源方案，并且泄漏一块内存。
type PowerImportPowerSchemeFn =
    unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut ffi::GUID) -> u32;
type PowerSetActiveSchemeFn = unsafe extern "system" fn(*mut c_void, *const ffi::GUID) -> u32;
type PowerGetActiveSchemeFn = unsafe extern "system" fn(*mut c_void, *mut *mut ffi::GUID) -> u32;

struct PowerApi {
    module: ffi::HANDLE,
    import: Option<PowerImportPowerSchemeFn>,
    set_active: Option<PowerSetActiveSchemeFn>,
    get_active: Option<PowerGetActiveSchemeFn>,
}

impl PowerApi {
    fn load() -> Option<PowerApi> {
        unsafe {
            let name = ffi::wide("POWRPROF.dll");
            let module = ffi::LoadLibraryW(name.as_ptr());
            if module.is_null() {
                return None;
            }
            let sym = |s: &str| -> *mut c_void {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                ffi::GetProcAddress(module, b.as_ptr())
            };
            let import = sym("PowerImportPowerScheme");
            let set_active = sym("PowerSetActiveScheme");
            let get_active = sym("PowerGetActiveScheme");
            Some(PowerApi {
                module,
                import: if import.is_null() {
                    None
                } else {
                    Some(std::mem::transmute::<*mut c_void, PowerImportPowerSchemeFn>(import))
                },
                set_active: if set_active.is_null() {
                    None
                } else {
                    Some(std::mem::transmute::<*mut c_void, PowerSetActiveSchemeFn>(set_active))
                },
                get_active: if get_active.is_null() {
                    None
                } else {
                    Some(std::mem::transmute::<*mut c_void, PowerGetActiveSchemeFn>(get_active))
                },
            })
        }
    }
}

impl Drop for PowerApi {
    fn drop(&mut self) {
        unsafe {
            ffi::FreeLibrary(self.module);
        }
    }
}

/// 当前活动方案的 GUID 字符串，形如 "381b4222-f694-41f0-9685-ff5bb260df2e"。
pub fn active_scheme() -> Option<String> {
    let api = PowerApi::load()?;
    unsafe {
        let get = api.get_active?;
        let mut p: *mut ffi::GUID = std::ptr::null_mut();
        if get(std::ptr::null_mut(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let g = *p;
        ffi::LocalFree(p as ffi::HANDLE);
        Some(format_guid(&g))
    }
}

/// 激活一个已知 GUID 的方案。
pub fn set_active(guid: &str) -> bool {
    let api = match PowerApi::load() {
        Some(a) => a,
        None => return false,
    };
    let g = match parse_guid(guid) {
        Some(g) => g,
        None => return false,
    };
    unsafe {
        match api.set_active {
            Some(f) => f(std::ptr::null_mut(), &g) == 0,
            None => false,
        }
    }
}

/// 导入 .pow（注册表 hive 格式）并返回其中新注册的方案 GUID。
pub fn import_pow(path: &str) -> Option<String> {
    let api = PowerApi::load()?;
    unsafe {
        let import = api.import?;
        let w = ffi::wide(path);
        // *DestinationSchemeGuid 传 NULL，让函数分配；成功后必须 LocalFree（同 active_scheme）。
        let mut p: *mut ffi::GUID = std::ptr::null_mut();
        if import(std::ptr::null_mut(), w.as_ptr(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let g = *p;
        ffi::LocalFree(p as ffi::HANDLE);
        // 真正的健全性检查：全零 GUID 视为导入未落地。
        if g.data1 == 0 && g.data2 == 0 && g.data3 == 0 && g.data4 == [0u8; 8] {
            return None;
        }
        Some(format_guid(&g))
    }
}

/// 一步到位：导入 .pow 并激活；成功返回 (上一个方案 GUID, 新方案 GUID)。
pub fn switch_to_pow(pow_path: &Path) -> Option<(String, String)> {
    let prev = active_scheme()?;
    let new_guid = import_pow(&pow_path.to_string_lossy())?;
    if !set_active(&new_guid) {
        return None;
    }
    Some((prev, new_guid))
}

/// 直接激活一个已知 GUID（方案此前已被导入过时可用，无需 .pow 文件）。
pub fn switch_to_guid(guid: &str) -> Option<(String, String)> {
    let prev = active_scheme()?;
    if !set_active(guid) {
        return None;
    }
    Some((prev, guid.to_string()))
}

/// GPP_Maxed.pow 里记录的方案 GUID（从 PowerData\GPP_Maxed.pow 的 regf 内容读出）。
pub const GPP_MAXED_GUID: &str = "9fda562c-d377-43d5-bb2c-afcb0a22c547";

const POW_NAMES: [&str; 2] = ["GPP_Maxed.pow", "GPP_Excellence.pow"];

/// 定位游戏加加自带的电源方案文件。
///
/// 原版从 `<GamePP>\win32\PowerData\` 取 .pow；本程序是独立安装的，所以按以下顺序找：
///   1. `--pow` 显式指定
///   2. 程序目录下 `PowerData\`（如果用户自己放了）
///   3. `%LOCALAPPDATA%\GamePP\Apps\<appid>\<版本>\win32\PowerData\`（取版本号最大的）
///   4. `%ProgramFiles%\GamePP\win32\PowerData\` 等常见位置
pub fn resolve_pow(explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        let pb = PathBuf::from(p);
        return if pb.is_file() { Some(pb) } else { None };
    }

    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("PowerData"));
            roots.push(dir.join(r"win32\PowerData"));
        }
    }

    // %LOCALAPPDATA%\GamePP\Apps\<appid>\<version>\win32\PowerData
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let apps = PathBuf::from(&local).join(r"GamePP\Apps");
        if let Ok(ids) = std::fs::read_dir(&apps) {
            let mut versions: Vec<(Vec<u32>, PathBuf)> = Vec::new();
            for id in ids.flatten() {
                if let Ok(vers) = std::fs::read_dir(id.path()) {
                    for v in vers.flatten() {
                        let p = v.path();
                        if !p.is_dir() {
                            continue;
                        }
                        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                        versions.push((version_key(&name), p.join(r"win32\PowerData")));
                    }
                }
            }
            // 版本号大的优先
            versions.sort_by(|a, b| b.0.cmp(&a.0));
            roots.extend(versions.into_iter().map(|(_, p)| p));
        }
    }

    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(env) {
            let p = PathBuf::from(base).join("GamePP");
            roots.push(p.join("PowerData"));
            roots.push(p.join(r"win32\PowerData"));
        }
    }

    for root in roots {
        for name in POW_NAMES {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn version_key(s: &str) -> Vec<u32> {
    s.split('.').filter_map(|p| p.parse::<u32>().ok()).collect()
}

fn format_guid(g: &ffi::GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

fn parse_guid(s: &str) -> Option<ffi::GUID> {
    let s = s.trim().trim_matches(|c| c == '{' || c == '}');
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let d4: Vec<u8> = (0..parts[3].len() / 2)
        .map(|i| u8::from_str_radix(&parts[3][i * 2..i * 2 + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    let d5: Vec<u8> = (0..parts[4].len() / 2)
        .map(|i| u8::from_str_radix(&parts[4][i * 2..i * 2 + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    if d4.len() != 2 || d5.len() != 6 {
        return None;
    }
    Some(ffi::GUID {
        data1: u32::from_str_radix(parts[0], 16).ok()?,
        data2: u16::from_str_radix(parts[1], 16).ok()?,
        data3: u16::from_str_radix(parts[2], 16).ok()?,
        data4: [
            d4[0], d4[1], d5[0], d5[1], d5[2], d5[3], d5[4], d5[5],
        ],
    })
}
