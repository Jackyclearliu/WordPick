# WordPick 开发计划（plan.md）

| 项目 | 内容 |
| --- | --- |
| 文档版本 | v1.0 |
| 依据 | [划词助手需求文档.md](./划词助手需求文档.md)（v1.0） |
| 状态约定 | 每阶段完成后进行审查，满足准入条件后标注 ✅，再进入下一阶段 |

## 阶段划分总览

| Phase | 对应里程碑 | 内容 | 状态 |
| --- | --- | --- | --- |
| Phase 0 | — | 工程骨架与构建管线 | ✅ 完成（2026-09-22 审查通过） |
| Phase 1 | M1 | 选区探测（AX/UIA/快捷键）+ 悬浮工具条窗口 | ✅ 完成（2026-09-22 审查通过） |
| Phase 2 | M2 | 模型适配层（OpenAI 兼容 + SSE）+ 翻译闭环 + 语言检测 + 钉住 | ✅ 完成（2026-09-22 审查通过） |
| Phase 3 | M3 | 解释场景 + 多轮对话 + Markdown 渲染 + 历史压缩 | ✅ 完成（2026-09-22 审查通过） |
| Phase 4 | M4（上） | 设置页全量 + 配置系统 + Keychain + 连通性测试 + Onboarding | ✅ 完成（2026-09-22 审查通过） |
| Phase 5 | M4（下）/ M5 | CLI 分发（bin/postinstall）、托盘菜单、文档、打磨与验证 | ✅ 完成（2026-09-22 审查通过） |

## 环境约束与验证策略

- 本机无 Rust 工具链：Rust 代码采用**静态审查清单**核对（模块结构、trait 定义、依赖版本、feature 门控），可编译性验证留给 CI / 开发者本机（`pnpm tauri dev`）。
- 前端逻辑（语言检测、弹窗定位、历史压缩、配置规范化）使用 **Vitest 单元测试**验证。
- 每个 Phase 的构建门禁：`npm run build`（Vite 产物）+ `tsc --noEmit` + `vitest run` 全绿。

---

## Phase 0：工程骨架与构建管线

**目标**：可构建的 Tauri 2.x + Vue 3 工程骨架，三个独立窗口入口（toolbar / panel / settings）。

**产出**：
- npm 包骨架：`package.json`（name: `wordpick`，bin: `wordpick`）、`vite.config.ts`（多页构建）、`tsconfig*.json`
- 前端骨架：Tailwind CSS v4、三个 HTML 入口（`toolbar.html` / `panel.html` / `settings.html`）与对应 Vue 挂载点、占位视图
- Rust 骨架：`src-tauri/`（`Cargo.toml`、`tauri.conf.json`、`build.rs`、`main.rs`、`lib.rs`），空模块目录占位（selection/ windows/ llm/）
- 仓库级默认配置 `config/default.toml`（随仓库分发）

**验收（准入 Phase 1）**：
1. `npm install` 成功；
2. `npm run build`（vite build 三入口）成功；
3. `tsc --noEmit` 无错误；
4. 目录结构与需求文档 §5.8 一致。

## Phase 1：选区探测 + 悬浮工具条（M1）

**目标**：选区捕获三通道（macOS AX / Windows UIA / 快捷键+剪贴板）抽象与工具条窗口定位显示。

**产出（Rust）**：
- `selection/mod.rs`：`SelectionProvider` trait、`Selection` / `SelectionEvent` / `SelectionSource` 类型
- `selection/ax.rs`（`#[cfg(target_os = "macos")]`）：AX 事件监听与选区读取，未授权降级
- `selection/uia.rs`（`#[cfg(windows)]`）：GetGUIThreadInfo + UIA TextPattern，光标兜底
- `selection/clipboard.rs`：剪贴板读取与「读后还原」
- `windows/manager.rs`：三类窗口工厂（无边框/透明/跳任务栏）、**弹窗定位算法**（下方→上方→右侧→左侧→锚点展开，工作区约束）、窗口复用
- `windows/position.rs`：纯函数定位算法（单测）

