// Portions derived from Hermes WeCom callback adapter (MIT License, Copyright (c) 2025 Nous Research).
// Self-built apps: verify GET and decrypt POST, ack before the agent runs, reply with message/send.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::task::JoinHandle;

use super::common::{http_client, split_text, PlatformContext};
use super::types::{
    ConnectionState, CredentialInput, ImPlatform, InboundMessage, OutboundMessage, StatusUpdate,
    WecomCallbackConfig,
};
use super::wecom_crypto::{CryptError, WxCrypt};

const MAX_BODY: usize = 65_536;
const SEND_BYTES: usize = 2048;
const API_ROOT: &str = "https://qyapi.weixin.qq.com";
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(2);
const UNSUPPORTED_NOTE: &str =
    "当前企业微信自建应用只接收文本。图片、文件、语音和视频请改用文字说明，本次附件没有交给助手。";

struct CachedToken {
    token: String,
    expires_at: Instant,
}

struct CallbackRt {
    crypt: WxCrypt,
    corp_id: String,
    agent_id: i64,
    secret: String,
    api_root: String,
    http: reqwest::Client,
    inbound: mpsc::Sender<InboundMessage>,
    app_token: Mutex<Option<CachedToken>>,
    send_lock: Mutex<()>,
    sent_streams: Mutex<std::collections::HashSet<String>>,
}

#[derive(Deserialize, Default)]
struct SigQuery {
    #[serde(default)]
    msg_signature: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    nonce: String,
    #[serde(default)]
    echostr: String,
}

enum Accepted {
    Ignore,
    Explain { user: String },
    Text(InboundMessage),
}

pub async fn run(
    config: WecomCallbackConfig,
    credentials: CredentialInput,
    ctx: PlatformContext,
) -> Result<(), String> {
    if !config.enabled {
        set_status(&ctx.status, ConnectionState::Disabled, "未启用").await;
        return Ok(());
    }
    let corp_id = config.corp_id.trim();
    let agent_raw = config.agent_id.trim();
    if corp_id.is_empty()
        || agent_raw.is_empty()
        || credentials.secret.trim().is_empty()
        || credentials.token.trim().is_empty()
        || credentials.encoding_aes_key.trim().len() != 43
    {
        set_status(
            &ctx.status,
            ConnectionState::Error,
            "自建应用的企业 ID、应用 ID 或密钥未配置",
        )
        .await;
        return Err("fatal: 自建应用的企业 ID、应用 ID 或密钥未配置".into());
    }
    let agent_id: i64 = agent_raw
        .parse()
        .map_err(|_| "fatal: 自建应用 AgentId 不是整数")?;
    let crypt = WxCrypt::new(
        credentials.token.trim(),
        credentials.encoding_aes_key.trim(),
        corp_id,
    )
    .map_err(|_| "fatal: 自建应用 EncodingAESKey 无效")?;
    let host = if config.webhook.host.trim().is_empty() {
        "127.0.0.1"
    } else {
        config.webhook.host.trim()
    };
    let path = normalize_path(&config.webhook.path);
    let rt = Arc::new(CallbackRt {
        crypt,
        corp_id: corp_id.to_owned(),
        agent_id,
        secret: credentials.secret,
        api_root: API_ROOT.to_owned(),
        http: http_client()?,
        inbound: ctx.inbound.clone(),
        app_token: Mutex::new(None),
        send_lock: Mutex::new(()),
        sent_streams: Mutex::new(std::collections::HashSet::new()),
    });
    let mut router = Router::new().route(&path, get(verify).post(receive));
    if path != "/health" {
        router = router.route("/health", get(health));
    }
    let router = router
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(rt.clone());
    let listener = TcpListener::bind(bind_addr(host, config.webhook.port))
        .await
        .map_err(|_| format!("企业微信回调无法监听 {host}:{}", config.webhook.port))?;
    set_status(
        &ctx.status,
        ConnectionState::Connected,
        "回调已监听，访问令牌将在发送时重试",
    )
    .await;
    supervise(listener, router, rt, ctx.outbound, ctx.shutdown, ctx.status).await
}

