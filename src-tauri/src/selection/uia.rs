//! Windows 选区通道（需求文档 §5.3）：
//! 1) **低层鼠标事件触发**：拖选松开或双击后模拟 Ctrl+C 取词；点击工具条外部
//!    在鼠标按下时立即关闭。Chromium 系（Chrome/Edge/VS Code/Electron）不响应
//!    WM_COPY、不触发 EVENT_OBJECT_TEXTSELECTIONCHANGED，UIA 树也可能惰性不可见；
//! 2) SetWinEventHook 订阅 EVENT_OBJECT_TEXTSELECTIONCHANGED（记事本/Office 等
//!    经典应用事件驱动），配合哨兵式 WM_COPY 取词；
//! 3) GetGUIThreadInfo 取 caret/选区矩形定位（取不到以光标位置兜底）。
//!
//! 设计要点：不做周期性剪贴板写入（剪贴板历史工具零干扰），只在鼠标选择完成与
//! 「选区变更事件」时探测；非文本剪贴板（图片）跳过探测。
use std::sync::mpsc::{Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    EVENT_OBJECT_TEXTSELECTIONCHANGED, GUITHREADINFO, WINEVENT_OUTOFCONTEXT,
};

use super::clipboard;
use super::{Selection, SelectionEvent, SelectionSource};

static CTX: OnceLock<(Sender<SelectionEvent>, tauri::AppHandle)> = OnceLock::new();
static HOOK: OnceLock<isize> = OnceLock::new();

/// 两路探测（WinEvent / 拖选释放）共享的选区状态：按文本去重，避免重复上报
struct ProbeState {
    last_text: Option<String>,
    /// 最近一次「外部应用」前台窗口（自家工具条/面板抢焦点时忽略，见 poll_loop）
    last_foreign_hwnd: Option<isize>,
}
static STATE: OnceLock<parking_lot::Mutex<ProbeState>> = OnceLock::new();

/// 统一上报：有变化才发事件（Selected 按文本去重；Cleared 仅在确有选区时发一次）
fn handle_probe(tx: &Sender<SelectionEvent>, sel: Option<Selection>) {
    let Some(state) = STATE.get() else { return };
    let mut st = state.lock();
    match sel {
        Some(s) => {
            // 比较忽略首尾空白：WM_COPY 与模拟 Ctrl+C 两路探测返回的文本可能差一个换行
            let same = st
                .last_text
                .as_deref()
                .is_some_and(|t| t.trim() == s.text.trim());
            if !same {
                st.last_text = Some(s.text.clone());
                let _ = tx.send(SelectionEvent::Selected(s));
            }
        }
        None => {
            if st.last_text.is_some() {
                st.last_text = None;
                let _ = tx.send(SelectionEvent::Cleared);
            }
        }
    }
}

/// 前台切换检查周期（轻量，不触碰剪贴板）
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DRAG_MIN_MOVE: i32 = 6;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_MAX_MOVE: i32 = 4;
const DOUBLE_CLICK_PROBE_DELAY: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseAction {
    Clear,
    Probe,
}

enum MouseEvent {
    Down { at: Instant, pos: (i32, i32) },
    Up { at: Instant, pos: (i32, i32) },
}

#[derive(Default)]
struct MouseState {
    down: Option<(Instant, (i32, i32))>,
    last_click: Option<(Instant, (i32, i32))>,
    double_click: Option<Instant>,
}

fn point_in_toolbar(pos: (i32, i32), toolbar: Option<(i32, i32, i32, i32)>) -> bool {
    toolbar.is_some_and(|(x, y, w, h)| pos.0 >= x && pos.0 <= x + w && pos.1 >= y && pos.1 <= y + h)
}

