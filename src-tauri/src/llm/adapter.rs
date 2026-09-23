//! OpenAI 兼容 Chat Completions 适配器（需求文档 §5.5）：
//! reqwest SSE 流式请求，chunk 经 Tauri event 推送至 WebView；
//! 网络类错误指数退避重试（≤2 次，NFR 4.3）；429 明确提示；支持中途停止。
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::config::{resolve_env_placeholder, ProviderConfig};
use crate::llm::prompt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Translate,
    Explain,
    Chat,
}

/// UI 发起的流式对话请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub request_id: String,
    pub provider: ProviderConfig,
    pub model: String,
    pub scenario: Scenario,
    pub messages: Vec<ChatMessage>, // 已含 system prompt 与完整上下文
    pub max_tokens: Option<u32>,
}

/// 发往 WebView 的流式事件（wp://chat-chunk）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Chunk {
        request_id: String,
        delta: String,
    },
    Done {
        request_id: String,
        usage: Option<Usage>,
    },
    Error {
        request_id: String,
        kind: ErrorKind,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Network,
    RateLimited,
    InvalidKey,
    BadRequest,
    Server,
    Unknown,
}

/// 活动请求的停止开关
static STOP_FLAGS: Lazy<parking_lot::Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| parking_lot::Mutex::new(HashMap::new()));

pub fn stop(request_id: &str) {
    if let Some(flag) = STOP_FLAGS.lock().get(request_id) {
        flag.store(true, Ordering::SeqCst);
    }
}

