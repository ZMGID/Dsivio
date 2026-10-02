//! TikTok Login Kit OAuth v2 (PKCE) and Content Posting API direct post.
use super::transport::{bearer, form_body, json_number, HttpRequest, HttpResponse, Transport, TransportError};
use super::{Credential, Privacy, PublishError, PublishStatus};
use serde_json::{json, Value};
use std::sync::Arc;

const AUTH: &str = "https://www.tiktok.com/v2/auth/authorize/";
const TOKEN: &str = "https://open.tiktokapis.com/v2/oauth/token/";
const API: &str = "https://open.tiktokapis.com";
const SCOPE: &str = "user.info.basic,video.publish,video.list";

/// Media transfer guide: maximum uploaded video is 4 GB.
pub(crate) const MAX_VIDEO_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub(crate) struct Authorized {
    pub credential: Credential,
    pub remote_id: String,
    pub name: String,
    pub fans: Option<f64>,
}

pub(crate) struct UploadStart {
    pub publish_id: String,
    pub upload_url: String,
}

pub(crate) fn authorize_url(client_key: &str, redirect_uri: &str, state: &str, challenge: &str) -> Result<String, String> {
    let mut url = url::Url::parse(AUTH).map_err(|_| "TikTok 授权地址无效")?;
    url.query_pairs_mut()
        .append_pair("client_key", client_key)
        .append_pair("scope", SCOPE)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

/// Direct Post `privacy_level`: `PUBLIC_TO_EVERYONE`, `MUTUAL_FOLLOW_FRIENDS`, `SELF_ONLY`.
/// `unlisted` is not a TikTok level (`FOLLOWER_OF_CREATOR` is followers, not unlisted).
pub(crate) fn privacy_level(privacy: Privacy) -> Result<&'static str, PublishError> {
    match privacy {
        Privacy::Public => Ok("PUBLIC_TO_EVERYONE"),
        Privacy::Friends => Ok("MUTUAL_FOLLOW_FRIENDS"),
        Privacy::Private => Ok("SELF_ONLY"),
        Privacy::Unlisted => Err(PublishError::unsupported("TikTok 没有 unlisted，可选 public、friends、private")),
    }
}

pub(crate) fn allowed_container(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".mp4") || lower.ends_with(".webm") || lower.ends_with(".mov") || lower.ends_with(".qt")
}

/// Official FILE_UPLOAD rule: chunks are 5–64MB except a file under 5MB (one chunk)
/// and the final chunk, which absorbs the remainder (up to 128MB).
/// `50_000_123` bytes with a `10_000_000` chunk size is 5 chunks, last length `10_000_123`.
pub(crate) fn chunk_ranges(size: u64, chunk: u64) -> Vec<(u64, u64)> {
    if size == 0 || chunk == 0 {
        return Vec::new();
    }
    if size <= chunk {
        return vec![(0, size - 1)];
    }
    let count = size / chunk;
    (0..count)
        .map(|index| {
            let start = index * chunk;
            let end = if index + 1 == count { size - 1 } else { start + chunk - 1 };
            (start, end)
        })
        .collect()
}

pub(crate) fn plan_chunks(size: u64) -> Result<Vec<(u64, u64)>, String> {
    if size == 0 {
        return Err("视频文件是空的".into());
    }
    const MIN: u64 = 5_000_000;
    const TARGET: u64 = 10_000_000;
    const MAX_SINGLE: u64 = 64_000_000;
    let chunk = if size < MIN * 2 {
        size
    } else if size <= MAX_SINGLE {
        MIN
    } else {
        TARGET
    };
    let ranges = chunk_ranges(size, chunk);
    if ranges.is_empty() || ranges.len() > 1000 {
        return Err("视频分片数量无效".into());
    }
    Ok(ranges)
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
            ("client_key", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ],
    )
    .await?;
    let mut credential = credential_from_token(client_id, client_secret, redirect_uri, &token, "").map_err(AuthFail::Failed)?;
    let remote_id = token
        .get("open_id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthFail::Failed("TikTok 没有返回 open_id".into()))?
        .to_string();
    let profile = creator_info(transport, &credential.access_token).await?;
    let _ = refresh_if_needed(transport, &mut credential).await;
    Ok(Authorized {
        credential,
        remote_id,
        name: profile.0,
        fans: profile.1,
    })
}

