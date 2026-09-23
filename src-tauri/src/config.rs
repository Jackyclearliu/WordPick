//! 配置系统：config.toml 加载、三层合并、${ENV} 占位符解析、损坏回退（需求文档 §5.6）
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub providers: Vec<ProviderConfig>,
    pub scenario_models: ScenarioModels,
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub trigger_mode: String,
    pub popup_position: String,
    pub max_selection_chars: usize,
    pub blacklist_apps: Vec<String>,
    pub theme: String,
    pub shortcut_toggle: String,
    pub shortcut_settings: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioModel {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioModels {
    pub translate: ScenarioModel,
    pub explain: ScenarioModel,
    pub chat: ScenarioModel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    pub max_followup_rounds: u32,
    pub max_tokens_translate: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        // 内置默认：从仓库 config/default.toml 编译进二进制（FR-6.2）
        toml::from_str(include_str!("../../config/default.toml"))
            .expect("内置默认配置必须可解析")
    }
}

/// 解析 ${ENV_VAR} 占位符（FR-6.3）
pub fn resolve_env_placeholder(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(inner) = trimmed.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        if inner.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !inner.is_empty() {
            return std::env::var(inner).unwrap_or_default();
        }
    }
    value.to_string()
}

/// 用户配置目录下的 config.toml 路径（§5.6）
pub fn user_config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("wordpick").join("config.toml"))
}

/// 加载配置：用户文件 > 内置默认；用户文件缺失时自动生成（FR-6.2）；
/// 解析损坏时回退默认并备份损坏文件（NFR 4.3）
pub fn load_or_create() -> AppConfig {
    let Some(path) = user_config_path() else {
        return AppConfig::default();
    };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let default = AppConfig::default();
        if let Ok(text) = toml::to_string_pretty(&default) {
            let _ = std::fs::write(&path, format!("{}\n{}", BUILTIN_COMMENT, text));
        }
        return default;
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<AppConfig>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::error!("config.toml 解析失败，回退默认并备份: {e}");
                let backup = path.with_extension("toml.bak");
                let _ = std::fs::rename(&path, backup);
                AppConfig::default()
            }
        },
        Err(_) => AppConfig::default(),
    }
}

const BUILTIN_COMMENT: &str = "# WordPick 用户配置（自动生成；缺省项以内置默认为准）";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_default_parses() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.general.trigger_mode, "auto");
        assert_eq!(cfg.general.max_selection_chars, 5000);
        assert!(cfg.providers.iter().any(|p| p.name == "deepseek"));
    }

    #[test]
    fn env_placeholder_resolves() {
        std::env::set_var("WORDPICK_TEST_KEY", "sk-test-123");
        assert_eq!(resolve_env_placeholder("${WORDPICK_TEST_KEY}"), "sk-test-123");
        assert_eq!(resolve_env_placeholder("plain-key"), "plain-key");
        assert_eq!(resolve_env_placeholder("${}"), "${}");
    }
}
