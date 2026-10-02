//! YouTube OAuth 2.0 installed-app loopback with PKCE, and resumable videos.insert.
use super::tiktok::{AuthFail, StepFail};
use super::transport::{bearer, form_body, json_number, HttpRequest, HttpResponse, Transport, TransportError};
use super::{Credential, Privacy, PublishError, PublishStatus};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";

/// `videos.insert`: maximum file size is 256 GB. `privacyStatus` is public, private, or unlisted.
pub(crate) const MAX_VIDEO_BYTES: u64 = 256 * 1024 * 1024 * 1024;

pub(crate) struct Authorized {
    pub credential: Credential,
    pub remote_id: String,
    pub name: String,
    pub fans: Option<f64>,
}

pub(crate) fn authorize_url(client_id: &str, redirect_uri: &str, state: &str, challenge: &str) -> Result<String, String> {
    let mut url = url::Url::parse(AUTH).map_err(|_| "YouTube 授权地址无效")?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(url.to_string())
}

pub(crate) fn privacy_status(privacy: Privacy) -> Result<&'static str, PublishError> {
    match privacy {
        Privacy::Public => Ok("public"),
        Privacy::Unlisted => Ok("unlisted"),
        Privacy::Private => Ok("private"),
        Privacy::Friends => Err(PublishError::unsupported("YouTube 没有 friends，可选 public、private、unlisted")),
    }
}

/// Bind `127.0.0.1:0` and capture one OAuth redirect. Returns the redirect URI and the callback URL.
pub(crate) fn capture_loopback() -> Result<(String, std::sync::mpsc::Receiver<String>), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| format!("无法打开本机回调端口：{error}"))?;
    let port = listener.local_addr().map_err(|error| format!("无法读取回调端口：{error}"))?.port();
    let redirect = format!("http://127.0.0.1:{port}/");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("youtube-oauth-loopback".into())
        .spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buffer = [0u8; 8192];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let text = String::from_utf8_lossy(&buffer[..n]);
            let path = text.split_whitespace().nth(1).unwrap_or("/");
            let _ = tx.send(format!("http://127.0.0.1:{port}{path}"));
            let body = "Authorization received. You can close this window and return to Dsivio.";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        })
        .map_err(|error| format!("无法等待授权回调：{error}"))?;
    Ok((redirect, rx))
}

pub(crate) async fn exchange(
    transport: &Arc<dyn Transport>,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> Result<Authorized, AuthFail> {
    let token = token_request(
        transport,
        &[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ],
    )
    .await?;
    let credential = credential_from_token(client_id, client_secret, redirect_uri, &token, "").map_err(AuthFail::Failed)?;
    let (remote_id, name, fans) = channel(transport, &credential.access_token).await?;
    Ok(Authorized { credential, remote_id, name, fans })
}

pub(crate) async fn refresh(transport: &Arc<dyn Transport>, cred: &mut Credential) -> Result<(), super::tiktok::RefreshFail> {
    let token = token_request(
        transport,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", cred.refresh_token.as_str()),
            ("client_id", cred.client_id.as_str()),
            ("client_secret", cred.client_secret.as_str()),
        ],
    )
    .await
    .map_err(super::tiktok::RefreshFail::from_auth)?;
    apply_token(cred, &token).map_err(super::tiktok::RefreshFail::Transient)
}

