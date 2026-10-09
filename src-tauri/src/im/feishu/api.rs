//! Tenant token cache and IM OpenAPI calls. Responses are reduced to code/msg; bodies are not logged.

use super::inbound::{BotIdentity, Download, ResourceKind};
use crate::im::common::{http_client, MAX_MEDIA_BYTES};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const UA: &str = "Dsivio/1.1 IM channel";

pub struct Api {
    client: reqwest::Client,
    base: String,
    app_id: String,
    secret: String,
    token: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    value: String,
    refresh_at: Instant,
}

impl Api {
    pub fn new(base: &str, app_id: &str, secret: &str) -> Result<Self, String> {
        Ok(Self {
            client: http_client()?,
            base: base.trim_end_matches('/').to_owned(),
            app_id: app_id.to_owned(),
            secret: secret.to_owned(),
            token: Mutex::new(None),
        })
    }

    pub async fn tenant_token(&self) -> Result<String, String> {
        if let Some(token) = self.token.lock().as_ref() {
            if Instant::now() < token.refresh_at {
                return Ok(token.value.clone());
            }
        }
        let response = self
            .client
            .post(format!(
                "{}/open-apis/auth/v3/tenant_access_token/internal",
                self.base
            ))
            .json(&json!({"app_id": self.app_id, "app_secret": self.secret}))
            .send()
            .await
            .map_err(|_| {
                "\u{98de}\u{4e66}\u{51ed}\u{8bc1}\u{8bf7}\u{6c42}\u{5931}\u{8d25}".to_string()
            })?;
        let value = response.json::<Value>().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{51ed}\u{8bc1}\u{54cd}\u{5e94}\u{65e0}\u{6548}".to_string()
        })?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        let token = value
            .get("tenant_access_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_owned();
        if code != 0 || token.is_empty() {
            return Err(format!("fatal: 飞书凭证被拒绝（{code}）"));
        }
        let expire = value.get("expire").and_then(|v| v.as_u64()).unwrap_or(7200);
        let early = expire.min(600).max(30);
        let refresh_at = Instant::now() + Duration::from_secs(expire.saturating_sub(early));
        *self.token.lock() = Some(CachedToken {
            value: token.clone(),
            refresh_at,
        });
        Ok(token)
    }

    pub async fn bot_identity(&self) -> Result<BotIdentity, String> {
        let value = self
            .authed_json(reqwest::Method::GET, "/open-apis/bot/v3/info", None)
            .await?;
        let bot = value
            .get("bot")
            .or_else(|| value.pointer("/data/bot"))
            .cloned()
            .unwrap_or(Value::Null);
        Ok(BotIdentity {
            open_id: bot
                .get("open_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_owned(),
            name: bot
                .get("app_name")
                .or_else(|| bot.get("bot_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_owned(),
        })
    }

    pub async fn endpoint(&self) -> Result<Endpoint, String> {
        let response = self.client.post(format!("{}/callback/ws/endpoint", self.base))
            .header(reqwest::header::USER_AGENT, UA)
            .header("locale", "zh")
            .json(&json!({"AppID": self.app_id, "AppSecret": self.secret}))
            .send().await.map_err(|_| "\u{98de}\u{4e66}\u{957f}\u{8fde}\u{63a5}\u{5165}\u{53e3}\u{8bf7}\u{6c42}\u{5931}\u{8d25}".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "飞书长连接入口不可用（{}）",
                response.status().as_u16()
            ));
        }
        let value = response.json::<Value>().await.map_err(|_| "\u{98de}\u{4e66}\u{957f}\u{8fde}\u{63a5}\u{5165}\u{53e3}\u{54cd}\u{5e94}\u{65e0}\u{6548}".to_string())?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code == 1000040350 {
            return Err("fatal: \u{98de}\u{4e66}\u{957f}\u{8fde}\u{63a5}\u{5df2}\u{88ab}\u{5176}\u{4ed6}\u{5ba2}\u{6237}\u{7aef}\u{5360}\u{7528}".into());
        }
        if code != 0 && code != 1 && code != 1000040343 {
            return Err(format!("fatal: 飞书长连接被拒绝（{code}）"));
        }
        if code != 0 {
            return Err(format!("飞书长连接入口暂时不可用（{code}）"));
        }
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let url = data
            .get("URL")
            .or_else(|| data.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if !url.starts_with("wss://") {
            return Err(
                "\u{98de}\u{4e66}\u{957f}\u{8fde}\u{63a5}\u{5730}\u{5740}\u{65e0}\u{6548}".into(),
            );
        }
        let conf = data
            .get("ClientConfig")
            .or_else(|| data.get("client_config"))
            .cloned()
            .unwrap_or(Value::Null);
        let ping = conf
            .get("PingInterval")
            .or_else(|| conf.get("ping_interval"))
            .and_then(|v| v.as_u64())
            .unwrap_or(120);
        Ok(Endpoint {
            url,
            ping_interval: ping.clamp(10, 600),
        })
    }

    pub async fn send_message(
        &self,
        chat_id: &str,
        reply_to: Option<&str>,
        thread_id: Option<&str>,
        msg_type: &str,
        content: &str,
    ) -> Result<String, String> {
        let token = self.tenant_token().await?;
        let thread = match thread_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) if safe_id(id) => Some(id),
            Some(_) => return Err("飞书话题 ID 无效".into()),
            None => None,
        };
        let reply = reply_to.map(str::trim).filter(|id| safe_id(id));
        let mut skipped_reply = false;
        loop {
            let uuid = uuid::Uuid::new_v4().simple().to_string();
            let (url, body, replied) = if let Some(message_id) = reply.filter(|_| !skipped_reply) {
                (
                    format!("{}/open-apis/im/v1/messages/{message_id}/reply", self.base),
                    json!({
                        "content": content,
                        "msg_type": msg_type,
                        "reply_in_thread": thread.is_some(),
                        "uuid": uuid,
                    }),
                    true,
                )
            } else if let Some(thread_id) = thread {
                (
                    format!(
                        "{}/open-apis/im/v1/messages?receive_id_type=thread_id",
                        self.base
                    ),
                    json!({
                        "receive_id": thread_id,
                        "content": content,
                        "msg_type": msg_type,
                        "uuid": uuid,
                    }),
                    false,
                )
            } else {
                (
                    format!(
                        "{}/open-apis/im/v1/messages?receive_id_type=chat_id",
                        self.base
                    ),
                    json!({
                        "receive_id": chat_id,
                        "content": content,
                        "msg_type": msg_type,
                        "uuid": uuid,
                    }),
                    false,
                )
            };
            let value = self.post_json(&token, &url, body).await?;
            let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            if replied && matches!(code, 230011 | 231003) && !skipped_reply {
                skipped_reply = true;
                continue;
            }
            if code != 0 {
                return Err(api_error(
                    &self.secret,
                    &token,
                    code,
                    value.get("msg").and_then(|v| v.as_str()).unwrap_or(""),
                ));
            }
            return value
                .pointer("/data/message_id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| "飞书消息响应缺少 message_id".into());
        }
    }

    pub async fn update_message(
        &self,
        message_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<(), String> {
        if !safe_id(message_id) {
            return Err("\u{98de}\u{4e66}\u{6d88}\u{606f} ID \u{65e0}\u{6548}".into());
        }
        let token = self.tenant_token().await?;
        let url = format!("{}/open-apis/im/v1/messages/{message_id}", self.base);
        let response = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .json(&json!({"msg_type": msg_type, "content": content}))
            .send()
            .await
            .map_err(|_| {
                "\u{98de}\u{4e66}\u{6d88}\u{606f}\u{66f4}\u{65b0}\u{5931}\u{8d25}".to_string()
            })?;
        let value = response.json::<Value>().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{6d88}\u{606f}\u{66f4}\u{65b0}\u{54cd}\u{5e94}\u{65e0}\u{6548}"
                .to_string()
        })?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(api_error(
                &self.secret,
                &token,
                code,
                value.get("msg").and_then(|v| v.as_str()).unwrap_or(""),
            ));
        }
        Ok(())
    }

    pub async fn upload(&self, path: &str) -> Result<Uploaded, String> {
        let bytes = tokio::fs::read(path).await.map_err(|_| {
            "\u{65e0}\u{6cd5}\u{8bfb}\u{53d6}\u{5f85}\u{53d1}\u{9001}\u{9644}\u{4ef6}".to_string()
        })?;
        if bytes.len() > MAX_MEDIA_BYTES {
            return Err(
                "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{8d85}\u{8fc7} 20 MB \u{9650}\u{5236}".into(),
            );
        }
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file.bin");
        let token = self.tenant_token().await?;
        let ext = std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
        ) {
            let part = reqwest::multipart::Part::bytes(bytes).file_name(name.to_owned());
            let form = reqwest::multipart::Form::new()
                .text("image_type", "message")
                .part("image", part);
            let value = self
                .post_multipart(
                    &token,
                    &format!("{}/open-apis/im/v1/images", self.base),
                    form,
                )
                .await?;
            let key = value
                .pointer("/data/image_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            if key.is_empty() {
                return Err(
                    "\u{98de}\u{4e66}\u{56fe}\u{7247}\u{4e0a}\u{4f20}\u{5931}\u{8d25}".into(),
                );
            }
            return Ok(Uploaded::Image(key));
        }
        let (file_type, kind) = route_file(&ext);
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name.to_owned());
        let form = reqwest::multipart::Form::new()
            .text("file_type", file_type)
            .text("file_name", name.to_owned())
            .part("file", part);
        let value = self
            .post_multipart(
                &token,
                &format!("{}/open-apis/im/v1/files", self.base),
                form,
            )
            .await?;
        let key = value
            .pointer("/data/file_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if key.is_empty() {
            return Err("\u{98de}\u{4e66}\u{6587}\u{4ef6}\u{4e0a}\u{4f20}\u{5931}\u{8d25}".into());
        }
        Ok(Uploaded::File { key, kind })
    }

    pub async fn download_resource(
        &self,
        message_id: &str,
        item: &Download,
    ) -> Result<Vec<u8>, String> {
        if !safe_id(message_id) || !safe_id(&item.key) {
            return Err("\u{98de}\u{4e66}\u{9644}\u{4ef6} ID \u{65e0}\u{6548}".into());
        }
        let kinds: &[&str] = match item.kind {
            ResourceKind::Image => &["image"],
            ResourceKind::Audio => &["audio", "file"],
            ResourceKind::Media => &["media", "file"],
            ResourceKind::File => &["file"],
        };
        let mut last =
            "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{4e0b}\u{8f7d}\u{5931}\u{8d25}".to_string();
        for kind in kinds {
            match self.fetch_resource(message_id, &item.key, kind).await {
                Ok(bytes) => return Ok(bytes),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    async fn fetch_resource(
        &self,
        message_id: &str,
        key: &str,
        kind: &str,
    ) -> Result<Vec<u8>, String> {
        let token = self.tenant_token().await?;
        let url = format!(
            "{}/open-apis/im/v1/messages/{message_id}/resources/{key}?type={kind}",
            self.base
        );
        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|_| {
                "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{4e0b}\u{8f7d}\u{5931}\u{8d25}".to_string()
            })?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if content_type.starts_with("application/json") || !status.is_success() {
            let value = response.json::<Value>().await.unwrap_or(Value::Null);
            let code = value
                .get("code")
                .and_then(|v| v.as_i64())
                .unwrap_or(i64::from(status.as_u16()));
            return Err(api_error(
                &self.secret,
                &token,
                code,
                value.get("msg").and_then(|v| v.as_str()).unwrap_or(""),
            ));
        }
        read_limited(response).await
    }

    async fn authed_json(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let token = self.tenant_token().await?;
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{63a5}\u{53e3}\u{8bf7}\u{6c42}\u{5931}\u{8d25}".to_string()
        })?;
        let value = response.json::<Value>().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{63a5}\u{53e3}\u{54cd}\u{5e94}\u{65e0}\u{6548}".to_string()
        })?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code != 0 {
            return Err(api_error(
                &self.secret,
                &token,
                code,
                value.get("msg").and_then(|v| v.as_str()).unwrap_or(""),
            ));
        }
        Ok(value)
    }

    async fn post_json(&self, token: &str, url: &str, body: Value) -> Result<Value, String> {
        let response = self
            .client
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|_| {
                "\u{98de}\u{4e66}\u{6d88}\u{606f}\u{53d1}\u{9001}\u{5931}\u{8d25}".to_string()
            })?;
        response.json::<Value>().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{6d88}\u{606f}\u{54cd}\u{5e94}\u{65e0}\u{6548}".to_string()
        })
    }

    async fn post_multipart(
        &self,
        token: &str,
        url: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value, String> {
        let response = self
            .client
            .post(url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|_| "\u{98de}\u{4e66}\u{4e0a}\u{4f20}\u{5931}\u{8d25}".to_string())?;
        let value = response.json::<Value>().await.map_err(|_| {
            "\u{98de}\u{4e66}\u{4e0a}\u{4f20}\u{54cd}\u{5e94}\u{65e0}\u{6548}".to_string()
        })?;
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(api_error(
                &self.secret,
                token,
                code,
                value.get("msg").and_then(|v| v.as_str()).unwrap_or(""),
            ));
        }
        Ok(value)
    }
}