impl MouseState {
    fn on_event(
        &mut self,
        event: MouseEvent,
        toolbar: Option<(i32, i32, i32, i32)>,
    ) -> Option<MouseAction> {
        match event {
            MouseEvent::Down { at, pos } => {
                let double_click = self.last_click.is_some_and(|(previous, previous_pos)| {
                    at.saturating_duration_since(previous) <= DOUBLE_CLICK_INTERVAL
                        && (pos.0 - previous_pos.0).abs() <= DOUBLE_CLICK_MAX_MOVE
                        && (pos.1 - previous_pos.1).abs() <= DOUBLE_CLICK_MAX_MOVE
                });
                self.double_click = double_click.then_some(at);
                if double_click {
                    self.last_click = None;
                }
                self.down = Some((at, pos));
                (!point_in_toolbar(pos, toolbar) && toolbar.is_some()).then_some(MouseAction::Clear)
            }
            MouseEvent::Up { at, pos } => {
                let Some((_at, down_pos)) = self.down.take() else {
                    if point_in_toolbar(pos, toolbar) {
                        return None;
                    }
                    return self.double_click.is_some().then_some(MouseAction::Clear);
                };
                let dragged =
                    (pos.0 - down_pos.0).abs() + (pos.1 - down_pos.1).abs() > DRAG_MIN_MOVE;
                if !dragged && point_in_toolbar(down_pos, toolbar) {
                    self.double_click = None;
                    return None;
                }
                if dragged {
                    self.double_click = None;
                    self.last_click = None;
                    Some(MouseAction::Probe)
                } else {
                    if self.double_click.is_none() {
                        self.last_click = Some((at, pos));
                    }
                    Some(MouseAction::Clear)
                }
            }
        }
    }

    fn pending_double_click(&mut self, now: Instant, button_down: bool) -> Option<MouseAction> {
        if button_down {
            return None;
        }
        let ready = self
            .double_click
            .is_some_and(|at| now.saturating_duration_since(at) >= DOUBLE_CLICK_PROBE_DELAY);
        if ready {
            self.double_click = None;
            Some(MouseAction::Probe)
        } else {
            None
        }
    }
}

/// 启动 WinEventHook 与低层鼠标/键盘钩子（事件经 channel 发给统一分发器）
pub fn start(tx: Sender<SelectionEvent>, app: tauri::AppHandle) {
    if CTX.set((tx, app)).is_err() {
        return;
    }
    let _ = STATE.set(parking_lot::Mutex::new(ProbeState {
        last_text: None,
        last_foreign_hwnd: None,
    }));
    let (mouse_tx, mouse_rx) = std::sync::mpsc::channel();
    let _ = MOUSE_TX.set(mouse_tx);
    std::thread::spawn(|| unsafe {
        // 低层键盘钩子（FR-2.4 数字键）安装在本线程：LL 钩子回调经安装线程的
        // 消息队列分发，本线程有健壮的 GetMessageW 循环，回调绝不积压
        // （曾装在 poll_loop 并用 MsgWait+PeekMessage 泵消息：热循环下按键被
        // 系统丢弃，表现为「Ctrl+C/Delete/打字全灭」，直到进程退出才恢复）。
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowsHookExW, WH_KEYBOARD_LL, WH_MOUSE_LL,
        };
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(toolbar_kb_hook), hmod, 0) {
            Ok(h) => {
                let _ = KB_HOOK.set(h.0 as isize);
                log::info!("toolbar keyboard hook installed");
            }
            Err(e) => {
                log::warn!("toolbar keyboard hook install failed（数字键快捷触发不可用）: {e}")
            }
        }
        match SetWindowsHookExW(WH_MOUSE_LL, Some(selection_mouse_hook), hmod, 0) {
            Ok(h) => {
                let _ = MOUSE_HOOK.set(h.0 as isize);
                log::info!("selection mouse hook installed");
            }
            Err(e) => log::warn!("selection mouse hook install failed: {e}"),
        }

        let hook: HWINEVENTHOOK = SetWinEventHook(
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            None,
            Some(on_selection_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let _ = HOOK.set(hook.0 as isize);
        log::info!("UIA/WinEvent selection watcher started");

        // 消息循环：本线程的 WinEvent 回调需要消息队列
        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        while windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            let _ = windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    });
    std::thread::spawn(move || poll_loop(mouse_rx));
    log::info!("event-driven selection watcher started");
}

// ---------------------------------------------------------------------------
// 工具条键盘接管（FR-2.4）：低层键盘钩子，仅当光标位于工具条热区内时吞掉
// 1/2/3（翻译/解释/设置）与 Esc（关闭）；工具条可见时其它按键只关闭工具条并原样放行。
// 不抢焦点——焦点若被工具条持有会吞掉目标应用的一切按键（Ctrl+C/Delete/输入全部失效）。
// ---------------------------------------------------------------------------

/// 低层钩子（进程生命周期内常驻；工具条隐藏时热区为 None，键盘钩子直通）
static KB_HOOK: OnceLock<isize> = OnceLock::new();
static MOUSE_HOOK: OnceLock<isize> = OnceLock::new();
static MOUSE_TX: OnceLock<Sender<MouseEvent>> = OnceLock::new();