pub(crate) async fn channel(transport: &Arc<dyn Transport>, access_token: &str) -> Result<(String, String, Option<f64>), AuthFail> {
    let response = send(
        transport,
        "GET",
        "https://www.googleapis.com/youtube/v3/channels?part=snippet,statistics&mine=true".into(),
        access_token,
        Vec::new(),
        Vec::new(),
    )
    .await
    .map_err(|error| AuthFail::Transport(error.0))?;
    let value = response.json().unwrap_or(Value::Null);
    if response.status >= 500 {
        return Err(AuthFail::Failed(youtube_message(&value).unwrap_or_else(|| format!("YouTube 频道查询失败（{}）", response.status))));
    }
    if response.status >= 400 || value.get("error").is_some() {
        let message = youtube_message(&value).unwrap_or_else(|| format!("YouTube 频道查询失败（{}）", response.status));
        return Err(if matches!(classify_platform(response.status, &value), PublishStatus::Rejected) {
            AuthFail::Rejected(message)
        } else {
            AuthFail::Failed(message)
        });
    }
    let item = value["items"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| AuthFail::Failed("YouTube 没有返回频道".into()))?;
    let remote_id = item["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthFail::Failed("YouTube 没有返回频道 id".into()))?
        .to_string();
    let name = item["snippet"]["title"].as_str().filter(|value| !value.is_empty()).unwrap_or("YouTube").to_string();
    let hidden = item["statistics"]["hiddenSubscriberCount"].as_bool().unwrap_or(false);
    let fans = if hidden { None } else { json_number(&item["statistics"]["subscriberCount"]) };
    Ok((remote_id, name, fans))
}

pub(crate) async fn start_session(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    title: &str,
    description: &str,
    tags: &[String],
    privacy: Privacy,
    mime: &str,
    video_size: u64,
) -> Result<String, StepFail> {
    let privacy_status = match privacy_status(privacy) {
        Ok(status) => status,
        Err(error) => return Err(StepFail::Definite(PublishStatus::Rejected, error.to_string())),
    };
    let mut snippet = json!({
        "title": title,
        "description": description,
        "categoryId": "22"
    });
    // videos.insert has no Shorts field. Caller tags are sent as-is; #Shorts is not required.
    if !tags.is_empty() {
        snippet["tags"] = json!(tags.iter().map(|tag| tag.trim().trim_start_matches('#').to_string()).filter(|tag| !tag.is_empty()).collect::<Vec<_>>());
    }
    let body = json!({
        "snippet": snippet,
        "status": { "privacyStatus": privacy_status, "selfDeclaredMadeForKids": false }
    });
    let response = send(
        transport,
        "POST",
        "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status".into(),
        access_token,
        vec![
            ("Content-Type".into(), "application/json; charset=UTF-8".into()),
            ("X-Upload-Content-Type".into(), mime.into()),
            ("X-Upload-Content-Length".into(), video_size.to_string()),
        ],
        serde_json::to_vec(&body).unwrap_or_default(),
    )
    .await
    .map_err(|error| StepFail::Transport(error.0))?;
    if let Some(location) = response.header("Location").map(str::to_string).filter(|value| !value.is_empty()) {
        if (200..300).contains(&response.status) || response.status == 308 {
            return Ok(location);
        }
    }
    let value = response.json().unwrap_or(Value::Null);
    if response.status >= 500 {
        return Err(StepFail::Transport(youtube_message(&value).unwrap_or_else(|| format!("YouTube 上传会话失败（{}）", response.status))));
    }
    let message = youtube_message(&value).unwrap_or_else(|| format!("YouTube 没有返回上传会话（{}）", response.status));
    Err(StepFail::Definite(classify_platform(response.status, &value), message))
}

pub(crate) async fn upload_bytes(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    session_url: &str,
    mime: &str,
    start: u64,
    end: u64,
    total: u64,
    bytes: Vec<u8>,
) -> Result<Option<Value>, StepFail> {
    let response = send(
        transport,
        "PUT",
        session_url.into(),
        access_token,
        vec![
            ("Content-Type".into(), mime.into()),
            ("Content-Length".into(), bytes.len().to_string()),
            ("Content-Range".into(), format!("bytes {start}-{end}/{total}")),
        ],
        bytes,
    )
    .await
    .map_err(|error| StepFail::Transport(error.0))?;
    if response.status == 308 {
        return Ok(None);
    }
    if matches!(response.status, 200 | 201) {
        return Ok(Some(response.json().unwrap_or(Value::Null)));
    }
    if response.status >= 500 {
        return Err(StepFail::Transport(format!("YouTube 上传返回 {}", response.status)));
    }
    let value = response.json().unwrap_or(Value::Null);
    let message = youtube_message(&value).unwrap_or_else(|| format!("YouTube 上传返回 {}", response.status));
    Err(StepFail::Definite(classify_platform(response.status, &value), message))
}

/// Query a resumable session without sending media bytes (`Content-Range: bytes */size`).
pub(crate) async fn probe_session(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    session_url: &str,
    total: u64,
) -> Result<Option<Value>, StepFail> {
    let response = send(
        transport,
        "PUT",
        session_url.into(),
        access_token,
        vec![
            ("Content-Length".into(), "0".into()),
            ("Content-Range".into(), format!("bytes */{total}")),
        ],
        Vec::new(),
    )
    .await
    .map_err(|error| StepFail::Transport(error.0))?;
    if response.status == 308 {
        return Ok(None);
    }
    if matches!(response.status, 200 | 201) {
        return Ok(Some(response.json().unwrap_or(Value::Null)));
    }
    if response.status >= 500 {
        return Err(StepFail::Transport(format!("YouTube 上传查询返回 {}", response.status)));
    }
    Err(StepFail::Definite(PublishStatus::Uncertain, format!("YouTube 上传查询返回 {}", response.status)))
}

pub(crate) async fn video_list(transport: &Arc<dyn Transport>, access_token: &str, video_id: &str) -> Result<Value, StepFail> {
    let url = format!(
        "https://www.googleapis.com/youtube/v3/videos?part=status,statistics,processingDetails&id={}",
        urlencoding(video_id)
    );
    let response = send(transport, "GET", url, access_token, Vec::new(), Vec::new())
        .await
        .map_err(|error| StepFail::Transport(error.0))?;
    if response.status >= 500 {
        return Err(StepFail::Transport(format!("YouTube 状态查询返回 {}", response.status)));
    }
    let value = response.json().map_err(|message| StepFail::Definite(PublishStatus::Uncertain, message))?;
    if response.status >= 400 || value.get("error").is_some() {
        let message = youtube_message(&value).unwrap_or_else(|| format!("YouTube 状态查询失败（{}）", response.status));
        return Err(StepFail::Definite(PublishStatus::Uncertain, message));
    }
    Ok(value)
}

pub(crate) fn map_video(item: &Value) -> (PublishStatus, Option<String>) {
    let status = &item["status"];
    if let Some(reason) = nonempty(status.get("rejectionReason")) {
        return (PublishStatus::Rejected, Some(reason));
    }
    if let Some(reason) = nonempty(status.get("failureReason")) {
        return (PublishStatus::Failed, Some(reason));
    }
    let upload = status["uploadStatus"].as_str().unwrap_or("");
    let processing = item["processingDetails"]["processingStatus"].as_str().unwrap_or("");
    match upload {
        "rejected" => (PublishStatus::Rejected, Some("rejected".into())),
        "deleted" => (PublishStatus::Rejected, Some("deleted".into())),
        "failed" => (PublishStatus::Failed, Some("failed".into())),
        "processed" => (PublishStatus::Published, None),
        "uploaded" => match processing {
            "succeeded" => (PublishStatus::Published, None),
            "failed" => (PublishStatus::Failed, Some("processing failed".into())),
            "terminated" => (PublishStatus::Failed, Some("terminated".into())),
            _ => (PublishStatus::Processing, None),
        },
        "" => (PublishStatus::Uncertain, Some("YouTube 没有返回 uploadStatus".into())),
        other => (PublishStatus::Uncertain, Some(format!("YouTube 状态 {other}"))),
    }
}

pub(crate) fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn nonempty(value: Option<&Value>) -> Option<String> {
    value.and_then(|item| item.as_str()).filter(|text| !text.is_empty()).map(str::to_string)
}

async fn token_request(transport: &Arc<dyn Transport>, pairs: &[(&str, &str)]) -> Result<Value, AuthFail> {
    let request = HttpRequest {
        method: "POST".into(),
        url: TOKEN.into(),
        headers: vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())],
        body: form_body(pairs),
    };
    let response = transport.send(request).await.map_err(|error| AuthFail::Transport(error.0))?;
    let value = response.json().unwrap_or(Value::Null);
    if let Some(error) = value.get("error").and_then(|item| item.as_str()) {
        let description = value.get("error_description").and_then(|item| item.as_str()).unwrap_or("");
        return Err(super::tiktok::oauth_failure(response.status, error, description));
    }
    if response.status >= 400 {
        return Err(super::tiktok::oauth_failure(response.status, "", ""));
    }
    if value.get("access_token").and_then(|item| item.as_str()).unwrap_or("").is_empty() {
        return Err(AuthFail::Failed("YouTube 没有返回 access_token".into()));
    }
    Ok(value)
}

