//! 弹窗定位算法（需求文档 §5.4）：
//! 首选选区下方 8px、水平居中；越界则依次尝试 上方 → 右侧 → 左侧；
//! 均不可行则以锚点为左上角展开。工作区 = 显示器可视区扣除 Dock/任务栏。

/// 矩形（逻辑像素）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, r: &Rect) -> bool {
        r.x >= self.x
            && r.y >= self.y
            && r.x + r.w <= self.x + self.w
            && r.y + r.h <= self.y + self.h
    }
}

/// 弹窗位置偏好
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPosition {
    Below,
    TopRight,
}

const GAP: i32 = 8;

/// 计算弹窗位置：返回窗口左上角坐标，保证完整落入工作区（尽力而为）
pub fn locate(
    anchor: (i32, i32),
    size: (i32, i32),
    workarea: Rect,
    pref: PopupPosition,
) -> (i32, i32) {
    let (ax, ay) = anchor;
    let (w, h) = size;

    let candidates: [(i32, i32); 5] = match pref {
        // 下方居中 → 上方居中 → 右侧 → 左侧 → 锚点左上展开
        PopupPosition::Below => [
            (ax - w / 2, ay + GAP),
            (ax - w / 2, ay - GAP - h),
            (ax + GAP, ay - h / 2),
            (ax - GAP - w, ay - h / 2),
            (ax, ay),
        ],
        // 右上方 → 下方 → 左侧 → 上方 → 锚点左上展开
        PopupPosition::TopRight => [
            (ax + GAP, ay - GAP - h),
            (ax - w / 2, ay + GAP),
            (ax - GAP - w, ay - h / 2),
            (ax - w / 2, ay - GAP - h),
            (ax, ay),
        ],
    };

    for &(x, y) in &candidates {
        let win = Rect { x, y, w, h };
        if workarea.contains(&win) {
            return (x, y);
        }
    }
    // 最终兜底：夹取到工作区内（窗口大于工作区时贴边，避免负坐标/越界）
    let clampv = |v: i32, lo: i32, hi: i32| if hi < lo { lo } else { v.clamp(lo, hi) };
    let x = clampv(ax, workarea.x, workarea.x + workarea.w - w);
    let y = clampv(ay, workarea.y, workarea.y + workarea.h - h);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };

    #[test]
    fn prefers_below() {
        let pos = locate((960, 300), (300, 200), SCREEN, PopupPosition::Below);
        assert_eq!(pos, (960 - 150, 308));
    }

    #[test]
    fn flips_above_when_no_space_below() {
        // 锚点贴屏幕底部：下方无空间 → 上方
        let pos = locate((960, 1060), (300, 200), SCREEN, PopupPosition::Below);
        assert_eq!(pos, (810, 1060 - 8 - 200));
    }

    #[test]
    fn falls_to_right_when_left_edge() {
        // 锚点贴左缘：下方/上方/左侧均越界 → 右侧
        let wa = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 1080,
        };
        let pos = locate((10, 540), (300, 200), wa, PopupPosition::Below);
        assert_eq!(pos, (18, 440));
    }

    #[test]
    fn clamps_as_last_resort() {
        let wa = Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 150,
        };
        let pos = locate((100, 100), (300, 200), wa, PopupPosition::Below);
        assert_eq!(pos, (0, 0));
    }

    #[test]
    fn top_right_pref() {
        let pos = locate((960, 300), (300, 200), SCREEN, PopupPosition::TopRight);
        assert_eq!(pos, (968, 92));
    }
}
