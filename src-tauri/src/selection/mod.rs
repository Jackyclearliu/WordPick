//! 选区探测：macOS AX / Windows UIA / 快捷键+剪贴板 三通道统一抽象。
//! 详见需求文档 §5.3。
pub mod clipboard;

#[cfg(target_os = "macos")]
pub mod ax;

#[cfg(windows)]
pub mod uia;

pub mod r#trait;

pub use r#trait::{Selection, SelectionEvent, SelectionProvider, SelectionSource};
