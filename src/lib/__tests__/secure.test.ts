import { describe, expect, it } from 'vitest'
import { maskKey } from '../settings-client'

describe('API Key 掩码（FR-6.3：任何界面不回显完整 Key）', () => {
  it('仅显示后 4 位', () => {
    expect(maskKey('sk-abcdef123456')).toBe('****3456')
  })

  it('短 Key 完全遮蔽', () => {
    expect(maskKey('abcd')).toBe('****')
    expect(maskKey('ab')).toBe('****')
  })

  it('掩码结果不泄漏原 Key 的任何前段', () => {
    const key = 'sk-supersecret-value-9999'
    expect(maskKey(key)).not.toContain('supersecret')
    expect(maskKey(key)).not.toContain('sk-')
  })
})
