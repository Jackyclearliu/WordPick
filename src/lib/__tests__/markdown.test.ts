import { describe, expect, it } from 'vitest'
import { createIncrementalRenderer } from '../markdown'

describe('流式 Markdown 增量渲染（§5.1）', () => {
  it('追加渲染：chunk 逐段出现', () => {
    const r = createIncrementalRenderer()
    expect(r.append('# 标题')).toContain('<h1>')
    expect(r.append('\n\n正文')).toContain('正文')
    expect(r.raw).toBe('# 标题\n\n正文')
  })

  it('增量解析：解析调用次数 = chunk 次数（不重复解析整篇）', () => {
    const r = createIncrementalRenderer()
    for (let i = 0; i < 10; i++) r.append(`段落${i}\n\n`)
    expect(r.parseCalls).toBe(10)
  })

  it('代码块被高亮管线处理（shiki 未就绪时降级为 pre/code 转义）', () => {
    const r = createIncrementalRenderer()
    const html = r.append('```js\nconst a = 1 < 2\n```')
    expect(html).toContain('<pre')
    expect(html).not.toContain('const a = 1 < 2 未转义') // 原样尖括号不得泄漏
    expect(html).toMatch(/&lt;|shiki/)
  })

  it('AI 输出中的内联 HTML 不执行（html:false）', () => {
    const r = createIncrementalRenderer()
    const html = r.append('<script>alert(1)</script>')
    expect(html).not.toContain('<script>')
  })
})
