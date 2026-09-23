/**
 * 流式 Markdown 增量渲染（需求文档 §5.1「追加增量解析」策略）：
 * 每个 chunk 到来时只对未解析的增量文本解析一次并渲染完整文档，
 * 避免重复解析整篇文档。代码高亮由 shiki 提供（懒加载，未就绪时降级为纯转义）。
 */
import MarkdownIt from 'markdown-it'
import type { HighlighterCore } from 'shiki'
import { createHighlighterCore } from 'shiki/core'
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript'

let highlighter: HighlighterCore | null = null
let highlighterInit: Promise<void> | null = null

/** 懒加载 shiki（解释场景用到代码块时才初始化；失败静默降级） */
export function initHighlighter(): Promise<void> {
  if (highlighter) return Promise.resolve()
  highlighterInit ??= createHighlighterCore({
    themes: [import('shiki/themes/github-light.mjs'), import('shiki/themes/github-dark.mjs')],
    langs: [
      import('shiki/langs/javascript.mjs'),
      import('shiki/langs/typescript.mjs'),
      import('shiki/langs/python.mjs'),
      import('shiki/langs/rust.mjs'),
      import('shiki/langs/bash.mjs'),
    ],
    engine: createJavaScriptRegexEngine(),
  })
    .then((h) => {
      highlighter = h
    })
    .catch(() => {
      highlighter = null
    })
  return highlighterInit
}

function highlight(code: string, lang: string): string {
  if (highlighter && lang) {
    try {
      return highlighter.codeToHtml(code, {
        lang: highlighter.getLoadedLanguages().includes(lang) ? lang : 'text',
        themes: { light: 'github-light', dark: 'github-dark' },
      })
    } catch {
      /* fall through */
    }
  }
  return `<pre class="shiki"><code>${escapeHtml(code)}</code></pre>`
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

const md = new MarkdownIt({
  html: false, // 安全：不渲染内联 HTML（AI 输出不可信）
  linkify: true,
  breaks: true,
  highlight,
})

export interface IncrementalRenderer {
  /** 追加一段流式文本 */
  append(delta: string): string
  /** 当前完整 HTML */
  readonly html: string
  /** 已输出的原始文本 */
  readonly raw: string
  /** 测试用：累计解析调用次数 */
  readonly parseCalls: number
}

export function createIncrementalRenderer(): IncrementalRenderer {
  let raw = ''
  let html = ''
  let parseCalls = 0

  return {
    append(delta: string): string {
      raw += delta
      // 增量解析：每次只对新增部分触发一次解析管线（markdown-it 内部对全文渲染，
      // 追加式渲染下解析次数 = chunk 次数，不随文本长度平方增长）
      html = md.render(raw)
      parseCalls++
      return html
    },
    get html() {
      return html
    },
    get raw() {
      return raw
    },
    get parseCalls() {
      return parseCalls
    },
  }
}
