/**
 * 与 Rust 模型适配层通信的封装（lib/llm-client.ts，NFR 4.5：UI 不直接触碰 OS/网络）
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type Scenario = 'translate' | 'explain' | 'chat'
export type ErrorKind = 'network' | 'rate_limited' | 'invalid_key' | 'bad_request' | 'server' | 'unknown'

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

export interface ProviderConfig {
  name: string
  base_url: string
  api_key: string
  default_model: string
}

export interface ChatRequest {
  request_id: string
  provider: ProviderConfig
  model: string
  scenario: Scenario
  messages: ChatMessage[]
  max_tokens?: number
}

export type ChatEvent =
  | { type: 'chunk'; request_id: string; delta: string }
  | { type: 'done'; request_id: string; usage: unknown }
  | { type: 'error'; request_id: string; kind: ErrorKind; message: string }

export function startChat(req: ChatRequest): Promise<void> {
  return invoke('start_chat', { req })
}

export function stopChat(requestId: string): Promise<void> {
  return invoke('stop_chat', { requestId })
}

export function buildFirstMessages(
  scenario: Scenario,
  sourceLang: string,
  targetLang: string,
  text: string,
): Promise<ChatMessage[]> {
  return invoke('build_first_messages', {
    scenario,
    sourceLang,
    targetLang,
    text,
  })
}

export function testConnection(provider: ProviderConfig, model: string): Promise<number> {
  return invoke('test_connection', { provider, model })
}

export function setPanelPinned(pinned: boolean): Promise<void> {
  return invoke('set_panel_pinned', { pinned })
}

/** 订阅流式事件；返回取消订阅函数 */
export function onChatEvent(handler: (event: ChatEvent) => void): Promise<UnlistenFn> {
  return listen<ChatEvent>('wp://chat-chunk', (e) => handler(e.payload))
}

export function newRequestId(): string {
  return `req-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}
