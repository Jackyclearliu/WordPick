import { describe, expect, it } from 'vitest'
import { locate, type Rect } from '../position'

const SCREEN: Rect = { x: 0, y: 0, w: 1920, h: 1080 }

describe('弹窗定位算法（Rust position.rs 的可执行规格）', () => {
  it('首选选区下方、水平居中', () => {
    expect(locate([960, 300], [300, 200], SCREEN, 'below')).toEqual([810, 308])
  })

  it('下方无空间时翻转到上方', () => {
    expect(locate([960, 1060], [300, 200], SCREEN, 'below')).toEqual([810, 852])
  })

  it('左侧贴边时翻转到右侧', () => {
    // 锚点贴左缘：下方/上方/左侧均越界 → 右侧
    const wa: Rect = { x: 0, y: 0, w: 400, h: 1080 }
    expect(locate([10, 540], [300, 200], wa, 'below')).toEqual([18, 440])
  })

  it('四周均不可行时夹取到工作区内', () => {
    const wa: Rect = { x: 0, y: 0, w: 200, h: 150 }
    expect(locate([100, 100], [300, 200], wa, 'below')).toEqual([0, 0])
  })

  it('右上方偏好（top-right）', () => {
    expect(locate([960, 300], [300, 200], SCREEN, 'top-right')).toEqual([968, 92])
  })

  it('多显示器：工作区为次屏坐标时正常定位', () => {
    const second: Rect = { x: 1920, y: 0, w: 2560, h: 1440 }
    expect(locate([2500, 700], [300, 200], second, 'below')).toEqual([2350, 708])
  })
})
