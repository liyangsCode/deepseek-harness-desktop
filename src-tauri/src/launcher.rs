//! 自起分支：以子进程方式启动 `dsh web --no-open --port 0`，
//! 按行读它的标准输出，等到带登录凭证的地址打印出来。

use anyhow::{bail, Context};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 已启动并就绪的 dsh web 子进程。
pub struct SpawnedDsh {
    pub child: Child,
    /// dsh 打印的带登录凭证的完整地址。
    pub url: String,
}

/// 启动 dsh web 并等它打印地址；超时则杀掉子进程再报错。
pub fn spawn_and_wait_url(dsh: &Path, bin_dirs: &[PathBuf], timeout: Duration) -> anyhow::Result<SpawnedDsh> {
    let mut child = Command::new(dsh)
        .args(["web", "--no-open", "--port", "0"])
        .env("PATH", crate::env_detect::child_path(bin_dirs))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("启动 dsh web 子进程失败")?;

    // stderr 开线程消费丢弃，避免管道缓冲写满把子进程卡死
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            BufReader::new(stderr).lines().map_while(Result::ok).for_each(drop);
        });
    }

    // stdout 按行读，等 `dsh web: <带凭证地址>` 那一行；行尾可能带 LAN 地址，按空格切掉
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(url) = line.strip_prefix("dsh web: ").and_then(|rest| rest.split_whitespace().next()) {
                let _ = tx.send(url.to_string());
                return;
            }
        }
    });

    match rx.recv_timeout(timeout) {
        Ok(url) => Ok(SpawnedDsh { child, url }),
        Err(_) => {
            terminate_child(child);
            bail!("等待 dsh web 就绪超时（{} 秒）", timeout.as_secs());
        }
    }
}

/// 结束子进程：先 SIGTERM 给它清理机会，2 秒内不退出再 SIGKILL。
pub fn terminate_child(mut child: Child) {
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    // 轮询等它自己退出
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait().map(|status| status.is_some()).unwrap_or(true) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();
}
