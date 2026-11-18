---
description: WordPick 的项目架构、开发命令、代码约定、测试发布流程与安全注意事项，供 AI 编码代理使用。
---

# WordPick 开发代理指南

> 本文依据当前代码与配置编写，面向不了解本项目的编码代理。项目版本为 `1.0.0`。

## 目录

- [项目概述](#项目概述)
- [技术栈与关键配置](#技术栈与关键配置)
- [术语速览](#术语速览)
- [目录与模块分工](#目录与模块分工)
- [运行时架构](#运行时架构)
- [构建与开发命令](#构建与开发命令)
- [代码风格与修改约定](#代码风格与修改约定)
- [测试与验证](#测试与验证)
- [CI 与发布流程](#ci-与发布流程)
- [安全与平台注意事项](#安全与平台注意事项)
- [参考资料](#参考资料)

## 项目概述

WordPick 是系统级划词助手：用户在其他应用中选中文本后，应用显示迷你工具条，提供翻译、解释、设置和后续追问。模型能力由用户配置的 OpenAI 兼容服务提供，项目本身不内置模型，也没有遥测端点。

应用是后台常驻的 Tauri 2 桌面程序，包含三个业务窗口：

- `toolbar`：无焦点抢占的工具条，数字键 `1`/`2`/`3` 分别触发翻译/解释/设置，`Esc` 或任意其他按键隐藏；触发动作后自动关闭。
- `panel`：翻译、解释和多轮追问面板，支持流式 Markdown、停止、重试、复制、朗读和钉住。
- `settings`：提供方、场景模型、API Key、触发模式、快捷键、主题、黑名单和开机自启设置。

全局快捷键默认为 `Alt+Q`（唤起）和 `Alt+,`（设置）。自动选区捕获实现面向 Windows 和 macOS；其他平台的监听层仅保留快捷键通道。Release 工作流当前只构建 macOS 与 Windows。

## 技术栈与关键配置

- 前端：Vue 3、TypeScript、Vite 6、Pinia、Tailwind CSS 4。
- 桌面与系统层：Tauri 2、Rust 2021 edition（最低 Rust 1.77）。
- 模型与渲染：Rust `reqwest` + SSE，前端 `markdown-it` + 懒加载的 `shiki`。
- 系统能力：Tauri 单实例、全局快捷键、剪贴板、开机自启插件；Windows UIA/Win32 API；macOS Accessibility API。
- 安全存储：`keyring`，对应 macOS Keychain 和 Windows Credential Manager。
- 测试：Vitest 与 Rust 内置 `#[cfg(test)]` 单元测试。
- 包管理与分发：本地文档以 pnpm 为主，CI 使用 npm；npm 包通过 `bin/wordpick.js` 启动平台二进制。

关键文件：

- `package.json`：Node 要求为 `>=18`，定义前端脚本、CLI 入口和 npm 包内容。
- `vite.config.ts`：三个 HTML 入口、`@` 到 `src` 的别名、固定端口 `5173`、Tailwind 插件。
- `tsconfig.json` / `tsconfig.node.json`：严格 TypeScript 检查，包含 Vue、TS 以及 `bin/` 下的 JavaScript。
- `src-tauri/Cargo.toml`：Rust 依赖、平台依赖和最低工具链版本。
- `src-tauri/tauri.conf.json`：Tauri 开发/构建钩子、隐藏的主窗口、bundle 配置；`frontendDist` 为 `../dist`。
- `src-tauri/capabilities/default.json`：四个窗口标签及 Tauri 权限白名单。
- `config/default.toml`：通过 `include_str!` 编译进 Rust 二进制的完整默认配置。
- `.github/workflows/ci.yml` / `release.yml`：CI 和 tag 发布流程。
- `pnpm-workspace.yaml`：当前只配置依赖构建许可，不表示这是多包 monorepo。

需求文档曾提到 shadcn-vue 等备选方案，但当前依赖中没有组件库；不要把规划文档中的备选技术当作已安装依赖。

## 术语速览

| 术语 | 本项目中的含义 |
| --- | --- |
| Tauri | 以 Rust 核心和系统 WebView 构建桌面应用的框架 |
| WebView | 渲染三个 Vue 窗口的系统浏览器组件 |
| command / event | 前端调用 Rust 的命令通道，以及 Rust 向前端推送的事件通道 |
| UIA | Windows UI Automation，用于只读获取支持 `TextPattern` 的应用选区 |
| AX | macOS Accessibility API，用于监听和读取辅助功能选区 |
| SSE | Server-Sent Events；此处指模型 Chat Completions 的流式响应格式 |
| keyring | 将 API Key 写入操作系统安全存储的 Rust 库 |
| Pinia | 面板会话使用的 Vue 状态管理库 |

## 目录与模块分工

```text
├── toolbar.html / panel.html / settings.html  # 三个 Vite 页面入口
├── src/
│   ├── entries/                   # 每个窗口独立的 Vue 应用挂载与 Pinia 初始化
│   ├── views/                     # ToolbarApp.vue / PanelApp.vue / SettingsApp.vue
│   ├── stores/session.ts          # 单次面板会话状态
│   ├── lib/
│   │   ├── bridge*.ts             # 工具条命令、Selection/PanelContext 跨端类型
│   │   ├── llm-client.ts          # 对话 commands 与 wp://chat-chunk 事件
│   │   ├── settings-client.ts     # 配置、Key 和连通性命令
│   │   ├── markdown.ts            # 流式 Markdown 与 shiki 高亮
│   │   ├── chat-history.ts        # 历史保留、压缩与摘要请求
│   │   ├── lang-detect.ts         # 本地启发式语言检测
│   │   ├── position.ts            # Rust 定位算法的 TypeScript 可执行规格
│   │   ├── config-shared.ts       # 环境变量占位符等纯函数规格
│   │   ├── theme.ts               # 三窗口主题应用
│   │   └── __tests__/             # Vitest 单元测试
│   └── style.css                  # Tailwind 导入与全局窗口样式
├── src-tauri/src/
│   ├── main.rs                    # 调用 wordpick::run()
│   ├── lib.rs                     # 插件、启动流程、选区分发、快捷键、command 注册
│   ├── commands.rs                # UI 与 Rust 的 Tauri command 边界
│   ├── state.rs                   # AppState 与待取走的 PanelContext
│   ├── config.rs                  # 默认配置、用户配置加载/保存支持、损坏回退
│   ├── secure.rs                  # keyring 与 Key 掩码
│   ├── selection/                 # 统一 Selection 抽象、Windows/macOS/剪贴板实现
│   ├── windows/                   # 动态窗口工厂、复用、定位和工作区约束
│   ├── llm/                       # OpenAI 兼容 SSE、prompt、内置提供方模板
│   └── tray.rs                    # 托盘菜单与开机自启入口
├── bin/                           # Node CLI 与可单测的纯逻辑
├── scripts/postinstall.js         # 按平台下载 Release 二进制
├── config/default.toml            # 内置默认用户配置
├── docs/                          # 使用与模型配置文档
└── doc/                           # 需求文档和分阶段开发记录
```

`dist/`、`src-tauri/target/`、`src-tauri/gen/` 和下载到 `bin/` 的平台二进制均为生成物，不要手工维护或提交。

## 运行时架构

### 启动与选区链路

1. `src-tauri/src/lib.rs` 初始化日志和 Tauri 插件，加载配置并创建共享 `AppState`。
2. 启动时写入临时目录中的 `wordpick.pid`，建立托盘；没有可用 Key 时打开设置引导。
3. 自动模式下，Windows 使用 UIA `TextPattern`、WinEvent 和拖选释放探测；macOS 使用 AX 通知。快捷键路径优先直读 Windows UIA 选区，失败后读取剪贴板。
4. 选区事件进入统一 channel，经空白/长度/黑名单过滤和 180ms 防抖后显示工具条；清除事件隐藏工具条。
5. `windows/manager.rs` 按需创建并复用业务窗口。`tauri.conf.json` 中的 `main` 只是 1×1、隐藏、不聚焦的宿主窗口，不是设置或面板界面。

### 翻译、解释与追问链路

1. 工具条通过 `toolbar_action` 把 `(selection, scenario)` 放进 `pending_panel` 并打开 `panel`。
2. `PanelApp.vue` 挂载时通过 `take_panel_context` 取走上下文，同时读取配置；前端检测语言并调用 `build_first_messages` 生成 system prompt 与首轮消息。
3. 前端按 `scenario_models` 选择提供方和模型，通过 `start_chat` 提交 `ChatRequest`。
4. Rust 解析 Key，向 `${base_url}/chat/completions` 发起流式请求，再以 `wp://chat-chunk` 事件发送 `chunk` / `done` / `error`。
5. 面板按 `request_id` 区分当前展示流、过期流和摘要流，以 `markdown.ts` 渲染增量内容。网络错误最多重试两次；已经产生输出后不重试，避免重复文本。
6. 追问保留原文与对话历史；超过阈值后压缩早期轮次，并通过独立的摘要请求生成真实摘要。翻译追问切换到 `chat` 场景。

### 配置与 CLI

- 用户配置路径由 `dirs::config_dir()/wordpick/config.toml` 得出：Windows 通常为 `%APPDATA%\wordpick\config.toml`，macOS 为 `~/Library/Application Support/wordpick/config.toml`。
- 首次运行生成用户配置；解析失败时重命名为 `config.toml.bak`，并回退到编译进二进制的默认配置。
- Key 读取优先级为系统安全存储，其次配置中的 `${ENV_NAME}` 或明文。保存设置会写文件并更新内存态，但监听线程、快捷键等启动时捕获的配置仍需重启后完全生效。
- 当前 Rust 配置结构按完整 `AppConfig` 反序列化用户文件；不要假设缺省字段会自动与内置默认逐字段合并。
- `wordpick run|settings|stop|status` 由 `bin/wordpick.js` 处理；`run`/`settings` 查找 npm 包内或 Cargo 构建目录中的二进制，`status`/`stop` 使用 pid 文件。

## 构建与开发命令

前置环境：Node.js 18+、pnpm、Rust 工具链；Windows 运行时需要 WebView2。CI 使用 Node.js 20 和 stable Rust。

```bash
# 安装依赖；源码开发可跳过 npm postinstall 的 Release 二进制下载
WORDPICK_SKIP_BINARY_DOWNLOAD=1 pnpm install

# 完整桌面应用：自动启动 Vite（5173）并编译/运行 Rust
pnpm tauri dev

# 仅启动 Web 前端；Tauri commands/events 在普通浏览器中不可用
pnpm dev
pnpm preview

# 前端检查与产物
pnpm typecheck
pnpm test
pnpm build

# 桌面发布构建；beforeBuildCommand 会先执行 pnpm build
pnpm tauri build
```

Rust 命令在 `src-tauri/` 下执行：

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test
cargo check --all-targets
cargo clippy --all-targets
```

Vite 使用严格固定端口。修改 `vite.config.ts` 的端口时，必须同步修改 `src-tauri/tauri.conf.json` 的 `build.devUrl`。新增窗口页面时，同步检查 HTML 入口、Vite `rollupOptions.input`、Rust 窗口 URL/label 和 capability 的 `windows` 列表。

`tauri.conf.json` 当前配置的 bundle targets 是 `app` 和 `nsis`；实际可用产物由构建平台决定。

## 代码风格与修改约定

- 注释、文档和界面文案主要使用中文；标识符使用英文。保留现有需求编号（如 `FR-5.4`）注释，它们用于追踪验收要求。
- TypeScript/JavaScript/Vue 使用 2 空格缩进、单引号、通常不写分号；ESM 导入，前端路径别名使用 `@/`。
- Vue 使用 `<script setup lang="ts">` 和组合式 API；视图集中在 `src/views/`，不要按旧规划移到 `src/components/`。
- 样式使用 Tailwind 4 utility class。SFC 的 scoped 样式中若使用 `@apply`，按现有写法添加 `@reference "tailwindcss"`。
- Rust 使用标准 `rustfmt` 风格和平台 `#[cfg(...)]` 门控；平台 API 放在对应实现中，公共数据结构保持在平台无关模块。
- UI 不直接实现模型 HTTP 或系统选区逻辑。新系统/模型能力经 Rust 实现，再由 `commands.rs` 和 `src/lib/*client.ts`/`bridge*.ts` 暴露。
- 新 Tauri command 需要在 `src-tauri/src/lib.rs` 的 `invoke_handler!` 注册，并添加对应前端封装。使用新的插件 API 时同步收窄地更新 `src-tauri/capabilities/default.json`。
- 修改跨端结构时同时更新 Rust 与 TypeScript：`Selection`、`PanelContext`、`Scenario`、`ChatRequest`、`ChatEvent`、`AppConfig`、`ProviderConfig`。注意 serde 的 `snake_case` 枚举值和 Tauri 参数转换。
- 修改配置字段时同步更新 `config/default.toml`、Rust 配置结构、前端 `AppConfig`、设置界面和测试；考虑已有用户配置的向后兼容。
- `src/lib/position.ts` 与 `src-tauri/src/windows/position.rs` 是同一算法的双实现；`config-shared.ts` 也承担 Rust 行为的可执行规格。修改任一侧时同步另一侧及测试。
- 内置提供方模板同时存在于 `config/default.toml`、`llm/providers.rs` 和 `settings-client.ts`，新增或修改模板时保持一致。
- 项目没有 ESLint/Prettier 脚本，也没有独立的 Vitest 配置文件；不要假定存在这些工具。

## 测试与验证

### 自动化测试

```bash
# 根目录
pnpm typecheck
pnpm test
pnpm build

# src-tauri/
cargo fmt --all -- --check
cargo test
cargo check --all-targets
```

Vitest 测试位于 `src/lib/__tests__/`，当前覆盖：

- 弹窗定位与多显示器工作区；
- 本地语言检测和默认目标语言；
- 对话历史压缩及原文保留；
- 配置占位符/层级选择纯函数；
- Key 掩码；
- Markdown 增量渲染、HTML 禁用的安全行为；
- 面板流式、完成与错误状态的响应式更新；
- CLI 参数、平台二进制名和 pid 路径。

Rust 单元测试内嵌在 `config.rs`、`secure.rs` 和 `windows/position.rs`。当前没有端到端测试框架、模型 HTTP mock 或真实系统选区的自动化测试。

2026-11-18 在当前 Windows 环境实际验证：Vitest 8 个文件共 40 项测试通过，Rust 10 项单元测试通过；`vue-tsc`、Vite 三入口构建、Rust fmt check 和 `cargo check --all-targets` 均通过。Vite 构建仅有关于同一 Tauri event 模块同时静态/动态导入的 chunk 提示。

### 手工验证

涉及以下修改时，仅跑单元测试不够，应使用 `pnpm tauri dev` 做真机验证：

- 选区/剪贴板：浏览器、编辑器、Office、终端，拖选、取消、切换应用和并发复制。
- 窗口：多显示器、高 DPI、屏幕边缘、焦点不抢占、面板失焦/钉住、重复打开设置。
- 模型：使用测试提供方验证 SSE、停止、重试、429/401、断流和追问历史；不要把真实 Key 写入测试或日志。
- 托盘/生命周期：单实例、快捷键、开机自启、退出以及 CLI `run/settings/stop/status`。

## CI 与发布流程

### CI

`.github/workflows/ci.yml` 在 `main` push 和所有 pull request 上运行：

- 前端 Ubuntu job：Node 20，`npm ci`（设置 `WORDPICK_SKIP_BINARY_DOWNLOAD=1`），随后 typecheck、Vitest 和 Vite build。
- Rust matrix job：macOS 与 Windows，依次执行 `cargo fmt --all -- --check`、`cargo test`、`cargo check --all-targets`。

仓库同时维护 `package-lock.json` 和 `pnpm-lock.yaml`；变更依赖时两者都要更新，避免本地与 CI 解析结果脱节。

### Release 与 npm

`.github/workflows/release.yml` 由 `v*` tag 或 GitHub Actions 手动触发：

1. macOS latest 构建 `darwin-arm64`，macOS 13 构建 `darwin-x64`，Windows latest 构建 `win32-x64`。
2. `tauri-apps/tauri-action` 创建草稿 GitHub Release。
3. 桌面构建完成后，`publish-npm` 使用 `NPM_TOKEN` 发布公开 npm 包。

发布前同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 的版本号。npm `files` 仅包含 `bin`、`scripts`、`dist`、`README.md` 和 `LICENSE`；安装时 `scripts/postinstall.js` 根据 platform/arch 下载 `wordpick-<platform>-<arch>[.exe]`。

postinstall 支持：

- `WORDPICK_SKIP_BINARY_DOWNLOAD=1`：跳过下载；
- `WORDPICK_BINARY_MIRROR`：替换 Release 下载镜像前缀；
- 下载失败只警告，不阻断 npm 安装。

## 安全与平台注意事项

- API Key 使用 service `com.wordpick.app`、provider 名作为账号写入系统安全存储。界面只显示 `****后4位`；不要在日志、错误、测试快照或文档中输出完整 Key。
- 优先使用 keyring 或 `${ENV_NAME}`，不要提交真实用户配置、`.env` 或下载的含敏感环境信息的文件。
- 网络请求只应发送到用户配置的 provider Base URL。不要新增隐式遥测或未声明的模型端点。
- AI 输出按不可信内容处理。`markdown-it` 当前设置 `html: false`；使用 `v-html` 的链路必须保留此约束，不要直接开启原始 HTML。
- `src-tauri/tauri.conf.json` 当前为 `csp: null`，不能假设已有 CSP 防护；扩大 Tauri capability 或窗口权限前需确认必要性。
- Windows 窗口创建不能在同步 command/事件回调线程直接执行，现有代码会投递到 Tauri 异步运行时，避免 WebView2 死锁。
- 窗口定位混用系统 API 与 Tauri 物理像素；不要再次除以 DPI scale。工具条的隐藏→定位→显示→置顶需保持主线程顺序。
- 工具条依赖 `WS_EX_NOACTIVATE` 和低层键盘钩子来避免抢焦点。仅在热区内吞掉 `1/2/3/Esc`；其他按键只隐藏可见工具条，必须继续传递给原应用。不要让工具条普通聚焦。
- Windows 剪贴板探测需串行并避开非文本内容；WinEvent 高频路径只读 UIA，不写剪贴板。终端窗口禁止模拟 `Ctrl+C`，否则可能向控制台进程发送中断。
- macOS 自动选区依赖辅助功能权限；未授权时保留快捷键降级路径。

## 参考资料

- `README.md`：安装、环境变量、用户路径与常用命令。
- `docs/usage.md`：用户操作、触发模式和常见问题。
- `docs/model-config.md`：OpenAI 兼容提供方、Key 优先级和成本控制。
- `doc/划词助手需求文档.md`：FR/NFR 验收要求及原始技术方案；其中目录和依赖选型可能属于规划，以当前代码为准。
- `doc/plan.md`：各 Phase 的实现和审查记录。

---

> **文档版本**：v1.0
> **最后更新**：2026-11-18
> **适用项目版本**：WordPick 1.0.0