const VK_1: u32 = 0x31;
const VK_2: u32 = 0x32;
const VK_3: u32 = 0x33;
const VK_ESCAPE: u32 = 0x1B;

unsafe extern "system" fn toolbar_kb_hook(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
    };
    if ncode >= 0 && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN) {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let rect = *crate::windows::manager::TOOLBAR_RECT.lock();
        if let Some((x, y, w, h)) = rect {
            let action = match kb.vkCode {
                VK_1 => Some("translate"),
                VK_2 => Some("explain"),
                VK_3 => Some("settings"),
                VK_ESCAPE => Some("__esc__"),
                _ => None,
            };
            let in_hotzone = match clipboard::cursor_pos() {
                Some((cx, cy)) => cx >= x && cx <= x + w && cy >= y && cy <= y + h,
                None => false,
            };
            if in_hotzone {
                if let Some(action) = action {
                    if let Some((_tx, app)) = CTX.get() {
                        if action == "__esc__" {
                            crate::windows::manager::hide_toolbar(app);
                        } else {
                            use tauri::Manager;
                            let state = app.state::<crate::state::AppState>();
                            crate::commands::perform_toolbar_action(action, app, &state);
                        }
                    }
                    return LRESULT(1); // 仅热区内的功能键吞掉，不再转发
                }
            }
            if let Some((_tx, app)) = CTX.get() {
                crate::windows::manager::hide_toolbar(app);
            }
        }
    }
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

unsafe extern "system" fn selection_mouse_hook(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP,
    };
    if ncode >= 0 {
        let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let at = Instant::now();
        let pos = (mouse.pt.x, mouse.pt.y);
        let event = match wparam.0 as u32 {
            WM_LBUTTONDOWN => Some(MouseEvent::Down { at, pos }),
            WM_LBUTTONUP => Some(MouseEvent::Up { at, pos }),
            _ => None,
        };
        if let (Some(tx), Some(event)) = (MOUSE_TX.get(), event) {
            let _ = tx.send(event);
        }
    }
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

unsafe extern "system" fn on_selection_changed(
    _hwineventhook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _idobject: i32,
    _idchild: i32,
    _ideventthread: u32,
    _dwmseventtime: u32,
) {
    let Some((tx, app)) = CTX.get() else { return };
    // WebView2 等自家窗口也会触发选区变更事件：忽略本进程窗口，避免自我探测
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == std::process::id() || left_button_down() {
        return;
    }
    let sel = read_selection(app, hwnd);
    handle_probe(tx, sel);
}

/// 鼠标事件驱动：拖选松开或双击后取词；点击工具条外部在按下时立即关闭。
/// 前台切换检查仍使用轻量周期，所有 UIA/剪贴板读取都留在本线程而非钩子回调。
fn poll_loop(mouse_rx: Receiver<MouseEvent>) {
    let mut mouse = MouseState::default();
    loop {
        let event = mouse_rx.recv_timeout(POLL_INTERVAL).ok();
        let Some((tx, app)) = CTX.get() else { continue };
        let fg = foreground_hwnd();

        if let Some(hwnd) = fg {
            let Some(state) = STATE.get() else { continue };
            let mut st = state.lock();
            if st.last_foreign_hwnd != Some(hwnd.0 as isize) {
                st.last_foreign_hwnd = Some(hwnd.0 as isize);
                if st.last_text.is_some() {
                    st.last_text = None;
                    drop(st);
                    let _ = tx.send(SelectionEvent::Cleared);
                }
            }
        }

        if let Some(event) = event {
            let toolbar = *crate::windows::manager::TOOLBAR_RECT.lock();
            if let Some(action) = mouse.on_event(event, toolbar) {
                apply_mouse_action(action, toolbar, fg, tx, app);
            }
        }
        if let Some(action) = mouse.pending_double_click(Instant::now(), left_button_down()) {
            apply_mouse_action(action, None, fg, tx, app);
        }
    }
}

fn apply_mouse_action(
    action: MouseAction,
    toolbar: Option<(i32, i32, i32, i32)>,
    fg: Option<HWND>,
    tx: &Sender<SelectionEvent>,
    app: &tauri::AppHandle,
) {
    match action {
        MouseAction::Clear => {
            if toolbar.is_some() {
                crate::windows::manager::hide_toolbar(app);
                if let Some(state) = STATE.get() {
                    state.lock().last_text = None;
                }
                let _ = tx.send(SelectionEvent::Cleared);
            } else if fg.is_some() {
                handle_probe(tx, None);
            }
        }
        MouseAction::Probe => {
            if let Some(target) = fg {
                let sel = read_selection_after_drag(app, target);
                handle_probe(tx, sel);
            }
        }
    }
}

