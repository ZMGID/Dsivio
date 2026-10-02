//! Content-platform publish: one implementation for TikTok and YouTube.
mod flow;
mod store;
mod tiktok;
mod transport;
mod types;
mod youtube;
mod cli;
pub mod service;

pub use types::{
    AccountStatus, ContentPlatform, Privacy, PublishAccount, PublishAppConfig, PublishBeginResult, PublishBlock,
    PublishError, PublishRecord, PublishRecordFilter, PublishRequest, PublishStatus, PublishSubmitResult, StatKey,
    VideoStats,
};
pub(crate) use types::Credential;

use base64::Engine;
use flow::{KeyringVault, Service, Vault};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use store::database_path;
use transport::LiveTransport;
use std::sync::Arc;

pub const SUBCOMMAND: &str = "publish";
pub use cli::run;

struct Pending {
    config: PublishAppConfig,
    state: String,
    verifier: String,
    redirect_uri: String,
    created_at: i64,
    callback_rx: Option<std::sync::mpsc::Receiver<String>>,
    callback: Option<String>,
}

static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn live() -> Result<Service<KeyringVault>, PublishError> {
    Ok(Service {
        db_path: database_path()?,
        transport: Arc::new(LiveTransport::new().map_err(PublishError::internal)?),
        vault: KeyringVault,
    })
}

pub fn publish_begin(mut config: PublishAppConfig) -> Result<PublishBeginResult, PublishError> {
    config.client_id = config.client_id.trim().to_string();
    config.client_secret = config.client_secret.trim().to_string();
    config.redirect_uri = config.redirect_uri.trim().to_string();
    if config.client_id.is_empty() || config.client_secret.is_empty() || config.client_id.len() > 256 || config.client_secret.len() > 2048 {
        return Err(PublishError::invalid("请填写有效的平台应用 ID 和密钥"));
    }
    let (verifier, challenge) = pkce_pair();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let (redirect_uri, mode, callback_rx) = if config.platform == ContentPlatform::Youtube && config.redirect_uri.is_empty() {
        let (redirect, rx) = youtube::capture_loopback().map_err(PublishError::internal)?;
        (redirect, "loopback".to_string(), Some(rx))
    } else if config.redirect_uri.is_empty() {
        return Err(PublishError::invalid("请填写回调地址"));
    } else {
        (config.redirect_uri.clone(), "redirect".to_string(), None)
    };
    let url = match config.platform {
        ContentPlatform::Tiktok => tiktok::authorize_url(&config.client_id, &redirect_uri, &state, &challenge).map_err(PublishError::internal)?,
        ContentPlatform::Youtube => youtube::authorize_url(&config.client_id, &redirect_uri, &state, &challenge).map_err(PublishError::internal)?,
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut entries = pending().lock().map_err(|_| PublishError::internal("授权状态不可用"))?;
    let now = chrono::Utc::now().timestamp();
    entries.retain(|_, item| now - item.created_at < 3600);
    if entries.len() >= 16 {
        return Err(PublishError::invalid("待完成授权过多，请稍后重试"));
    }
    entries.insert(
        request_id.clone(),
        Pending { config, state, verifier, redirect_uri, created_at: now, callback_rx, callback: None },
    );
    Ok(PublishBeginResult { request_id, url, mode })
}

pub async fn publish_complete(request_id: String, callback_url: String) -> Result<PublishAccount, PublishError> {
    if callback_url.len() > 8192 {
        return Err(PublishError::invalid("回调地址过长"));
    }
    let mut pending_auth = pending()
        .lock()
        .map_err(|_| PublishError::internal("授权状态不可用"))?
        .remove(&request_id)
        .ok_or_else(|| PublishError::invalid("本次授权已过期，请重新发起绑定"))?;
    if chrono::Utc::now().timestamp() - pending_auth.created_at > 3600 {
        return Err(PublishError::invalid("本次授权已过期，请重新发起绑定"));
    }
    let callback = if !callback_url.trim().is_empty() {
        callback_url
    } else if let Some(saved) = pending_auth.callback.clone() {
        saved
    } else if let Some(rx) = pending_auth.callback_rx.take() {
        let captured = tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(Duration::from_secs(180)).map_err(|_| PublishError::timeout("等待本机授权回调超时"))
        })
        .await
        .map_err(|_| PublishError::internal("等待授权回调失败"))??;
        pending_auth.callback = Some(captured.clone());
        captured
    } else {
        return Err(PublishError::invalid("请粘贴授权回调地址"));
    };
    let code = match callback_code(&callback, &pending_auth.state) {
        Ok(code) => code,
        Err(error) => {
            pending().lock().map_err(|_| PublishError::internal("授权状态不可用"))?.insert(request_id, pending_auth);
            return Err(error);
        }
    };
    let service = match live() {
        Ok(service) => service,
        Err(error) => {
            pending().lock().map_err(|_| PublishError::internal("授权状态不可用"))?.insert(request_id, pending_auth);
            return Err(error);
        }
    };
    let exchanged = match pending_auth.config.platform {
        ContentPlatform::Tiktok => tiktok::exchange(
            &service.transport,
            &pending_auth.config.client_id,
            &pending_auth.config.client_secret,
            &pending_auth.redirect_uri,
            &code,
            &pending_auth.verifier,
        )
        .await
        .map(|item| (item.credential, item.remote_id, item.name, item.fans))
        .map_err(tiktok::AuthFail::into_publish),
        ContentPlatform::Youtube => youtube::exchange(
            &service.transport,
            &pending_auth.config.client_id,
            &pending_auth.config.client_secret,
            &pending_auth.redirect_uri,
            &code,
            &pending_auth.verifier,
        )
        .await
        .map(|item| (item.credential, item.remote_id, item.name, item.fans))
        .map_err(tiktok::AuthFail::into_publish),
    };
    let (credential, remote_id, name, fans) = match exchanged {
        Ok(item) => item,
        Err(error) => {
            pending().lock().map_err(|_| PublishError::internal("授权状态不可用"))?.insert(request_id, pending_auth);
            return Err(error);
        }
    };
    let existing = service.existing_account(pending_auth.config.platform, &remote_id)?;
    let previous = existing.as_ref().and_then(|id| service.get_account(id).ok());
    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let previous_credential = if previous.is_some() { service.vault.load(&id).ok() } else { None };
    service.vault.save(&id, &credential)?;
    let now = chrono::Utc::now().to_rfc3339();
    let account = PublishAccount {
        id: id.clone(),
        platform: pending_auth.config.platform,
        remote_id,
        name,
        bound_at: previous.as_ref().map(|item| item.bound_at.clone()).unwrap_or_else(|| now.clone()),
        checked_at: now,
        status: AccountStatus::Connected,
        detail: None,
        fans,
    };
    if let Err(error) = service.put_account(&account) {
        if let Some(credential) = previous_credential {
            let _ = service.vault.save(&id, &credential);
        } else {
            let _ = service.vault.delete(&id);
        }
        pending().lock().map_err(|_| PublishError::internal("授权状态不可用"))?.insert(request_id, pending_auth);
        return Err(error);
    }
    Ok(account)
}

