import { describe, expect, it } from 'vitest'
import {
  buildSummaryRequest,
  compressHistory,
  countRounds,
  needsCompression,
} from '../chat-history'
import type { ChatMessage } from '../llm-client'

function round(n: number): ChatMessage[] {
  return [
    { role: 'user', content: `第${n}轮问题` },
    { role: 'assistant', content: `第${n}轮回答` },
  ]
}

/** 构造 system + 原文 + n 轮追问的消息序列 */
function history(n: number): ChatMessage[] {
  return [
    { role: 'system', content: 'system prompt' },
    { role: 'user', content: '原文：这是一段需要解释的文本' },
    ...Array.from({ length: n }, (_, i) => round(i + 1)).flat(),
  ]
}

describe('对话历史压缩（FR-4.3）', () => {
  it('轮数统计', () => {
    expect(countRounds(history(10))).toBe(11) // 原文 + 10 轮
  })

  it('未超阈值不压缩', () => {
    const msgs = history(3)
    expect(needsCompression(msgs, 8)).toBe(false)
  })

  it('10 轮追问压缩后仍保留原文与 system（验收标准）', () => {
    const msgs = history(10)
    const r = compressHistory(msgs, 4)
    expect(r.originalKept).toBe(true)
    expect(r.messages[0].role).toBe('system')
    expect(r.messages[1].content).toContain('原文')
    // 最近 4 轮保留
    expect(r.messages.at(-1)?.content).toBe('第10轮回答')
    expect(r.messages.at(-2)?.content).toBe('第10轮问题')
    // 早期轮次被压缩
    expect(r.compressedCount).toBeGreaterThan(0)
    expect(r.messages.some((m) => m.content.includes('[历史压缩]'))).toBe(true)
    expect(r.messages.some((m) => m.content === '第1轮回答')).toBe(false)
  })

  it('压缩后消息条数显著下降', () => {
    const before = history(10).length
    const after = compressHistory(history(10), 4).messages.length
    expect(after).toBeLessThan(before - 4)
  })

  it('摘要请求构造（不超过 200 字要求 + 角色标注）', () => {
    const dropped = round(1).concat(round(2))
    const req = buildSummaryRequest(dropped)
    expect(req[0].role).toBe('system')
    expect(req[1].content).toContain('用户：第1轮问题')
    expect(req[1].content).toContain('助手：第2轮回答')
  })

  it('空历史安全返回', () => {
    const r = compressHistory([])
    expect(r.messages).toEqual([])
    expect(r.originalKept).toBe(false)
  })
})
