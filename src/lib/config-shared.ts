/**
 * 配置解析的纯函数（Rust config.rs 的 TS 可执行规格）：
 * ${ENV_VAR} 占位符解析（FR-6.3）与读取优先级说明。
 */

/** 解析 ${ENV_VAR} 占位符；非占位符原样返回 */
export function resolveEnvPlaceholder(value: string, env: Record<string, string> = {}): string {
  const trimmed = value.trim()
  const m = /^\$\{([A-Z_]+)\}$/.exec(trimmed)
  if (m) {
    return env[m[1]] ?? ''
  }
  return value
}

/** 读取优先级：环境变量 > 用户配置 > 内置默认（§5.6） */
export function pick<T>(...layers: Array<T | undefined>): T | undefined {
  for (const layer of layers) {
    if (layer !== undefined) return layer
  }
  return undefined
}