/// 解析 API Key：keychain 优先 → 配置明文 / ${ENV} 占位符（FR-6.3）
fn resolve_api_key(provider: &ProviderConfig) -> Result<String, ErrorKind> {
    if let Ok(key) = crate::secure::read_key(&provider.name) {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    let key = resolve_env_placeholder(&provider.api_key);
    if key.is_empty() {
        return Err(ErrorKind::InvalidKey);
    }
    Ok(key)
}

/// 场景化 system prompt
pub fn system_prompt(scenario: Scenario, source_lang: &str, target_lang: &str) -> String {
    match scenario {
        Scenario::Translate => prompt::TRANSLATE_TEMPLATE
            .replace("{source_lang}", source_lang)
            .replace("{target_lang}", target_lang),
        Scenario::Explain => prompt::EXPLAIN_TEMPLATE.to_string(),
        Scenario::Chat => prompt::CHAT_TEMPLATE.to_string(),
    }
}

#[derive(Serialize)]
struct CompletionBody {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

/// 发事件到 WebView；失败必须落日志（事件丢失正是「卡在正在生成」的隐形元凶）
fn emit_chat(app: &tauri::AppHandle, event: ChatEvent) {
    if let Err(e) = app.emit("wp://chat-chunk", event) {
        log::error!("emit chat event failed: {e}");
    }
}

/// 发起流式对话：SSE chunk 逐条 emit；结束时发 Done/Error
pub async fn chat_stream(app: tauri::AppHandle, req: ChatRequest) {
    log::info!(
        "chat_stream start: provider={} model={} scenario={:?} request_id={} messages={} user_text={:?}",
        req.provider.name,
        req.model,
        req.scenario,
        req.request_id,
        req.messages.len(),
        req.messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| &m.content[..m.content.len().min(120)])
    );
    let flag = Arc::new(AtomicBool::new(false));
    STOP_FLAGS
        .lock()
        .insert(req.request_id.clone(), flag.clone());
    let streamed = Arc::new(AtomicBool::new(false));

    let result = run_with_retry(&app, &req, &flag, &streamed).await;
    let _ = STOP_FLAGS.lock().remove(&req.request_id);

    match &result {
        Ok(()) => log::info!("chat_stream done: request_id={}", req.request_id),
        Err((kind, message)) => log::warn!(
            "chat_stream error: request_id={} kind={:?} message={message}",
            req.request_id,
            kind
        ),
    }

    if let Err((kind, message)) = result {
        emit_chat(
            &app,
            ChatEvent::Error {
                request_id: req.request_id,
                kind,
                message,
            },
        );
    }
}

const MAX_RETRIES: u32 = 2; // 网络类错误最多 2 次（NFR 4.3）

async fn run_with_retry(
    app: &tauri::AppHandle,
    req: &ChatRequest,
    flag: &AtomicBool,
    streamed: &AtomicBool,
) -> Result<(), (ErrorKind, String)> {
    let mut attempt = 0;
    loop {
        match run_once(app, req, flag, streamed).await {
            Ok(()) => return Ok(()),
            Err(
                e @ (ErrorKind::RateLimited, _)
                | e @ (ErrorKind::InvalidKey, _)
                | e @ (ErrorKind::BadRequest, _),
            ) => {
                return Err(e); // 不重试：限流 / Key 无效 / 请求错误直接反馈
            }
            Err(e) if streamed.load(Ordering::SeqCst) => {
                // 流已开始再重试会把全文重发一遍，前端渲染器叠加导致整段重复（NFR 4.3：
                // 流式中断保留已输出内容）。直接报错保留已有输出，交给用户重试。
                log::warn!(
                    "chat stream interrupted after output started, no retry: {:?}",
                    e.0
                );
                return Err(e);
            }
            Err(e) if attempt < MAX_RETRIES => {
                attempt += 1;
                log::warn!("chat attempt {attempt} failed ({:?}), backoff…", e.0);
                tokio::time::sleep(Duration::from_millis(400 * (1 << attempt))).await;
                if flag.load(Ordering::SeqCst) {
                    return Ok(()); // 停止后不再重试
                }
            }
            Err(e) => return Err(e),
        }
    }
}

async fn run_once(
    app: &tauri::AppHandle,
    req: &ChatRequest,
    flag: &AtomicBool,
    streamed: &AtomicBool,
) -> Result<(), (ErrorKind, String)> {
    let key = resolve_api_key(&req.provider).map_err(|k| {
        log::warn!(
            "resolve_api_key failed: provider={} kind={:?}",
            req.provider.name,
            k
        );
        (k, "API Key 未配置或无效".into())
    })?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| (ErrorKind::Network, e.to_string()))?;

    let url = format!(
        "{}/chat/completions",
        req.provider.base_url.trim_end_matches('/')
    );
    let body = CompletionBody {
        model: req.model.clone(),
        messages: req.messages.clone(),
        stream: true,
        max_tokens: req.max_tokens,
        temperature: match req.scenario {
            Scenario::Translate => Some(0.3),
            _ => None,
        },
    };

    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                (ErrorKind::Network, e.to_string())
            } else {
                (ErrorKind::Unknown, e.to_string())
            }
        })?;

    log::info!("chat HTTP {}: request_id={}", resp.status(), req.request_id);
    match resp.status() {
        StatusCode::OK => {}
        StatusCode::TOO_MANY_REQUESTS => {
            return Err((
                ErrorKind::RateLimited,
                "请求过于频繁（429 限流），请稍后再试".into(),
            ));
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err((ErrorKind::InvalidKey, "API Key 无效或未授权".into()));
        }
        s if s.is_client_error() => {
            let text = resp.text().await.unwrap_or_default();
            return Err((
                ErrorKind::BadRequest,
                format!("请求被服务端拒绝（{s}）：{text}"),
            ));
        }
        s => {
            let text = resp.text().await.unwrap_or_default();
            return Err((ErrorKind::Server, format!("服务端错误（{s}）：{text}")));
        }
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        if flag.load(Ordering::SeqCst) {
            return Ok(()); // 用户点击停止：保留已输出内容（FR-5.4）
        }
        let bytes = chunk.map_err(|e| (ErrorKind::Network, e.to_string()))?;
        buf.push_str(&String::from_utf8_lossy(&bytes));

        // SSE 按空行分包（先切出事件段再清空缓冲区，避免借用冲突）
        while let Some(pos) = buf.find("\n\n") {
            let rest = buf.split_off(pos + 2);
            let event = std::mem::replace(&mut buf, rest);
            for line in event.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    emit_chat(
                        app,
                        ChatEvent::Done {
                            request_id: req.request_id.clone(),
                            usage: None,
                        },
                    );
                    return Ok(());
                }
                if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(delta) = parsed
                        .choices
                        .first()
                        .and_then(|c| c.delta.content.clone())
                        .filter(|d| !d.is_empty())
                    {
                        streamed.store(true, Ordering::SeqCst);
                        emit_chat(
                            app,
                            ChatEvent::Chunk {
                                request_id: req.request_id.clone(),
                                delta,
                            },
                        );
                    }
                }
            }
        }
    }
    emit_chat(
        app,
        ChatEvent::Done {
            request_id: req.request_id.clone(),
            usage: None,
        },
    );
    Ok(())
}

/// 连通性测试（FR-6.5）：最小化请求，返回延迟或错误类型
pub async fn test_connection(
    provider: &ProviderConfig,
    model: &str,
) -> Result<u128, (ErrorKind, String)> {
    let started = std::time::Instant::now();
    let key = resolve_api_key(provider).map_err(|k| (k, "API Key 未配置或无效".into()))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| (ErrorKind::Network, e.to_string()))?;
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
    });
    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| (ErrorKind::Network, e.to_string()))?;
    match resp.status() {
        StatusCode::OK => Ok(started.elapsed().as_millis()),
        StatusCode::TOO_MANY_REQUESTS => Err((ErrorKind::RateLimited, "429 限流".into())),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            Err((ErrorKind::InvalidKey, "API Key 无效".into()))
        }
        s => Err((ErrorKind::Server, format!("服务端返回 {s}"))),
    }
}