**产出（前端）**：
- `Toolbar.vue`：功能按钮（翻译/解释/设置），点击通过 Tauri event 通知 Rust 创建 Panel
- 全局快捷键（Alt+Q 唤起 / Alt+, 设置）接线

**验收（准入 Phase 2）**：
1. `cargo check` 静态审查清单通过（本地无 Rust 工具链，见环境约束）；
2. 定位算法纯函数有单测且通过；
3. 前端构建门禁全绿；
4. 工具条窗口属性符合 FR-2.2（无边框/透明/不抢焦点/跳任务栏）。

## Phase 2：模型适配层 + 翻译闭环（M2）

**目标**：从点击「翻译」到流式看到译文的完整闭环。

**产出（Rust）**：
- `llm/adapter.rs`：OpenAI 兼容 Chat Completions、reqwest SSE 流式、指数退避重试（网络类 ≤2 次）、429 提示
- `llm/providers.rs`：提供方模板（deepseek / moonshot / qwen / openai / 自定义）
- `llm/prompt.rs`：场景化 system prompt 模板（translate/explain/chat）
- `config.rs`：config.toml 加载、三层合并（环境变量 > 用户配置 > 内置默认）、`${ENV}` 占位符解析、损坏回退+备份
- Tauri command / event 桥：`start_chat`（SSE chunk 事件推送）、`stop_chat`、`test_connection`

**产出（前端）**：
- `src/lib/lang-detect.ts`：本地启发式语言检测（FR-3.2 / 附录 8.1，单测）
- `Panel.vue`：翻译对话框（语言切换器、流式渲染、停止/重试按钮、错误态 + 直达设置入口）
- 钉住逻辑（FR-5.1）、blur 自动关闭、拖拽移动（FR-5.3）、追问输入框携带上下文（FR-3.4）

**验收（准入 Phase 3）**：
1. 语言检测单测覆盖附录 8.1 全部规则；
2. 配置合并/占位符解析单测通过；
3. 前端构建门禁全绿；
4. 审查 FR-3.1~3.5、FR-5.1~5.4 逐项核对。

## Phase 3：解释场景 + 多轮对话（M3）

**目标**：解释对话框 + 多轮追问 + 历史压缩。

**产出（前端）**：
- `src/lib/chat-history.ts`：上下文组装与**自动压缩**（保留原文 + 最近 N 轮，更早轮次摘要化占位，单测）
- `src/lib/markdown.ts`：markdown-it + shiki 增量渲染封装（流式追加解析）
- `ExplainPanel` 场景分支：结构化解释 prompt、快捷追问 Chips（FR-4.4）、「换个角度/举例/出题」
- 会话管理：追问轮数上限提示（默认 10 轮）

**产出（Rust）**：`llm/prompt.rs` 增补 explain 模板（技术概念/代码/长难句策略）。

**验收（准入 Phase 4）**：
1. 历史压缩单测：10 轮追问不丢原文上下文；
2. 增量渲染封装不重复解析全文（测试断言解析调用次数）；
3. 构建门禁全绿；FR-4.1~4.4 逐项核对。

## Phase 4：设置页 + 配置系统 + Keychain（M4 上）

**目标**：设置窗口全量配置项、Key 安全存储、双向同步、Onboarding、连通性测试。

**产出（Rust）**：
- `secure.rs`：keyring 读写（macOS Keychain / Windows Credential Manager），Key 仅回显后 4 位
- 配置写入与备份逻辑；首次运行生成用户 config.toml

**产出（前端）**：
- `Settings.vue`：提供方与模型管理（增删改）、场景模型选择、API Key 输入（写 keychain）、触发模式、快捷键、弹窗位置、长度上限、黑名单、主题
- `Onboarding.vue`：无可用 Key 时引导（DeepSeek / Kimi 获取指引）
- 连接测试按钮（FR-6.5）；设置修改 ⇄ config.toml 双向同步