async fn supervise(
    listener: TcpListener,
    router: Router,
    rt: Arc<CallbackRt>,
    outbound: mpsc::Receiver<OutboundMessage>,
    mut shutdown: watch::Receiver<bool>,
    status: mpsc::Sender<StatusUpdate>,
) -> Result<(), String> {
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut server = tokio::spawn(serve_http(listener, router, stop_rx));
    let probe_rt = rt.clone();
    let probe_status = status.clone();
    let probe = tokio::spawn(async move {
        if probe_rt.access_token().await.is_ok() {
            set_status(
                &probe_status,
                ConnectionState::Connected,
                "自建应用回调已连接",
            )
            .await;
        }
    });
    let mut outbound_task =
        tokio::spawn(outbound_worker(outbound, rt, stop_tx.subscribe(), status));
    let mut server_done = false;
    let mut outbound_done = false;
    let outcome = tokio::select! {
        result = &mut server => {
            server_done = true;
            match result {
                Ok(Ok(())) => Ok(()),
                _ => Err("企业微信回调服务已停止".to_string()),
            }
        }
        _ = &mut outbound_task => {
            outbound_done = true;
            Ok(())
        }
        done = shutdown.wait_for(|stop| *stop) => {
            let _ = done;
            Ok(())
        }
    };
    let _ = stop_tx.send(true);
    if server_done {
        drop(server);
    } else {
        finish_task(server, SHUTDOWN_BUDGET).await;
    }
    if outbound_done {
        drop(outbound_task);
    } else {
        finish_task(outbound_task, SHUTDOWN_BUDGET).await;
    }
    finish_task(probe, SHUTDOWN_BUDGET).await;
    outcome
}

async fn serve_http(
    listener: TcpListener,
    router: Router,
    mut stop: watch::Receiver<bool>,
) -> Result<(), String> {
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = stop.wait_for(|stop| *stop).await;
        })
        .await
        .map_err(|_| "企业微信回调服务已停止".to_string())
}

fn watch_stopped(stop: &watch::Receiver<bool>) -> bool {
    *stop.borrow()
}

