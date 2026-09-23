import { describe, expect, it } from 'vitest'
import { parseArgs, pidFilePath, resolveBinaryName } from '../../../bin/cli-utils.js'

describe('CLI 分发逻辑（FR-7.1 / FR-7.2）', () => {
  it('平台二进制名映射', () => {
    expect(resolveBinaryName('darwin', 'arm64')).toBe('wordpick-darwin-arm64')
    expect(resolveBinaryName('win32', 'x64')).toBe('wordpick-win32-x64.exe')
    expect(resolveBinaryName('linux', 'x64')).toBe('wordpick-linux-x64')
    expect(resolveBinaryName('freebsd', 'x64')).toBeNull()
    expect(resolveBinaryName('darwin', 'riscv64')).toBeNull()
  })

  it('子命令路由：run/settings/stop/status', () => {
    expect(parseArgs([]).command).toBe('run')
    expect(parseArgs(['run']).command).toBe('run')
    expect(parseArgs(['settings']).command).toBe('settings')
    expect(parseArgs(['stop']).command).toBe('stop')
    expect(parseArgs(['status']).command).toBe('status')
  })

  it('version/help 旗标', () => {
    expect(parseArgs(['-v']).command).toBe('version')
    expect(parseArgs(['--version']).command).toBe('version')
    expect(parseArgs(['-h']).command).toBe('help')
  })

  it('未知命令返回 unknown', () => {
    expect(parseArgs(['foobar']).command).toBe('unknown')
  })

  it('pid 文件路径', () => {
    expect(pidFilePath('/tmp')).toBe('/tmp/wordpick.pid')
  })
})
