/**
 * 弹窗定位算法（Rust windows/position.rs 的 TS 可执行规格，单测以此为准）。
 * 首选选区下方 8px、水平居中；越界依次尝试 上方 → 右侧 → 左侧 → 锚点左上展开；
 * 最终兜底夹取到工作区内。需求文档 §5.4。
 */
export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export type PopupPosition = 'below' | 'top-right'

const GAP = 8

function contains(outer: Rect, r: Rect): boolean {
  return r.x >= outer.x && r.y >= outer.y && r.x + r.w <= outer.x + outer.w && r.y + r.h <= outer.y + outer.h
}

export function locate(
  anchor: [number, number],
  size: [number, number],
  workarea: Rect,
  pref: PopupPosition,
): [number, number] {
  const [ax, ay] = anchor
  const [w, h] = size

  const candidates: Array<[number, number]> =
    pref === 'below'
      ? [
          [ax - w / 2, ay + GAP],
          [ax - w / 2, ay - GAP - h],
          [ax + GAP, ay - h / 2],
          [ax - GAP - w, ay - h / 2],
          [ax, ay],
        ]
      : [
          [ax + GAP, ay - GAP - h],
          [ax - w / 2, ay + GAP],
          [ax - GAP - w, ay - h / 2],
          [ax - w / 2, ay - GAP - h],
          [ax, ay],
        ]

  for (const [x, y] of candidates) {
    if (contains(workarea, { x, y, w, h })) return [x, y]
  }
  // 最终兜底：夹取到工作区内（窗口大于工作区时贴边，避免负坐标/越界）
  const clamp = (v: number, lo: number, hi: number) => (hi < lo ? lo : Math.min(Math.max(v, lo), hi))
  return [clamp(ax, workarea.x, workarea.x + workarea.w - w), clamp(ay, workarea.y, workarea.y + workarea.h - h)]
}