pub(crate) async fn refresh(transport: &Arc<dyn Transport>, cred: &mut Credential) -> Result<(), RefreshFail> {
    let token = token_request(
        transport,
        &[
            ("client_key", cred.client_id.as_str()),
            ("client_secret", cred.client_secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", cred.refresh_token.as_str()),
        ],
    )
    .await
    .map_err(RefreshFail::from_auth)?;
    apply_token(cred, &token).map_err(RefreshFail::Transient)
}

pub(crate) async fn refresh_if_needed(transport: &Arc<dyn Transport>, cred: &mut Credential) -> Result<(), RefreshFail> {
    if cred.expires_at > chrono::Utc::now().timestamp() + 60 && !cred.access_token.is_empty() {
        return Ok(());
    }
    refresh(transport, cred).await
}

#[derive(Debug)]
pub(crate) enum RefreshFail {
    Reauth(String),
    Transient(String),
}
impl RefreshFail {
    pub(crate) fn from_auth(fail: AuthFail) -> Self {
        match fail {
            AuthFail::Rejected(message) => Self::Reauth(message),
            AuthFail::Transport(message) | AuthFail::Failed(message) => Self::Transient(message),
        }
    }
}

/// OAuth token call. Transport failures are not argument errors.
#[derive(Debug)]
pub(crate) enum AuthFail {
    Transport(String),
    Rejected(String),
    Failed(String),
}

impl AuthFail {
    pub(crate) fn into_publish(self) -> PublishError {
        match self {
            Self::Transport(message) | Self::Failed(message) => PublishError::failed(message),
            Self::Rejected(message) => PublishError::rejected(message),
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Transport(message) | Self::Rejected(message) | Self::Failed(message) => message,
        }
    }

    pub(crate) fn exit_class(&self) -> PublishStatus {
        match self {
            Self::Rejected(_) => PublishStatus::Rejected,
            Self::Transport(_) | Self::Failed(_) => PublishStatus::Failed,
        }
    }
}

pub(crate) async fn creator_info(transport: &Arc<dyn Transport>, access_token: &str) -> Result<(String, Option<f64>), AuthFail> {
    let response = send_json(
        transport,
        "POST",
        format!("{API}/v2/post/publish/creator_info/query/"),
        access_token,
        &json!({}),
    )
    .await
    .map_err(|error| AuthFail::Transport(error.0))?;
    let body = ok_body(&response).map_err(|fail| match fail {
        StepFail::Transport(message) => AuthFail::Transport(message),
        StepFail::Definite(PublishStatus::Rejected, message) => AuthFail::Rejected(message),
        StepFail::Definite(_, message) => AuthFail::Failed(message),
    })?;
    let data = &body["data"];
    let name = data["creator_nickname"]
        .as_str()
        .filter(|value| !value.is_empty())
        .or_else(|| data["creator_username"].as_str())
        .unwrap_or("TikTok")
        .to_string();
    Ok((name, json_number(&data["follower_count"])))
}

pub(crate) async fn init_upload(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    title: &str,
    description: &str,
    tags: &[String],
    privacy: Privacy,
    video_size: u64,
    ranges: &[(u64, u64)],
) -> Result<UploadStart, StepFail> {
    let level = match privacy_level(privacy) {
        Ok(level) => level,
        Err(error) => return Err(StepFail::Definite(PublishStatus::Rejected, error.to_string())),
    };
    let chunk_size = ranges[0].1 - ranges[0].0 + 1;
    let body = json!({
        "post_info": {
            "title": caption(title, description, tags),
            "privacy_level": level,
            "disable_duet": false,
            "disable_comment": false,
            "disable_stitch": false
        },
        "source_info": {
            "source": "FILE_UPLOAD",
            "video_size": video_size,
            "chunk_size": chunk_size,
            "total_chunk_count": ranges.len()
        }
    });
    let response = send_json(transport, "POST", format!("{API}/v2/post/publish/video/init/"), access_token, &body)
        .await
        .map_err(StepFail::transport)?;
    if response.status >= 500 {
        return Err(StepFail::Transport(brief(&response)));
    }
    let value = response.json().map_err(|message| StepFail::Definite(PublishStatus::Failed, message))?;
    if let Some((code, message)) = api_error(&value) {
        return Err(StepFail::Definite(classify_api_code(&code), message));
    }
    if (400..500).contains(&response.status) {
        return Err(StepFail::Definite(PublishStatus::Rejected, brief(&response)));
    }
    let publish_id = value["data"]["publish_id"].as_str().unwrap_or("").to_string();
    let upload_url = value["data"]["upload_url"].as_str().unwrap_or("").to_string();
    if publish_id.is_empty() || upload_url.is_empty() {
        return Err(StepFail::Definite(PublishStatus::Failed, "TikTok 没有返回上传地址".into()));
    }
    Ok(UploadStart { publish_id, upload_url })
}

pub(crate) async fn put_chunk(
    transport: &Arc<dyn Transport>,
    upload_url: &str,
    mime: &str,
    start: u64,
    end: u64,
    total: u64,
    bytes: Vec<u8>,
) -> Result<(), StepFail> {
    let request = HttpRequest {
        method: "PUT".into(),
        url: upload_url.to_string(),
        headers: vec![
            ("Content-Type".into(), mime.into()),
            ("Content-Length".into(), bytes.len().to_string()),
            ("Content-Range".into(), format!("bytes {start}-{end}/{total}")),
        ],
        body: bytes,
    };
    let response = transport.send(request).await.map_err(|error| StepFail::Transport(error.0))?;
    let last = end + 1 == total;
    if last && matches!(response.status, 200 | 201 | 204) {
        return Ok(());
    }
    if !last && response.status == 206 {
        return Ok(());
    }
    if response.status >= 500 {
        return Err(StepFail::Transport(format!("TikTok 上传返回 {}", response.status)));
    }
    if (400..500).contains(&response.status) {
        return Err(StepFail::Definite(PublishStatus::Rejected, format!("TikTok 拒绝了上传（{}）", response.status)));
    }
    Err(StepFail::Definite(PublishStatus::Failed, format!("TikTok 上传返回 {}", response.status)))
}

pub(crate) struct StatusSnapshot {
    pub status: PublishStatus,
    pub public_id: Option<String>,
    pub reason: Option<String>,
}

pub(crate) async fn fetch_status(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    publish_id: &str,
) -> Result<StatusSnapshot, StepFail> {
    let response = send_json(
        transport,
        "POST",
        format!("{API}/v2/post/publish/status/fetch/"),
        access_token,
        &json!({ "publish_id": publish_id }),
    )
    .await
    .map_err(StepFail::transport)?;
    if response.status >= 500 {
        return Err(StepFail::Transport(brief(&response)));
    }
    let value = response.json().map_err(|message| StepFail::Transport(message))?;
    if let Some((code, message)) = api_error(&value) {
        if response.status >= 500 || code == "internal_error" {
            return Err(StepFail::Transport(message));
        }
        return Err(StepFail::Definite(PublishStatus::Uncertain, message));
    }
    let status = value["data"]["status"].as_str().unwrap_or("");
    let fail_reason = value["data"]["fail_reason"].as_str().filter(|value| !value.is_empty());
    let public_id = value["data"]["publicaly_available_post_id"]
        .as_array()
        .and_then(|items| items.iter().find_map(|item| item.as_str()))
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (mapped, reason) = map_status(status, fail_reason);
    Ok(StatusSnapshot { status: mapped, public_id, reason })
}

pub(crate) fn map_status(status: &str, fail_reason: Option<&str>) -> (PublishStatus, Option<String>) {
    match status {
        "PROCESSING_UPLOAD" | "PROCESSING_DOWNLOAD" | "SEND_TO_USER_INBOX" => (PublishStatus::Processing, None),
        "PUBLISH_COMPLETE" => (PublishStatus::Published, None),
        "FAILED" => {
            let reason = fail_reason.unwrap_or("FAILED").to_string();
            const REJECTED: &[&str] = &[
                "spam_risk_too_many_posts",
                "spam_risk_user_banned_from_posting",
                "spam_risk_text",
                "auth_removed",
            ];
            if REJECTED.contains(&reason.as_str()) {
                (PublishStatus::Rejected, Some(reason))
            } else {
                (PublishStatus::Failed, Some(reason))
            }
        }
        other => (PublishStatus::Uncertain, Some(format!("TikTok 状态 {other}"))),
    }
}

pub(crate) async fn video_share_url(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    video_id: &str,
) -> Result<Option<String>, TransportError> {
    let url = format!("{API}/v2/video/query/?fields=id,share_url,view_count,like_count,comment_count,share_count");
    let response = send_json(transport, "POST", url, access_token, &json!({ "filters": { "video_ids": [video_id] } }))
        .await?;
    let value = response.json().map_err(TransportError)?;
    Ok(value["data"]["videos"]
        .as_array()
        .and_then(|videos| videos.first())
        .and_then(|video| video["share_url"].as_str())
        .map(str::to_string))
}

pub(crate) async fn video_stats(
    transport: &Arc<dyn Transport>,
    access_token: &str,
    video_id: &str,
) -> Result<Value, String> {
    let url = format!("{API}/v2/video/query/?fields=id,share_url,view_count,like_count,comment_count,share_count");
    let response = send_json(transport, "POST", url, access_token, &json!({ "filters": { "video_ids": [video_id] } }))
        .await
        .map_err(|error| error.0)?;
    let value = response.json()?;
    if let Some((_, message)) = api_error(&value) {
        return Err(message);
    }
    value["data"]["videos"]
        .as_array()
        .and_then(|videos| videos.iter().find(|video| video["id"].as_str() == Some(video_id)))
        .cloned()
        .ok_or_else(|| "TikTok 没有返回这条视频".to_string())
}

#[derive(Debug)]
pub(crate) enum StepFail {
    Transport(String),
    Definite(PublishStatus, String),
}
impl StepFail {
    fn transport(error: TransportError) -> Self {
        Self::Transport(error.0)
    }
}

/// Content Posting API `error.code`. These are platform decisions, never local argument errors.
/// `internal_error` is a definite failure (exit 4). Documented refusals are rejected (exit 3).
pub(crate) fn classify_api_code(code: &str) -> PublishStatus {
    const REJECTED: &[&str] = &[
        "access_token_invalid",
        "scope_not_authorized",
        "rate_limit_exceeded",
        "invalid_params",
        "spam_risk_too_many_posts",
        "spam_risk_user_banned_from_posting",
        "spam_risk_text",
        "reached_active_user_cap",
        "unaudited_client_can_only_post_to_private_accounts",
        "privacy_level_option_mismatch",
        "url_ownership_unverified",
    ];
    if REJECTED.contains(&code) {
        PublishStatus::Rejected
    } else if code == "internal_error" {
        PublishStatus::Failed
    } else {
        PublishStatus::Rejected
    }
}

/// OAuth token endpoint. 4xx is rejected (exit 3). Transport and 5xx are failed (exit 4).
/// Nothing here is an invalid-argument exit.
pub(crate) fn oauth_failure(status: u16, error: &str, description: &str) -> AuthFail {
    let message = if error.is_empty() {
        format!("令牌请求失败（{status}）")
    } else if description.is_empty() || description == error {
        error.to_string()
    } else {
        format!("{error}: {description}")
    };
    const REAUTH: &[&str] = &[
        "invalid_grant",
        "invalid_token",
        "access_denied",
        "unauthorized_client",
        "invalid_client",
        "insufficient_scope",
    ];
    if status >= 500 || error == "server_error" || error == "temporarily_unavailable" {
        AuthFail::Failed(message)
    } else if REAUTH.contains(&error) || status == 401 || status == 403 || status == 429 || status >= 400 || !error.is_empty() {
        AuthFail::Rejected(if REAUTH.contains(&error) || status == 401 || status == 403 {
            format!("需要重新授权（{message}）")
        } else {
            message
        })
    } else {
        AuthFail::Failed(message)
    }
}

fn caption(title: &str, description: &str, tags: &[String]) -> String {
    let mut text = if description.trim().is_empty() {
        title.trim().to_string()
    } else if title.trim().is_empty() {
        description.trim().to_string()
    } else {
        format!("{}\n{}", title.trim(), description.trim())
    };
    for tag in tags {
        let clean = tag.trim().trim_start_matches('#');
        if clean.is_empty() {
            continue;
        }
        let hash = format!("#{clean}");
        if !text.contains(&hash) {
            text.push(' ');
            text.push_str(&hash);
        }
    }
    text.chars().take(2200).collect()
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
        return Err(oauth_failure(response.status, error, description));
    }
    if response.status >= 400 {
        return Err(oauth_failure(response.status, "", ""));
    }
    if value.get("access_token").and_then(|item| item.as_str()).unwrap_or("").is_empty() {
        return Err(AuthFail::Failed("TikTok 没有返回 access_token".into()));
    }
    Ok(value)
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
        .ok_or("TikTok 没有返回 access_token")?
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

async fn send_json(
    transport: &Arc<dyn Transport>,
    method: &str,
    url: String,
    access_token: &str,
    body: &Value,
) -> Result<HttpResponse, TransportError> {
    transport
        .send(HttpRequest {
            method: method.into(),
            url,
            headers: vec![
                bearer(access_token),
                ("Content-Type".into(), "application/json; charset=UTF-8".into()),
            ],
            body: serde_json::to_vec(body).unwrap_or_default(),
        })
        .await
}

fn api_error(value: &Value) -> Option<(String, String)> {
    let code = value["error"]["code"].as_str().unwrap_or("");
    if code.is_empty() || code == "ok" {
        return None;
    }
    let message = value["error"]["message"].as_str().filter(|text| !text.is_empty()).unwrap_or(code);
    Some((code.to_string(), message.to_string()))
}

fn ok_body(response: &HttpResponse) -> Result<Value, StepFail> {
    if response.status >= 500 {
        return Err(StepFail::Transport(brief(response)));
    }
    let value = response.json().map_err(StepFail::Transport)?;
    if let Some((code, message)) = api_error(&value) {
        if code == "internal_error" {
            return Err(StepFail::Transport(message));
        }
        return Err(StepFail::Definite(classify_api_code(&code), message));
    }
    if (400..500).contains(&response.status) {
        return Err(StepFail::Definite(PublishStatus::Rejected, brief(response)));
    }
    Ok(value)
}

fn brief(response: &HttpResponse) -> String {
    let text = String::from_utf8_lossy(&response.body);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        format!("TikTok 请求失败（{}）", response.status)
    } else {
        trimmed.chars().take(300).collect()
    }
}

