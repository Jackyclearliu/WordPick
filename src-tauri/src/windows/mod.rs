//! 窗口模型：工具条 / 对话框 / 设置（需求文档 §5.4）
pub mod manager;
pub mod position;

/// 三类业务窗口（均为 decorations:false, transparent:true, skipTaskbar:true）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// 工具条：无焦点抢占、尺寸内容自适应、跟随选区显示/隐藏
    Toolbar,
    /// 对话框：可聚焦、可拖拽、钉住时 alwaysOnTop
    Panel,
    /// 设置：标准有焦点的普通窗口
    Settings,
}
