//! 应用菜单：中英文两套文案按系统首选语言切换。
//! 「检查更新」比对本地 `dsh --version` 与 npm 上 @deepseek-ai/dsh 的版本，
//! 不一致时经用户确认后执行 `npm i -g @deepseek-ai/dsh` 并重启应用生效。

use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::menu::{MenuBuilder, MenuEvent, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::env_detect;
use crate::AppState;

/// 「检查更新」菜单项 id。
const ID_CHECK_UPDATES: &str = "check-updates";

/// 菜单与对话框的全部文案，中英文各一份实例。
struct Texts {
    app_name: &'static str,
    check_updates: &'static str,
    about: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    quit: &'static str,
    file: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    dialog_title: &'static str,
    /// 含 {v} 占位。
    up_to_date: &'static str,
    /// 含 {latest}、{local} 占位。
    update_available: &'static str,
    not_installed: &'static str,
    check_failed: &'static str,
    btn_update: &'static str,
    btn_cancel: &'static str,
    btn_ok: &'static str,
    update_done: &'static str,
    /// 含 {error} 占位。
    update_failed: &'static str,
}

const EN: Texts = Texts {
    app_name: "DeepSeek Harness",
    check_updates: "Check for Updates…",
    about: "About DeepSeek Harness",
    services: "Services",
    hide: "Hide DeepSeek Harness",
    hide_others: "Hide Others",
    quit: "Quit DeepSeek Harness",
    file: "File",
    close_window: "Close Window",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    dialog_title: "Check for Updates",
    up_to_date: "You're up to date ({v}).",
    update_available: "The version on npm is {latest}; you have {local} installed.",
    not_installed: "No dsh installation detected; cannot check for updates.",
    check_failed: "Update check failed. Check your network connection and try again.",
    btn_update: "Update",
    btn_cancel: "Cancel",
    btn_ok: "OK",
    update_done: "Update complete. Click OK to restart the app.",
    update_failed: "Update failed: {error}",
};

const ZH: Texts = Texts {
    app_name: "DeepSeek Harness",
    check_updates: "检查更新…",
    about: "关于 DeepSeek Harness",
    services: "服务",
    hide: "隐藏 DeepSeek Harness",
    hide_others: "隐藏其他",
    quit: "退出 DeepSeek Harness",
    file: "文件",
    close_window: "关闭窗口",
    edit: "编辑",
    undo: "撤销",
    redo: "重做",
    cut: "剪切",
    copy: "拷贝",
    paste: "粘贴",
    select_all: "全选",
    dialog_title: "检查更新",
    up_to_date: "已是最新版本（{v}）。",
    update_available: "npm 上的版本是 {latest}，你当前安装的是 {local}。",
    not_installed: "未检测到 dsh 安装，无法检查更新。",
    check_failed: "检查失败，请检查网络后重试。",
    btn_update: "更新",
    btn_cancel: "取消",
    btn_ok: "确定",
    update_done: "更新完成，点击确定重启应用。",
    update_failed: "更新失败：{error}",
};

/// 按系统首选语言选文案：首选语言以 zh 开头用中文，否则英文。
fn texts() -> &'static Texts {
    if prefer_chinese() { &ZH } else { &EN }
}

/// 构建并设置应用菜单（在 setup 钩子里调用）。
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let t = texts();

    // 应用菜单：检查更新放第一行，其余是 macOS 标准项（关于/服务/隐藏/退出）
    let check_updates = MenuItemBuilder::with_id(ID_CHECK_UPDATES, t.check_updates).build(app)?;
    let app_menu = SubmenuBuilder::new(app, t.app_name)
        .item(&check_updates)
        .separator()
        .item(&PredefinedMenuItem::about(app, Some(t.about), None)?)
        .separator()
        .item(&PredefinedMenuItem::services(app, Some(t.services))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some(t.hide))?)
        .item(&PredefinedMenuItem::hide_others(app, Some(t.hide_others))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(t.quit))?)
        .build()?;

    // 文件菜单：关窗
    let file_menu = SubmenuBuilder::new(app, t.file)
        .item(&PredefinedMenuItem::close_window(app, Some(t.close_window))?)
        .build()?;

    // 编辑菜单：标准编辑项，网页里的输入框依赖它们
    let edit_menu = SubmenuBuilder::new(app, t.edit)
        .item(&PredefinedMenuItem::undo(app, Some(t.undo))?)
        .item(&PredefinedMenuItem::redo(app, Some(t.redo))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some(t.cut))?)
        .item(&PredefinedMenuItem::copy(app, Some(t.copy))?)
        .item(&PredefinedMenuItem::paste(app, Some(t.paste))?)
        .item(&PredefinedMenuItem::select_all(app, Some(t.select_all))?)
        .build()?;

    let menu = MenuBuilder::new(app).items(&[&app_menu, &file_menu, &edit_menu]).build()?;
    app.set_menu(menu)?;

    Ok(())
}

