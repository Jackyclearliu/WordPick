<script setup lang="ts">
import { onMounted, onUnmounted, nextTick } from 'vue'
import { toolbarAction, hideToolbar, type ToolbarAction } from '@/lib/bridge'

// 工具条（FR-2）：无边框透明圆角卡片；数字键快捷触发 + Esc 关闭（FR-2.4）
const buttons: Array<{ action: ToolbarAction; label: string; key: string }> = [
  { action: 'translate', label: '翻译', key: '1' },
  { action: 'explain', label: '解释', key: '2' },
  { action: 'settings', label: '设置', key: '3' },
]

async function trigger(action: ToolbarAction) {
  await toolbarAction(action)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    void hideToolbar()
    return
  }
  const hit = buttons.find((b) => b.key === e.key)
  if (hit) void trigger(hit.action)
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

// 内容自适应：将内容尺寸上报 Rust 调整窗口（窗口复用池，NFR 4.1）
onMounted(async () => {
  await nextTick()
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('fit_toolbar_window', {
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  })
})
</script>

<template>
  <div
    class="flex items-center gap-0.5 rounded-xl bg-white/95 px-1.5 py-1 text-[13px] shadow-xl ring-1 ring-black/10 backdrop-blur dark:bg-gray-900/95 dark:ring-white/10"
  >
    <button
      v-for="b in buttons"
      :key="b.action"
      class="flex items-center gap-1 rounded-lg px-2.5 py-1 text-gray-700 transition-colors hover:bg-gray-100 hover:text-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
      @click="trigger(b.action)"
    >
      <span class="text-[10px] text-gray-400 dark:text-gray-500">{{ b.key }}</span>
      {{ b.label }}
    </button>
  </div>
</template>
