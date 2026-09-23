/**
 * 面板会话状态（Pinia）：面板是会话制窗口（§5.4），store 承载一次会话的完整上下文。
 * Phase 2：翻译闭环；Phase 3：解释场景与多轮追问在此基础上扩展。
 */
import { defineStore } from 'pinia'
import { ref, shallowRef, type Ref } from 'vue'

import type { ChatMessage, ErrorKind, Scenario } from '@/lib/llm-client'
import { createIncrementalRenderer, type IncrementalRenderer } from '@/lib/markdown'
import type { Selection } from '@/lib/bridge-types'

export type PanelStatus = 'idle' | 'streaming' | 'done' | 'error' | 'stopped'

export interface PanelSession {
  selection: Selection
  scenario: Scenario
  /** 翻译场景的语言对 */
  sourceLang: string
  targetLang: string
  messages: ChatMessage[]
  status: PanelStatus
  error: { kind: ErrorKind; message: string } | null
  /** 当前流式输出的增量渲染器 */
  renderer: IncrementalRenderer
  /** 本轮追问次数（成本控制，默认上限 10） */
  followupRounds: number
}

export const useSessionStore = defineStore('session', () => {
  const session: Ref<PanelSession | null> = shallowRef(null)

  function begin(input: {
    selection: Selection
    scenario: Scenario
    sourceLang: string
    targetLang: string
    firstMessages: ChatMessage[]
  }): PanelSession {
    const s: PanelSession = {
      selection: input.selection,
      scenario: input.scenario,
      sourceLang: input.sourceLang,
      targetLang: input.targetLang,
      messages: input.firstMessages,
      status: 'idle',
      error: null,
      renderer: createIncrementalRenderer(),
      followupRounds: 0,
    }
    session.value = s
    return s
  }

  function reset(): void {
    session.value = null
  }

  return { session, begin, reset }
})

export const isTranslate = (s: Scenario) => s === 'translate'
export const isExplain = (s: Scenario) => s === 'explain' || s === 'chat'
