import { createApp } from 'vue'
import { createPinia } from 'pinia'
import SettingsApp from '@/views/SettingsApp.vue'
import '@/style.css'

// 设置窗口
createApp(SettingsApp).use(createPinia()).mount('#app')
