use serde::{Deserialize, Serialize};

/// 选中文本与其屏幕锚点（需求文档 §5.3 统一抽象）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub text: String,
    /// 选区末端的屏幕坐标（逻辑像素，多显示器感知）
    pub anchor: Option<(i32, i32)>,
    /// 来源通道
    pub source: SelectionSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    /// macOS 辅助功能 API
    Ax,
    /// Windows UI Automation / GetGUIThreadInfo
    Uia,
    /// 全局快捷键 + 剪贴板读取（兜底通道）
    Shortcut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionEvent {
    /// 选区就绪（含文本与锚点）
    Selected(Selection),
    /// 选区取消（点击空白、选区变化为空、切换应用）
    Cleared,
}

/// 选区探测统一接口：事件驱动 + 主动读取
pub trait SelectionProvider: Send + Sync {
    /// 事件驱动监听选区变化（非轮询，NFR 4.1）
    fn watch(&self, cb: impl Fn(SelectionEvent) + Send + 'static);
    /// 主动读取当前选区（快捷键通道用）
    fn current_selection(&self) -> Option<Selection>;
}
