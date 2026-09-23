# WordPick 🌸

**极致轻量、系统级、模型可自由配置的划词助手** | An ultra-lightweight, system-wide, bring-your-own-model text-selection assistant.

在任何应用中选中文本，即刻弹出迷你工具条：翻译、解释、追问——全部能力由你自配的在线大模型驱动，按场景自由切换模型（快任务用小模型省钱，重任务用大模型保质）。

Select text in any app and get a mini toolbar instantly: translate, explain, follow-up — powered by the model providers *you* configure, with per-scenario model routing.

---

## 特性 | Features

- 🪶 **极致轻量**：Tauri 2 + 系统 WebView，目标安装包 ≤ 15MB、空闲内存 ≤ 120MB
- 🖱️ **系统级划词**：浏览器 / 编辑器 / IDE / Office / 终端（快捷键兜底）全场景
- 🔀 **模型自由配置**：任意 OpenAI 兼容服务（DeepSeek / Kimi / Qwen / OpenAI / OneAPI / OpenRouter…），按场景选模型
- 🌊 **流式输出**：SSE 增量渲染 + Markdown / 代码高亮（shiki）
- 📌 **钉住对话框**：置顶并行工作，失焦自动关闭
- 🔐 **Key 安全存储**：macOS Keychain / Windows Credential Manager，仅回显后 4 位
- ⌨️ **快捷键兜底**：`Alt+Q` 唤起（读剪贴板）、`Alt+,` 设置；数字键 1/2/3 触发、Esc 关闭

## 安装 | Install

```bash
# npm 全局安装（自动下载平台二进制）
npm install -g wordpick
wordpick run

# npx 直接运行
npx wordpick run

# 源码运行
git clone git@github.com:Jackyclearliu/WordPick.git && cd WordPick
pnpm i
pnpm tauri dev
```