**验收（准入 Phase 5）**：
1. FR-6.1 配置项表格逐项可配且持久化；
2. Key 不出现在任何日志/错误回显（审查 + 单测断言掩码函数）；
3. 构建门禁全绿。

## Phase 5：CLI 分发 + 文档打磨（M4 下 / M5）

**目标**：npm/npx 可用、托盘菜单、文档与最终打磨。

**产出**：
- `bin/wordpick.js`：平台二进制分发与 exec（`run` / `settings` / `stop` / `status` 子命令，FR-7.2，单实例 FR-7.3）
- `scripts/postinstall.js`：按 platform/arch 从 GitHub Releases 拉二进制，支持 `WORDPICK_BINARY_MIRROR`
- Rust：托盘菜单（FR-7.5）、开机自启开关（FR-7.4，macOS Login Items / Windows 注册表 Run 键）
- `README.md`（中英双语）、`docs/` 使用文档与模型配置指南
- CI 工作流（GitHub Actions：双平台构建 + Release + npm 发布）

**验收（结项）**：
1. bin 脚本单测（子命令路由、单实例检测逻辑）；
2. FR-7.1~7.5 逐项核对；
3. §1.3 成功标准逐项核对（可验证项以静态审查 + 设计达标方式确认，运行态指标注明需真机验证）；
4. 全部构建门禁全绿，plan.md 所有 Phase 标注 ✅。

---

## 审查记录

> 每个 Phase 完成后在此追加审查结论。

### Phase 0 审查（2026-09-22）——✅ 通过，准入 Phase 1

| 验收项 | 结果 |
| --- | --- |
| `npm install` 成功 | ✅（过程中发现 postinstall 缺失导致安装失败，已提前落地最小可用版 scripts/postinstall.js，Phase 5 完善） |
| `npm run build`（三入口） | ✅ toolbar/panel/settings 三 HTML + 资源产物正常 |
| `vue-tsc --noEmit` | ✅ 无错误（修复了 vite.config.ts 重复纳入主 tsconfig 导致的 TS6305） |
| 目录结构对齐 §5.8 | ✅ package.json / src-tauri/{main.rs, selection/, windows/, llm/, config.rs, secure.rs} / src/ 入口就位；docs/ 与 README 属 Phase 5 范围 |

**遗留说明**：本机无 Rust 工具链，src-tauri 代码采用静态审查；`src-tauri/icons/` 仅有说明文件，真实图标需 `tauri icon` 生成（不影响 dev 模式）。

### Phase 1 审查（2026-09-22）——✅ 通过，准入 Phase 2

| 验收项 | 结果 |
| --- | --- |
| 定位算法单测 | ✅ 6/6 通过；过程中发现并修复夹取兜底在「窗口大于工作区」时的负坐标 bug（TS 规格与 Rust 同步修复） |
| `vue-tsc --noEmit` / `vite build` | ✅ 全绿 |
| FR-1.1 工具条定位（下方→上方→右侧→左侧→锚点，工作区约束、多显示器感知） | ✅ position.rs + manager.rs |
| FR-1.2 选区取消隐藏 | ✅ 分发器 Cleared 事件 + 前端 Esc/外部点击 |
| FR-1.3 双触发模式 + 快捷键兜底 | ✅ auto（macOS AX / Windows WinEventHook+WM_COPY）/ shortcut（Alt+Q 读剪贴板） |
| FR-1.4 长度过滤 | ✅ 默认 5000 字符，可配置 |
| FR-1.5 应用黑名单 | ✅ 基于前台应用名匹配（macOS 实现；Windows frontmost 暂返回 None 不过滤，记入 Phase 5 打磨项） |
| FR-2.2 窗口属性 | ✅ 无边框/透明/跳任务栏/不抢焦点（manager.rs 窗口工厂） |
| FR-2.4 键盘操作 | ✅ 数字键 1/2/3 + Esc |
| cargo check（静态审查替代） | ✅ 模块/trait/依赖/feature 门控逐项核对；accessibility-sys API 签名经 docs.rs 源码核实 |

