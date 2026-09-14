# DeepSeek Harness 桌面端

把 `dsh web` 的网页封装成 macOS 桌面应用（Tauri 2），双击即用，不再自动弹浏览器。

## 功能特性

- **开箱即用**：没装 dsh 也能跑——检测到未安装时弹窗询问，确认后自动执行 `npm i -g @deepseek-ai/dsh`；机器上连 Node.js 都没有时，提示先装 Node.js。
- **实例复用**：已在终端跑着的 `dsh web` 会被自动发现（ps 扫进程、lsof 问端口），读 `~/.dsh/.credentials.yaml` 自签会话 cookie 直接接入，不起第二个进程；同时有多个实例时列出端口让你选一个。
- **自动拉起**：没有在跑的实例时，以子进程启动 `dsh web --no-open --port 0`（随机端口），等它打印出带登录凭证的地址再加载窗口。
- **干净的收尾**：关闭窗口只结束本应用自己启动的 dsh 子进程，接入的已有实例不碰；应用崩溃遗留的孤儿子进程，下次启动时会被探测到并收养复用。
- **单实例**：重复打开应用只会把已有窗口拉到前台（tauri-plugin-single-instance）。
- **中英文菜单**：应用菜单按系统首选语言切换中英文，内置「检查更新…」——比对本地 `dsh --version` 与 npm 上 @deepseek-ai/dsh 的最新版本，不一致时经确认自动更新并重启生效。

## 启动流程

启动编排跑在独立线程，窗口先显示状态页、不被阻塞，按以下顺序走：

1. **检测环境**：按绝对路径找 dsh——依次扫 `~/.nvm/versions/node/*/bin`（版本号大的优先）、`/opt/homebrew/bin`、`/usr/local/bin`、`~/.hermes/node/bin`、`~/.local/bin`，都找不到再用登录 shell `zsh -l -i -c 'command -v dsh'` 兜底。从 Dock / 访达双击启动的应用拿不到终端里的 PATH，所以整个流程不依赖 PATH。找到后跑 `dsh --version` 验证可用。
2. **未安装则自动安装**：有 npm 才弹窗询问是否安装（npm 优先取 dsh 同目录那个，保证全局安装落到同一套 Node）；确认后执行安装，npm 输出逐行显示在状态页，装完重新检测一遍。
3. **探测实例**：`ps -ax` 扫出命令行是 `dsh web` / `dsh --profile web` 的进程，再用 `lsof` 问出它在 127.0.0.1 上监听的端口。
   - 找到 1 个 → 自签 cookie 接入（第 4 步）；
   - 找到多个 → 状态页列出端口让用户选一个接入；
   - 一个都没有 → 自己起一个（第 4 步）。
4. **接入或自起**：
   - *接入*：按 dsh 的浏览器会话认证格式（HMAC-SHA256 签名）自签 cookie，先发一个 HTTP 请求验证它被目标实例接受，再写进窗口的 cookie 存储并加载页面。验证失败（dsh 升级改了认证格式）时提示先关闭已有实例，不会静默登录成错误状态。
   - *自起*：以子进程启动 `dsh web --no-open --port 0`，最多等 60 秒读到它打印的带凭证地址，然后窗口加载；超时杀掉子进程并报错。
5. **收尾**：关闭窗口时先 SIGTERM、2 秒不退出再 SIGKILL，只回收本应用自己起的 dsh 子进程，复用来的不碰。

## 构建与运行

```sh
npm install          # 装 @tauri-apps/cli
npm run tauri build  # 产出 src-tauri/target/release/bundle/macos/DeepSeek Harness.app
```

开发调试：

```sh
npm run dev          # 等价于 cd src-tauri && cargo run
```

要求：macOS 12 及以上、Rust 工具链；Node 环境可选（没有时应用会引导安装 dsh，但 npm 本身需要先有 Node.js）。

## 目录结构

```
ui/
  status.html       启动状态页（检测 / 安装 / 多实例选择 / 错误提示）
  status.js         状态页逻辑：监听后端状态事件切换界面
src-tauri/src/
  main.rs           入口
  lib.rs            Tauri 骨架：单实例插件、共享状态、关窗收尾
  orchestrator.rs   启动流程编排（检测 → 探测 → 接入或自起 → 加载）
  env_detect.rs     绝对路径解析、dsh --version 检测、npm 版本查询与自动安装
  instance.rs       进程探测（ps）与端口读取（lsof）
  cookie_auth.rs    凭证文件解析、自签 cookie、有效性验证
  launcher.rs       自起 dsh 子进程、解析就绪地址、进程回收
  menu.rs           中英文应用菜单与「检查更新」
```

## 已知边界

- 只支持 macOS（实例探测依赖 macOS 的 ps / lsof 行为，打包目标只开了 .app）。
- 自签 cookie 复刻的是 dsh 的浏览器会话认证格式（当前 0.1.5-rc.1）；dsh 升级若改了格式，表现为「无法接入已在运行的 dsh」，按提示先关闭已有实例再重新打开应用即可，不会静默登录错状态。
- 「检查更新」更新的是 dsh 本体（npm 包）；桌面壳自身的自动更新未做。
- 未做：像素风启动动画、托盘 / 开机自启。
