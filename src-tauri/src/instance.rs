//! 实例探测：扫进程表找已在运行的 dsh web，并用 lsof 问出它监听的 127.0.0.1 端口。
//! 进程命令行用 ps 拿：macOS 上 sysinfo 拿不到其他进程的参数（实测 cmd() 返回空），
//! 而 ps -ax -o pid=,args= 可以。不用固定端口探测的原因：终端里的 dsh 可能起在任意端口。

use std::path::Path;
use std::process::{Command, Stdio};

/// 一个已在运行且确认在监听 127.0.0.1 的 dsh web 实例。
#[derive(Clone, serde::Serialize)]
pub struct DshInstance {
    pub pid: u32,
    pub port: u16,
}

/// 扫描进程表，返回所有可用的 dsh web 实例（按端口排序）。
pub fn find_running_instances() -> Vec<DshInstance> {
    let mut instances = Vec::new();
    for (pid, args) in process_args_table() {
        // 只关心命令行形如 `node .../dsh web` 的进程
        if !is_dsh_web_command(&args) {
            continue;
        }

        // 问出它监听的 127.0.0.1 端口；问不到（进程刚退出等）就不算可用实例
        if let Some(port) = listen_port_of(pid) {
            instances.push(DshInstance { pid, port });
        }
    }
    instances.sort_by_key(|instance| instance.port);
    instances.dedup_by_key(|instance| instance.port);

    instances
}

/// 用 ps 拿全量进程命令行，返回（进程号，参数序列）列表。
/// ps 用绝对路径：从图标启动的应用 PATH 是系统默认值，裸命令名找不到。
fn process_args_table() -> Vec<(u32, Vec<String>)> {
    let output = match Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=", "-o", "args="])
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    // 每行形如 ` 33876 node /Users/.../bin/dsh web`，按空白切分后第一项是进程号
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let pid: u32 = tokens.next()?.parse().ok()?;

            Some((pid, tokens.map(str::to_string).collect()))
        })
        .collect()
}

/// 命令行是否为 dsh web（`web` 是 `--profile web` 的硬编码别名，两种写法都算）。
fn is_dsh_web_command(args: &[String]) -> bool {
    // 定位参数里 basename 为 dsh 的可执行文件路径
    let Some(at) = args.iter().position(|arg| {
        Path::new(arg).file_name().is_some_and(|name| name == "dsh")
    }) else {
        return false;
    };

    // 紧跟其后的参数：`dsh web ...` 或 `dsh --profile web ...`
    match args.get(at + 1..) {
        Some([first, ..]) if first == "web" => true,
        Some([flag, profile, ..]) if flag == "--profile" && profile == "web" => true,
        _ => false,
    }
}

/// 用 macOS 自带的 lsof 读进程在 127.0.0.1 上监听的 TCP 端口。
/// lsof 用绝对路径，原因同 ps。
fn listen_port_of(pid: u32) -> Option<u16> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p"])
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 逐行找形如 `TCP 127.0.0.1:3080 (LISTEN)` 的行，提取端口
    stdout.lines().find_map(|line| {
        if !line.contains("(LISTEN)") {
            return None;
        }
        let token = line.split_whitespace().find(|token| token.starts_with("127.0.0.1:"))?;

        token.strip_prefix("127.0.0.1:")?.parse().ok()
    })
}
