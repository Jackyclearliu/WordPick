<script setup lang="ts">
/**
 * 对话框窗口（FR-3 翻译 / FR-4 解释与多轮对话 / FR-5 通用行为）。
 * Phase 3：解释场景快捷追问 Chips、对话历史自动压缩（FR-4.3）、shiki 代码高亮。
 */
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'

import { takePanelContext, type PanelContext } from '@/lib/bridge-types'
import { LANG_NAMES, defaultTarget, detectLang, type LangCode } from '@/lib/lang-detect'
import {
  buildFirstMessages,
  newRequestId,
  onChatEvent,
  setPanelPinned,
  startChat,
  stopChat,
  type ChatEvent,
  type ChatMessage,
  type ProviderConfig,
  type Scenario,
} from '@/lib/llm-client'
import {
  compressHistory,
  buildSummaryRequest,
  needsCompression,
  KEEP_ROUNDS,
} from '@/lib/chat-history'
import { createIncrementalRenderer, initHighlighter } from '@/lib/markdown'
import { isTranslate, useSessionStore, type PanelSession } from '@/stores/session'

interface AppConfig {
  general: { theme: string }
  providers: ProviderConfig[]
  scenario_models: Record<Scenario, { provider: string; model: string }>
  limits: { max_followup_rounds: number; max_tokens_translate: number }
}

/** 快捷追问 Chips（FR-4.4） */
const EXPLAIN_CHIPS = ['换个角度解释', '举例说明', '出几道练习题']

const store = useSessionStore()
const win = getCurrentWindow()

// ---------------------------------------------------------------- 会话初始化
const context = ref<PanelContext | null>(null)
const config = ref<AppConfig | null>(null)
const renderedHtml = ref('')
const bodyEl = ref<HTMLElement | null>(null)
const followup = ref('')
const followupLimitHint = ref(false)
const pinned = ref(false)

const scenarioMeta: Record<Scenario, { title: string; barClass: string }> = {
  translate: { title: '翻译', barClass: 'bg-sky-500' },
  explain: { title: '解释', barClass: 'bg-violet-500' },
  chat: { title: '追问', barClass: 'bg-violet-500' },
}

const session = computed<PanelSession | null>(() => store.session)
const streaming = computed(() => session.value?.status === 'streaming')
const errored = computed(() => session.value?.status === 'error')

const langOptions = Object.entries(LANG_NAMES) as Array<[LangCode, string]>

function providerFor(scenario: Scenario): { provider: ProviderConfig; model: string } | null {
  const cfg = config.value
  if (!cfg) return null
  const sm = cfg.scenario_models[scenario]
  const provider = cfg.providers.find((p) => p.name === sm.provider) ?? cfg.providers[0]
  if (!provider) return null
  return { provider, model: sm.model || provider.default_model }
}

function newRenderer(s: PanelSession) {
  Object.assign(s, { renderer: createIncrementalRenderer() })
  renderedHtml.value = ''
}

/** 在途请求登记表：区分「展示流」与「摘要流」，互不干扰 */
const inflight = new Map<string, { kind: 'display' | 'summary'; buffer: string }>()

async function runRequest(msgs: ChatMessage[], scenario: Scenario, kind: 'display' | 'summary') {
  const s = session.value
  const target = providerFor(scenario)
  if (!s || !target) return
  const request_id = newRequestId()
  inflight.set(request_id, { kind, buffer: '' })
  if (kind === 'display') {
    s.status = 'streaming'
    s.error = null
  }
  await startChat({
    request_id,
    provider: target.provider,
    model: target.model,
    scenario,
    messages: msgs,
    max_tokens: scenario === 'translate' ? config.value?.limits.max_tokens_translate : undefined,
  })
}

async function startFirstRound() {
  const ctx = context.value
  const cfg = config.value
  if (!ctx || !cfg) return
  if (ctx.scenario === 'explain') void initHighlighter()
  const detected = detectLang(ctx.selection.text)
  const target = defaultTarget(detected)
  const msgs = await buildFirstMessages(ctx.scenario, LANG_NAMES[detected], LANG_NAMES[target], ctx.selection.text)
  store.begin({
    selection: ctx.selection,
    scenario: ctx.scenario,
    sourceLang: detected,
    targetLang: target,
    firstMessages: msgs,
  })
  followupLimitHint.value = false
  await runRequest(msgs, ctx.scenario, 'display')
}

