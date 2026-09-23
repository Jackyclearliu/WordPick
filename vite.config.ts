import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

// 三个独立窗口入口：工具条 / 对话框 / 设置
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    rollupOptions: {
      input: {
        toolbar: fileURLToPath(new URL('./toolbar.html', import.meta.url)),
        panel: fileURLToPath(new URL('./panel.html', import.meta.url)),
        settings: fileURLToPath(new URL('./settings.html', import.meta.url)),
      },
    },
    // Tauri 期望固定端口、相对基路径
    target: 'es2021',
    minify: 'esbuild',
    sourcemap: false,
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
})
