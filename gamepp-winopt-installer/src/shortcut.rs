//! 创建 .lnk 快捷方式（IShellLinkW + IPersistFile，手写 COM 调用）。

use crate::winapi as w;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

type ReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;
type QueryInterfaceFn =
    unsafe extern "system" fn(*mut c_void, *const w::GUID, *mut *mut c_void) -> i32;
type StrFn = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
type IconFn = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
type SaveFn = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;

// IShellLinkW 的 vtable 索引（前 3 个是 IUnknown）
const IDX_QUERY_INTERFACE: usize = 0;
const IDX_RELEASE: usize = 2;
const IDX_SET_DESCRIPTION: usize = 7;
const IDX_SET_WORKING_DIR: usize = 9;
const IDX_SET_ICON_LOCATION: usize = 17;
const IDX_SET_PATH: usize = 20;

// IPersistFile 的 vtable 索引
const IDX_PF_RELEASE: usize = 2;
const IDX_PF_SAVE: usize = 6;

fn vtbl_of(obj: *mut c_void) -> *const *const c_void {
    unsafe { *(obj as *mut *const *const c_void) }
}

fn slot<T: Copy>(vtbl: *const *const c_void, index: usize) -> T {
    unsafe { std::mem::transmute_copy(&*vtbl.add(index)) }
}

pub fn create_lnk(
    lnk: &Path,
    target: &Path,
    workdir: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = lnk.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    unsafe {
        let hr = w::CoInitializeEx(std::ptr::null_mut(), w::COINIT_APARTMENTTHREADED);
        // RPC_E_CHANGED_MODE 表示已用别的模式初始化过，可以继续用
        let need_uninit = hr == 0;
        if hr < 0 && hr as u32 != 0x8001_0106 {
            return Err(format!("CoInitializeEx 失败: 0x{:08x}", hr as u32));
        }

        let mut link: *mut c_void = std::ptr::null_mut();
        let hr = w::CoCreateInstance(
            &w::CLSID_SHELLLINK,
            std::ptr::null_mut(),
            w::CLSCTX_INPROC_SERVER,
            &w::IID_ISHELLLINKW,
            &mut link,
        );
        if hr < 0 || link.is_null() {
            if need_uninit {
                w::CoUninitialize();
            }
            return Err(format!("CoCreateInstance(ShellLink) 失败: 0x{:08x}", hr as u32));
        }

        let vt = vtbl_of(link);

        let set_path: StrFn = slot(vt, IDX_SET_PATH);
        let set_desc: StrFn = slot(vt, IDX_SET_DESCRIPTION);
        let set_workdir: StrFn = slot(vt, IDX_SET_WORKING_DIR);
        let set_icon: IconFn = slot(vt, IDX_SET_ICON_LOCATION);

        let t = w::wide(&target.to_string_lossy());
        let wd = w::wide(&workdir.to_string_lossy());
        let desc = w::wide(description);

        let mut result = Ok(());
        if set_path(link, t.as_ptr()) < 0 {
            result = Err("IShellLink::SetPath 失败".to_string());
        }
        if result.is_ok() {
            set_workdir(link, wd.as_ptr());
            set_desc(link, desc.as_ptr());
            set_icon(link, t.as_ptr(), 0);
        }

        if result.is_ok() {
            let mut persist: *mut c_void = std::ptr::null_mut();
            let qi: QueryInterfaceFn = slot(vt, IDX_QUERY_INTERFACE);
            let hr = qi(link, &w::IID_IPERSISTFILE, &mut persist);
            if hr < 0 || persist.is_null() {
                result = Err(format!("QueryInterface(IPersistFile) 失败: 0x{:08x}", hr as u32));
            } else {
                let pvt = vtbl_of(persist);
                let save: SaveFn = slot(pvt, IDX_PF_SAVE);
                let release_pf: ReleaseFn = slot(pvt, IDX_PF_RELEASE);
                let lnk_w = w::wide(&lnk.to_string_lossy());
                if save(persist, lnk_w.as_ptr(), 1) < 0 {
                    result = Err("IPersistFile::Save 失败".to_string());
                }
                release_pf(persist);
            }
        }

        let release: ReleaseFn = slot(vt, IDX_RELEASE);
        release(link);
        if need_uninit {
            w::CoUninitialize();
        }
        result
    }
}

/// 所有用户的开始菜单目录。
pub fn start_menu_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string()),
    )
    .join(r"Microsoft\Windows\Start Menu\Programs")
}

/// 公共桌面目录。
pub fn public_desktop_dir() -> PathBuf {
    PathBuf::from(std::env::var("PUBLIC").unwrap_or_else(|_| r"C:\Users\Public".to_string()))
        .join("Desktop")
}

pub fn start_menu_lnk() -> PathBuf {
    start_menu_dir().join(r"GamePP-Lite\GamePP-Lite.lnk")
}

pub fn desktop_lnk() -> PathBuf {
    public_desktop_dir().join("GamePP-Lite.lnk")
}