pub async fn publish_list_accounts() -> Result<Vec<PublishAccount>, PublishError> {
    live()?.list_accounts()
}

pub async fn publish_refresh_account(id: String) -> Result<PublishAccount, PublishError> {
    live()?.refresh_account(&id).await
}

pub async fn publish_unbind(id: String) -> Result<(), PublishError> {
    let service = live()?;
    service.get_account(&id)?;
    let credential = service.vault.load(&id).ok();
    service.vault.delete(&id)?;
    if let Err(error) = service.delete_account(&id) {
        if let Some(credential) = credential {
            let _ = service.vault.save(&id, &credential);
        }
        return Err(error);
    }
    Ok(())
}

pub async fn publish_submit(request: PublishRequest) -> Result<PublishSubmitResult, PublishError> {
    live()?.submit(request, "workbench/publish").await
}

pub async fn publish_retry(id: String) -> Result<PublishRecord, PublishError> {
    live()?.retry(&id).await
}

pub async fn publish_list_records(filter: Option<PublishRecordFilter>) -> Result<Vec<PublishRecord>, PublishError> {
    live()?.list_records(&filter.unwrap_or_default())
}

pub async fn publish_refresh_record(id: String) -> Result<PublishRecord, PublishError> {
    live()?.refresh_record(&id).await
}

pub async fn publish_stats(id: String) -> Result<VideoStats, PublishError> {
    live()?.stats(&id).await
}

pub fn tool_definition() -> crate::mcp::ChatToolDefinition {
    crate::mcp::ChatToolDefinition {
        id: "native__publish".into(),
        name: "publish".into(),
        description: "Publish a local video to a bound TikTok or YouTube account, or read accounts, records, and stats. action=submit and action=retry upload a video and require approval. A published or uncertain record is never uploaded again; use action=status to query it. Missing stat fields are unsupported, not zero.".into(),
        source: "native".into(),
        server_id: None,
        server_name: Some("Kivio".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["accounts", "submit", "records", "status", "stats", "retry"] },
                "accountIds": { "type": "array", "items": { "type": "string" } },
                "videoPath": { "type": "string", "description": "Absolute path to the local video." },
                "title": { "type": "string" },
                "description": { "type": "string" },
                "privacy": { "type": "string", "enum": ["public", "private", "unlisted", "friends"] },
                "tags": { "type": "array", "items": { "type": "string" } },
                "id": { "type": "string", "description": "Publish record id for status, stats, or retry." },
                "filter": { "type": "object" }
            },
            "required": ["action"],
            "additionalProperties": false
        }),
        sensitive: true,
        annotations: None,
        output_schema: None,
    }
}