async fn until_stopped(stop: &mut watch::Receiver<bool>) {
    loop {
        if watch_stopped(stop) {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

async fn outbound_worker(
    mut outbound: mpsc::Receiver<OutboundMessage>,
    rt: Arc<CallbackRt>,
    mut stop: watch::Receiver<bool>,
    status: mpsc::Sender<StatusUpdate>,
) {
    loop {
        let message = tokio::select! {
            _ = until_stopped(&mut stop) => break,
            incoming = outbound.recv() => {
                let Some(message) = incoming else { break };
                message
            }
        };
        let result = tokio::select! {
            _ = until_stopped(&mut stop) => break,
            result = deliver_outbound(&rt, message) => result,
        };
        if let Err(error) = result {
            set_status(&status, ConnectionState::Error, error).await;
        }
    }
}

async fn finish_task<T>(mut task: JoinHandle<T>, budget: Duration) {
    if task.is_finished() {
        let _ = task.await;
        return;
    }
    if tokio::time::timeout(budget, &mut task).await.is_err() {
        task.abort();
        let _ = task.await;
    }
}

async fn health() -> impl IntoResponse {
    axum::Json(json!({"status": "ok", "platform": "wecom_callback"}))
}

async fn verify(State(rt): State<Arc<CallbackRt>>, Query(query): Query<SigQuery>) -> Response {
    match rt.crypt.decrypt(
        &query.msg_signature,
        &query.timestamp,
        &query.nonce,
        &query.echostr,
    ) {
        Ok(plain) => text(StatusCode::OK, String::from_utf8_lossy(&plain).into_owned()),
        Err(_) => text(StatusCode::FORBIDDEN, "signature verification failed"),
    }
}

async fn receive(
    State(rt): State<Arc<CallbackRt>>,
    Query(query): Query<SigQuery>,
    body: axum::body::Bytes,
) -> Response {
    if body.len() > MAX_BODY {
        return text(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
    }
    let xml = String::from_utf8_lossy(&body);
    let encrypt = xml_fields(&xml).get("Encrypt").cloned().unwrap_or_default();
    let plain = match rt.crypt.decrypt(
        &query.msg_signature,
        &query.timestamp,
        &query.nonce,
        &encrypt,
    ) {
        Ok(plain) => plain,
        Err(CryptError::Signature) | Err(CryptError::ReceiveId) | Err(CryptError::Payload) => {
            return text(StatusCode::BAD_REQUEST, "invalid callback payload");
        }
    };
    let fields = xml_fields(&String::from_utf8_lossy(&plain));
    match accept(&rt.corp_id, &fields) {
        Accepted::Ignore => text(StatusCode::OK, "success"),
        Accepted::Explain { user } => {
            let rt = rt.clone();
            tokio::spawn(async move {
                let _ = rt.deliver(&user, UNSUPPORTED_NOTE).await;
            });
            text(StatusCode::OK, "success")
        }
        Accepted::Text(message) => match rt.inbound.send(message).await {
            Ok(()) => text(StatusCode::OK, "success"),
            Err(_) => text(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        },
    }
}

fn accept(corp_id: &str, fields: &BTreeMap<String, String>) -> Accepted {
    let msg_type = fields
        .get("MsgType")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let user = fields.get("FromUserName").cloned().unwrap_or_default();
    if matches!(
        msg_type.as_str(),
        "image" | "voice" | "video" | "shortvideo" | "file" | "location" | "link"
    ) {
        return if user.is_empty() {
            Accepted::Ignore
        } else {
            Accepted::Explain { user }
        };
    }
    if msg_type == "event" {
        let event = fields
            .get("Event")
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        if event == "enter_agent" || event == "subscribe" || user.is_empty() {
            return Accepted::Ignore;
        }
    } else if msg_type != "text" || user.is_empty() {
        return Accepted::Ignore;
    }
    let content = fields
        .get("Content")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if msg_type == "event" {
                "/start".into()
            } else {
                String::new()
            }
        });
    if content.is_empty() {
        return Accepted::Ignore;
    }
    let corp = fields
        .get("ToUserName")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| corp_id.to_owned());
    let msg_id = fields
        .get("MsgId")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            format!(
                "{user}:{}",
                fields.get("CreateTime").map(String::as_str).unwrap_or("0")
            )
        });
    Accepted::Text(InboundMessage {
        platform: ImPlatform::WecomCallback,
        message_id: msg_id,
        chat_id: format!("{corp}:{user}"),
        user_id: user.clone(),
        user_name: user,
        is_group: false,
        thread_id: None,
        reply_token: None,
        text: content,
        attachments: Vec::new(),
        attachment_failures: Vec::new(),
    })
}

async fn deliver_outbound(rt: &CallbackRt, message: OutboundMessage) -> Result<(), String> {
    let stream_id = message.stream_id.clone().filter(|id| !id.is_empty());
    if let Some(stream_id) = stream_id.as_deref() {
        if !message.finished || rt.sent_streams.lock().await.contains(stream_id) {
            return Ok(());
        }
    }
    let mut text = message.text;
    if !message.attachments.is_empty() {
        text.push_str("\n（自建应用回调不能发送图片或文件）");
    }
    rt.deliver(&message.chat_id, &text).await?;
    if let Some(stream_id) = stream_id {
        rt.sent_streams.lock().await.insert(stream_id);
    }
    Ok(())
}