// ---------------------------------------------------------------- 流式事件
let unlisten: (() => void) | null = null

async function scrollToEnd() {
  await nextTick()
  bodyEl.value?.scrollTo({ top: bodyEl.value.scrollHeight })
}

function handleChatEvent(e: ChatEvent) {
  const s = session.value
  const req = inflight.get(e.request_id)
  if (!s || !req) return

  if (e.type === 'chunk') {
    if (req.kind === 'display') {
      renderedHtml.value = s.renderer.append(e.delta)
      void scrollToEnd()
    } else {
      req.buffer += e.delta
    }
    return
  }

  if (e.type === 'done') {
    inflight.delete(e.request_id)
    if (req.kind === 'display') {
      s.messages.push({ role: 'assistant', content: s.renderer.raw })
      s.status = 'done'
    } else {
      applySummary(req.buffer)
    }
    return
  }

  if (e.type === 'error') {
    inflight.delete(e.request_id)
    if (req.kind === 'display') {
      s.error = { kind: e.kind, message: e.message }
      // 流式中断保留已输出内容（NFR 4.3）
      if (s.renderer.raw) s.messages.push({ role: 'assistant', content: s.renderer.raw })
      s.status = 'error'
    } else {
      // 摘要失败：保留占位符，不阻塞追问
      proceedFollowup(null)
    }
  }
}

// ---------------------------------------------------------------- 历史压缩（FR-4.3）
let queuedFollowup: string | null = null
let lastDropped: ChatMessage[] = []

function applySummary(summary: string) {
  const s = session.value
  if (!s) return
  // 用真实摘要替换压缩占位符
  const idx = s.messages.findIndex((m) => m.content.includes('[历史压缩]'))
  if (idx >= 0 && summary.trim()) {
    s.messages[idx] = { role: 'user', content: `[历史摘要] ${summary.trim()}` }
  }
  proceedFollowup(summary)
}

function proceedFollowup(_summary: string | null) {
  const s = session.value
  const text = queuedFollowup
  queuedFollowup = null
  if (!s || !text) return
  const scenario: Scenario = isTranslate(s.scenario) ? 'chat' : s.scenario
  const msgs = [...s.messages, { role: 'user' as const, content: text }]
  s.messages = msgs
  newRenderer(s)
  void runRequest(msgs, scenario, 'display')
}

// ---------------------------------------------------------------- 交互（FR-5）
async function togglePin() {
  pinned.value = !pinned.value
  await setPanelPinned(pinned.value)
}

async function closePanel() {
  for (const id of inflight.keys()) await stopChat(id)
  store.reset()
  await win.close()
}

async function stopStreaming() {
  for (const [id, req] of inflight) {
    if (req.kind === 'display') await stopChat(id)
  }
  const s = session.value
  if (s && s.status === 'streaming') {
    if (s.renderer.raw) s.messages.push({ role: 'assistant', content: s.renderer.raw })
    s.status = 'stopped'
  }
}

async function retry() {
  const s = session.value
  if (!s) return
  newRenderer(s)
  await runRequest(s.messages, s.scenario, 'display')
}

async function openSettings() {
  await invoke('open_settings')
}

/** 手动切换目标语言（FR-3.3）：按新语言对重翻 */
async function onTargetChange() {
  const s = session.value
  if (!s || !isTranslate(s.scenario)) return
  const msgs = await buildFirstMessages(
    'translate',
    LANG_NAMES[s.sourceLang as LangCode],
    LANG_NAMES[s.targetLang as LangCode],
    s.selection.text,
  )
  s.messages = msgs
  newRenderer(s)
  await runRequest(msgs, 'translate', 'display')
}

