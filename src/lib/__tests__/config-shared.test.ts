import { describe, expect, it } from 'vitest'
import { pick, resolveEnvPlaceholder } from '../config-shared'

describe('配置解析（Rust config.rs 的可执行规格）', () => {
  it('解析 ${ENV_VAR} 占位符', () => {
    expect(resolveEnvPlaceholder('${DEEPSEEK_API_KEY}', { DEEPSEEK_API_KEY: 'sk-abc' })).toBe('sk-abc')
    expect(resolveEnvPlaceholder('${MISSING}')).toBe('')
  })

  it('明文原样返回', () => {
    expect(resolveEnvPlaceholder('plain-key')).toBe('plain-key')
  })

  it('非法占位符原样返回', () => {
    expect(resolveEnvPlaceholder('${}')).toBe('${}')
    expect(resolveEnvPlaceholder('${lowercase}')).toBe('${lowercase}')
    expect(resolveEnvPlaceholder('prefix-${VAR}')).toBe('prefix-${VAR}')
  })

  it('读取优先级：环境变量 > 用户配置 > 内置默认', () => {
    expect(pick(undefined, 'user', 'default')).toBe('user')
    expect(pick(undefined, undefined, 'default')).toBe('default')
    expect(pick('env', 'user', 'default')).toBe('env')
  })
})
