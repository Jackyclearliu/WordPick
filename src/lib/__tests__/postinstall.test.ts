import { afterEach, describe, expect, it, vi } from 'vitest'
import { resolveBinaryName } from '../../../bin/cli-utils.js'

const mocks = vi.hoisted(() => ({
  spawnSync: vi.fn(),
}))
const runtime = (globalThis as unknown as { process: { platform: string; arch: string } }).process

vi.mock('node:child_process', () => ({ spawnSync: mocks.spawnSync }))
vi.mock('node:fs', () => ({
  existsSync: vi.fn(() => false),
  mkdirSync: vi.fn(),
  createWriteStream: vi.fn(),
}))

afterEach(() => {
  vi.unstubAllEnvs()
  vi.resetModules()
  mocks.spawnSync.mockReset()
})

describe('npm 安装后二进制下载', () => {
  it('从当前 GitHub 仓库下载对应版本与平台的二进制', async () => {
    vi.stubEnv('npm_package_version', '1.0.0')
    mocks.spawnSync.mockReturnValue({ status: 0 })

    await import('../../../scripts/postinstall.js')

    const binaryName = resolveBinaryName(runtime.platform, runtime.arch)
    expect(binaryName).not.toBeNull()
    expect(mocks.spawnSync).toHaveBeenCalledWith(
      runtime.platform === 'win32' ? 'curl.exe' : 'curl',
      [
        '-fSL',
        '-o',
        expect.any(String),
        `https://github.com/Jackyclearliu/WordPick/releases/download/v1.0.0/${binaryName}`,
      ],
      { stdio: 'inherit' },
    )
  })
})
