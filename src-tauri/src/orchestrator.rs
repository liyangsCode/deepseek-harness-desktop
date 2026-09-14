//! 启动流程编排：检测环境 → 探测实例 → 自起或复用 → 窗口加载地址。
//! 整个流程跑在独立线程里，可以阻塞等待子进程输出和前端用户答复。

use crate::{cookie_auth, env_detect, instance, launcher, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, WebviewWindow};

/// 推给前端的状态事件名，前端按 payload.state 切换界面。
const EVENT_STATE: &str = "dsh-state";
/// 等 dsh web 打印就绪地址的最长时间。
const SPAWN_TIMEOUT: Duration = Duration::from_secs(60);
/// 启动动画最短播放时长：复用实例能秒开，也要让动画播完再切页面。
const MIN_SPLASH: Duration = Duration::from_millis(3400);

/// 启动编排主流程。
pub fn orchestrate(app: tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let splash_started = Instant::now();

    // 第一步：检测 dsh / npm，未安装则走自动安装流程
    emit(&window, json!({ "state": "checking" }));
    let toolchain = env_detect::detect();
    let (toolchain, dsh) = match toolchain.usable_dsh() {
        Some(dsh) => {
            eprintln!("dsh-desktop: 检测到 dsh：{}", dsh.display());
            (toolchain, dsh)
        }
        None => match ensure_dsh_installed(&app, &window, &toolchain) {
            Some(pair) => pair,
            None => return,
        },
    };

    // 第二步：探测已在运行的 dsh web 实例
    let instances = instance::find_running_instances();
    eprintln!("dsh-desktop: 探测到 {} 个已在运行的 dsh web 实例：{:?}", instances.len(), instances.iter().map(|item| item.port).collect::<Vec<_>>());
    let reuse_port = match instances.as_slice() {
        [] => None,
        [only] => Some(only.port),
        many => pick_instance(&app, &window, many),
    };

    // 第三步：有实例就自签 cookie 复用，没有就自己起一个
    match reuse_port {
        Some(port) => reuse_branch(&window, port, splash_started),
        None => spawn_branch(&app, &window, &dsh, &toolchain.bin_dirs, splash_started),
    }
}

/// 未检测到 dsh 时的安装流程；用户取消、没 npm 或安装失败都返回 None。
fn ensure_dsh_installed(app: &tauri::AppHandle, window: &WebviewWindow, toolchain: &env_detect::Toolchain) -> Option<(env_detect::Toolchain, PathBuf)> {
    // 有 npm 才能自动安装；连 npm 都没有只能提示先装 Node.js
    let Some(npm) = toolchain.npm.clone() else {
        emit(window, json!({ "state": "need_install", "has_npm": false }));
        return None;
    };

    // 问用户是否自动安装；直接关窗按取消处理
    emit(window, json!({ "state": "need_install", "has_npm": true }));
    if !wait_install_reply(app) {
        return None;
    }

    // 执行安装，npm 输出逐行推给前端展示
    emit(window, json!({ "state": "installing" }));
    let progress = |line: String| emit(window, json!({ "state": "installing", "line": line }));
    if let Err(error) = env_detect::install_dsh(&npm, &toolchain.bin_dirs, &progress) {
        emit(window, json!({ "state": "error", "message": format!("自动安装 dsh 失败：{error}") }));
        return None;
    }

    // 装完重新检测，确认 dsh 可用
    emit(window, json!({ "state": "checking" }));
    let toolchain = env_detect::detect();
    match toolchain.usable_dsh() {
        Some(dsh) => Some((toolchain, dsh)),
        None => {
            emit(window, json!({ "state": "error", "message": "安装完成后仍找不到 dsh，请检查 Node 环境后重试".to_string() }));
            None
        }
    }
}

/// 等前端「是否安装」的答复。
fn wait_install_reply(app: &tauri::AppHandle) -> bool {
    let (tx, rx) = mpsc::channel();
    *app.state::<AppState>().install_reply.lock().unwrap() = Some(tx);

    rx.recv().unwrap_or(false)
}

/// 多个实例时让前端列端口给用户选；用户直接关窗时用列表第一个兜底。
fn pick_instance(app: &tauri::AppHandle, window: &WebviewWindow, instances: &[instance::DshInstance]) -> Option<u16> {
    let (tx, rx) = mpsc::channel();
    *app.state::<AppState>().pick_reply.lock().unwrap() = Some(tx);
    emit(window, json!({ "state": "pick_instance", "instances": instances }));

    rx.recv().ok().or_else(|| instances.first().map(|instance| instance.port))
}

