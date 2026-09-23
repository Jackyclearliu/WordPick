/** 设置页与 Rust 核心层的通信封装（FR-6） */
import { invoke } from '@tauri-apps/api/core'
import type { ChatMessage, ProviderConfig } from './llm-client'

export interface GeneralConfig {
  trigger_mode: 'auto' | 'shortcut'
  popup_position: 'below' | 'top-right'
  max_selection_chars: number
  blacklist_apps: string[]
  theme: 'system' | 'light' | 'dark'
  shortcut_toggle: string
  shortcut_settings: string
}

export interface ScenarioModel {
  provider: string
  model: string
}

export interface AppConfig {
  general: GeneralConfig
  providers: ProviderConfig[]
  scenario_models: Record<'translate' | 'explain' | 'chat', ScenarioModel>
  limits: { max_followup_rounds: number; max_tokens_translate: number }
}

export const PROVIDER_TEMPLATES: Array<{
  name: string
  base_url: string
  default_model: string
  docs_url: string
  key_hint: string
}> = [
  {
    name: 'deepseek',
    base_url: 'https://api.deepseek.com/v1',
    default_model: 'deepseek-v4-flash',
    docs_url: 'https://platform.deepseek.com/api_keys',
    key_hint: 'platform.deepseek.com → API keys',
  },
  {
    name: 'moonshot',
    base_url: 'https://api.moonshot.cn/v1',
    default_model: 'kimi-k2.6',
    docs_url: 'https://platform.moonshot.cn/console/api-keys',
    key_hint: 'platform.moonshot.cn → API Key 管理',
  },
  {
    name: 'qwen',
    base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    default_model: 'qwen3.7-flash',
    docs_url: 'https://bailian.console.aliyun.com/',
    key_hint: '阿里云百炼控制台 → API-KEY',
  },
  {
    name: 'openai',
    base_url: 'https://api.openai.com/v1',
    default_model: 'gpt-5.6-sol',
    docs_url: 'https://platform.openai.com/api-keys',
    key_hint: 'platform.openai.com → API keys',
  },
]

export function getConfig(): Promise<AppConfig> {
  return invoke('get_config')
}

export function saveConfig(config: AppConfig): Promise<void> {
  return invoke('save_config', { config })
}

export function saveApiKey(provider: string, key: string): Promise<void> {
  return invoke('save_api_key', { provider, key })
}

export function getApiKeyMask(provider: string): Promise<string | null> {
  return invoke('get_api_key_mask', { provider })
}

export function hasAnyApiKey(): Promise<boolean> {
  return invoke('has_any_api_key')
}

export function testProviderConnection(provider: ProviderConfig, model: string): Promise<number> {
  return invoke('test_connection', { provider, model })
}

/** Key 掩码函数（与 Rust secure::mask_key 一致；测试断言「不回显完整 Key」） */
export function maskKey(key: string): string {
  return key.length <= 4 ? '****' : `****${key.slice(-4)}`
}

export type { ChatMessage }