impl CallbackRt {
    async fn deliver(&self, chat_id: &str, content: &str) -> Result<(), String> {
        let _gate = self.send_lock.lock().await;
        let user = chat_id
            .split_once(':')
            .map(|(_, user)| user)
            .unwrap_or(chat_id);
        if user.is_empty() || content.is_empty() {
            return Ok(());
        }
        for chunk in callback_chunks(content) {
            self.send_one(user, &chunk).await?;
        }
        Ok(())
    }

    async fn send_one(&self, user: &str, content: &str) -> Result<(), String> {
        let payload = json!({
            "touser": user,
            "msgtype": "text",
            "agentid": self.agent_id,
            "text": { "content": content },
            "safe": 0,
        });
        for attempt in 0..2 {
            let token = self.access_token().await?;
            let response = self
                .http
                .post(format!(
                    "{}/cgi-bin/message/send?access_token={token}",
                    self.api_root
                ))
                .json(&payload)
                .send()
                .await
                .map_err(|_| "企业微信发送失败".to_string())?;
            let data: Value = response
                .json()
                .await
                .map_err(|_| "企业微信发送失败".to_string())?;
            let code = data.get("errcode").and_then(Value::as_i64).unwrap_or(-1);
            if matches!(code, 40001 | 42001) && attempt == 0 {
                *self.app_token.lock().await = None;
                continue;
            }
            if code != 0 {
                return Err(format!("企业微信发送失败 (errcode={code})"));
            }
            return Ok(());
        }
        Err("企业微信发送失败 (errcode=40001)".into())
    }

    async fn access_token(&self) -> Result<String, String> {
        if let Some(cached) = self.app_token.lock().await.as_ref() {
            if cached.expires_at > Instant::now() + Duration::from_secs(60) {
                return Ok(cached.token.clone());
            }
        }
        let response = self
            .http
            .get(format!("{}/cgi-bin/gettoken", self.api_root))
            .query(&[
                ("corpid", self.corp_id.as_str()),
                ("corpsecret", self.secret.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "企业微信令牌请求失败".to_string())?;
        let data: Value = response
            .json()
            .await
            .map_err(|_| "企业微信令牌响应无效".to_string())?;
        let code = data.get("errcode").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 {
            return Err(format!("企业微信令牌刷新失败 (errcode={code})"));
        }
        let token = data
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .ok_or("企业微信令牌响应缺少令牌")?;
        let expires_in = data
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(7200)
            .clamp(120, 7200) as u64;
        *self.app_token.lock().await = Some(CachedToken {
            token: token.to_owned(),
            expires_at: Instant::now() + Duration::from_secs(expires_in),
        });
        Ok(token.to_owned())
    }
}

fn callback_chunks(text: &str) -> Vec<String> {
    split_text(text, SEND_BYTES)
}

fn xml_fields(xml: &str) -> BTreeMap<String, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut current = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(event)) => {
                current = String::from_utf8_lossy(event.name().as_ref()).into_owned();
            }
            Ok(quick_xml::events::Event::End(_) | quick_xml::events::Event::Empty(_)) => {
                current.clear()
            }
            Ok(quick_xml::events::Event::Text(event)) => {
                if !current.is_empty() {
                    if let Ok(text) = event.xml_content() {
                        fields.entry(current.clone()).or_default().push_str(&text);
                    }
                }
            }
            Ok(quick_xml::events::Event::GeneralRef(event)) => {
                if !current.is_empty() {
                    if let Ok(Some(ch)) = event.resolve_char_ref() {
                        fields.entry(current.clone()).or_default().push(ch);
                    } else if let Ok(name) = event.decode() {
                        if let Some(text) = quick_xml::escape::resolve_predefined_entity(&name) {
                            fields.entry(current.clone()).or_default().push_str(text);
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::CData(event)) => {
                if !current.is_empty() {
                    fields
                        .entry(current.clone())
                        .or_default()
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    fields
}

fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        "/wecom/callback".into()
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

fn bind_addr(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn text(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body.into(),
    )
        .into_response()
}

async fn set_status(
    status: &mpsc::Sender<StatusUpdate>,
    state: ConnectionState,
    message: impl Into<String>,
) {
    let _ = status
        .send(StatusUpdate {
            platform: ImPlatform::WecomCallback,
            state,
            message: message.into(),
        })
        .await;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::State;
    use axum::routing::{get, post};
    use axum::Router;
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc, oneshot, watch, Mutex, Notify};

    use super::super::wecom_crypto::{signature, WxCrypt};
    use super::*;

    #[test]
    fn unicode_chunks_stay_inside_2048_bytes() {
        let text = "你".repeat(683);
        assert_eq!("你".len() * 683, 2049);
        let chunks = callback_chunks(&text);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), text);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.len() <= 2048 && chunk.chars().all(|ch| ch == '你')));
    }

