<script setup lang="ts">
/**
 * 设置窗口（FR-6）：提供方与模型、API Key（keychain）、触发模式、快捷键、
 * 弹窗位置、长度上限、黑名单、主题；与 config.toml 双向同步（FR-6.2）。
 * 含首次启动引导 Onboarding（FR-6.4）与连接测试（FR-6.5）。
 */
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

import {
  getApiKeyMask,
  getConfig,
  hasAnyApiKey,
  PROVIDER_TEMPLATES,
  saveApiKey,
  saveConfig,
  testProviderConnection,
  type AppConfig,
  type GeneralConfig,
} from '@/lib/settings-client'
import { applyConfiguredTheme } from '@/lib/theme'
import type { ProviderConfig } from '@/lib/llm-client'

// ---------------------------------------------------------------- 状态
const config = ref<AppConfig | null>(null)
const keyMasks = ref<Record<string, string>>({})
const keyInputs = ref<Record<string, string>>({})
const showOnboarding = ref(false)
const saving = ref(false)
const savedTip = ref(false)
const testResults = ref<Record<string, { ok: boolean; text: string }>>({})

// Onboarding 引导（FR-6.4）
const onboardingKey = ref('')
const onboardingProvider = ref('deepseek')

const scenarios = [
  { key: 'translate', label: '翻译（高频，快+便宜）' },
  { key: 'explain', label: '解释（重任务，质量优先）' },
  { key: 'chat', label: '追问' },
] as const

const theme = computed(() => config.value?.general.theme ?? 'system')
const autostart = ref(false)

onMounted(async () => {
  config.value = await getConfig()
  await refreshMasks()
  showOnboarding.value = !(await hasAnyApiKey())
  autostart.value = await invoke('get_autostart')
  await applyConfiguredTheme()
})

async function refreshMasks() {
  if (!config.value) return
  for (const p of config.value.providers) {
    keyMasks.value[p.name] = (await getApiKeyMask(p.name)) ?? ''
  }
}

async function toggleAutostart() {
  autostart.value = !autostart.value
  await invoke('set_autostart', { enabled: autostart.value })
}

// ---------------------------------------------------------------- 提供方管理（FR-6.1）
function addProvider(templateName: string) {
  const cfg = config.value
  if (!cfg) return
  if (cfg.providers.some((p) => p.name === templateName)) return
  const t = PROVIDER_TEMPLATES.find((t) => t.name === templateName)
  if (!t) return
  cfg.providers.push({ name: t.name, base_url: t.base_url, api_key: '', default_model: t.default_model })
}

function addCustomProvider() {
  const cfg = config.value
  if (!cfg) return
  let n = 1
  while (cfg.providers.some((p) => p.name === `custom-${n}`)) n++
  cfg.providers.push({ name: `custom-${n}`, base_url: '', api_key: '', default_model: '' })
}

function removeProvider(name: string) {
  const cfg = config.value
  if (!cfg) return
  cfg.providers = cfg.providers.filter((p) => p.name !== name)
}

/** 保存 Key：写入系统安全存储（keychain，FR-6.3），成功后仅回显掩码 */
async function applyKey(provider: string) {
  const key = keyInputs.value[provider]?.trim()
  if (!key) return
  await saveApiKey(provider, key)
  keyInputs.value[provider] = ''
  keyMasks.value[provider] = (await getApiKeyMask(provider)) ?? ''
}

// ---------------------------------------------------------------- 连接测试（FR-6.5）
async function testConnection(p: ProviderConfig) {
  testResults.value[p.name] = { ok: false, text: '测试中…' }
  try {
    const ms = await testProviderConnection(p, p.default_model)
    testResults.value[p.name] = { ok: true, text: `连通正常（${ms}ms）` }
  } catch (e) {
    testResults.value[p.name] = { ok: false, text: `失败：${String(e)}` }
  }
}

// ---------------------------------------------------------------- 保存（FR-6.2 双向同步）
async function save() {
  const cfg = config.value
  if (!cfg) return
  saving.value = true
  try {
    // 先落所有已输入的 Key
    for (const [name, key] of Object.entries(keyInputs.value)) {
      if (key?.trim()) await saveApiKey(name, key.trim())
    }
    await saveConfig(JSON.parse(JSON.stringify(cfg)))
    savedTip.value = true
    setTimeout(() => (savedTip.value = false), 2000)
  } finally {
    saving.value = false
  }
}

