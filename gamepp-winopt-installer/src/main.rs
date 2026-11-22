//! GamePP-Lite 安装 / 卸载程序。
//!
//! 单文件、零依赖：主程序与文档在构建时用 include_bytes! 嵌入，安装时释放即可。
//!
//!   gamepp-lite-setup.exe                     图形化控制台向导
//!   gamepp-lite-setup.exe /S /D=C:\Path      静默安装到指定目录
//!   gamepp-lite-setup.exe /uninstall         卸载（可从控制面板调用）
//!   gamepp-lite-setup.exe --dry-run          只打印计划，不动系统

mod registry;
mod shortcut;
mod winapi;
use winapi as w;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

const APP_NAME: &str = "GamePP-Lite";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const PUBLISHER: &str = "gamepp-lite";

const PAYLOAD_EXE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload/gamepp-lite.exe"));
const PAYLOAD_README: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload/README.md"));
const PAYLOAD_REPORT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload/REVERSE-REPORT.md"));
const PAYLOAD_ARCH: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload/ARCHITECTURE.md"));

#[derive(PartialEq, Eq, Clone, Copy)]
enum Cmd {
    Install,
    Uninstall,
    Help,
}

struct Args {
    cmd: Cmd,
    dir: Option<PathBuf>,
    no_registry: bool,
    no_shortcuts: bool,
    desktop: bool,
    assume_yes: bool,
    elevated: bool,
    dry_run: bool,
    run_after: bool,
    test_shortcut: Option<PathBuf>,
}

fn parse_args(raw: &[String]) -> Args {
    let mut a = Args {
        cmd: Cmd::Install,
        dir: None,
        no_registry: false,
        no_shortcuts: false,
        desktop: false,
        assume_yes: false,
        elevated: false,
        dry_run: false,
        run_after: false,
        test_shortcut: None,
    };
    for (i, arg) in raw.iter().enumerate() {
        match arg.as_str() {
            "/?" | "/h" | "/help" | "--help" | "-h" => a.cmd = Cmd::Help,
            "/uninstall" | "--uninstall" | "/remove" | "-u" => a.cmd = Cmd::Uninstall,
            "/S" | "/s" | "/silent" | "--silent" | "-y" | "/y" => a.assume_yes = true,
            "/desktop" | "--desktop" => a.desktop = true,
            "/noregistry" | "--no-registry" => a.no_registry = true,
            "/noshortcuts" | "--no-shortcuts" => a.no_shortcuts = true,
            "--dry-run" | "/dryrun" => a.dry_run = true,
            "/run" | "--run" => a.run_after = true,
            "--elevated" => a.elevated = true,
            "--test-shortcut" => {
                if let Some(v) = raw.get(i + 1) {
                    a.test_shortcut = Some(PathBuf::from(v.trim_matches('"')));
                }
            }
            "/dir" | "--dir" | "/D" | "/d" => {
                if let Some(v) = raw.get(i + 1) {
                    a.dir = Some(PathBuf::from(v.trim_matches('"')));
                }
            }
            other => {
                if let Some(v) = other.strip_prefix("/D=").or_else(|| other.strip_prefix("/d=")) {
                    a.dir = Some(PathBuf::from(v.trim_matches('"')));
                }
            }
        }
    }
    a
}

fn default_dir() -> PathBuf {
    let base = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    PathBuf::from(base).join("GamePP-Lite")
}

fn self_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gamepp-lite-setup.exe"))
}