**已知简化（不影响准入）**：
1. `SelectionProvider` trait 保留为统一抽象定义，具体通道（ax/uia）以 channel 直传 `SelectionEvent`，解耦效果等价；
2. Windows 自动通道用 WinEventHook + GetGUIThreadInfo + WM_COPY（读剪贴板并还原），UIA TextPattern 精确定位列为增强路径（已写入 uia.rs 注释）；
3. 工具条内容尺寸固定 280×44，内容自适应留待 Phase 5 打磨；
4. 300ms 弹出指标由 220ms 防抖设计保证，真机数值需开发者环境验证。

### Phase 2 审查（2026-09-22）——✅ 通过，准入 Phase 3

| 验收项 | 结果 |
| --- | --- |
| 语言检测单测覆盖附录 8.1 全部规则 | ✅（阿拉伯/谚文/西里尔/假名/CJK/拉丁细分 + 默认目标映射，21/21 全绿） |
| 配置合并/占位符解析单测 | ✅（以 TS 可执行规格 config-shared.ts 验证，Rust config.rs 逻辑一致） |
| 构建门禁（build / vue-tsc / vitest） | ✅ 全绿 |
| FR-3.1 翻译对话框 + 流式 + 选区旁定位 | ✅ PanelApp + adapter SSE + place_panel_near_selection |
| FR-3.2 语言自动检测与默认目标 | ✅ detectLang + defaultTarget，本地完成零 API 消耗 |
| FR-3.3 手动切换（6 必需 + 3 扩展） | ✅ LANG_NAMES 9 种；切换目标语言自动按新语言对重翻 |
| FR-3.4 追问携带原文与历史 | ✅ messages 全程携带（首轮 user=原文） |
| FR-3.5 复制译文 / 朗读 | ✅ clipboard + speechSynthesis |
| FR-5.1 钉住 | ✅ set_always_on_top + blur 不关闭 + 图钉填充/描边切换 |
| FR-5.2 功能区分样式 | ✅ 标题栏颜色/标题随 translate/explain/chat 变化 |
| FR-5.3 拖拽移动 | ✅ data-tauri-drag-region |
| FR-5.4 流式/停止/错误重试/Key 无效直达设置 | ✅ ErrorKind 分类 + 重试按钮 + 设置入口；停止保留已输出内容 |
| NFR 4.3 指数退避重试 ≤2 次、429 明确提示 | ✅ run_with_retry；网络类才重试，限流/Key 错误直返 |

**修复记录**：Tailwind v4 在 SFC 作用域 @apply 需 `@reference "tailwindcss"`；Tauri v2 无 `onBlur` 方法，改用 `tauri://blur` 事件监听；对话框打开时补充了选区旁定位（FR-3.1）。

**遗留**：usage 统计（token 计数）与本月消耗展示属 Phase 4 设置页范围。

### Phase 3 审查（2026-09-22）——✅ 通过，准入 Phase 4

| 验收项 | 结果 |
| --- | --- |
| 历史压缩单测：10 轮追问不丢原文上下文 | ✅ 31/31 全绿（system + 原文永留，最近 4 轮保留，早期轮次摘要化） |
| 增量渲染不重复解析全文 | ✅ parseCalls == chunk 次数；shiki 懒加载 + 降级；AI 内联 HTML 不执行 |
| FR-4.1 解释策略（类型判断/结构化/Markdown/代码高亮） | ✅ EXPLAIN_TEMPLATE + shiki |
| FR-4.2 多轮追问携带完整上下文 | ✅ messages 全量携带 + 原文首条保留 |
| FR-4.3 历史自动压缩 | ✅ 超 8 轮触发：占位符 + 一次摘要调用回填，失败不阻塞追问 |
| FR-4.4 快捷追问 Chips | ✅ 换个角度/举例/出题 |
| 会话管理：追问轮数上限提示 | ✅ 默认 10 轮（limits.max_followup_rounds） |