/** 追问（FR-3.4 / FR-4.2）：携带原文与完整历史；超阈值先压缩（FR-4.3） */
async function sendFollowup(preset?: string) {
  const s = session.value
  const text = (preset ?? followup.value).trim()
  if (!s || !text || streaming.value) return
  const cfg = config.value
  if (cfg && s.followupRounds >= cfg.limits.max_followup_rounds) {
    followupLimitHint.value = true
    return
  }
  s.followupRounds++
  followup.value = ''

  if (needsCompression(s.messages, KEEP_ROUNDS * 2)) {
    // 先压缩：早期轮次替换为占位符；再发起一次摘要请求，完成后自动续上本次追问
    const { messages, dropped } = compressHistory(s.messages, KEEP_ROUNDS)
    s.messages = messages
    lastDropped = dropped
    queuedFollowup = text
    await runRequest(buildSummaryRequest(lastDropped), 'chat', 'summary')
    return
  }

  const scenario: Scenario = isTranslate(s.scenario) ? 'chat' : s.scenario
  const msgs = [...s.messages, { role: 'user' as const, content: text }]
  s.messages = msgs
  newRenderer(s)
  await runRequest(msgs, scenario, 'display')
}

function copyTranslation() {
  const s = session.value
  if (s?.renderer.raw) void navigator.clipboard.writeText(s.renderer.raw)
}

function speak(text: string, lang: string) {
  const u = new SpeechSynthesisUtterance(text)
  u.lang = lang
  speechSynthesis.speak(u)
}

// ---------------------------------------------------------------- 生命周期
let unblur: (() => void) | null = null

onMounted(async () => {
  ;[context.value, config.value] = await Promise.all([
    takePanelContext(),
    invoke<AppConfig>('get_config'),
  ])
  unlisten = await onChatEvent(handleChatEvent)
  // 非钉住状态：失焦自动关闭（FR-5.1）
  const { listen: listenWin } = await import('@tauri-apps/api/event')
  unblur = await listenWin('tauri://blur', async () => {
    if (!pinned.value) await closePanel()
  })
  await startFirstRound()
})