pub async fn tool_call(app: &tauri::AppHandle, arguments: serde_json::Value) -> Result<crate::mcp::types::McpToolCallResult, String> {
    let value = dispatch_op(app, arguments, "chat").await.map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into());
    Ok(crate::mcp::types::McpToolCallResult {
        content,
        is_error: false,
        raw: value,
        artifacts: Vec::new(),
        structured_content: None,
        follow_up_user_messages: Vec::new(),
    })
}

/// CLI / loopback entry. `Err` is [`PublishError`]; call `error.exit_code()`.
/// There is no `exit_code(&str)` shim.
///
/// Platform API results are never exit 2:
/// - `Err(Rejected)` 3 and `Err(Failed)` 4 are only token or profile calls that produced no record
/// - a publish that reached the platform is `Ok`; `rejected` / `failed` / `uncertain` stay on `status`
pub async fn handle(app: &tauri::AppHandle, op: serde_json::Value) -> Result<serde_json::Value, PublishError> {
    dispatch_op(app, op, "cli/publish").await
}

async fn dispatch_op(_app: &tauri::AppHandle, op: serde_json::Value, origin: &str) -> Result<serde_json::Value, PublishError> {
    let action = op.get("action").and_then(|value| value.as_str()).unwrap_or("");
    let service = live()?;
    let result = match action {
        "accounts" => serde_json::to_value(service.list_accounts()?).map_err(|error| PublishError::internal(error.to_string()))?,
        "begin" => {
            let config: PublishAppConfig = serde_json::from_value(op.get("config").cloned().unwrap_or(serde_json::Value::Null))
                .map_err(|error| PublishError::invalid(format!("授权参数无效：{error}")))?;
            serde_json::to_value(publish_begin(config)?).map_err(|error| PublishError::internal(error.to_string()))?
        }
        "complete" => {
            let request_id = op["requestId"].as_str().unwrap_or("").to_string();
            let callback = op["callbackUrl"].as_str().unwrap_or("").to_string();
            serde_json::to_value(publish_complete(request_id, callback).await?).map_err(|error| PublishError::internal(error.to_string()))?
        }
        "unbind" => {
            publish_unbind(op["accountId"].as_str().unwrap_or("").to_string()).await?;
            serde_json::json!({ "ok": true })
        }
        "refreshAccount" => {
            serde_json::to_value(service.refresh_account(op["accountId"].as_str().unwrap_or("")).await?)
                .map_err(|error| PublishError::internal(error.to_string()))?
        }
        "submit" => {
            let raw = if op.get("request").is_some() {
                op.get("request").cloned().unwrap_or(serde_json::Value::Null)
            } else {
                let mut body = op.clone();
                if let Some(map) = body.as_object_mut() {
                    map.remove("action");
                    map.remove("id");
                    map.remove("filter");
                }
                body
            };
            let request: PublishRequest = serde_json::from_value(raw).map_err(|error| PublishError::invalid(format!("发布参数无效：{error}")))?;
            serde_json::to_value(service.submit(request, origin).await?).map_err(|error| PublishError::internal(error.to_string()))?
        }
        "retry" => serde_json::to_value(service.retry(op["id"].as_str().unwrap_or("")).await?).map_err(|error| PublishError::internal(error.to_string()))?,
        "records" => {
            let filter = serde_json::from_value(op.get("filter").cloned().unwrap_or(serde_json::json!({}))).unwrap_or_default();
            serde_json::to_value(service.list_records(&filter)?).map_err(|error| PublishError::internal(error.to_string()))?
        }
        "refresh" | "status" => {
            serde_json::to_value(service.refresh_record(op["id"].as_str().unwrap_or("")).await?).map_err(|error| PublishError::internal(error.to_string()))?
        }
        "stats" => serde_json::to_value(service.stats(op["id"].as_str().unwrap_or("")).await?).map_err(|error| PublishError::internal(error.to_string()))?,
        "" => return Err(PublishError::invalid("缺少 action")),
        other => return Err(PublishError::invalid(format!("未知发布操作 {other}"))),
    };
    Ok(result)
}

fn callback_code(raw: &str, expect_state: &str) -> Result<String, PublishError> {
    let url = url::Url::parse(raw).map_err(|_| PublishError::invalid("回调地址无效"))?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Err(PublishError::rejected(format!("授权被拒绝：{error}")));
    }
    if state.as_deref() != Some(expect_state) {
        return Err(PublishError::invalid("授权 state 不匹配"));
    }
    code.filter(|value| !value.is_empty()).ok_or_else(|| PublishError::invalid("回调中没有授权码"))
}

#[cfg(test)]
mod tests;