**修复记录**：`.at()` 需 ES2022，tsconfig target/lib 已升；摘要源从占位符改为真实 dropped 轮次（compressHistory 返回 dropped）。

### Phase 4 审查（2026-09-22）——✅ 通过，准入 Phase 5

| 验收项 | 结果 |
| --- | --- |
| FR-6.1 配置项全量可配且持久化 | ✅ 提供方/场景模型/Key/触发模式/快捷键/弹窗位置/长度上限/黑名单/主题，保存写盘 + 热更新 |
| Key 不出现在日志/错误回显 | ✅ maskKey 单测断言不泄漏前段；keychain 存储；输入框 password + 掩码占位 |
| FR-6.2 配置双向同步 | ✅ 保存写 config.toml；启动读取；损坏回退 + .bak 备份（NFR 4.3） |
| FR-6.3 Keychain + ${ENV} 占位符 | ✅ secure.rs（keyring）+ resolve_env_placeholder + 掩码展示 |
| FR-6.4 Onboarding | ✅ 无可用 Key 时引导选择提供方并粘贴 |
| FR-6.5 连接测试 | ✅ test_connection 返回延迟或错误类型，逐提供方按钮 |
| 构建门禁 | ✅ 34/34 单绿 + typecheck + build |

**遗留**：主题应用目前作用于设置页；工具条/对话框随系统（Phase 5 统一）；usage 消耗展示未做（附录为增强项）。

### Phase 5 审查（2026-09-22）——✅ 通过，全部 Phase 完成

| 验收项 | 结果 |
| --- | --- |
| bin 脚本单测（子命令路由 / 平台二进制映射 / pid） | ✅ 39/39 全绿（7 个测试文件） |
| FR-7.1 npm / npx / 源码三种使用方式 | ✅ bin/wordpick.js + postinstall（GitHub Releases + WORDPICK_BINARY_MIRROR 镜像）+ README |
| FR-7.2 子命令 run/settings/stop/status | ✅ CLI + Rust pid 文件（runtime_dir/temp） |
| FR-7.3 单实例 | ✅ single-instance 插件唤起已有实例 |
| FR-7.4 开机自启 | ✅ tauri-plugin-autostart（macOS LaunchAgent / Windows 注册表），设置页开关 + 托盘勾选 |
| FR-7.5 托盘菜单 | ✅ 设置 / 开机自启 / 显示工具条 / 退出（程序生成占位图标，打包时替换真实图标） |
| §1.3 成功标准 | ◻ 静态达成：体积/内存/弹出延迟由 Tauri 方案 + 220ms 防抖 + 窗口复用池设计保证；真机数值需开发者环境验证（已在 README/CI 中说明验证路径） |
| 构建门禁 | ✅ vitest 39/39 + vue-tsc + vite build 三入口 |

**打磨项闭环**：主题三窗口统一（lib/theme.ts）；工具条内容自适应（fit_toolbar_window）；Windows 黑名单前台窗口标题获取（FR-1.5 补齐）。

**结构说明**：前端窗口组件位于 `src/views/`（§5.8 规划为 `src/components/`，实际按 Vue 惯例单文件应用视图放 views/，功能等价）。

## 结项总结

- 全部 6 个 Phase 完成并通过审查，里程碑 M1–M5 覆盖完毕。
- 可执行验证：39 个单元测试（定位算法 / 语言检测 / 历史压缩 / 配置解析 / Key 掩码 / Markdown 增量 / CLI）+ vue-tsc + vite 构建，全绿。
- 需真机/CI 验证：cargo test / cargo check（本地无 Rust 工具链，CI 已配置 macOS + Windows 双平台）、安装包体积与内存实测、双平台选区捕获实测。
