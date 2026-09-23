/** CLI 纯逻辑（可被 vitest 直接测试） */

/** 平台二进制名（FR-7.1 / §5.9） */
export function resolveBinaryName(platform, arch) {
  const platforms = { darwin: 'darwin', win32: 'win32', linux: 'linux' }
  const arches = { x64: 'x64', arm64: 'arm64' }
  const p = platforms[platform]
  const a = arches[arch]
  if (!p || !a) return null
  return `wordpick-${p}-${a}${p === 'win32' ? '.exe' : ''}`
}

/** 解析子命令（FR-7.2）；缺省视为 run */
export function parseArgs(argv) {
  const args = argv.filter((a) => !a.startsWith('-'))
  const cmd = args[0] ?? 'run'
  const known = ['run', 'settings', 'stop', 'status', 'help', 'version']
  if (argv.includes('-v') || argv.includes('--version')) return { command: 'version' }
  if (argv.includes('-h') || argv.includes('--help')) return { command: 'help' }
  if (!known.includes(cmd)) return { command: 'unknown', input: cmd }
  return { command: cmd }
}

/** 后台进程 pid 文件（status / stop 用） */
export function pidFilePath(baseDir) {
  return `${baseDir}/wordpick.pid`
}
