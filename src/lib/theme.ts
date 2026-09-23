/**
 * 主题统一应用（FR-5.6）：读取配置主题（system/light/dark），
 * 在 documentElement 上切换 dark class；工具条 / 面板 / 设置共用。
 */
import { getConfig } from './settings-client'

export async function applyConfiguredTheme(): Promise<void> {
  const cfg = await getConfig().catch(() => null)
  const theme = cfg?.general.theme ?? 'system'
  const dark =
    theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.classList.toggle('dark', dark)
}