/// 前台窗口（跳过 WordPick 自身窗口，避免自我探测）
fn foreground_hwnd() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        Some(fg)
    }
}

fn left_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    (unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16) & 0x8000 != 0
}

/// 拖选释放后取词：优先只读 UIA，Chromium 等不支持的应用再模拟 Ctrl+C。
/// ⚠️ 终端类窗口禁用：conhost 会把 Ctrl+C 中断转发给所有附着该控制台的进程
/// （dev 模式下直接杀死本应用，exit 0xc000013a），并可能干扰用户终端内运行的程序。
fn read_selection_after_drag(app: &tauri::AppHandle, hwnd: HWND) -> Option<Selection> {
    if is_console_window(hwnd) {
        // 终端（conhost/Windows Terminal 等）：禁止模拟 Ctrl+C（中断转发会杀死附着进程）；
        // UIA TextPattern 直读优先，个别终端退回 WM_COPY 哨兵（仅拖选释放瞬间一次，非高频）
        let text = match read_selection_uia(hwnd) {
            Some(t) => Some(t),
            None => {
                log::info!(
                    "console: UIA selection unavailable (hwnd={:#x}), fallback WM_COPY",
                    hwnd.0 as usize
                );
                clipboard::copy_selection_via_wm_copy(app, hwnd)
            }
        }?;
        return build_selection(text, anchor_for(hwnd));
    }
    let text = read_selection_uia(hwnd).or_else(|| clipboard::simulate_ctrl_c_probe(app))?;
    build_selection(text, anchor_for(hwnd))
}

/// WinEvent 通道：UIA TextPattern 只读取词。
/// ⚠️ 事件路径禁用一切剪贴板写入（WM_COPY 哨兵）：TEXTSELECTIONCHANGED 在输入时高频
/// 触发，「写哨兵→复制→读后还原」会覆盖用户紧随其后的 Ctrl+C 复制（系统复制「失效」）。
fn read_selection(_app: &tauri::AppHandle, hwnd: HWND) -> Option<Selection> {
    let text = read_selection_uia(hwnd)?;
    build_selection(text, anchor_for(hwnd))
}

