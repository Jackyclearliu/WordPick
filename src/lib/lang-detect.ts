/**
 * 本地启发式语言检测（需求文档 FR-3.2 / 附录 8.1）。
 * 不消耗 API 调用：按字符集分布粗判，拉丁语系再按变音符与高频词细分。
 */

export type LangCode = 'zh' | 'en' | 'de' | 'fr' | 'ja' | 'ar' | 'ko' | 'ru' | 'es'

export const LANG_NAMES: Record<LangCode, string> = {
  zh: '中文（简体）',
  en: '英语',
  de: '德语',
  fr: '法语',
  ja: '日语',
  ar: '阿拉伯语',
  ko: '韩语',
  ru: '俄语',
  es: '西班牙语',
}

/** FR-3.2 默认目标语言：英→中，中→英，其他→中 */
export function defaultTarget(lang: LangCode): LangCode {
  if (lang === 'en') return 'zh'
  if (lang === 'zh') return 'en'
  return 'zh'
}

const inRange = (cp: number, lo: number, hi: number) => cp >= lo && cp <= hi

function countScript(text: string): Record<string, number> {
  const counts: Record<string, number> = {
    arabic: 0,
    hangul: 0,
    cyrillic: 0,
    kana: 0,
    cjk: 0,
    latin: 0,
  }
  for (const ch of text) {
    const cp = ch.codePointAt(0)!
    if (inRange(cp, 0x0600, 0x06ff) || inRange(cp, 0x0750, 0x077f)) counts.arabic++
    else if (inRange(cp, 0xac00, 0xd7af) || inRange(cp, 0x1100, 0x11ff)) counts.hangul++
    else if (inRange(cp, 0x0400, 0x04ff)) counts.cyrillic++
    else if (inRange(cp, 0x3040, 0x309f) || inRange(cp, 0x30a0, 0x30ff)) counts.kana++
    else if (inRange(cp, 0x4e00, 0x9fff)) counts.cjk++
    else if (inRange(cp, 0x0041, 0x024f)) counts.latin++
  }
  return counts
}

/** 拉丁语系细分：变音符 + 高频词粗判，默认英语（附录 8.1） */
export function guessLatinLang(text: string): LangCode {
  const lower = text.toLowerCase()
  const has = (s: string) => lower.includes(s)
  const word = (w: string) => new RegExp(`\\b${w}\\b`).test(lower)

  // 西班牙语特征最强：ñ 是独家信号
  if (/[ñÑ]/.test(text) || (has('¿') && has('?'))) return 'es'
  // 德语：ß 独家；äöü + 德语高频词
  if (/[ß]/.test(text) || (/[äöüÄÖÜ]/.test(text) && (word('der') || word('die') || word('das') || word('und') || word('ist') || word('nicht'))))
    return 'de'
  // 法语：éèêàçâîôû + 高频词
  if (
    /[àâçèéêëîïôùûœ]/.test(text) &&
    (word('le') || word('la') || word('les') || word('des') || word('une') || word('est') || word('pour') || word('avec'))
  )
    return 'fr'
  return 'en'
}

export function detectLang(text: string): LangCode {
  const counts = countScript(text)
  const entries = Object.entries(counts).sort((a, b) => b[1] - a[1])
  const [top, topCount] = entries[0]
  if (topCount === 0) return 'en' // 纯符号/数字，按英语兜底（可手动切换）

  switch (top) {
    case 'arabic':
      return 'ar'
    case 'hangul':
      return 'ko'
    case 'cyrillic':
      return 'ru'
    case 'kana':
      return 'ja'
    case 'cjk':
      return 'zh'
    default:
      return guessLatinLang(text)
  }
}
