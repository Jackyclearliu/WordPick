import { createApp } from 'vue'
import { createPinia } from 'pinia'
import PanelApp from '@/views/PanelApp.vue'
import '@/style.css'
import { applyConfiguredTheme } from '@/lib/theme'

// 对话框窗口：翻译 / 解释 / 追问
void applyConfiguredTheme()
createApp(PanelApp).use(createPinia()).mount('#app')