onUnmounted(() => {
  unlisten?.()
  unblur?.()
})
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden rounded-xl bg-white/95 shadow-2xl ring-1 ring-black/10 backdrop-blur dark:bg-gray-900/95 dark:ring-white/10">
    <!-- 标题栏：拖拽移动（FR-5.3）+ 功能区分样式（FR-5.2） -->
    <header
      data-tauri-drag-region
      class="flex shrink-0 cursor-move items-center gap-2 rounded-t-xl px-3 py-2"
      :class="session ? scenarioMeta[session.scenario].barClass : 'bg-gray-500'"
    >
      <span class="text-sm font-medium text-white">{{ session ? scenarioMeta[session.scenario].title : 'WordPick' }}</span>
      <div class="ml-auto flex items-center gap-1">
        <button
          class="rounded p-1 text-white/80 hover:bg-white/20 hover:text-white"
          :class="pinned ? 'opacity-100' : 'opacity-60'"
          :title="pinned ? '取消钉住' : '钉住'"
          @click="togglePin"
        >
          <svg v-if="pinned" width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z"/></svg>
          <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z"/></svg>
        </button>
        <button class="rounded p-1 text-white/80 hover:bg-white/20 hover:text-white" title="关闭" @click="closePanel">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6L6 18M6 6l12 12"/></svg>
        </button>
      </div>
    </header>

    <!-- 语言切换器（翻译场景，FR-3.3） -->
    <div v-if="session && isTranslate(session.scenario)" class="flex shrink-0 items-center gap-2 border-b border-gray-100 px-3 py-1.5 text-xs dark:border-gray-800">
      <select v-model="session.sourceLang" class="rounded bg-transparent text-gray-600 dark:text-gray-300">
        <option v-for="[code, name] in langOptions" :key="code" :value="code">{{ name }}</option>
      </select>
      <span class="text-gray-400">→</span>
      <select v-model="session.targetLang" class="rounded bg-transparent text-gray-600 dark:text-gray-300" @change="onTargetChange">
        <option v-for="[code, name] in langOptions" :key="code" :value="code">{{ name }}</option>
      </select>
      <button class="ml-auto rounded px-1.5 py-0.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-800" title="复制译文" @click="copyTranslation">复制</button>
      <button class="rounded px-1.5 py-0.5 text-gray-400 hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-800" title="朗读译文"
        @click="session && speak(session.renderer.raw, session.targetLang)">朗读</button>
    </div>

    <!-- 正文：流式输出（FR-5.4） -->
    <div ref="bodyEl" class="selectable flex-1 overflow-y-auto px-3 py-2 text-sm leading-relaxed text-gray-800 dark:text-gray-100">
      <div v-if="!renderedHtml && streaming" class="text-gray-400">正在生成…</div>
      <div v-else-if="renderedHtml" class="prose-sm" v-html="renderedHtml"></div>
    </div>

    <!-- 错误态：友好错误 + 重试 / 直达设置（FR-5.4） -->
    <div v-if="errored && session?.error" class="shrink-0 border-t border-red-100 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300">
      <p>{{ session.error.message }}</p>
      <div class="mt-1.5 flex gap-2">
        <button class="rounded bg-red-600 px-2 py-1 text-white hover:bg-red-500" @click="retry">重试</button>
        <button v-if="session.error.kind === 'invalid_key'" class="rounded border border-red-300 px-2 py-1 hover:bg-red-100 dark:border-red-800 dark:hover:bg-red-900" @click="openSettings">配置 API Key</button>
      </div>
    </div>

    <!-- 快捷追问 Chips（FR-4.4，解释/追问场景） -->
    <div v-if="session && !isTranslate(session.scenario) && !streaming" class="flex shrink-0 flex-wrap gap-1.5 border-t border-gray-100 px-2 pt-2 dark:border-gray-800">
      <button
        v-for="chip in EXPLAIN_CHIPS"
        :key="chip"
        class="rounded-full border border-violet-200 px-2.5 py-1 text-xs text-violet-600 hover:bg-violet-50 dark:border-violet-900 dark:text-violet-300 dark:hover:bg-violet-950"
        @click="sendFollowup(chip)"
      >
        {{ chip }}
      </button>
    </div>

    <!-- 追问输入（FR-3.4 / FR-4.2） -->
    <div class="shrink-0 border-t border-gray-100 p-2 dark:border-gray-800">
      <div v-if="followupLimitHint" class="mb-1 text-[11px] text-amber-600">追问轮数较多，建议关闭本窗口后重新划词开始新会话（成本控制，§5.5）</div>
      <div class="flex items-center gap-1.5">
        <input
          v-model="followup"
          class="flex-1 rounded-lg border border-gray-200 bg-transparent px-2.5 py-1.5 text-sm outline-none focus:border-sky-400 dark:border-gray-700"
          placeholder="继续追问…（Enter 发送）"
          :disabled="streaming"
          @keydown.enter="sendFollowup()"
        />
        <button v-if="streaming" class="rounded-lg bg-gray-200 px-2.5 py-1.5 text-sm text-gray-700 hover:bg-gray-300 dark:bg-gray-700 dark:text-gray-200" @click="stopStreaming">停止</button>
        <button v-else class="rounded-lg bg-sky-500 px-2.5 py-1.5 text-sm text-white hover:bg-sky-400 disabled:opacity-40" :disabled="!followup.trim()" @click="sendFollowup()">发送</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Tailwind v4：在 SFC 作用域样式中使用 @apply 需引入参考 */
@reference "tailwindcss";

.prose-sm :deep(pre) {
  @apply overflow-x-auto rounded-lg bg-gray-100 p-2 text-xs dark:bg-gray-800;
}
.prose-sm :deep(code) {
  @apply rounded bg-gray-100 px-1 text-[0.85em] dark:bg-gray-800;
}
.prose-sm :deep(p) {
  @apply my-1.5;
}
.prose-sm :deep(ul) {
  @apply my-1.5 list-disc pl-5;
}
.prose-sm :deep(h1), .prose-sm :deep(h2), .prose-sm :deep(h3) {
  @apply my-2 font-semibold;
}
</style>
