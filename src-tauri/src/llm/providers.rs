//! 内置提供方模板：Phase 2 填充默认 base_url 与说明。
pub struct ProviderTemplate {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub docs_url: &'static str,
}

pub const TEMPLATES: &[ProviderTemplate] = &[
    ProviderTemplate {
        name: "deepseek",
        base_url: "https://api.deepseek.com/v1",
        default_model: "deepseek-v4-flash",
        docs_url: "https://platform.deepseek.com/",
    },
    ProviderTemplate {
        name: "moonshot",
        base_url: "https://api.moonshot.cn/v1",
        default_model: "kimi-k2.6",
        docs_url: "https://platform.moonshot.cn/",
    },
    ProviderTemplate {
        name: "qwen",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen3.7-flash",
        docs_url: "https://bailian.console.aliyun.com/",
    },
    ProviderTemplate {
        name: "openai",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-5.6-sol",
        docs_url: "https://platform.openai.com/",
    },
];