    #[test]
    fn formatted_xml_preserves_ids_and_mixed_content() {
        let fields = xml_fields("<xml>\n<ToUserName>wwcorp</ToUserName>\n<FromUserName>user-1</FromUserName>\n<MsgType>text</MsgType>\n<Content>你好 &amp; <![CDATA[<世界>]]> &#x1F600;</Content>\n<MsgId>msg-1</MsgId>\n</xml>");
        match accept("wwcorp", &fields) {
            Accepted::Text(message) => {
                assert_eq!(message.user_id, "user-1");
                assert_eq!(message.chat_id, "wwcorp:user-1");
                assert_eq!(message.message_id, "msg-1");
                assert_eq!(message.text, "你好 & <世界> 😀");
            }
            _ => panic!("valid callback must reach the text turn"),
        }
    }

    #[test]
    fn attachment_callback_is_not_a_text_turn() {
        let mut fields = BTreeMap::new();
        fields.insert("MsgType".into(), "image".into());
        fields.insert("FromUserName".into(), "user-1".into());
        match accept("wwcorp", &fields) {
            Accepted::Explain { user } => assert_eq!(user, "user-1"),
            Accepted::Text(_) => panic!("attachment must not enter the agent"),
            Accepted::Ignore => panic!("known sender should be told attachments are unsupported"),
        }
    }

    #[test]
    fn signature_helper_sorts_inputs() {
        let left = super::super::wecom_crypto::signature("t", "1", "n", "e");
        let right = super::super::wecom_crypto::signature("e", "n", "1", "t");
        assert_eq!(left, right);
        assert_ne!(
            left,
            super::super::wecom_crypto::signature("t", "1", "n", "other")
        );
    }

    #[derive(Clone)]
    struct SendGate {
        entered: Arc<Mutex<Option<oneshot::Sender<()>>>>,
        release: Arc<Notify>,
    }

