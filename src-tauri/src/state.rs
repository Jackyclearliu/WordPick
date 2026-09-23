//! 应用共享状态
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::llm::adapter::Scenario;
use crate::selection::Selection;

/// 面板上下文：点击工具条功能时携带 (selection, scenario)（§5.4 功能会话制）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelContext {
    pub selection: Selection,
    pub scenario: Scenario,
}

/// 注意：内部锁全部经 Arc 共享，Clone 后多份句柄读写同一数据
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<parking_lot::RwLock<AppConfig>>,
    /// 最近一次选区（工具条显示时最新有效选区）
    pub last_selection: Arc<Mutex<Option<Selection>>>,
    /// 待面板读取的上下文（面板 on mount 通过 command 取走）
    pub pending_panel: Arc<Mutex<Option<PanelContext>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(parking_lot::RwLock::new(config)),
            last_selection: Arc::new(Mutex::new(None)),
            pending_panel: Arc::new(Mutex::new(None)),
        }
    }
}
