import { createPinia, setActivePinia } from 'pinia'
import { describe, expect, it } from 'vitest'
import { computed } from 'vue'

import { useSessionStore } from '@/stores/session'

function beginSession() {
  const store = useSessionStore()
  store.begin({
    selection: { text: '测试', anchor: null, source: 'uia' },
    scenario: 'explain',
    sourceLang: 'zh',
    targetLang: 'en',
    firstMessages: [],
  })
  return store
}

describe('面板会话状态响应式更新', () => {
  it('流式、完成与错误状态变化会触发依赖更新', () => {
    setActivePinia(createPinia())
    const store = beginSession()
    const status = computed(() => store.session?.status)
    const error = computed(() => store.session?.error?.message)

    expect(status.value).toBe('idle')
    store.session!.status = 'streaming'
    expect(status.value).toBe('streaming')
    store.session!.status = 'done'
    expect(status.value).toBe('done')
    store.session!.status = 'error'
    store.session!.error = { kind: 'network', message: '请求超时' }
    expect(status.value).toBe('error')
    expect(error.value).toBe('请求超时')
  })
})