fn quoted_args(raw: &[String]) -> String {
    raw.iter()
        .map(|a| {
            if a.contains(' ') && !a.starts_with('"') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_line() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    s.trim().to_string()
}

fn ask(prompt: &str, default: &str) -> String {
    print!("{} [{}]: ", prompt, default);
    let _ = std::io::stdout().flush();
    let v = read_line();
    if v.is_empty() {
        default.to_string()
    } else {
        v
    }
}

fn ask_yes_no(prompt: &str, default: bool) -> bool {
    let hint = if default { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", prompt, hint);
    let _ = std::io::stdout().flush();
    let v = read_line().to_lowercase();
    if v.is_empty() {
        return default;
    }
    v.starts_with('y')
}

fn pause_if_interactive() {
    if std::io::stdin().is_terminal() {
        print!("\n按回车键退出...");
        let _ = std::io::stdout().flush();
        let _ = read_line();
    }
}

fn banner() {
    println!("============================================================");
    println!("  {} 安装程序  v{}", APP_NAME, VERSION);
    println!("  游戏识别 + Windows 优化（Rust + egui）");
    println!("============================================================");
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw);

    if args.cmd == Cmd::Help {
        usage();
        return;
    }

    // 诊断：单独验证 IShellLink 调用是否可用
    if let Some(lnk) = &args.test_shortcut {
        let target = PathBuf::from(r"C:/Windows/System32/notepad.exe");
        match shortcut::create_lnk(lnk, &target, Path::new(r"C:/Windows/System32"), "GamePP-Lite 快捷方式自检") {
            Ok(_) => println!("OK 已创建: {}", lnk.display()),
            Err(e) => println!("FAIL {}", e),
        }
        return;
    }

    let interactive = std::io::stdin().is_terminal() && !args.assume_yes;
    if interactive {
        banner();
        println!();
    }

    let code = match args.cmd {
        Cmd::Install => install(&args, &raw, interactive),
        Cmd::Uninstall => uninstall(&args, &raw, interactive),
        Cmd::Help => 0,
    };

    if interactive {
        pause_if_interactive();
    }
    std::process::exit(code);
}

fn usage() {
    println!("{} 安装程序 v{}", APP_NAME, VERSION);
    println!();
    println!("用法:");
    println!("  gamepp-lite-setup.exe                      交互式安装");
    println!("  gamepp-lite-setup.exe /S [/D=<目录>]       静默安装（/D 必须放最后）");
    println!("  gamepp-lite-setup.exe /S /desktop          静默安装并建桌面快捷方式");
    println!("  gamepp-lite-setup.exe /uninstall           卸载");
    println!("  gamepp-lite-setup.exe --dry-run            只打印将要做的操作");
    println!();
    println!("可选开关:");
    println!("  /noshortcuts    不创建快捷方式");
    println!("  /noregistry     不写控制面板卸载项");
    println!("  /run            安装完成后立即启动程序（配合 /S 使用）");
    println!("  /y              全部使用默认值，不再询问");
}

// ---------------------------------------------------------------- install

fn install(args: &Args, raw: &[String], interactive: bool) -> i32 {
    let mut dir = args.dir.clone().unwrap_or_else(default_dir);
    let mut create_menu = !args.no_shortcuts;
    let mut create_desktop = args.desktop;

    if interactive {
        println!("安装目录: 默认 {}", dir.display());
        let input = ask("安装目录", &dir.to_string_lossy());
        dir = PathBuf::from(input);
        create_menu = ask_yes_no("创建开始菜单快捷方式", create_menu);
        create_desktop = ask_yes_no("创建桌面快捷方式", create_desktop);
        println!();
    }

    println!("安装到: {}", dir.display());
    println!("  - gamepp-lite.exe");
    println!("  - uninstall.exe");
    println!("  - README.md");
    println!(r"  - docs\REVERSE-REPORT.md");
    println!(r"  - docs\ARCHITECTURE.md");
    if !args.no_registry {
        println!(r"注册表: HKLM\{}", registry::UNINSTALL_KEY);
    }
    if create_menu {
        println!("开始菜单: {}", shortcut::start_menu_lnk().display());
    }
    if create_desktop {
        println!("桌面快捷方式: {}", shortcut::desktop_lnk().display());
    }

    if args.dry_run {
        println!("\n[dry-run] 未做任何修改。");
        return 0;
    }

    if let Err(code) = require_write(&dir, args, raw, Vec::new()) {
        return code;
    }
    if let Err(e) = std::fs::create_dir_all(&dir) {
        w::message_box(
            &format!("创建安装目录失败:\n{}\n{}", dir.display(), e),
            APP_NAME,
            w::MB_OK | w::MB_ICONERROR,
        );
        return 1;
    }

    let mut total: u64 = 0;
    let files: [(&str, &[u8]); 4] = [
        ("gamepp-lite.exe", PAYLOAD_EXE),
        ("README.md", PAYLOAD_README),
        (r"docs\REVERSE-REPORT.md", PAYLOAD_REPORT),
        (r"docs\ARCHITECTURE.md", PAYLOAD_ARCH),
    ];
    for (name, bytes) in files {
        let path = dir.join(name);
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        match std::fs::write(&path, bytes) {
            Ok(_) => {
                total += bytes.len() as u64;
                println!("  已写入 {}", name);
            }
            Err(e) => {
                w::message_box(&format!("写入 {} 失败:\n{}", path.display(), e), APP_NAME, w::MB_OK | w::MB_ICONERROR);
                return 1;
            }
        }
    }

    // 卸载器 = 自身副本
    let uninstaller = dir.join("uninstall.exe");
    match std::fs::copy(self_exe(), &uninstaller) {
        Ok(n) => {
            total += n;
            println!("  已写入 uninstall.exe");
        }
        Err(e) => println!("  ! 复制卸载器失败: {}（卸载将通过安装包本身执行）", e),
    }

    if !args.no_registry {
        let uninstall_str = format!("\"{}\" /uninstall", uninstaller.display());
        let quiet_str = format!("{} /S", uninstall_str);
        let icon_str = format!("{}\\gamepp-lite.exe", dir.display());
        let location = dir.to_string_lossy().to_string();
        let values: Vec<(&str, registry::Value)> = vec![
            ("DisplayName", registry::Value::Str(APP_NAME)),
            ("DisplayVersion", registry::Value::Str(VERSION)),
            ("Publisher", registry::Value::Str(PUBLISHER)),
            ("InstallLocation", registry::Value::Str(&location)),
            ("DisplayIcon", registry::Value::Str(&icon_str)),
            ("UninstallString", registry::Value::Str(&uninstall_str)),
            ("QuietUninstallString", registry::Value::Str(&quiet_str)),
            ("EstimatedSize", registry::Value::Dword((total / 1024) as u32)),
            ("NoModify", registry::Value::Dword(1)),
            ("NoRepair", registry::Value::Dword(1)),
        ];
        match registry::write_values(registry::UNINSTALL_KEY, &values) {
            Ok(_) => println!("  已写入控制面板卸载项"),
            Err(e) => {
                println!("  ! 写注册表失败: {}", e);
                if !w::is_elevated() {
                    println!("    （提示：写 HKLM 需要管理员权限）");
                }
            }
        }
    }

    if !args.no_shortcuts {
        let exe = dir.join("gamepp-lite.exe");
        if create_menu {
            match shortcut::create_lnk(
                &shortcut::start_menu_lnk(),
                &exe,
                &dir,
                "GamePP-Lite — 游戏识别与 Windows 优化",
            ) {
                Ok(_) => println!("  已创建开始菜单快捷方式"),
                Err(e) => println!("  ! 开始菜单快捷方式失败: {}", e),
            }
        }
        if create_desktop {
            match shortcut::create_lnk(
                &shortcut::desktop_lnk(),
                &exe,
                &dir,
                "GamePP-Lite — 游戏识别与 Windows 优化",
            ) {
                Ok(_) => println!("  已创建桌面快捷方式"),
                Err(e) => println!("  ! 桌面快捷方式失败: {}", e),
            }
        }
    }

    println!();
    println!("安装完成。");
    println!("  程序: {}\\gamepp-lite.exe", dir.display());

    // 可选：装完直接拉起来（GUI 程序，不需要管理员；优化页自己会提示提权）
    let should_run = if interactive {
        ask_yes_no("立即启动 GamePP-Lite", true)
    } else {
        args.run_after
    };
    if should_run {
        let exe = dir.join("gamepp-lite.exe");
        match std::process::Command::new(&exe).spawn() {
            Ok(_) => println!("  已启动 {}", exe.display()),
            Err(e) => println!("  ! 启动失败: {}（可手动双击 {}）", e, exe.display()),
        }
    }
    println!("  双击运行即可；「Windows 优化」需要在 ② 页点「以管理员身份重启」。");
    println!("  配置与日志: %LOCALAPPDATA%\\GamePP-Lite\\");
    0
}

/// 确认目标目录可写；不可写且未提权时自动拉起提权副本。
/// 返回 Ok(()) 表示可以继续，Err(code) 表示以该退出码结束当前进程。
fn require_write(dir: &Path, args: &Args, raw: &[String], elevated_args: Vec<String>) -> Result<(), i32> {
    if args.dry_run {
        return Ok(());
    }
    let _ = std::fs::create_dir_all(dir);
    let probe = dir.join(".gpp-write-test");
    let writable = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
        .is_ok();
    let _ = std::fs::remove_file(&probe);
    if writable {
        return Ok(());
    }

    if w::is_elevated() || args.elevated {
        w::message_box(
            &format!("没有权限写入:\n{}\n\n请换一个目录，或以管理员身份运行。", dir.display()),
            APP_NAME,
            w::MB_OK | w::MB_ICONERROR,
        );
        return Err(1);
    }

    let mut extra: Vec<String> = raw.to_vec();
    extra.extend(elevated_args);
    extra.push("--elevated".to_string());
    println!("需要管理员权限，正在请求提权...");
    if w::relaunch_elevated(&self_exe(), &quoted_args(&extra)) {
        Err(0)
    } else {
        w::message_box("请求提权失败（可能被 UAC 取消）。", APP_NAME, w::MB_OK | w::MB_ICONERROR);
        Err(1)
    }
}

// -------------------------------------------------------------- uninstall

fn uninstall(args: &Args, raw: &[String], interactive: bool) -> i32 {
    let dir = args
        .dir
        .clone()
        .or_else(|| registry::read_string(registry::UNINSTALL_KEY, "InstallLocation").map(PathBuf::from))
        .or_else(|| self_exe().parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(default_dir);

    println!("将要卸载: {}", dir.display());

    if args.dry_run {
        println!("  - 删除快捷方式");
        println!("  - 删除注册表项 HKLM\\{}", registry::UNINSTALL_KEY);
        println!("  - 删除目录 {}", dir.display());
        println!("\n[dry-run] 未做任何修改。");
        return 0;
    }

    if interactive && !ask_yes_no("确认卸载？", false) {
        println!("已取消。");
        return 0;
    }

    let dir_arg = vec![format!("--dir={}", dir.display())];
    if let Err(code) = require_write(&dir, args, raw, dir_arg) {
        return code;
    }

    // 快捷方式
    let mut menu = shortcut::start_menu_lnk();
    if menu.exists() && std::fs::remove_file(&menu).is_ok() {
        println!("  已删除开始菜单快捷方式");
    }
    if let Some(parent) = menu.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    menu = shortcut::desktop_lnk();
    if menu.exists() && std::fs::remove_file(&menu).is_ok() {
        println!("  已删除桌面快捷方式");
    }

    // 注册表
    if !args.no_registry {
        match registry::delete_key(registry::UNINSTALL_KEY) {
            Ok(_) => println!("  已删除控制面板卸载项"),
            Err(e) => println!("  ! 删除注册表失败: {}", e),
        }
    }

    // 文件：删掉除“正在运行的自己”之外的所有文件
    let self_path = std::fs::canonicalize(self_exe()).ok();
    let mut left = 0usize;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_self = self_path
                .as_ref()
                .map(|s| std::fs::canonicalize(&path).map(|p| &p == s).unwrap_or(false))
                .unwrap_or(false);
            if is_self {
                left += 1;
                continue;
            }
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    if left > 0 {
        // 正在运行的卸载器自身删不掉，登记到重启后删除
        unsafe {
            let p = w::wide(&dir.join("uninstall.exe").to_string_lossy());
            w::MoveFileExW(p.as_ptr(), std::ptr::null(), w::MOVEFILE_DELAY_UNTIL_REBOOT);
        }
        let _ = std::fs::remove_dir(&dir);
        println!("  卸载器自身将在下次重启后自动删除。");
    } else {
        let _ = std::fs::remove_dir(&dir);
    }

    println!();
    println!("卸载完成。");
    0
}
