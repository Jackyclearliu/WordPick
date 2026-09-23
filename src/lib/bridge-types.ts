/** 跨端共享类型（与 Rust selection::Selection 对齐） */

export type SelectionSource = 'ax' | 'uia' | 'shortcut'

export interface Selection {
  text: string
  anchor: [number, number] | null
  source: SelectionSource
}

export interface PanelContext {
  selection: Selection
  scenario: 'translate' | 'explain' | 'chat'
}

/** 面板 on mount 时从 Rust 取走上下文 */
export async function takePanelContext(): Promise<PanelContext | null> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<PanelContext | null>('take_panel_context')
}
