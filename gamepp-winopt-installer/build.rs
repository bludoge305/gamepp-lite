//! 把要安装的载荷（主程序、README、逆向报告）嵌进安装包里。
//!
//! 先构建 `gamepp-lite`，再构建本工程；缺失时直接报错并提示。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy(src: &Path, dst: &Path) {
    if !src.exists() {
        panic!(
            "缺少载荷文件: {}\n请先执行: gamepp-lite\\build.cmd（或 cargo build --release --manifest-path gamepp-lite/Cargo.toml）",
            src.display()
        );
    }
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::copy(src, dst).unwrap_or_else(|e| panic!("复制 {} 失败: {}", src.display(), e));
    println!("cargo:rerun-if-changed={}", src.display());
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().expect("installer 应位于仓库子目录").to_path_buf();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("payload");

    let exe = root.join("gamepp-lite/target/release/gamepp-lite.exe");
    copy(&exe, &out.join("gamepp-lite.exe"));

    let readme = root.join("gamepp-lite/README.md");
    copy(&readme, &out.join("README.md"));

    let report = root.join("report/gamepp-four-features-reverse-report.md");
    copy(&report, &out.join("REVERSE-REPORT.md"));

    let arch = root.join("gamepp-lite/docs/architecture.md");
    copy(&arch, &out.join("ARCHITECTURE.md"));

    println!("cargo:rerun-if-changed=build.rs");

    embed_icon_and_version(&root, &out);
}

fn find_windres() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WINDRES") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = Path::new(&home)
            .join(".cargo")
            .join("mingw-binutils")
            .join("bin")
            .join("windres.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    for name in ["windres.exe", "windres"] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(name));
        }
    }
    None
}

/// 给 setup.exe 也配上图标与版本信息（右键属性里能看到"安装程序"）。
fn embed_icon_and_version(root: &Path, out: &Path) {
    let ico = root.join("gamepp-lite/assets/app.ico");
    if !ico.is_file() {
        println!("cargo:warning=缺少 app.ico，安装包将没有图标");
        return;
    }
    let Some(windres) = find_windres() else {
        println!("cargo:warning=未找到 windres，安装包将没有图标");
        return;
    };
    // 版本号从 Cargo.toml 来，避免和 CARGO_PKG_VERSION 漂移。
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let num: Vec<&str> = ver.split('.').collect();
    let (v0, v1, v2) = (
        num.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0),
        num.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0),
        num.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0),
    );
    let ico = ico.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"IDI_APPICON ICON "{ico}"
1 VERSIONINFO
 FILEVERSION {v0},{v1},{v2},0
 PRODUCTVERSION {v0},{v1},{v2},0
 FILEOS 0x40004
 FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "gamepp-lite"
      VALUE "FileDescription", "GamePP-Lite Setup - installer and uninstaller"
      VALUE "FileVersion", "{ver}"
      VALUE "InternalName", "gamepp-lite-setup"
      VALUE "OriginalFilename", "gamepp-lite-setup.exe"
      VALUE "ProductName", "GamePP-Lite"
      VALUE "ProductVersion", "{ver}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_path = out.join("setup.rc");
    let obj_path = out.join("setup-res.o");
    let cpp = out.join("rc-cpp.cmd");
    let shim = "@echo off\r\n:loop\r\nif \"%~1\"==\"\" goto dump\r\nset \"LAST=%~1\"\r\nshift\r\ngoto loop\r\n:dump\r\ntype \"%LAST%\"\r\n";
    if fs::write(&rc_path, rc).is_err() || fs::write(&cpp, shim).is_err() {
        println!("cargo:warning=写 .rc 失败，安装包将没有图标");
        return;
    }
    let ok = Command::new(&windres)
        .arg(&rc_path)
        .arg("-o")
        .arg(&obj_path)
        .arg("--output-format=coff")
        .arg("--preprocessor")
        .arg(&cpp)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && obj_path.is_file() {
        println!("cargo:rustc-link-arg={}", obj_path.display());
    } else {
        println!("cargo:warning=windres 失败，安装包将没有图标");
    }
}
