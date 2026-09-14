//! 环境检测与自动安装。
//! 从 Dock / 访达双击启动的应用拿不到用户 PATH（不加载 ~/.zshrc、~/.zprofile），
//! 所以这里一律按绝对路径找 dsh / npm，起子进程时再把候选 bin 目录显式拼进 PATH。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 本机 Node 工具链的解析结果。
pub struct Toolchain {
    /// dsh 可执行文件绝对路径；未安装为 None。
    pub dsh: Option<PathBuf>,
    /// npm 可执行文件绝对路径；机器上没有 Node 环境为 None。
    pub npm: Option<PathBuf>,
    /// 候选 bin 目录全集，起子进程时拼进 PATH，保证 node / npm 能被找到。
    pub bin_dirs: Vec<PathBuf>,
}

impl Toolchain {
    /// dsh 已安装且 `--version` 能跑通时，返回其绝对路径。
    pub fn usable_dsh(&self) -> Option<PathBuf> {
        let dsh = self.dsh.as_ref()?;
        dsh_version(dsh, &self.bin_dirs).map(|_| dsh.clone())
    }
}

/// 全量检测：解析候选 bin 目录，再找 dsh 和 npm 的绝对路径。
pub fn detect() -> Toolchain {
    let bin_dirs = candidate_bin_dirs();

    // dsh 优先在候选目录里找，找不到再用登录 shell 兜底问一句
    let dsh = find_executable(&bin_dirs, "dsh").or_else(|| resolve_via_login_shell("dsh"));

    // npm 优先取 dsh 同目录那一个，保证全局安装目录落到同一套 Node 上
    let npm = dsh
        .as_ref()
        .and_then(|path| path.parent().map(|dir| dir.join("npm")))
        .filter(|path| path.is_file())
        .or_else(|| find_executable(&bin_dirs, "npm"))
        .or_else(|| resolve_via_login_shell("npm"));

    Toolchain { dsh, npm, bin_dirs }
}

/// 子进程 PATH：候选 bin 目录在前，系统默认 PATH 兜底，再叠加当前进程 PATH（终端启动场景）。
pub fn child_path(bin_dirs: &[PathBuf]) -> String {
    let mut parts: Vec<String> = bin_dirs.iter().map(|dir| dir.to_string_lossy().into_owned()).collect();
    parts.push("/usr/bin:/bin:/usr/sbin:/sbin".to_string());
    if let Ok(existing) = std::env::var("PATH") {
        parts.push(existing);
    }

    parts.join(":")
}

/// 执行 `dsh --version`，跑通且退出码为 0 时返回版本号字符串。
pub fn dsh_version(dsh: &Path, bin_dirs: &[PathBuf]) -> Option<String> {
    let output = Command::new(dsh)
        .arg("--version")
        .env("PATH", child_path(bin_dirs))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(version).filter(|text| !text.is_empty())
}

/// 查询 npm 上 @deepseek-ai/dsh 的最新版本号；npm 跑不通、断网或输出为空都返回 None。
pub fn npm_latest_version(npm: &Path, bin_dirs: &[PathBuf]) -> Option<String> {
    let output = Command::new(npm)
        .args(["view", "@deepseek-ai/dsh", "version"])
        .env("PATH", child_path(bin_dirs))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(version).filter(|text| !text.is_empty())
}

/// 执行 `npm i -g @deepseek-ai/dsh`，stdout / stderr 逐行回调给前端展示。
pub fn install_dsh(npm: &Path, bin_dirs: &[PathBuf], on_line: &(dyn Fn(String) + Sync)) -> anyhow::Result<()> {
    use std::io::{BufRead, BufReader};

    let mut child = Command::new(npm)
        .args(["i", "-g", "@deepseek-ai/dsh"])
        .env("PATH", child_path(bin_dirs))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // npm 的进度和日志主要走 stderr；用作用域线程借回调转发两路输出，结束时自动收齐
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                on_line(line);
            }
        });
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            on_line(line);
        }
    });

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("npm 退出码 {:?}", status.code());
    }

    Ok(())
}

/// 按优先级收集本机可能装有 node / npm / dsh 的 bin 目录（只保留真实存在的）。
fn candidate_bin_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut dirs = Vec::new();

    // nvm 管理的各版本 Node，版本号大的排前面
    let mut nvm_dirs: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(home.join(".nvm/versions/node"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let version = parse_version(&entry.file_name().to_string_lossy())?;
            Some((version, entry.path().join("bin")))
        })
        .collect();
    nvm_dirs.sort_by(|a, b| b.0.cmp(&a.0));
    dirs.extend(nvm_dirs.into_iter().map(|(_, dir)| dir));

    // 其他常见安装位置：Homebrew、官方安装包、hermes、用户本地 bin
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(home.join(".hermes/node/bin"));
    dirs.push(home.join(".local/bin"));

    dirs.retain(|dir| dir.is_dir());
    dirs
}

/// 把 `v24.16.0` 这类目录名解析成可排序的版本号元组。
fn parse_version(name: &str) -> Option<Vec<u64>> {
    let numbers: Vec<u64> = name
        .strip_prefix('v')?
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<Vec<_>>>()?;

    Some(numbers)
}

/// 在候选 bin 目录里按优先级找第一个存在的可执行文件。
fn find_executable(bin_dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    bin_dirs.iter().map(|dir| dir.join(name)).find(|path| path.is_file())
}

/// 兜底：用登录 shell 问命令的绝对路径（能加载 ~/.zprofile 和 ~/.zshrc）。
fn resolve_via_login_shell(name: &str) -> Option<PathBuf> {
    let output = Command::new("/bin/zsh")
        .args(["-l", "-i", "-c"])
        .arg(format!("command -v {name}"))
        .stderr(Stdio::null())
        .output()
        .ok()?;

    // .zshrc 可能有额外输出，取最后一行绝对路径
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.lines().rev().map(str::trim).find(|line| line.starts_with('/'))?;

    Some(PathBuf::from(path))
}