/// `videos.insert` / Data API errors. Quota, auth, and invalid metadata are rejected (exit 3).
/// Other definite platform failures are failed (exit 4). Callers turn 5xx into transport.
pub(crate) fn classify_platform(status: u16, value: &Value) -> PublishStatus {
    let reason = value["error"]["errors"][0]["reason"].as_str().unwrap_or("");
    const REJECTED: &[&str] = &[
        "authError",
        "unauthorized",
        "forbidden",
        "insufficientPermissions",
        "quotaExceeded",
        "dailyLimitExceeded",
        "rateLimitExceeded",
        "userRateLimitExceeded",
        "uploadLimitExceeded",
        "youtubeSignupRequired",
        "authenticatedUserAccountSuspended",
        "accountClosed",
        "accountDisabled",
        "channelClosed",
        "channelSuspended",
        "invalidTitle",
        "invalidDescription",
        "invalidTags",
        "invalidCategoryId",
        "invalidFilename",
        "invalidVideoMetadata",
    ];
    if status == 401 || status == 403 || status == 429 || REJECTED.contains(&reason) || reason.starts_with("invalid") || youtube_reauth(value) {
        PublishStatus::Rejected
    } else {
        PublishStatus::Failed
    }
}

fn credential_from_token(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    token: &Value,
    previous_refresh: &str,
) -> Result<Credential, String> {
    let mut credential = Credential {
        client_id: client_id.into(),
        client_secret: client_secret.into(),
        redirect_uri: redirect_uri.into(),
        access_token: String::new(),
        refresh_token: previous_refresh.into(),
        expires_at: 0,
        scope: String::new(),
    };
    apply_token(&mut credential, token)?;
    Ok(credential)
}