/// 复用分支：自签 cookie 接入已在运行的实例，不起第二个进程；失败则提示用户先关闭它。
fn reuse_branch(window: &WebviewWindow, port: u16, splash_started: Instant) {
    if let Err(error) = attach_to_instance(window, port, splash_started) {
        emit(window, json!({ "state": "reuse_failed", "port": port, "message": error.to_string() }));
    }
}

/// 自签 cookie、验证有效性、写进窗口、加载页面。
fn attach_to_instance(window: &WebviewWindow, port: u16, splash_started: Instant) -> anyhow::Result<()> {
    let authority = format!("127.0.0.1:{port}");

    // 按 dsh 的格式自签 cookie，先用 HTTP 请求验证它被目标实例接受
    let secret = cookie_auth::read_browser_session_secret()?;
    let (name, value, expires_at_ms) = cookie_auth::build_session_cookie(&secret, &authority);
    if !cookie_auth::verify_cookie(&authority, &name, &value, port) {
        anyhow::bail!("自签 cookie 未被接受（dsh 可能升级改了认证格式）".to_string());
    }
    eprintln!("dsh-desktop: 自签 cookie 验证通过，接入 {authority}");

    // 写进窗口的 cookie 存储，再加载页面
    let expires = time::OffsetDateTime::from_unix_timestamp((expires_at_ms / 1000) as i64)?;
    let cookie = tauri::webview::Cookie::build((name, value))
        .domain("127.0.0.1")
        .path("/")
        .http_only(true)
        .expires(expires)
        .build();
    window.set_cookie(cookie)?;

    // 复用实例能秒开：先补足启动动画的最短播放时长再切页面
    wait_min_splash(splash_started);
    window.navigate(url::Url::parse(&format!("http://{authority}/"))?)?;

    Ok(())
}

/// 自起分支：启动 dsh web 子进程，登记归属后按它的端口自签 cookie 接入首页。
fn spawn_branch(app: &tauri::AppHandle, window: &WebviewWindow, dsh: &Path, bin_dirs: &[PathBuf], splash_started: Instant) {
    eprintln!("dsh-desktop: 没有已在运行的实例，启动 dsh web 子进程");
    match launcher::spawn_and_wait_url(dsh, bin_dirs, SPAWN_TIMEOUT) {
        Ok(spawned) => {
            // 登记子进程归属，关窗时只回收自己起的这个
            *app.state::<AppState>().owned_child.lock().unwrap() = Some(spawned.child);

            // dsh 打印的地址要用 ?token= 换会话 cookie，而那个 Set-Cookie 带 SameSite=Strict，
            // 窗口从状态页跨站跳过去时 WebKit 不会带上它，页面只会停在 401 文本页；
            // 所以自起也和复用分支走同一条路：自签 cookie 写进窗口，再加载 127.0.0.1 首页
            // 那个 token 是能换会话 cookie 的凭证，日志和界面一律只留端口，不落地址原文
            let Some(port) = spawned_port(&spawned.url) else {
                emit(window, json!({ "state": "error", "message": "dsh 打印的就绪地址里读不出端口（地址带登录凭证，已省略原文）" }));
                return;
            };
            eprintln!("dsh-desktop: dsh web 就绪，端口 {port}");
            if let Err(error) = attach_to_instance(window, port, splash_started) {
                emit(window, json!({ "state": "error", "message": format!("接入自起的 dsh web（127.0.0.1:{port}）失败：{error}") }));
            }
        }
        Err(error) => emit(window, json!({ "state": "error", "message": error.to_string() })),
    }
}

/// 从 dsh 打印的就绪地址里取它监听的端口。
fn spawned_port(ready_url: &str) -> Option<u16> {
    url::Url::parse(ready_url).ok()?.port()
}

/// 补足启动动画的最短播放时长；已超过则立即返回。
fn wait_min_splash(started: Instant) {
    if let Some(remain) = MIN_SPLASH.checked_sub(started.elapsed()) {
        std::thread::sleep(remain);
    }
}

/// 向主窗口推送状态事件。
fn emit(window: &WebviewWindow, payload: Value) {
    if let Err(error) = window.emit(EVENT_STATE, payload) {
        eprintln!("dsh-desktop: 推送状态事件失败：{error}");
    }
}