/// UIA TextPattern 直读选区文本（只读，不碰剪贴板、不发按键）。
/// 适用于终端（conhost/Windows Terminal）、记事本等绝大多数应用；
/// Chromium 系 UIA 树惰性不可见，返回 None（由拖选释放的 Ctrl+C 通道负责）。
pub fn read_selection_uia(hwnd: HWND) -> Option<String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED); // 每线程一次；已初始化返回 S_FALSE
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok()?;
        // TextPattern 常在子元素上（如终端的 TermControl、记事本的 Edit）：
        // 先取事件所属窗口的元素，拿不到 TextPattern 再退回光标下的元素
        let element = match automation.ElementFromHandle(hwnd) {
            Ok(el) if el.GetCurrentPattern(UIA_TextPatternId).is_ok() => el,
            _ => {
                let (x, y) = clipboard::cursor_pos()?;
                automation
                    .ElementFromPoint(windows::Win32::Foundation::POINT { x, y })
                    .ok()?
            }
        };
        let pattern = element
            .GetCurrentPattern(UIA_TextPatternId)
            .ok()?
            .cast::<IUIAutomationTextPattern>()
            .ok()?;
        let ranges = pattern.GetSelection().ok()?;
        let len = ranges.Length().ok()?;
        let mut parts: Vec<String> = Vec::new();
        for i in 0..len {
            let Ok(range) = ranges.GetElement(i) else {
                continue;
            };
            if let Ok(text) = range.GetText(-1) {
                let s = String::from_utf16_lossy(text.as_wide());
                if !s.trim().is_empty() {
                    parts.push(s);
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

/// 快捷键通道辅助：直读前台窗口的 UIA 选区（终端里剪贴板常是旧内容，Alt+Q 不应读它）
#[cfg(windows)]
pub fn foreground_selection_text() -> Option<String> {
    let fg = foreground_hwnd()?;
    read_selection_uia(fg)
}

fn build_selection(text: String, anchor: Option<(i32, i32)>) -> Option<Selection> {
    if text.trim().is_empty() {
        return None;
    }
    Some(Selection {
        text,
        anchor,
        source: SelectionSource::Uia,
    })
}

/// 锚点：caret 仅在「可信且离光标不远」时采用——XAML 应用（新版记事本）的
/// GetGUIThreadInfo 可能给出原点处的无效 caret，直接把工具条弹到屏幕左上角。
/// 拖选释放时光标就在选区末端，作为 caret 缺失/不可信时的兜底。
fn anchor_for(hwnd: HWND) -> Option<(i32, i32)> {
    let caret = unsafe { caret_anchor(hwnd) };
    let cursor = clipboard::cursor_pos();
    match (caret, cursor) {
        (Some(c), Some(m)) if (c.0 - m.0).abs() + (c.1 - m.1).abs() <= 400 => Some(c),
        (_, Some(m)) => Some(m),
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// 前台窗口所属进程是否为控制台/终端类（对它们禁止模拟 Ctrl+C）
fn is_console_window(hwnd: HWND) -> bool {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(h);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit('\\').next().unwrap_or_default().to_lowercase();
        matches!(
            name.as_str(),
            "conhost.exe"
                | "openconsole.exe"
                | "windowsterminal.exe"
                | "wezterm-gui.exe"
                | "alacritty.exe"
                | "tabby.exe"
                | "hyper.exe"
        )
    }
}

/// GetGUIThreadInfo 取选区/caret 矩形中心（逻辑像素）
unsafe fn caret_anchor(hwnd: HWND) -> Option<(i32, i32)> {
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if thread_id == 0 {
        return None;
    }
    unsafe {
        if GetGUIThreadInfo(thread_id, &mut info).is_err() {
            return None;
        }
    }
    let rc = info.rcCaret;
    if rc.right <= rc.left || rc.bottom <= rc.top {
        return None;
    }
    Some(((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2))
}

/// 前台窗口标题（应用黑名单用，FR-1.5）
pub fn frontmost_app_name() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    let mut buf = [0u16; 256];
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let len = GetWindowTextW(hwnd, &mut buf);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::{MouseAction, MouseEvent, MouseState};
    use std::time::{Duration, Instant};

    #[test]
    fn fast_drag_still_requests_selection_probe() {
        let now = Instant::now();
        let mut state = MouseState::default();
        assert_eq!(
            state.on_event(
                MouseEvent::Down {
                    at: now,
                    pos: (10, 10),
                },
                None,
            ),
            None
        );
        assert_eq!(
            state.on_event(
                MouseEvent::Up {
                    at: now + Duration::from_millis(40),
                    pos: (40, 10),
                },
                None,
            ),
            Some(MouseAction::Probe)
        );
    }

    #[test]
    fn double_click_requests_delayed_selection_probe() {
        let now = Instant::now();
        let mut state = MouseState::default();
        let pos = (20, 20);
        state.on_event(MouseEvent::Down { at: now, pos }, None);
        assert_eq!(
            state.on_event(
                MouseEvent::Up {
                    at: now + Duration::from_millis(40),
                    pos,
                },
                None,
            ),
            Some(MouseAction::Clear)
        );
        state.on_event(
            MouseEvent::Down {
                at: now + Duration::from_millis(80),
                pos,
            },
            None,
        );
        state.on_event(
            MouseEvent::Up {
                at: now + Duration::from_millis(120),
                pos,
            },
            None,
        );
        assert_eq!(
            state.pending_double_click(now + Duration::from_millis(159), false),
            None
        );
        assert_eq!(
            state.pending_double_click(now + Duration::from_millis(160), false),
            Some(MouseAction::Probe)
        );
    }

    #[test]
    fn mouse_down_outside_toolbar_clears_immediately() {
        let now = Instant::now();
        let mut state = MouseState::default();
        let toolbar = Some((100, 100, 180, 44));
        assert_eq!(
            state.on_event(
                MouseEvent::Down {
                    at: now,
                    pos: (20, 20),
                },
                toolbar,
            ),
            Some(MouseAction::Clear)
        );
        assert_eq!(
            state.on_event(
                MouseEvent::Down {
                    at: now,
                    pos: (120, 120),
                },
                toolbar,
            ),
            None
        );
        assert_eq!(
            state.on_event(
                MouseEvent::Up {
                    at: now + Duration::from_millis(40),
                    pos: (120, 120),
                },
                toolbar,
            ),
            None
        );
    }
}
