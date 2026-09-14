//! DeepSeek Harness 桌面端：把 dsh web 的网页封装成桌面应用。
//! 启动流程见 orchestrator 模块；本文件只管 Tauri 应用骨架。

mod cookie_auth;
mod env_detect;
mod instance;
mod launcher;
mod menu;
mod orchestrator;

use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use tauri::Manager;

/// 应用共享状态。
pub struct AppState {
    /// 本应用自己起的 dsh 子进程；复用已有实例时为空。
    owned_child: Mutex<Option<Child>>,
    /// 等待前端「是否安装 dsh」的答复通道。
    install_reply: Mutex<Option<Sender<bool>>>,
    /// 等待前端「多实例选择」的答复通道。
    pick_reply: Mutex<Option<Sender<u16>>>,
}

impl AppState {
    /// 取出并结束本应用自己起的 dsh 子进程；没有则空操作。
    pub fn terminate_owned_child(&self) {
        if let Some(child) = self.owned_child.lock().unwrap().take() {
            launcher::terminate_child(child);
        }
    }
}

/// 前端「安装 / 取消安装」按钮的答复。
#[tauri::command]
fn install_decision(confirmed: bool, state: tauri::State<'_, AppState>) {
    if let Some(reply) = state.install_reply.lock().unwrap().take() {
        let _ = reply.send(confirmed);
    }
}

/// 前端多实例列表里用户选中的端口。
#[tauri::command]
fn pick_instance(port: u16, state: tauri::State<'_, AppState>) {
    if let Some(reply) = state.pick_reply.lock().unwrap().take() {
        let _ = reply.send(port);
    }
}

pub fn run() {
    tauri::Builder::default()
        // 桌面应用自身单实例：第二个副本启动时把已有窗口拉到前台，然后自己退出
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            owned_child: Mutex::new(None),
            install_reply: Mutex::new(None),
            pick_reply: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![install_decision, pick_instance])
        .on_menu_event(menu::handle_event)
        .setup(|app| {
            // 应用菜单（中英文按系统语言切换，含「检查更新」）
            menu::setup(app)?;

            // 启动编排跑在独立线程，不阻塞窗口显示
            let handle = app.handle().clone();
            std::thread::spawn(move || orchestrator::orchestrate(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗收尾：只结束本应用自己起的 dsh 子进程，复用来的不碰
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<AppState>().terminate_owned_child();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