> 国内加速：设置环境变量 `WORDPICK_BINARY_MIRROR=https://gh-proxy.example.com/...` 使用镜像源。
>
> Windows 也可直接下载 [Releases](https://github.com/Jackyclearliu/WordPick/releases) 中的
> `WordPick_x.x.x_x64-setup.exe` 安装包（约 4MB，安装后自动创建开始菜单项与开机自启项）。

## 快速上手：配置模型 Key

WordPick 本身**不内置任何模型**，首次运行会弹出设置窗口引导接入你的模型服务
（任意 OpenAI 兼容 API：DeepSeek / Kimi / Qwen / OpenAI / OneAPI / OpenRouter…）。

两种方式二选一：

1. **设置界面**（推荐）：托盘图标（蓝色 W）→ 设置 → 填 Base URL、API Key、模型名。
   Key 优先存入 Windows 凭据管理器 / macOS Keychain，仅回显后 4 位。
2. **环境变量 + 配置占位符**：配置文件中的 `api_key` 支持 `${ENV_NAME}` 占位符，
   运行时从进程环境变量解析，明文不落盘（见下节）。

## 环境变量 | Environment Variables

| 变量 | 用途 | 必需 |
| --- | --- | --- |
| `DEEPSEEK_API_KEY` | DeepSeek 提供方 Key（配置占位符 `${DEEPSEEK_API_KEY}` 引用） | 按所用提供方 |
| `MOONSHOT_API_KEY` | Kimi（月之暗面）提供方 Key | 按所用提供方 |
| `DASHSCOPE_API_KEY` | 阿里云百炼（Qwen）提供方 Key | 按所用提供方 |
| `OPENAI_API_KEY` | OpenAI 提供方 Key | 按所用提供方 |
| `WORDPICK_BINARY_MIRROR` | npm 安装二进制下载镜像源（国内加速） | 否 |
| `WORDPICK_SKIP_BINARY_DOWNLOAD` | 设为 `1` 跳过 postinstall 二进制下载（源码调试时） | 否 |

**Windows（PowerShell，仅当前会话生效）：**

```powershell
# 临时设置（当前终端窗口）
$env:DEEPSEEK_API_KEY = "sk-xxxxxxxx"

# 永久设置（用户级，新开终端/重启应用后生效；设置后需重启应用读取）
[Environment]::SetEnvironmentVariable("DEEPSEEK_API_KEY", "sk-xxxxxxxx", "User")
```

也可以在「设置 → 系统 → 系统信息 → 高级系统设置 → 环境变量」中图形化添加。

**macOS / Linux：**

```bash
export DEEPSEEK_API_KEY="sk-xxxxxxxx"   # 写入 ~/.zshrc / ~/.bashrc 可永久生效
```

> ⚠️ 环境变量在**应用启动时**读取：修改后需完全退出（托盘图标右键 → 退出）再启动。

## 本地运行与调试 | Running Locally

前置要求：

- Node.js ≥ 18、pnpm（`npm i -g pnpm`）
- Rust 工具链（[rustup](https://rustup.rs/) 默认安装即可）
- Windows 需 WebView2 运行时（Win10 1809+ / Win11 内置）

```bash
git clone git@github.com:Jackyclearliu/WordPick.git
cd WordPick

# 1) 安装依赖（postinstall 会自动下载平台二进制，源码调试可设 WORDPICK_SKIP_BINARY_DOWNLOAD=1 跳过）
pnpm i

# 2) 配置模型 Key（见上一节；不配置也能启动，只是翻译/解释会提示未配置）

# 3) 开发模式运行（改 Rust/前端代码自动热重载；终端可见 info 级运行日志）
pnpm tauri dev
```

开发模式行为说明：

- 首次启动未配置 API Key 时自动弹出设置窗口（onboarding）
- `pnpm tauri dev` = Vite 前端（localhost:5173）+ Rust 后端（cargo run）；
  若报 `Port 5173 is already in use`，结束残留 node 进程或改 `vite.config.ts` 端口后同步修改 `src-tauri/tauri.conf.json` 的 `devUrl`
- 划词链路日志（终端实时可见）：
  `selection → show toolbar: chars=N anchor=...`（选区→弹条）、
  `selection cleared → hide toolbar`（选区消失→隐藏）、
  `chat_stream start/done`（模型调用起止）——排查问题请先贴这几行
- 用户配置在 `%APPDATA%\wordpick\config.toml`（Windows）/
  `~/Library/Application Support/wordpick/config.toml`（macOS），
  设置界面与文件双向同步，可直接手改后重启生效

常用命令：

```bash
pnpm dev           # 仅前端 Vite 开发服务器
pnpm build         # 前端三入口产物
pnpm typecheck     # vue-tsc 类型检查
pnpm test          # vitest 单元测试（定位算法 / 语言检测 / 历史压缩 / 配置解析 / CLI）
pnpm tauri dev     # 完整桌面应用开发模式
pnpm tauri build   # 打 release 安装包（NSIS .exe / macOS .dmg）
```

## 命令 | Commands

| 命令 | 说明 |
| --- | --- |
| `wordpick run` | 启动后台常驻进程（重复启动仅唤起，单实例） |
| `wordpick settings` | 打开设置 |
| `wordpick stop` | 退出后台进程 |
| `wordpick status` | 查看运行状态与版本 |

## 配置 | Configuration

首次运行自动生成用户配置（`config/default.toml` 为内置默认值）：

- macOS: `~/Library/Application Support/wordpick/config.toml`
- Windows: `%APPDATA%\wordpick\config.toml`

设置界面的修改与配置文件**双向同步**。API Key 优先存入系统安全存储；配置文件中也支持环境变量占位符：

```toml
[[providers]]
name = "deepseek"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"   # 从进程环境变量解析，不保存明文
```

详见 [docs/model-config.md](docs/model-config.md) 与 [使用文档](docs/usage.md)。

## 平台支持 | Platform Support

- macOS 13+（Ventura 及以上；首次运行需授予「辅助功能」权限，未授权自动降级为快捷键模式）
- Windows 10 1809+（内置 WebView2 检测引导）
- Linux：后续可选目标（当前仅快捷键通道）

## 已知限制 | Known Limitations

- 终端及部分自绘 UI 应用不向系统暴露选区 → 使用 `Alt+Q` 快捷键唤起（读取剪贴板并还原）
- 选区坐标为「尽力而为」：拿不到坐标时以光标位置定位

## 开发 | Development

项目结构：Tauri 2（Rust 系统层：选区探测 UIA/剪贴板、窗口管理、OpenAI 兼容 SSE 适配、keyring）+ Vue 3（工具条 / 面板 / 设置三窗口）。需求与迭代计划见 [doc/划词助手需求文档.md](doc/划词助手需求文档.md) 与 [doc/plan.md](doc/plan.md)，模型配置细节见 [docs/model-config.md](docs/model-config.md)，使用说明见 [docs/usage.md](docs/usage.md)。

```bash
# Rust 侧检查（src-tauri 目录）
cargo clippy --all-targets   # 静态检查
cargo test                   # 单元测试（定位算法等）
cargo fmt                    # 格式化
```

## License

MIT
