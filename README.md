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
git clone <repo> && cd WordPick
pnpm i
pnpm tauri dev
```

> 国内加速：设置环境变量 `WORDPICK_BINARY_MIRROR=https://gh-proxy.example.com/...` 使用镜像源。

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

```bash
npm i
npm run dev        # Vite 开发服务器（窗口 UI）
npm run build      # 前端三入口产物
npm run typecheck  # vue-tsc
npm test           # vitest 单元测试（定位算法 / 语言检测 / 历史压缩 / 配置解析 / CLI）
pnpm tauri dev     # 完整桌面应用（需 Rust 工具链）
```

## 架构 | Architecture

Tauri 2（Rust 系统层：选区探测 AX/UIA/剪贴板、窗口管理、OpenAI 兼容 SSE 适配、keyring） + Vue 3（工具条 / 面板 / 设置三窗口）。详见 [doc/划词助手需求文档.md](doc/划词助手需求文档.md) 与 [doc/plan.md](doc/plan.md)。

## License

MIT
