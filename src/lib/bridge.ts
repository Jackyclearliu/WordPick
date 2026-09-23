import { invoke } from '@tauri-apps/api/core'

/** UI 层与 Rust 核心层的通信封装（lib/llm-client.ts 的通用部分，NFR 4.5） */

export type ToolbarAction = 'translate' | 'explain' | 'settings'

export async function toolbarAction(action: ToolbarAction): Promise<void> {
  await invoke('toolbar_action', { action })
}

export async function hideToolbar(): Promise<void> {
  await invoke('hide_toolbar')
}
