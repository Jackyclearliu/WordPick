/**
 * 多轮对话历史组装与自动压缩（FR-4.3）：
 * 上下文随轮数增长时，保留 system、原文（首条 user）与最近 N 轮，
 * 更早轮次摘要化，防止超长上下文造成的费用与延迟上升（§5.5 成本控制）。
 */
import type { ChatMessage } from './llm-client'

/** 保留的最近轮数（每轮 = user + assistant 两条） */
export const KEEP_ROUNDS = 4

export function countRounds(messages: ChatMessage[]): number {
  return messages.filter((m) => m.role === 'user').length
}

export function needsCompression(messages: ChatMessage[], maxRounds = KEEP_ROUNDS * 2): boolean {
  return countRounds(messages) > maxRounds
}

export interface CompressionResult {
  messages: ChatMessage[]
  /** 被压缩掉的消息条数 */
  compressedCount: number
  /** 被压缩掉的消息（用于生成真实摘要，回填占位符） */
  dropped: ChatMessage[]
  /** 原文（首条 user）是否保留 */
  originalKept: boolean
}

/**
 * 压缩历史：system + 原文 + 最近 keepRounds 轮 + 占位摘要消息。
 * 原文永远保留（验收：10 轮追问不丢原文上下文）。
 */
export function compressHistory(messages: ChatMessage[], keepRounds = KEEP_ROUNDS): CompressionResult {
  if (messages.length === 0) {
    return { messages: [], compressedCount: 0, dropped: [], originalKept: false }
  }
  const system = messages.filter((m) => m.role === 'system')
  const nonSystem = messages.filter((m) => m.role !== 'system')
  const original = nonSystem[0] ?? null
  const rest = original ? nonSystem.slice(1) : nonSystem

  const keptTail = rest.slice(-keepRounds * 2)
  const dropped = rest.slice(0, Math.max(0, rest.length - keptTail.length))

  const placeholder: ChatMessage | null = dropped.length
    ? {
        role: 'user',
        content: `[历史压缩] 此前 ${Math.ceil(dropped.length / 2)} 轮对话已摘要化，细节从略；如需要可针对原文重新提问。`,
      }
    : null

  const out = [
    ...system,
    ...(original ? [original] : []),
    ...(placeholder ? [placeholder] : []),
    ...keptTail,
  ]
  return {
    messages: out,
    compressedCount: dropped.length,
    dropped,
    originalKept: original !== null && out.includes(original),
  }
}

/** 生成摘要化请求的 prompt（压缩完成后用一次轻量调用生成真实摘要，替换占位符） */
export function buildSummaryRequest(dropped: ChatMessage[]): ChatMessage[] {
  const transcript = dropped
    .map((m) => `${m.role === 'user' ? '用户' : '助手'}：${m.content.slice(0, 500)}`)
    .join('\n')
  return [
    {
      role: 'system' as const,
      content: '你是会话摘要器。将以下对话压缩为不超过 200 字的中文要点摘要，保留关键结论与术语。',
    },
    { role: 'user' as const, content: transcript },
  ]
}