pub(crate) fn read_range(path: &str, start: u64, end: u64) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|_| "视频文件不存在".to_string())?;
    file.seek(SeekFrom::Start(start)).map_err(|error| error.to_string())?;
    let len = usize::try_from(end - start + 1).map_err(|_| "分片过大".to_string())?;
    let mut buffer = vec![0u8; len];
    file.read_exact(&mut buffer).map_err(|error| format!("读取视频失败：{error}"))?;
    Ok(buffer)
}

pub(crate) fn video_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".mov") || lower.ends_with(".qt") {
        "video/quicktime"
    } else if lower.ends_with(".webm") {
        "video/webm"
    } else {
        "video/mp4"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_chunk_example_splits_fifty_megabytes_into_five_ranges() {
        let ranges = chunk_ranges(50_000_123, 10_000_000);
        assert_eq!(ranges.len(), 5);
        assert_eq!(ranges[0], (0, 9_999_999));
        assert_eq!(ranges[4].0, 40_000_000);
        assert_eq!(ranges[4].1 - ranges[4].0 + 1, 10_000_123);
    }

    #[test]
    fn status_mapping_matches_documented_values() {
        assert_eq!(map_status("PROCESSING_DOWNLOAD", None).0, PublishStatus::Processing);
        assert_eq!(map_status("PUBLISH_COMPLETE", None).0, PublishStatus::Published);
        assert_eq!(
            map_status("FAILED", Some("picture_size_check_failed")).0,
            PublishStatus::Failed
        );
        assert_eq!(map_status("FAILED", Some("spam_risk_text")).0, PublishStatus::Rejected);
        assert_eq!(map_status("FAILED", Some("auth_removed")).0, PublishStatus::Rejected);
        assert_eq!(map_status("FAILED", Some("file_format_check_failed")).0, PublishStatus::Failed);
        assert_eq!(classify_api_code("reached_active_user_cap"), PublishStatus::Rejected);
        assert_eq!(classify_api_code("invalid_params"), PublishStatus::Rejected);
        assert_eq!(classify_api_code("url_ownership_unverified"), PublishStatus::Rejected);
        assert_eq!(classify_api_code("privacy_level_option_mismatch"), PublishStatus::Rejected);
        assert_eq!(classify_api_code("internal_error"), PublishStatus::Failed);
        assert_eq!(oauth_failure(400, "invalid_grant", "expired").exit_class(), PublishStatus::Rejected);
        assert_eq!(oauth_failure(503, "server_error", "down").exit_class(), PublishStatus::Failed);
        assert_eq!(oauth_failure(401, "", "").exit_class(), PublishStatus::Rejected);
    }
}