fn apply_token(cred: &mut Credential, token: &Value) -> Result<(), String> {
    cred.access_token = token
        .get("access_token")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or("YouTube 没有返回 access_token")?
        .to_string();
    if let Some(refresh) = token.get("refresh_token").and_then(|value| value.as_str()).filter(|value| !value.is_empty()) {
        cred.refresh_token = refresh.to_string();
    }
    let expires_in = token.get("expires_in").and_then(json_number).unwrap_or(0.0) as i64;
    cred.expires_at = chrono::Utc::now().timestamp().saturating_add(expires_in);
    if let Some(scope) = token.get("scope").and_then(|value| value.as_str()) {
        cred.scope = scope.to_string();
    }
    Ok(())
}

async fn send(
    transport: &Arc<dyn Transport>,
    method: &str,
    url: String,
    access_token: &str,
    mut headers: Vec<(String, String)>,
    body: Vec<u8>,
) -> Result<HttpResponse, TransportError> {
    headers.insert(0, bearer(access_token));
    transport.send(HttpRequest { method: method.into(), url, headers, body }).await
}

fn youtube_message(value: &Value) -> Option<String> {
    value["error"]["message"].as_str().filter(|text| !text.is_empty()).map(str::to_string)
}

fn youtube_reauth(value: &Value) -> bool {
    let reason = value["error"]["errors"][0]["reason"].as_str().unwrap_or("");
    reason == "authError" || reason == "unauthorized" || value["error"]["code"].as_i64() == Some(401)
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