pub struct Endpoint {
    pub url: String,
    pub ping_interval: u64,
}

pub enum Uploaded {
    Image(String),
    File { key: String, kind: &'static str },
}

fn route_file(ext: &str) -> (&'static str, &'static str) {
    match ext {
        "ogg" | "opus" => ("opus", "audio"),
        "mp4" | "mov" | "avi" | "m4v" => ("mp4", "media"),
        "pdf" => ("pdf", "file"),
        "doc" | "docx" => ("doc", "file"),
        "xls" | "xlsx" => ("xls", "file"),
        "ppt" | "pptx" => ("ppt", "file"),
        _ => ("stream", "file"),
    }
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn api_error(secret: &str, token: &str, code: i64, msg: &str) -> String {
    let mut text = format!(
        "\u{98de}\u{4e66}\u{63a5}\u{53e3}\u{9519}\u{8bef} {code}: {}",
        msg.chars().take(180).collect::<String>()
    );
    if !secret.is_empty() {
        text = text.replace(secret, "[redacted]");
    }
    if token.len() > 8 {
        text = text.replace(token, "[redacted]");
    }
    text
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|n| n > MAX_MEDIA_BYTES as u64)
    {
        return Err(
            "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{8d85}\u{8fc7} 20 MB \u{9650}\u{5236}".into(),
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{8bfb}\u{53d6}\u{5931}\u{8d25}".to_string()
    })? {
        if bytes.len().saturating_add(chunk.len()) > MAX_MEDIA_BYTES {
            return Err(
                "\u{98de}\u{4e66}\u{9644}\u{4ef6}\u{8d85}\u{8fc7} 20 MB \u{9650}\u{5236}".into(),
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn api_errors_drop_secrets() {
        let text = api_error(
            "super-secret",
            "tenant-token-value",
            999,
            "bad super-secret tenant-token-value",
        );
        assert!(!text.contains("super-secret"));
        assert!(!text.contains("tenant-token-value"));
        assert!(text.contains("999"));
    }

    #[test]
    fn file_routes_match_feishu_upload_types() {
        assert_eq!(route_file("opus"), ("opus", "audio"));
        assert_eq!(route_file("mp4"), ("mp4", "media"));
        assert_eq!(route_file("pdf"), ("pdf", "file"));
        assert_eq!(route_file("zip"), ("stream", "file"));
    }

    #[tokio::test]
    async fn withdrawn_thread_reply_is_not_sent_to_the_group() {
        let (base, mock) = super::mock_http::spawn_mock(super::mock_http::MockOpts {
            message_delay: Duration::ZERO,
            fail_reply_once: Some(230011),
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        let id = api
            .send_message(
                "oc_group",
                Some("om_old"),
                Some("omt_topic"),
                "text",
                "{\"text\":\"hello\"}",
            )
            .await
            .unwrap();
        assert!(id.starts_with("om_srv_"));
        let hits = mock.hits.lock().clone();
        let messages: Vec<_> = hits
            .iter()
            .filter(|hit| hit.path.contains("/im/v1/messages"))
            .collect();
        assert!(messages[0].path.contains("/messages/om_old/reply"), "{}", messages[0].path);
        assert!(messages[0].body.contains("\"reply_in_thread\":true"));
        assert!(messages[1].path.contains("receive_id_type=thread_id"), "{}", messages[1].path);
        assert!(messages[1].body.contains("\"receive_id\":\"omt_topic\""));
        assert!(messages.iter().all(|hit| !hit.body.contains("oc_group")));
    }

    #[tokio::test]
    async fn thread_without_reply_target_uses_thread_receive_id() {
        let (base, mock) = super::mock_http::spawn_mock(super::mock_http::MockOpts {
            message_delay: Duration::ZERO,
            fail_reply_once: None,
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        api.send_message(
            "oc_group",
            None,
            Some("omt_topic"),
            "text",
            "{\"text\":\"stay\"}",
        )
        .await
        .unwrap();
        let hits = mock.hits.lock().clone();
        let message = hits
            .iter()
            .find(|hit| hit.path.contains("/im/v1/messages"))
            .unwrap();
        assert!(message.path.contains("receive_id_type=thread_id"));
        assert!(message.body.contains("\"receive_id\":\"omt_topic\""));
        assert!(!message.body.contains("oc_group"));
    }
}

#[cfg(test)]
pub(super) mod mock_http {
    use parking_lot::Mutex;
    use std::time::Duration;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Clone, Debug)]
    pub struct MockHit {
        pub method: String,
        pub path: String,
        pub body: String,
    }

    #[derive(Clone)]
    pub struct MockApi {
        pub hits: Arc<Mutex<Vec<MockHit>>>,
    }

    pub struct MockOpts {
        pub message_delay: Duration,
        pub fail_reply_once: Option<i64>,
    }

    struct Shared {
        hits: Arc<Mutex<Vec<MockHit>>>,
        message_delay: Duration,
        fail_reply: Mutex<Option<i64>>,
        seq: AtomicU64,
    }

    pub async fn spawn_mock(opts: MockOpts) -> (String, MockApi) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::new(Shared {
            hits: hits.clone(),
            message_delay: opts.message_delay,
            fail_reply: Mutex::new(opts.fail_reply_once),
            seq: AtomicU64::new(1),
        });
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                let shared = shared.clone();
                tokio::spawn(serve(sock, shared));
            }
        });
        (format!("http://{addr}"), MockApi { hits })
    }

    async fn serve(mut sock: tokio::net::TcpStream, shared: Arc<Shared>) {
        let mut buf = Vec::new();
        loop {
            let header_end = loop {
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos;
                }
                let mut tmp = [0u8; 4096];
                let n = match sock.read(&mut tmp).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&tmp[..n]);
                if buf.len() > 2 * 1024 * 1024 {
                    return;
                }
            };
            let header = String::from_utf8_lossy(&buf[..header_end]).into_owned();
            let expect = header.to_ascii_lowercase().contains("expect: 100-continue");
            buf.drain(..header_end + 4);
            if expect && sock.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await.is_err() {
                return;
            }
            let length = header
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            while buf.len() < length {
                let mut tmp = [0u8; 8192];
                let n = match sock.read(&mut tmp).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&tmp[..n]);
            }
            let body = String::from_utf8_lossy(&buf.drain(..length).collect::<Vec<_>>()).into_owned();
            let mut parts = header.lines().next().unwrap_or("").split_whitespace();
            let method = parts.next().unwrap_or("").to_owned();
            let path = parts.next().unwrap_or("").to_owned();
            shared.hits.lock().push(MockHit {
                method: method.clone(),
                path: path.clone(),
                body: body.clone(),
            });
            let is_message = path.contains("/im/v1/messages");
            if is_message && !shared.message_delay.is_zero() {
                tokio::time::sleep(shared.message_delay).await;
            }
            let json = if path.contains("tenant_access_token") {
                r#"{"code":0,"tenant_access_token":"t-test","expire":7200}"#.to_owned()
            } else if path.contains("/im/v1/files") || path.contains("/im/v1/images") {
                r#"{"code":0,"data":{"file_key":"fk_mock","image_key":"img_mock"}}"#.to_owned()
            } else if path.contains("/reply") {
                if let Some(code) = shared.fail_reply.lock().take() {
                    format!(r#"{{"code":{code},"msg":"withdrawn"}}"#)
                } else {
                    let n = shared.seq.fetch_add(1, Ordering::Relaxed);
                    format!(r#"{{"code":0,"data":{{"message_id":"om_srv_{n}"}}}}"#)
                }
            } else if is_message || method == "PUT" {
                let n = shared.seq.fetch_add(1, Ordering::Relaxed);
                format!(r#"{{"code":0,"data":{{"message_id":"om_srv_{n}"}}}}"#)
            } else {
                r#"{"code":0}"#.to_owned()
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{json}",
                json.len()
            );
            if sock.write_all(resp.as_bytes()).await.is_err() {
                return;
            }
        }
    }
}
