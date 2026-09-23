# 模型配置指南

WordPick 的模型接入以 **OpenAI 兼容协议**为基准：任意提供方只需 `base_url` + `api_key` + `model` 三项即可接入，天然支持 OneAPI、OpenRouter 等聚合网关。

## 场景化模型选择

高频场景用「快+便宜」的 Flash 级模型，重场景用更强的 Pro 级模型（设置 → 场景模型）：

| 场景 | 默认推荐 | 说明 |
| --- | --- | --- |
| 翻译 | DeepSeek V4 Flash / Qwen3.7-Flash | 最高频调用，Flash 级速度最快、成本可忽略 |
| 解释 / 追问 | DeepSeek V4 Pro | 解释质量取决于模型本体能力；追求长上下文可切 Kimi K2.6 |

## 内置提供方模板

| 提供方 | Base URL | 默认模型 | Key 获取 |
| --- | --- | --- | --- |
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-v4-flash` | platform.deepseek.com → API keys |
| Moonshot (Kimi) | `https://api.moonshot.cn/v1` | `kimi-k2.6` | platform.moonshot.cn → API Key 管理 |
| Qwen（阿里云百炼） | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen3.7-flash` | 阿里云百炼控制台 → API-KEY |
| OpenAI | `https://api.openai.com/v1` | `gpt-5.6-sol` | platform.openai.com → API keys |

> 模型价格变动频繁，以上仅为默认设计依据；工具内所有模型名与参数均由你的配置决定。

## API Key 的三种提供方式（优先级从高到低）

1. **系统安全存储**：设置界面输入并「保存 Key」→ 存入 macOS Keychain / Windows Credential Manager
2. **环境变量占位符**：配置文件写 `"${DEEPSEEK_API_KEY}"`，运行时从进程环境解析（推荐：配置文件中不落明文）
3. **配置明文**：本地自用时可直接写明文

任何界面、日志、错误上报均不回显完整 Key（仅后 4 位）。

## 自定义提供方（OneAPI / OpenRouter / 私有网关）

设置 → 「+ 自定义」，填入网关的 OpenAI 兼容地址即可，例如：

```toml
[[providers]]
name = "openrouter"
base_url = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"
default_model = "anthropic/claude-sonnet-4"
```

## 成本控制设计

- 对话历史自动压缩：超过 8 轮后早期轮次摘要化（保留原文与最近 4 轮）
- 翻译请求 `max_tokens` 上限（默认 2048，可配置）
- 追问超过 10 轮时提示开新会话
- 建议翻译场景固定使用 Flash 级模型
