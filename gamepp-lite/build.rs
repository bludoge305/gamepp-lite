//! 把图标与版本信息编进 exe。
//!
//! Rust 没有内置"给 PE 加资源"的能力，windows-gnu 工具链自带 `windres`（binutils），
//! 于是这里生成一个 .rc 交给 windres 编成 COFF 对象，再用 `cargo:rustc-link-arg` 链接进去。
//!
//! 找不到 windres 时**不报错**，只打一条 warning 继续构建 —— 图标是锦上添花，
//! 不该让没装 binutils 的人编不过。

use std::path::{Path, PathBuf};
use std::process::Command;

fn find_windres() -> Option<PathBuf> {
    // 1) 显式指定
    if let Ok(p) = std::env::var("WINDRES") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    // 2) 本机约定的 binutils 目录（与 README 里的构建说明一致）
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
    // 3) PATH
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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ico = manifest.join("assets").join("app.ico");
    if !ico.is_file() {
        println!("cargo:warning=缺少 assets/app.ico，跳过图标");
        return;
    }
    let Some(windres) = find_windres() else {
        println!("cargo:warning=未找到 windres，exe 将没有图标与版本信息");
        return;
    };

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ver: Vec<&str> = std::env::var("CARGO_PKG_VERSION")
        .unwrap_or_else(|_| "0.0.0".into())
        .leak()
        .split('.')
        .collect();
    let (v0, v1, v2) = (
        ver.first().and_then(|s| s.parse().ok()).unwrap_or(0u32),
        ver.get(1).and_then(|s| s.parse().ok()).unwrap_or(0u32),
        ver.get(2).and_then(|s| s.parse().ok()).unwrap_or(0u32),
    );

    // 路径统一用正斜杠，windres 对反斜杠转义很敏感。
    let ico_str = ico.display().to_string().replace('\\', "/");
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
      VALUE "FileDescription", "GamePP-Lite - game detection and Windows optimization"
      VALUE "FileVersion", "{v0}.{v1}.{v2}"
      VALUE "InternalName", "gamepp-lite"
      VALUE "OriginalFilename", "gamepp-lite.exe"
      VALUE "ProductName", "GamePP-Lite"
      VALUE "ProductVersion", "{v0}.{v1}.{v2}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico_str,
        v0 = v0,
        v1 = v1,
        v2 = v2
    );
    let rc_path = out.join("app.rc");
    let obj_path = out.join("app-res.o");
    if std::fs::write(&rc_path, rc).is_err() {
        println!("cargo:warning=写 .rc 失败，跳过图标");
        return;
    }

    // windres 默认用 `gcc -E` 预处理 .rc，而本机 binutils 里没有 gcc/g++。
    // 我们的 .rc 里没有任何 #include/#define，所以用一个"原样输出"的批处理当预处理器：
    // 忽略 windres 传进来的 -xc/-D 等参数，只把最后一个参数（.rc 文件）type 出来。
    let cpp = out.join("rc-cpp.cmd");
    let shim = "@echo off\r\n\
                :loop\r\n\
                if \"%~1\"==\"\" goto dump\r\n\
                set \"LAST=%~1\"\r\n\
                shift\r\n\
                goto loop\r\n\
                :dump\r\n\
                type \"%LAST%\"\r\n";
    if std::fs::write(&cpp, shim).is_err() {
        println!("cargo:warning=写预处理器失败，跳过图标");
        return;
    }

    let status = Command::new(&windres)
        .arg(&rc_path)
        .arg("-o")
        .arg(&obj_path)
        .arg("--output-format=coff")
        .arg("--preprocessor")
        .arg(&cpp)
        .status();
    match status {
        Ok(s) if s.success() && obj_path.is_file() => {
            println!("cargo:rustc-link-arg={}", obj_path.display());
        }
        _ => println!("cargo:warning=windres 执行失败，exe 将没有图标（构建继续）"),
    }
}
