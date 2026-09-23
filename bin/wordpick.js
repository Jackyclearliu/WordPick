#!/usr/bin/env node
/**
 * WordPick CLI 入口（FR-7.2）：
 *   wordpick run      启动后台常驻进程（单实例由 Rust 层保证，FR-7.3）
 *   wordpick settings 打开设置
 *   wordpick stop     退出后台进程
 *   wordpick status   查看运行状态与版本
 */
import { existsSync, readFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawn, spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { platform, arch } from 'node:process'

import { resolveBinaryName, parseArgs, pidFilePath } from './cli-utils.js'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const version = process.env.npm_package_version ?? '0.1.0'

const { command } = parseArgs(process.argv.slice(2))

/** 定位平台二进制：npm 包内 → 源码 target 目录（开发） */
function findBinary() {
  const name = resolveBinaryName(platform, arch)
  const candidates = [
    join(root, 'bin', name),
    join(root, 'src-tauri', 'target', 'release', name.endsWith('.exe') ? 'wordpick.exe' : 'wordpick'),
    join(root, 'src-tauri', 'target', 'debug', name.endsWith('.exe') ? 'wordpick.exe' : 'wordpick'),
  ]
  return candidates.find((p) => existsSync(p)) ?? null
}

function isRunning() {
  const pidFile = pidFilePath(tmpdir())
  if (!existsSync(pidFile)) return { running: false, pid: null }
  const pid = Number(readFileSync(pidFile, 'utf8').trim())
  if (!Number.isInteger(pid) || pid <= 0) return { running: false, pid: null }
  try {
    process.kill(pid, 0)
    return { running: true, pid }
  } catch {
    return { running: false, pid }
  }
}

switch (command) {
  case 'status': {
    const { running, pid } = isRunning()
    console.log(`wordpick v${version} — ${running ? `运行中 (pid ${pid})` : '未运行'}`)
    process.exit(running ? 0 : 1)
    break
  }
  case 'stop': {
    const { running, pid } = isRunning()
    if (!running) {
      console.log('wordpick 未在运行')
      process.exit(0)
    }
    try {
      process.kill(pid, platform === 'win32' ? undefined : 'SIGTERM')
      console.log(`已发送退出信号 (pid ${pid})`)
    } catch (e) {
      console.error(`退出失败: ${e}`)
      process.exit(1)
    }
    break
  }
  case 'run':
  case 'settings': {
    const binary = findBinary()
    if (!binary) {
      console.error('未找到 wordpick 二进制。npm 安装请检查 postinstall 是否成功；源码运行请执行: pnpm tauri build 或 pnpm tauri dev')
      process.exit(1)
    }
    const child = spawn(binary, command === 'settings' ? ['--settings'] : [], {
      stdio: 'ignore',
      detached: true,
    })
    child.on('error', (e) => {
      console.error(`启动失败: ${e.message}`)
      process.exit(1)
    })
    child.unref()
    console.log(`wordpick ${command === 'settings' ? 'settings ' : ''}已启动`)
    break
  }
  case 'version':
    console.log(`wordpick v${version}`)
    break
  default:
    console.log(`wordpick v${version}\n用法: wordpick <run|settings|stop|status>`)
    process.exit(command === 'help' ? 0 : 1)
}

// spawnSync 引用避免 tree-shake 误报未使用
void spawnSync