// ---------------------------------------------------------------- Onboarding（FR-6.4）
async function finishOnboarding() {
  const key = onboardingKey.value.trim()
  if (!key) return
  await saveApiKey(onboardingProvider.value, key)
  const cfg = config.value
  if (cfg && !cfg.providers.some((p) => p.name === onboardingProvider.value)) {
    const t = PROVIDER_TEMPLATES.find((t) => t.name === onboardingProvider.value)
    if (t) {
      cfg.providers.push({ name: t.name, base_url: t.base_url, api_key: '', default_model: t.default_model })
      await saveConfig(JSON.parse(JSON.stringify(cfg)))
    }
  }
  keyMasks.value[onboardingProvider.value] = (await getApiKeyMask(onboardingProvider.value)) ?? ''
  showOnboarding.value = false
}

const general = computed<GeneralConfig | null>(() => config.value?.general ?? null)

/** 黑名单编辑：文本 <-> 列表 */
const blacklistText = computed({
  get: () => (config.value?.general.blacklist_apps ?? []).join(', '),
  set: (v: string) => {
    if (config.value) {
      config.value.general.blacklist_apps = v.split(/[,，]/).map((s) => s.trim()).filter(Boolean)
    }
  },
})
</script>

<template>
  <!-- Onboarding：无可用 Key 时引导（FR-6.4） -->
  <div v-if="showOnboarding" class="flex h-full flex-col items-center justify-center gap-4 bg-white p-8 dark:bg-gray-900">
    <h1 class="text-xl font-semibold text-gray-800 dark:text-gray-100">欢迎使用 WordPick 🎉</h1>
    <p class="max-w-md text-center text-sm text-gray-500 dark:text-gray-400">
      划词助手的翻译与解释由你自己的大模型 Key 驱动。请选择提供方并粘贴 API Key（Key 将存入系统安全存储，仅显示后 4 位）。
    </p>
    <select v-model="onboardingProvider" class="rounded-lg border border-gray-300 bg-transparent px-3 py-2 text-sm dark:border-gray-700 dark:text-gray-200">
      <option v-for="t in PROVIDER_TEMPLATES" :key="t.name" :value="t.name">{{ t.name }}（{{ t.key_hint }}）</option>
    </select>
    <input
      v-model="onboardingKey"
      type="password"
      placeholder="粘贴 API Key"
      class="w-80 rounded-lg border border-gray-300 bg-transparent px-3 py-2 text-sm dark:border-gray-700"
    />
    <button class="rounded-lg bg-sky-500 px-6 py-2 text-sm text-white hover:bg-sky-400 disabled:opacity-40" :disabled="!onboardingKey.trim()" @click="finishOnboarding">
      完成并开始使用
    </button>
    <button class="text-xs text-gray-400 underline" @click="showOnboarding = false">稍后再说</button>
  </div>

  <!-- 设置主界面（FR-6.1 全量配置项） -->
  <div v-else-if="config" class="h-full overflow-y-auto bg-white p-6 text-sm text-gray-800 dark:bg-gray-900 dark:text-gray-200">
    <div class="mb-4 flex items-center justify-between">
      <h1 class="text-lg font-semibold">WordPick 设置</h1>
      <div class="flex items-center gap-2">
        <span v-if="savedTip" class="text-xs text-green-600">已保存 ✓（重启后完全生效）</span>
        <button class="rounded-lg bg-sky-500 px-4 py-1.5 text-white hover:bg-sky-400 disabled:opacity-40" :disabled="saving" @click="save">
          {{ saving ? '保存中…' : '保存' }}
        </button>
      </div>
    </div>

    <!-- 提供方与模型 -->
    <section class="mb-6">
      <h2 class="mb-2 font-medium">提供方与模型</h2>
      <div class="mb-2 flex gap-2">
        <select class="rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700" @change="addProvider(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''">
          <option value="">+ 从模板添加…</option>
          <option v-for="t in PROVIDER_TEMPLATES" :key="t.name" :value="t.name">{{ t.name }}</option>
        </select>
        <button class="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-800" @click="addCustomProvider">+ 自定义（OneAPI / OpenRouter 等）</button>
      </div>

      <div v-for="p in config.providers" :key="p.name" class="mb-3 rounded-lg border border-gray-200 p-3 dark:border-gray-800">
        <div class="mb-2 flex items-center gap-2">
          <input v-model="p.name" class="w-28 rounded border border-gray-300 bg-transparent px-2 py-1 font-medium dark:border-gray-700" />
          <input v-model="p.default_model" class="flex-1 rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700" placeholder="默认模型，如 deepseek-v4-flash" />
          <button class="text-xs text-red-500 hover:underline" @click="removeProvider(p.name)">删除</button>
        </div>
        <input v-model="p.base_url" class="mb-2 w-full rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700" placeholder="Base URL，如 https://api.deepseek.com/v1" />
        <div class="flex items-center gap-2">
          <input
            v-model="keyInputs[p.name]"
            type="password"
            class="flex-1 rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700"
            :placeholder="keyMasks[p.name] ? `已保存（${keyMasks[p.name]}）` : 'API Key（保存到系统安全存储）'"
          />
          <button class="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-800" :disabled="!keyInputs[p.name]?.trim()" @click="applyKey(p.name)">保存 Key</button>
          <button class="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-800" @click="testConnection(p)">连接测试</button>
        </div>
        <p v-if="testResults[p.name]" class="mt-1 text-xs" :class="testResults[p.name].ok ? 'text-green-600' : 'text-red-500'">
          {{ testResults[p.name].text }}
        </p>
      </div>
    </section>

    <!-- 场景化模型选择（§5.5） -->
    <section class="mb-6">
      <h2 class="mb-2 font-medium">场景模型</h2>
      <div v-for="sc in scenarios" :key="sc.key" class="mb-2 flex items-center gap-2">
        <span class="w-44 text-xs text-gray-500">{{ sc.label }}</span>
        <select v-model="config.scenario_models[sc.key].provider" class="rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700">
          <option v-for="p in config.providers" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
        <input v-model="config.scenario_models[sc.key].model" class="w-48 rounded border border-gray-300 bg-transparent px-2 py-1 text-xs dark:border-gray-700" placeholder="模型名" />
      </div>
    </section>

    <!-- 通用 -->
    <section v-if="general" class="mb-6">
      <h2 class="mb-2 font-medium">通用</h2>
      <div class="grid grid-cols-2 gap-x-6 gap-y-3">
        <label class="flex items-center justify-between gap-2 text-xs">
          触发模式
          <select v-model="general.trigger_mode" class="rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700">
            <option value="auto">auto（监听选区）</option>
            <option value="shortcut">shortcut（快捷键唤起）</option>
          </select>
        </label>
        <label class="flex items-center justify-between gap-2 text-xs">
          弹窗位置
          <select v-model="general.popup_position" class="rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700">
            <option value="below">选区下方</option>
            <option value="top-right">右上方</option>
          </select>
        </label>
        <label class="flex items-center justify-between gap-2 text-xs">
          划词长度上限
          <input v-model.number="general.max_selection_chars" type="number" min="100" class="w-24 rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700" />
        </label>
        <label class="flex items-center justify-between gap-2 text-xs">
          主题
          <select v-model="general.theme" class="rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700" @change="applyConfiguredTheme">
            <option value="system">跟随系统</option>
            <option value="light">明</option>
            <option value="dark">暗</option>
          </select>
        </label>
        <label class="flex items-center gap-2 text-xs">
          <input type="checkbox" :checked="autostart" class="accent-sky-500" @change="toggleAutostart" />
          开机自启（FR-7.4）
        </label>
        <label class="flex items-center gap-2 text-xs">
          唤起快捷键 <input v-model="general.shortcut_toggle" class="w-20 rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700" />
        </label>
        <label class="flex items-center gap-2 text-xs">
          设置快捷键 <input v-model="general.shortcut_settings" class="w-20 rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700" />
        </label>
      </div>
      <label class="mt-3 block text-xs text-gray-500">
        应用黑名单（逗号分隔，FR-1.5）
        <input v-model="blacklistText" class="mt-1 w-full rounded border border-gray-300 bg-transparent px-2 py-1 dark:border-gray-700" placeholder="如 1Password, Steam" />
      </label>
    </section>
  </div>

  <div v-else class="flex h-full items-center justify-center bg-white text-sm text-gray-400 dark:bg-gray-900">加载中…</div>
</template>