    async fn token_ok() -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "errcode": 0,
            "access_token": "tok",
            "expires_in": 7200
        }))
    }

    async fn hold_send(State(gate): State<SendGate>) -> axum::Json<serde_json::Value> {
        if let Some(sender) = gate.entered.lock().await.take() {
            let _ = sender.send(());
        }
        gate.release.notified().await;
        axum::Json(serde_json::json!({"errcode": 0}))
    }

    #[tokio::test]
    async fn slow_send_api_still_acks_callback() {
        const TOKEN: &str = "token-demo";
        const AES: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
        let (entered_tx, entered_rx) = oneshot::channel();
        let gate = SendGate {
            entered: Arc::new(Mutex::new(Some(entered_tx))),
            release: Arc::new(Notify::new()),
        };
        let mock_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock_listener.local_addr().unwrap();
        let mock_router = Router::new()
            .route("/cgi-bin/gettoken", get(token_ok))
            .route("/cgi-bin/message/send", post(hold_send))
            .with_state(gate.clone());
        let mock_task = tokio::spawn(async move {
            let _ = axum::serve(mock_listener, mock_router).await;
        });
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if http
                    .get(format!("http://{mock_addr}/cgi-bin/gettoken"))
                    .send()
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("mock token API did not start");

        let crypt = WxCrypt::new(TOKEN, AES, "wwcorp").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (inbound_tx, mut inbound_rx) = mpsc::channel(4);
        let (status_tx, _status_rx) = mpsc::channel(8);
        let (out_tx, out_rx) = mpsc::channel(4);
        let (stop_tx, stop_rx) = watch::channel(false);
        let rt = Arc::new(CallbackRt {
            crypt: WxCrypt::new(TOKEN, AES, "wwcorp").unwrap(),
            corp_id: "wwcorp".into(),
            agent_id: 1,
            secret: "corp-secret".into(),
            api_root: format!("http://{mock_addr}"),
            http: super::super::common::http_client().unwrap(),
            inbound: inbound_tx,
            app_token: Mutex::new(None),
            send_lock: Mutex::new(()),
            sent_streams: Mutex::new(std::collections::HashSet::new()),
        });
        let router = Router::new()
            .route("/wecom/callback", get(verify).post(receive))
            .with_state(rt.clone());
        let supervised = tokio::spawn(supervise(listener, router, rt, out_rx, stop_rx, status_tx));
        out_tx
            .send(OutboundMessage {
                chat_id: "wwcorp:user-1".into(),
                is_group: false,
                thread_id: None,
                reply_token: None,
                reply_to: None,
                text: "hello".into(),
                attachments: Vec::new(),
                stream_id: None,
                finished: true,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), entered_rx)
            .await
            .expect("send API was not reached")
            .expect("send API dropped the barrier signal");

        let ts = "1700000000";
        let nonce = "nonce12345";
        let echo = crypt.encrypt_b64(b"echo-plain").unwrap();
        let echo_sig = signature(TOKEN, ts, nonce, &echo);
        let verified = tokio::time::timeout(
            Duration::from_secs(1),
            http.get(format!("http://{addr}/wecom/callback"))
                .query(&[
                    ("msg_signature", echo_sig.as_str()),
                    ("timestamp", ts),
                    ("nonce", nonce),
                    ("echostr", echo.as_str()),
                ])
                .send(),
        )
        .await
        .expect("verify did not return while send API was blocked")
        .unwrap();
        assert_eq!(verified.status(), reqwest::StatusCode::OK);
        assert_eq!(verified.text().await.unwrap(), "echo-plain");

        let plain = "<xml><ToUserName>wwcorp</ToUserName><FromUserName>user-9</FromUserName><MsgType>text</MsgType><Content>收到</Content><MsgId>m-9</MsgId></xml>";
        let encrypted = crypt.encrypt_b64(plain.as_bytes()).unwrap();
        let sig = signature(TOKEN, ts, nonce, &encrypted);
        let body = format!("<xml><Encrypt><![CDATA[{encrypted}]]></Encrypt></xml>");
        let received = tokio::time::timeout(
            Duration::from_secs(1),
            http.post(format!("http://{addr}/wecom/callback"))
                .query(&[
                    ("msg_signature", sig.as_str()),
                    ("timestamp", ts),
                    ("nonce", nonce),
                ])
                .header("content-type", "text/xml")
                .body(body)
                .send(),
        )
        .await
        .expect("receive did not return while send API was blocked")
        .unwrap();
        assert_eq!(received.status(), reqwest::StatusCode::OK);
        let inbound = tokio::time::timeout(Duration::from_secs(1), inbound_rx.recv())
            .await
            .expect("inbound was not delivered")
            .expect("inbound closed");
        assert_eq!(inbound.text, "收到");
        assert_eq!(inbound.chat_id, "wwcorp:user-9");
        assert!(!inbound.is_group);
        assert!(inbound.thread_id.is_none());

        let _ = stop_tx.send(true);
        tokio::time::timeout(Duration::from_secs(3), supervised)
            .await
            .expect("shutdown left the callback tasks running")
            .unwrap()
            .unwrap();
        mock_task.abort();
    }
}
