import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ToolbarApp from '@/views/ToolbarApp.vue'
import '@/style.css'
import { applyConfiguredTheme } from '@/lib/theme'

// 工具条窗口：无焦点抢占、不抢键盘，仅渲染功能按钮条
void applyConfiguredTheme()
createApp(ToolbarApp).use(createPinia()).mount('#app')
