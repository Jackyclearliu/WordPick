#!/usr/bin/env node
/**
 * WordPick postinstall：按平台/架构拉取 Tauri 二进制（需求文档 §5.9）。
 * 开发模式下若已设置 WORDPICK_SKIP_BINARY_DOWNLOAD=1（源码运行）则跳过。
 */
import { existsSync, mkdirSync, createWriteStream } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

if (process.env.WORDPICK_SKIP_BINARY_DOWNLOAD === '1') {
  console.log('[wordpick] 跳过二进制下载（WORDPICK_SKIP_BINARY_DOWNLOAD=1）')
  process.exit(0)
}

const platformMap = { darwin: 'darwin', win32: 'win32', linux: 'linux' }
const archMap = { x64: 'x64', arm64: 'arm64' }

const platform = platformMap[process.platform]
const arch = archMap[process.arch]

if (!platform || !arch) {
  console.warn(`[wordpick] 暂不支持 ${process.platform}/${process.arch}，请使用源码运行（pnpm tauri dev）`)
  process.exit(0)
}

const ext = platform === 'win32' ? '.exe' : ''
const binaryName = `wordpick-${platform}-${arch}${ext}`
const version = process.env.npm_package_version || '0.1.0'
const mirror = (process.env.WORDPICK_BINARY_MIRROR || 'https://github.com/wordpick/wordpick/releases/download').replace(/\/$/, '')
const url = `${mirror}/v${version}/${binaryName}`
const dest = join(root, 'bin', binaryName)

if (existsSync(dest)) {
  console.log(`[wordpick] 二进制已存在：${binaryName}`)
  process.exit(0)
}

console.log(`[wordpick] 下载 ${url} ...`)

// Phase 5 完善：优先 curl/wget，失败时给出源码运行指引
const downloader = process.platform === 'win32' ? 'curl.exe' : 'curl'
const result = spawnSync(downloader, ['-fSL', '-o', dest, url], { stdio: 'inherit' })
if (result.status !== 0) {
  console.warn('[wordpick] 二进制下载失败，可设置 WORDPICK_BINARY_MIRROR 使用镜像，或源码运行：pnpm i && pnpm tauri dev')
  process.exit(0) // 不阻断安装（npm install 本身成功）
}
console.log(`[wordpick] 已保存 ${dest}`)