/// 菜单点击分发：检查更新放后台线程跑，避免 npm 命令阻塞主线程。
pub fn handle_event(app: &tauri::AppHandle, event: MenuEvent) {
    if event.id().as_ref() != ID_CHECK_UPDATES {
        return;
    }

    let handle = app.clone();
    let t = texts();
    std::thread::spawn(move || check_updates(handle, t));
}

/// 检查更新主流程：比版本 → 询问 → 更新 → 重启。
fn check_updates(app: tauri::AppHandle, t: &'static Texts) {
    let toolchain = env_detect::detect();

    // 本地版本拿不到视为未安装（比如复用了别人起的实例）
    let Some(local) = toolchain.dsh.as_deref().and_then(|dsh| env_detect::dsh_version(dsh, &toolchain.bin_dirs)) else {
        show_info(&app, t, t.not_installed);
        return;
    };

    // npm 侧最新版本；npm 缺失或断网都归为检查失败
    let latest = toolchain
        .npm
        .as_deref()
        .and_then(|npm| env_detect::npm_latest_version(npm, &toolchain.bin_dirs));
    let Some(latest) = latest else {
        show_info(&app, t, t.check_failed);
        return;
    };

    // 只做相等判断不比大小：本地可能装着比 npm latest 更新的预发布版
    if local == latest {
        show_info(&app, t, &t.up_to_date.replace("{v}", &local));
        return;
    }

    // 版本不同，问用户要不要更新
    let message = t.update_available.replace("{latest}", &latest).replace("{local}", &local);
    let confirmed = app
        .dialog()
        .message(message)
        .title(t.dialog_title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(t.btn_update.to_string(), t.btn_cancel.to_string()))
        .blocking_show();
    if !confirmed {
        return;
    }

    // 执行更新；收集输出末尾若干行，失败时随提示展示
    let npm = toolchain.npm.clone().unwrap();
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let collect = {
        let tail = Arc::clone(&tail);
        move |line: String| {
            let mut lines = tail.lock().unwrap();
            lines.push(line);
            if lines.len() > 20 {
                lines.remove(0);
            }
        }
    };
    if let Err(error) = env_detect::install_dsh(&npm, &toolchain.bin_dirs, &collect) {
        let tail = tail.lock().unwrap().join("\n");
        let detail = if tail.is_empty() { error.to_string() } else { format!("{error}\n{tail}") };
        show_info(&app, t, &t.update_failed.replace("{error}", &detail));
        return;
    }

    // 更新成功后重启生效；先回收本应用自己起的旧 dsh 子进程，否则重启后会复用到旧版本实例
    show_info(&app, t, t.update_done);
    app.state::<AppState>().terminate_owned_child();
    app.restart();
}

/// 弹一个只有确定按钮的信息框。
fn show_info(app: &tauri::AppHandle, t: &Texts, message: &str) {
    app.dialog()
        .message(message)
        .title(t.dialog_title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCustom(t.btn_ok.to_string()))
        .blocking_show();
}

/// 读系统首选语言列表第一条是否中文；读不到按英文处理。
#[cfg(target_os = "macos")]
fn prefer_chinese() -> bool {
    // `defaults read -g AppleLanguages` 输出形如 ( "zh-Hans-CN", "en-CN" )，取第一对引号里的语言码
    let Ok(output) = Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() else {
        return false;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(start) = stdout.find('"') else { return false };
    let Some(end) = stdout[start + 1..].find('"') else { return false };

    stdout[start + 1..start + 1 + end].starts_with("zh")
}

/// 非 macOS 平台没有首选语言列表可查，按英文处理。
#[cfg(not(target_os = "macos"))]
fn prefer_chinese() -> bool {
    false
}
