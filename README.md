# DeepSeek Harness 桌面端

把 `dsh web` 的网页封装成 macOS 桌面应用（Tauri 2），双击即用，不再自动弹浏览器。

## 功能特性

- **开箱即用**：没装 dsh 也能跑——检测到未安装时弹窗询问，确认后自动执行 `npm i -g @deepseek-ai/dsh`；机器上连 Node.js 都没有时，提示先装 Node.js。
- **实例复用**：已在终端跑着的 `dsh web` 会被自动发现（ps 扫进程、lsof 问端口），读 `~/.dsh/.credentials.yaml` 自签会话 cookie 直接接入，不起第二个进程；同时有多个实例时列出端口让你选一个。
- **自动拉起**：没有在跑的实例时，以子进程启动 `dsh web --no-open --port 0`（随机端口），等它打印出带登录凭证的地址再加载窗口。
- **干净的收尾**：关闭窗口只结束本应用自己启动的 dsh 子进程，接入的已有实例不碰；应用崩溃遗留的孤儿子进程，下次启动时会被探测到并收养复用。
- **单实例**：重复打开应用只会把已有窗口拉到前台（tauri-plugin-single-instance）。
- **禁用右键菜单**：webview 注入初始化脚本拦掉 `contextmenu`，页面里右键不再弹出 WKWebView 自带的 Back / Reload 菜单（状态页和 dsh 界面都一样）。
- **像素风启动画面**：打开应用即是像素画面——深空底色上是 5x7 像素字体的品牌名，逐像素做红色迪斯科式跳色闪动，加载状态直接显示在品牌名下方；画面跟启动进度绑定，就绪即切入主界面，复用实例秒开时也保证播满最短时长。
- **中英文菜单**：应用菜单按系统首选语言切换中英文，内置「检查更新…」——比对本地 `dsh --version` 与 npm 上 @deepseek-ai/dsh 的最新版本，不一致时经确认自动更新并重启生效。

## 启动流程

启动编排跑在独立线程，窗口先显示像素启动画面（加载状态直接叠在画面上）、不被阻塞，就绪后切到 dsh 页面（画面最短播满 3.4 秒）。流程按以下顺序走：

1. **检测环境**：按绝对路径找 dsh——依次扫 `~/.nvm/versions/node/*/bin`（版本号大的优先）、`/opt/homebrew/bin`、`/usr/local/bin`、`~/.hermes/node/bin`、`~/.local/bin`，都找不到再用登录 shell `zsh -l -i -c 'command -v dsh'` 兜底。从 Dock / 访达双击启动的应用拿不到终端里的 PATH，所以整个流程不依赖 PATH。找到后跑 `dsh --version` 验证可用。
2. **未安装则自动安装**：有 npm 才弹窗询问是否安装（npm 优先取 dsh 同目录那个，保证全局安装落到同一套 Node）；确认后执行安装，npm 输出逐行显示在状态页，装完重新检测一遍。
3. **探测实例**：`ps -ax` 扫出命令行是 `dsh web` / `dsh --profile web` 的进程，再用 `lsof` 问出它在 127.0.0.1 上监听的端口。
   - 找到 1 个 → 自签 cookie 接入（第 4 步）；
   - 找到多个 → 状态页列出端口让用户选一个接入；
   - 一个都没有 → 自己起一个（第 4 步）。
4. **接入或自起**：
   - *接入*：按 dsh 的浏览器会话认证格式（HMAC-SHA256 签名）自签 cookie，先发一个 HTTP 请求验证它被目标实例接受，再写进窗口的 cookie 存储并加载页面。验证失败（dsh 升级改了认证格式）时提示先关闭已有实例，不会静默登录成错误状态。
   - *自起*：以子进程启动 `dsh web --no-open --port 0`，最多等 60 秒读到它打印的就绪地址，超时杀掉子进程并报错。拿到端口后不直接跳它打印的 `?token=` 地址，而是和「接入」走同一条路——自签 cookie 写进窗口，再加载 `http://127.0.0.1:<端口>/`：那个 token 地址靠响应里的 `Set-Cookie`（带 `SameSite=Strict`）换会话 cookie，而这次跳转由状态页发起、属于跨站跳转，WebKit 不会带上它，直接跳只会停在 401 提示文本上。
5. **收尾**：关闭窗口时先 SIGTERM、2 秒不退出再 SIGKILL，只回收本应用自己起的 dsh 子进程，复用来的不碰。

## 构建与运行

```sh
npm install          # 装 @tauri-apps/cli
npm run tauri build  # 产出 src-tauri/target/release/bundle/dmg/DeepSeek Harness_<版本>_<架构>.dmg
```

打包目标只开 dmg（`tauri.conf.json` 的 `bundle.targets`）。做 dmg 中间会生成 `.app`，打完由打包器自动清理，不会留在 `bundle/macos/` 下；要单独拿 `.app` 就用 `npm run build -- --bundles app`。

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
  splash.js         像素风启动画面：5x7 像素字体品牌名，逐像素红色迪斯科闪动
src-tauri/src/
  main.rs           入口
  lib.rs            Tauri 骨架：单实例插件、主窗口创建与初始化脚本注入、共享状态、关窗收尾
  orchestrator.rs   启动流程编排（检测 → 探测 → 接入或自起 → 加载）
  env_detect.rs     绝对路径解析、dsh --version 检测、npm 版本查询与自动安装
  instance.rs       进程探测（ps）与端口读取（lsof）
  cookie_auth.rs    凭证文件解析、自签 cookie、有效性验证
  launcher.rs       自起 dsh 子进程、解析就绪地址、进程回收
  menu.rs           中英文应用菜单与「检查更新」
```

## 已知边界

- 只支持 macOS（实例探测依赖 macOS 的 ps / lsof 行为，打包目标只出 dmg）。
- 自签 cookie 复刻的是 dsh 的浏览器会话认证格式（当前 0.1.5-rc.1）：复用已有实例、接入自己起的实例都靠它。dsh 升级若改了格式，表现为状态页报「自签 cookie 未被接受（dsh 可能升级改了认证格式）」，不会静默登录成错误状态；想确认是 dsh 侧改了格式，先在终端手动 `dsh web` 打开一次对比。
- 「检查更新」更新的是 dsh 本体（npm 包）；桌面壳自身的自动更新未做。
- 未做：托盘 / 开机自启。
