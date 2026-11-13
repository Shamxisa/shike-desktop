use crate::{models::Profile, AppResult, AppState};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

const KEY_SERVICE: &str = "com.shike.desktop.ai";

fn credential(id: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEY_SERVICE, id).map_err(|_| "无法访问 Windows 凭据管理器".into())
}

pub fn get_key(id: &str) -> AppResult<String> {
    credential(id)?
        .get_password()
        .map_err(|_| "请先为此服务保存 API Key".into())
}

pub fn save_key(id: &str, value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }
    credential(id)?
        .set_password(value.trim())
        .map_err(|_| "API Key 保存失败，请重试".into())
}

pub fn delete_key(id: &str) -> AppResult<()> {
    match credential(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("系统凭据删除失败，请重试".into()),
    }
}

#[tauri::command]
pub fn key_status(id: String) -> AppResult<bool> {
    match credential(&id)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("无法读取系统凭据状态".into()),
    }
}

#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, id: String, key: String) -> AppResult<()> {
    let store = state.store.lock().map_err(|_| "数据暂不可用")?;
    if !store.profiles.iter().any(|profile| profile.id == id) {
        return Err("请先保存服务配置".into());
    }
    drop(store);
    save_key(&id, &key)
}

#[tauri::command]
pub fn remove_api_key(id: String) -> AppResult<()> {
    delete_key(&id)
}

pub fn validate_profile(profile: &Profile) -> AppResult<()> {
    let url = reqwest::Url::parse(&profile.base_url).map_err(|_| "接口地址无效")?;
    if !["https", "http"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("请输入有效的 HTTP 或 HTTPS 接口基础地址".into());
    }
    if profile.name.trim().is_empty() || profile.model.trim().is_empty() {
        return Err("请填写服务名称和模型名称".into());
    }
    if profile.protocol != "chat-completions" {
        return Err("当前支持 Chat Completions 协议".into());
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamEvent {
    request_id: String,
    status: String,
    delta: String,
    error: Option<String>,
}

// HTTP chunks may split UTF-8 characters, SSE lines and complete events.
#[derive(Default)]
struct SseDecoder {
    bytes: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) -> AppResult<Vec<String>> {
        self.bytes.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.bytes.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|_| "流式响应编码无效")?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_string());
            }
        }
        if self.bytes.len() > 2_000_000 {
            return Err("流式响应事件过大".into());
        }
        Ok(events)
    }

    fn finish(&mut self) -> AppResult<Vec<String>> {
        self.push(b"\n\n")
    }
}

fn emit(app: &AppHandle, request_id: &str, status: &str, delta: &str, error: Option<String>) {
    let _ = app.emit(
        "ai-stream",
        StreamEvent {
            request_id: request_id.into(),
            status: status.into(),
            delta: delta.into(),
            error,
        },
    );
}

fn process_event(
    app: &AppHandle,
    id: &str,
    event: &str,
    key: &str,
    finished: &mut bool,
) -> AppResult<bool> {
    if event == "[DONE]" {
        *finished = true;
        return Ok(true);
    }
    let value: Value = serde_json::from_str(event).map_err(|_| "流式响应格式无效")?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("AI 服务返回错误");
        return Err(message.replace(key, "[已隐藏]").chars().take(300).collect());
    }
    if let Some(content) = value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
    {
        emit(app, id, "streaming", content, None);
    }
    if value
        .pointer("/choices/0/finish_reason")
        .is_some_and(|value| !value.is_null())
    {
        *finished = true;
    }
    Ok(false)
}

async fn stream(
    app: &AppHandle,
    client: reqwest::Client,
    profile: Profile,
    key: &str,
    messages: Vec<Message>,
    id: &str,
    token: CancellationToken,
) -> AppResult<()> {
    let base = profile.base_url.trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    };
    emit(app, id, "connecting", "", None);
    let response = tokio::select! {
        _ = token.cancelled() => { emit(app,id,"cancelled","",None); return Ok(()); },
        response = client.post(url).bearer_auth(key).json(&json!({"model":profile.model,"messages":messages,"stream":true})).send() => {
            response.map_err(|_| "无法连接 AI 服务，请检查地址、网络和服务状态")?
        }
    };
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "服务拒绝认证，请检查 API Key 和权限".into(),
            429 => "请求限流或额度不足，请稍后重试".into(),
            status => format!("AI 服务返回 HTTP {status}，请检查接口配置或服务状态"),
        });
    }
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut finished = false;
    loop {
        let chunk = tokio::select! {
            _ = token.cancelled() => { emit(app,id,"cancelled","",None); return Ok(()); },
            chunk = tokio::time::timeout(Duration::from_secs(90),stream.next()) => chunk.map_err(|_| "AI 服务长时间没有响应，请重试")?,
        };
        let events = match chunk {
            Some(Ok(bytes)) => decoder.push(&bytes)?,
            Some(Err(_)) => return Err("连接中断，已保留收到的文本".into()),
            None => {
                for event in decoder.finish()? {
                    process_event(app, id, &event, key, &mut finished)?;
                }
                if !finished {
                    return Err("服务提前结束响应，已保留收到的文本".into());
                }
                break;
            }
        };
        for event in events {
            if process_event(app, id, &event, key, &mut finished)? {
                emit(app, id, "completed", "", None);
                return Ok(());
            }
        }
    }
    emit(app, id, "completed", "", None);
    Ok(())
}

#[tauri::command]
pub fn start_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    request_id: String,
    messages: Vec<Message>,
) -> AppResult<()> {
    uuid::Uuid::parse_str(&request_id).map_err(|_| "请求编号无效")?;
    if messages.is_empty()
        || messages
            .iter()
            .any(|message| !["user", "assistant", "system"].contains(&message.role.as_str()))
    {
        return Err("对话内容无效".into());
    }
    let profile = state
        .store
        .lock()
        .map_err(|_| "数据暂不可用")?
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
        .ok_or("服务配置不存在")?;
    let key = get_key(&profile_id)?;
    let token = CancellationToken::new();
    let cancellations = state.cancellations.clone();
    {
        let mut requests = cancellations.lock().map_err(|_| "请求状态暂不可用")?;
        if requests.contains_key(&request_id) {
            return Err("请求已存在".into());
        }
        requests.insert(request_id.clone(), token.clone());
    }
    let client = state.client.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = stream(&app, client, profile, &key, messages, &request_id, token).await
        {
            emit(&app, &request_id, "failed", "", Some(error));
        }
        if let Ok(mut requests) = cancellations.lock() {
            requests.remove(&request_id);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_chat(state: State<'_, AppState>, request_id: String) -> AppResult<()> {
    if let Some(token) = state
        .cancellations
        .lock()
        .map_err(|_| "请求状态暂不可用")?
        .get(&request_id)
    {
        token.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_preserves_split_utf8_and_multiline_events() {
        let source = "data: {\"text\":\"你好\"}\r\n\r\ndata: [DONE]\n\n";
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        for byte in source.as_bytes() {
            events.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(events, vec!["{\"text\":\"你好\"}", "[DONE]"]);
    }
}
