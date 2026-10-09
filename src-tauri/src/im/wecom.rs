// Portions derived from Hermes WeCom adapter (MIT License, Copyright (c) 2025 Nous Research).
// Smart-bot WebSocket: subscribe, ping, callbacks, native stream bubbles, and passive group replies.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use super::common::{split_text, PlatformContext};
use super::types::{
    ConnectionState, CredentialInput, ImPlatform, InboundMessage, OutboundMessage, StatusUpdate,
    WecomConfig,
};
use super::wecom_media::{
    load_outbound, prepare_bytes, refs_from_body, store_inbound, upload_media, WecomRpc,
};

const DEFAULT_WS: &str = "wss://openws.work.weixin.qq.com";
const STREAM_SAFE_MS: u64 = 330_000;
const MAX_STREAM_BYTES: usize = 20_480;
const MAX_INTERMEDIATE_FRAMES: u32 = 85;
const MARKDOWN_BYTES: usize = 4_000;
const NORMAL_TOKENS: u32 = 24;
const RESERVED_TOKENS: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinalAck {
    Delivered,
    Expired,
    Failed,
}

pub(crate) fn classify_final_ack(code: i64, timed_out: bool) -> FinalAck {
    if timed_out || code == 0 || code == 6000 {
        FinalAck::Delivered
    } else if code == 846608 || code == 846604 {
        FinalAck::Expired
    } else {
        FinalAck::Failed
    }
}

pub(crate) fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextSend {
    Passive { req_id: String, content: String },
    Proactive { content: String },
}

pub(crate) fn plan_text(
    is_group: bool,
    req_id: Option<&str>,
    text: &str,
) -> Result<Vec<TextSend>, String> {
    let chunks = split_text(text, MARKDOWN_BYTES);
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    if is_group {
        let req_id = req_id.unwrap_or("").trim();
        if req_id.is_empty() {
            return Err("群聊缺少 req_id，不能主动发送".into());
        }
        return Ok(chunks
            .into_iter()
            .map(|content| TextSend::Passive {
                req_id: req_id.to_owned(),
                content,
            })
            .collect());
    }
    Ok(chunks
        .into_iter()
        .map(|content| TextSend::Proactive { content })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WireCmd {
    Stream {
        req_id: String,
        bubble_id: String,
        content: String,
        finish: bool,
    },
    Proactive {
        content: String,
    },
}

#[derive(Debug)]
struct Turn {
    req_id: String,
    bubble_id: String,
    last_sent: String,
    started_ms: u64,
    intermediates: u32,
    seeded: bool,
    is_group: bool,
}

#[derive(Debug, Default)]
pub(crate) struct StreamBook {
    turns: HashMap<String, Turn>,
    finalized: HashSet<String>,
    expired: HashSet<String>,
}

impl StreamBook {
    pub(crate) fn note_inbound(&mut self, chat_id: &str) {
        self.expired.remove(chat_id);
    }

    pub(crate) fn push(
        &mut self,
        now_ms: u64,
        chat_id: &str,
        stream_id: &str,
        req_id: Option<&str>,
        is_group: bool,
        text: &str,
        finished: bool,
    ) -> Result<Vec<WireCmd>, String> {
        if stream_id.is_empty() {
            return Ok(Vec::new());
        }
        if self.finalized.contains(stream_id) {
            return Ok(Vec::new());
        }
        if !self.turns.contains_key(stream_id) {
            if self.expired.contains(chat_id) {
                self.finalized.insert(stream_id.to_owned());
                return if finished {
                    self.fallback(chat_id, is_group, text)
                } else {
                    Ok(Vec::new())
                };
            }
            let req = req_id.unwrap_or("").trim();
            if req.is_empty() {
                if is_group {
                    return Err("群聊缺少 req_id，不能主动发送".into());
                }
                if finished {
                    self.finalized.insert(stream_id.to_owned());
                    return Ok(proactive_chunks(text));
                }
                return Ok(Vec::new());
            }
            self.turns.insert(
                stream_id.to_owned(),
                Turn {
                    req_id: req.to_owned(),
                    bubble_id: stream_id.to_owned(),
                    last_sent: String::new(),
                    started_ms: now_ms,
                    intermediates: 0,
                    seeded: false,
                    is_group,
                },
            );
        }
        let aged = self.turns.get(stream_id).is_some_and(|turn| {
            finished && now_ms.saturating_sub(turn.started_ms) >= STREAM_SAFE_MS
        });
        if aged {
            let group = self.turns.get(stream_id).is_some_and(|turn| turn.is_group);
            self.turns.remove(stream_id);
            self.finalized.insert(stream_id.to_owned());
            self.expired.insert(chat_id.to_owned());
            return if group {
                Err("群聊流式窗口已过期，不能主动发送".into())
            } else {
                Ok(proactive_chunks(text))
            };
        }
        let mut cmds = Vec::new();
        let turn = self.turns.get_mut(stream_id).expect("turn inserted");
        if !turn.seeded {
            cmds.push(stream_cmd(turn, "<think></think>".into(), false));
            turn.seeded = true;
            if text.is_empty() && !finished {
                return Ok(cmds);
            }
        }
        if finished {
            let mut final_text = truncate_utf8(text, MAX_STREAM_BYTES);
            if !final_text.is_empty() && final_text == turn.last_sent {
                final_text.push('\u{200b}');
                final_text = truncate_utf8(&final_text, MAX_STREAM_BYTES);
            }
            cmds.push(stream_cmd(turn, final_text, true));
            self.turns.remove(stream_id);
            self.finalized.insert(stream_id.to_owned());
            return Ok(cmds);
        }
        let truncated = truncate_utf8(text, MAX_STREAM_BYTES);
        if turn.intermediates >= MAX_INTERMEDIATE_FRAMES || truncated == turn.last_sent {
            return Ok(cmds);
        }
        cmds.push(stream_cmd(turn, truncated.clone(), false));
        turn.intermediates += 1;
        turn.last_sent = truncated;
        Ok(cmds)
    }

    fn fallback(&self, chat_id: &str, is_group: bool, text: &str) -> Result<Vec<WireCmd>, String> {
        let _ = chat_id;
        if is_group {
            Err("群聊流式窗口已过期，不能主动发送".into())
        } else {
            Ok(proactive_chunks(text))
        }
    }
}

fn proactive_chunks(text: &str) -> Vec<WireCmd> {
    split_text(text, MARKDOWN_BYTES)
        .into_iter()
        .map(|content| WireCmd::Proactive { content })
        .collect()
}

fn stream_cmd(turn: &Turn, content: String, finish: bool) -> WireCmd {
    WireCmd::Stream {
        req_id: turn.req_id.clone(),
        bubble_id: turn.bubble_id.clone(),
        content,
        finish,
    }
}

enum OutMsg {
    Text(String),
    Pong(Vec<u8>),
    Close,
}

struct ReplySlot {
    tx: watch::Sender<u64>,
    _keep: watch::Receiver<u64>,
    seq: u64,
    pending: bool,
    code: i64,
}

impl ReplySlot {
    fn new() -> Self {
        let (tx, rx) = watch::channel(0);
        Self {
            tx,
            _keep: rx,
            seq: 0,
            pending: false,
            code: 0,
        }
    }
}

struct Bucket {
    normal: u32,
    reserved: u32,
    window: Instant,
}

struct GwState {
    pending: HashMap<String, oneshot::Sender<Value>>,
    replies: HashMap<String, ReplySlot>,
    groups: HashSet<String>,
    req_ids: HashMap<String, String>,
    req_order: VecDeque<String>,
    book: StreamBook,
    buckets: HashMap<String, Bucket>,
}

struct Gateway {
    app: tauri::AppHandle,
    inbound: mpsc::Sender<InboundMessage>,
    status: mpsc::Sender<StatusUpdate>,
    write_tx: mpsc::Sender<OutMsg>,
    state: Mutex<GwState>,
    origin: Instant,
    stop: watch::Sender<bool>,
    gates: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    workers: Mutex<HashMap<String, mpsc::Sender<OutboundMessage>>>,
}

pub async fn run(
    config: WecomConfig,
    credentials: CredentialInput,
    mut ctx: PlatformContext,
) -> Result<(), String> {
    if !config.enabled {
        set_status(&ctx.status, ConnectionState::Disabled, "未启用").await;
        return Ok(());
    }
    if config.bot_id.trim().is_empty() || credentials.secret.trim().is_empty() {
        set_status(
            &ctx.status,
            ConnectionState::Error,
            "企业微信机器人 ID 或密钥未配置",
        )
        .await;
        return Err("fatal: 企业微信机器人 ID 或密钥未配置".into());
    }
    set_status(&ctx.status, ConnectionState::Connecting, "正在连接企业微信").await;
    let url = {
        let configured = config.websocket_url.trim();
        if configured.is_empty() {
            DEFAULT_WS.to_owned()
        } else {
            configured.to_owned()
        }
    };
    let (ws, _) = match tokio::time::timeout(Duration::from_secs(20), connect_async(&url)).await {
        Ok(Ok(connected)) => connected,
        Ok(Err(_)) => return Err("企业微信 WebSocket 连接失败".into()),
        Err(_) => return Err("企业微信 WebSocket 连接超时".into()),
    };
    let (mut write, mut read) = ws.split();
    let subscribe_id = format!("subscribe-{}", Uuid::new_v4().simple());
    let device_id = Uuid::new_v4().simple().to_string();
    let hello = json!({
        "cmd": "aibot_subscribe",
        "headers": { "req_id": subscribe_id },
        "body": { "bot_id": config.bot_id.trim(), "secret": credentials.secret, "device_id": device_id },
    }).to_string();
    write
        .send(text_message(hello))
        .await
        .map_err(|_| "企业微信订阅发送失败")?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("企业微信订阅超时".into());
        }
        let frame = tokio::time::timeout(remaining, read.next())
            .await
            .map_err(|_| "企业微信订阅超时".to_string())?
            .ok_or("企业微信连接在订阅时关闭")?
            .map_err(|_| "企业微信连接在订阅时失败".to_string())?;
        match frame {
            Message::Ping(payload) => {
                write
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| "企业微信连接在订阅时失败")?;
            }
            Message::Close(_) => return Err("企业微信连接在订阅时关闭".into()),
            other => {
                let Some(text) = frame_text(other) else {
                    continue;
                };
                let Some(payload) = parse_payload(&text) else {
                    continue;
                };
                if payload.get("cmd").and_then(Value::as_str) == Some("ping") {
                    continue;
                }
                if req_of(&payload) != subscribe_id {
                    continue;
                }
                if let Some(code) = failure_code(&payload) {
                    set_status(
                        &ctx.status,
                        ConnectionState::Error,
                        format!("企业微信拒绝了机器人凭证 (errcode={code})"),
                    )
                    .await;
                    return Err(format!("fatal: 企业微信拒绝了机器人凭证 (errcode={code})"));
                }
                break;
            }
        }
    }
    let (write_tx, mut write_rx) = mpsc::channel(64);
    let writer = tokio::spawn(async move {
        while let Some(message) = write_rx.recv().await {
            let sent = match message {
                OutMsg::Text(payload) => write.send(text_message(payload)).await,
                OutMsg::Pong(payload) => write.send(Message::Pong(payload.into())).await,
                OutMsg::Close => {
                    let _ = write.send(Message::Close(None)).await;
                    break;
                }
            };
            if sent.is_err() {
                break;
            }
        }
    });
    let (stop_tx, _) = watch::channel(false);
    let gateway = Arc::new(Gateway {
        app: ctx.app.clone(),
        inbound: ctx.inbound.clone(),
        status: ctx.status.clone(),
        write_tx: write_tx.clone(),
        state: Mutex::new(GwState {
            pending: HashMap::new(),
            replies: HashMap::new(),
            groups: HashSet::new(),
            req_ids: HashMap::new(),
            req_order: VecDeque::new(),
            book: StreamBook::default(),
            buckets: HashMap::new(),
        }),
        origin: Instant::now(),
        stop: stop_tx,
        gates: Mutex::new(HashMap::new()),
        workers: Mutex::new(HashMap::new()),
    });
    set_status(&ctx.status, ConnectionState::Connected, "企业微信已连接").await;
    let reader_gateway = gateway.clone();
    let mut reader_stop = gateway.stop.subscribe();
    let reader_tx = write_tx.clone();
    let mut reader = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.tick().await;
        loop {
            tokio::select! {
                _ = async { let _ = reader_stop.wait_for(|stop| *stop).await; } => return Ok(()),
                _ = heartbeat.tick() => {
                    let ping = json!({"cmd":"ping","headers":{"req_id": format!("ping-{}", Uuid::new_v4().simple())},"body":{}}).to_string();
                    if reader_tx.send(OutMsg::Text(ping)).await.is_err() { return Err("企业微信连接已断开".into()); }
                }
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(Message::Ping(payload))) => { let _ = reader_tx.send(OutMsg::Pong(payload.to_vec())).await; }
                        Some(Ok(Message::Close(_))) | None => return Err("企业微信连接已断开".into()),
                        Some(Ok(other)) => {
                            if let Some(text) = frame_text(other) {
                                if let Err(error) = reader_gateway.on_text(&text).await { return Err(error); }
                            }
                        }
                        Some(Err(_)) => return Err("企业微信连接已断开".into()),
                    }
                }
            }
        }
    });
    let mut shutdown = ctx.shutdown.clone();
    let result = loop {
        tokio::select! {
            end = &mut reader => break match end {
                Ok(result) => result,
                Err(_) => Err("企业微信连接已断开".into()),
            },
            message = ctx.outbound.recv() => match message {
                Some(message) => gateway.enqueue(message).await,
                None => break Ok(()),
            },
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break Ok(()); }
            }
        }
    };
    let _ = gateway.stop.send(true);
    let _ = write_tx.try_send(OutMsg::Close);
    writer.abort();
    reader.abort();
    if let Err(error) = &result {
        set_status(
            &ctx.status,
            ConnectionState::Error,
            error.trim_start_matches("fatal:").trim(),
        )
        .await;
    }
    result
}

impl WecomRpc for Gateway {
    async fn command(&self, cmd: &str, body: Value) -> Result<Value, String> {
        self.request(cmd, body).await
    }
}

impl Gateway {
    async fn enqueue(self: &Arc<Self>, message: OutboundMessage) {
        let chat_id = message.chat_id.clone();
        let sender = {
            let mut workers = self.workers.lock();
            if let Some(sender) = workers.get(&chat_id) {
                sender.clone()
            } else {
                let (tx, rx) = mpsc::channel(32);
                workers.insert(chat_id, tx.clone());
                let gateway = Arc::clone(self);
                tokio::spawn(async move {
                    gateway.chat_worker(rx).await;
                });
                tx
            }
        };
        let _ = sender.send(message).await;
    }

    async fn chat_worker(self: Arc<Self>, mut rx: mpsc::Receiver<OutboundMessage>) {
        let mut stop = self.stop.subscribe();
        loop {
            tokio::select! {
                _ = async { let _ = stop.wait_for(|stop| *stop).await; } => break,
                message = rx.recv() => {
                    let Some(message) = message else { break };
                    if let Err(error) = self.handle_out(message).await {
                        let _ = self.status.send(StatusUpdate { platform: ImPlatform::Wecom, state: ConnectionState::Error, message: error }).await;
                    }
                }
            }
        }
    }

    fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    async fn handle_out(&self, message: OutboundMessage) -> Result<(), String> {
        let (is_group, req_id) = {
            let state = self.state.lock();
            let req = req_for(&state, &message.chat_id, message.reply_token.as_deref());
            (is_group_chat(&state, &message.chat_id), req)
        };
        if let Some(stream_id) = message.stream_id.clone().filter(|id| !id.is_empty()) {
            let cmds = {
                let mut state = self.state.lock();
                state.book.push(
                    self.now_ms(),
                    &message.chat_id,
                    &stream_id,
                    req_id.as_deref(),
                    is_group,
                    &message.text,
                    message.finished,
                )?
            };
            for cmd in cmds {
                self.dispatch(&message.chat_id, is_group, cmd).await?;
            }
        } else if !message.text.is_empty() {
            self.send_text(&message.chat_id, is_group, req_id.as_deref(), &message.text)
                .await?;
        }
        for path in &message.attachments {
            self.send_attachment(&message.chat_id, is_group, req_id.as_deref(), path)
                .await?;
        }
        Ok(())
    }

    async fn dispatch(&self, chat_id: &str, is_group: bool, cmd: WireCmd) -> Result<(), String> {
        match cmd {
            WireCmd::Proactive { content } => {
                self.send_text(chat_id, is_group, None, &content).await
            }
            WireCmd::Stream {
                req_id,
                bubble_id,
                content,
                finish,
            } => {
                if finish {
                    self.acquire_token(chat_id, true).await;
                }
                let fallback = content.clone();
                let body = json!({"msgtype":"stream","stream":{"id": bubble_id, "finish": finish, "content": content}});
                match self.respond(&req_id, body, finish, !finish).await {
                    Ok(FinalAck::Delivered) => Ok(()),
                    Ok(_) if finish && !is_group => {
                        self.send_text(chat_id, false, None, &fallback).await
                    }
                    Ok(_) if finish => Err("群聊回复窗口不可用，不能主动发送".into()),
                    Ok(_) => Ok(()),
                    Err(()) => Err("企业微信连接已断开".into()),
                }
            }
        }
    }

    async fn send_text(
        &self,
        chat_id: &str,
        is_group: bool,
        req_id: Option<&str>,
        text: &str,
    ) -> Result<(), String> {
        for item in plan_text(is_group, req_id, text)? {
            self.acquire_token(chat_id, false).await;
            match item {
                TextSend::Passive { req_id, content } => {
                    let body = json!({"msgtype":"markdown","markdown":{"content": content}});
                    match self.respond(&req_id, body, true, false).await {
                        Ok(FinalAck::Delivered) => {}
                        Ok(_) => return Err("群聊回复窗口不可用，不能主动发送".into()),
                        Err(()) => return Err("企业微信连接已断开".into()),
                    }
                }
                TextSend::Proactive { content } => {
                    if is_group {
                        return Err("群聊缺少 req_id，不能主动发送".into());
                    }
                    self.request("aibot_send_msg", json!({"chatid": chat_id, "msgtype":"markdown","markdown":{"content": content}})).await?;
                }
            }
        }
        Ok(())
    }

    async fn send_attachment(
        &self,
        chat_id: &str,
        is_group: bool,
        req_id: Option<&str>,
        path: &str,
    ) -> Result<(), String> {
        if is_group && req_id.unwrap_or("").is_empty() {
            return Err("群聊缺少 req_id，不能主动发送".into());
        }
        let (bytes, filename) = load_outbound(path).await?;
        let prepared = prepare_bytes(bytes, &filename)?;
        let media_id = upload_media(
            self,
            &prepared.data,
            &prepared.media_type,
            &prepared.filename,
        )
        .await?;
        let mut body = serde_json::Map::new();
        body.insert("msgtype".into(), json!(prepared.media_type));
        body.insert(prepared.media_type.clone(), json!({"media_id": media_id}));
        self.acquire_token(chat_id, false).await;
        if is_group {
            let req_id = req_id.unwrap_or("");
            match self.respond(req_id, Value::Object(body), true, false).await {
                Ok(FinalAck::Delivered) => {}
                Ok(_) => return Err("群聊回复窗口不可用，不能主动发送".into()),
                Err(()) => return Err("企业微信连接已断开".into()),
            }
        } else {
            body.insert("chatid".into(), json!(chat_id));
            self.request("aibot_send_msg", Value::Object(body)).await?;
        }
        if let Some(note) = prepared.downgrade {
            self.send_text(chat_id, is_group, req_id, &note).await?;
        }
        Ok(())
    }

    async fn acquire_token(&self, chat_id: &str, control: bool) {
        let gate = {
            let mut gates = self.gates.lock();
            gates
                .entry(chat_id.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _chat = gate.lock().await;
        loop {
            let wait = {
                let mut state = self.state.lock();
                let bucket = state
                    .buckets
                    .entry(chat_id.to_owned())
                    .or_insert_with(|| Bucket {
                        normal: 0,
                        reserved: 0,
                        window: Instant::now(),
                    });
                let now = Instant::now();
                if now.duration_since(bucket.window) >= Duration::from_secs(60) {
                    bucket.normal = 0;
                    bucket.reserved = 0;
                    bucket.window = now;
                }
                if bucket.normal < NORMAL_TOKENS {
                    bucket.normal += 1;
                    return;
                }
                if control && bucket.reserved < RESERVED_TOKENS {
                    bucket.reserved += 1;
                    return;
                }
                Duration::from_secs(60).saturating_sub(now.duration_since(bucket.window))
            };
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }
    }

    async fn request(&self, cmd: &str, body: Value) -> Result<Value, String> {
        let req_id = format!("{cmd}-{}", Uuid::new_v4().simple());
        let (tx, rx) = oneshot::channel();
        self.state.lock().pending.insert(req_id.clone(), tx);
        let payload = json!({"cmd": cmd, "headers": {"req_id": req_id}, "body": body}).to_string();
        if self.write_tx.send(OutMsg::Text(payload)).await.is_err() {
            self.state.lock().pending.remove(&req_id);
            return Err("企业微信连接已断开".into());
        }
        let value = match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(value)) => value,
            _ => {
                self.state.lock().pending.remove(&req_id);
                return Err("企业微信请求超时".into());
            }
        };
        if let Some(code) = failure_code(&value) {
            if code == 846609 {
                let mut state = self.state.lock();
                state.req_ids.clear();
                state.req_order.clear();
            }
            return Err(format!("企业微信请求失败 (errcode={code})"));
        }
        Ok(value)
    }

    async fn respond(
        &self,
        req_id: &str,
        body: Value,
        wait_final: bool,
        skip_if_pending: bool,
    ) -> Result<FinalAck, ()> {
        if *self.stop.borrow() {
            return Err(());
        }
        if skip_if_pending
            && self
                .state
                .lock()
                .replies
                .get(req_id)
                .is_some_and(|slot| slot.pending)
        {
            return Ok(FinalAck::Delivered);
        }
        if wait_final {
            self.drain_reply(req_id).await;
        }
        let mut rx = self.reply_receiver(req_id);
        let start = *rx.borrow();
        if let Some(slot) = self.state.lock().replies.get_mut(req_id) {
            slot.pending = true;
        }
        let payload = json!({"cmd":"aibot_respond_msg","headers":{"req_id": req_id},"body": body})
            .to_string();
        if self.write_tx.send(OutMsg::Text(payload)).await.is_err() {
            if let Some(slot) = self.state.lock().replies.get_mut(req_id) {
                slot.pending = false;
            }
            return Err(());
        }
        if !wait_final {
            return Ok(FinalAck::Delivered);
        }
        let acked = tokio::time::timeout(
            Duration::from_secs(15),
            rx.wait_for(move |seq| *seq != start),
        )
        .await;
        let code = self
            .state
            .lock()
            .replies
            .get(req_id)
            .map(|slot| slot.code)
            .unwrap_or(0);
        if let Some(slot) = self.state.lock().replies.get_mut(req_id) {
            slot.pending = false;
        }
        Ok(match acked {
            Ok(Ok(_)) => classify_final_ack(code, false),
            _ => classify_final_ack(0, true),
        })
    }

    async fn drain_reply(&self, req_id: &str) {
        let pending = self
            .state
            .lock()
            .replies
            .get(req_id)
            .is_some_and(|slot| slot.pending);
        if !pending {
            return;
        }
        let mut rx = self.reply_receiver(req_id);
        let start = *rx.borrow();
        let _ = tokio::time::timeout(
            Duration::from_secs(15),
            rx.wait_for(move |seq| *seq != start),
        )
        .await;
        if let Some(slot) = self.state.lock().replies.get_mut(req_id) {
            slot.pending = false;
        }
    }

    fn reply_receiver(&self, req_id: &str) -> watch::Receiver<u64> {
        let mut state = self.state.lock();
        state
            .replies
            .entry(req_id.to_owned())
            .or_insert_with(ReplySlot::new)
            .tx
            .subscribe()
    }

    async fn on_text(&self, text: &str) -> Result<(), String> {
        let Some(payload) = parse_payload(text) else {
            return Ok(());
        };
        let cmd = payload.get("cmd").and_then(Value::as_str).unwrap_or("");
        if cmd == "aibot_event_callback" {
            if is_kick(&payload) {
                return Err("fatal: 企业微信连接被其他客户端踢下线，已停止重连".into());
            }
            return Ok(());
        }
        if cmd == "ping" {
            return Ok(());
        }
        if cmd == "aibot_msg_callback" || cmd == "aibot_callback" {
            self.on_callback(payload);
            return Ok(());
        }
        let req_id = req_of(&payload);
        let code = code_of(&payload);
        let mut state = self.state.lock();
        if let Some(slot) = state.replies.get_mut(&req_id) {
            if slot.pending {
                slot.code = code;
                slot.pending = false;
                slot.seq = slot.seq.wrapping_add(1);
                let _ = slot.tx.send(slot.seq);
                return Ok(());
            }
        }
        if let Some(tx) = state.pending.remove(&req_id) {
            let _ = tx.send(payload);
        }
        Ok(())
    }

    fn on_callback(&self, payload: Value) {
        let req_id = req_of(&payload);
        let Some(body) = payload.get("body").filter(|body| body.is_object()).cloned() else {
            return;
        };
        let sender = body.get("from").cloned().unwrap_or(Value::Null);
        let user_id = text_of(&sender, "userid");
        let user_name = {
            let name = text_of(&sender, "name");
            if name.is_empty() {
                user_id.clone()
            } else {
                name
            }
        };
        let mut chat_id = text_of(&body, "chatid");
        if chat_id.is_empty() {
            chat_id = user_id.clone();
        }
        if chat_id.is_empty() {
            return;
        }
        let is_group = text_of(&body, "chattype").eq_ignore_ascii_case("group");
        let mut text = message_text(&body);
        if is_group {
            text = strip_mention(&text);
        }
        let message_id = {
            let id = text_of(&body, "msgid");
            if id.is_empty() {
                if req_id.is_empty() {
                    Uuid::new_v4().simple().to_string()
                } else {
                    req_id.clone()
                }
            } else {
                id
            }
        };
        let refs = refs_from_body(&body);
        {
            let mut state = self.state.lock();
            if is_group {
                state.groups.insert(chat_id.clone());
            }
            remember(&mut state, &chat_id, &req_id);
            state.book.note_inbound(&chat_id);
        }
        if text.is_empty() && refs.is_empty() {
            return;
        }
        let app = self.app.clone();
        let inbound = self.inbound.clone();
        let reply_token = if req_id.is_empty() {
            None
        } else {
            Some(req_id)
        };
        tokio::spawn(async move {
            let mut attachments = Vec::new();
            for media in refs {
                if let Some(path) = store_inbound(&app, &message_id, &media).await {
                    attachments.push(path);
                }
            }
            if text.is_empty() && attachments.is_empty() {
                return;
            }
            let _ = inbound
                .send(InboundMessage {
                    platform: ImPlatform::Wecom,
                    message_id,
                    chat_id,
                    user_id,
                    user_name,
                    is_group,
                    thread_id: None,
                    reply_token,
                    text,
                    attachments,
                })
                .await;
        });
    }
}

fn remember(state: &mut GwState, chat_id: &str, req_id: &str) {
    if chat_id.is_empty() || req_id.is_empty() {
        return;
    }
    if state.req_ids.len() >= 1000 && !state.req_ids.contains_key(chat_id) {
        if let Some(old) = state.req_order.pop_front() {
            state.req_ids.remove(&old);
        }
    }
    state.req_ids.insert(chat_id.to_owned(), req_id.to_owned());
    state.req_order.retain(|chat| chat != chat_id);
    state.req_order.push_back(chat_id.to_owned());
}

fn req_for(state: &GwState, chat_id: &str, reply_token: Option<&str>) -> Option<String> {
    reply_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .or_else(|| state.req_ids.get(chat_id).cloned())
}

fn is_group_chat(state: &GwState, chat_id: &str) -> bool {
    state.groups.contains(chat_id) || chat_id.to_ascii_lowercase().starts_with("group")
}

fn message_text(body: &Value) -> String {
    let msgtype = text_of(body, "msgtype").to_ascii_lowercase();
    let mut parts = Vec::new();
    if msgtype == "mixed" {
        if let Some(items) = body.pointer("/mixed/msg_item").and_then(Value::as_array) {
            for item in items {
                if text_of(item, "msgtype").eq_ignore_ascii_case("text") {
                    let text = item
                        .pointer("/text/content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_owned();
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
            }
        }
    } else if let Some(text) = body.pointer("/text/content").and_then(Value::as_str) {
        let text = text.trim();
        if !text.is_empty() {
            parts.push(text.to_owned());
        }
    }
    if msgtype == "voice" {
        if let Some(text) = body.pointer("/voice/content").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                parts.push(text.to_owned());
            }
        }
    }
    let mut text = parts.join("\n");
    if let Some(quote) = body.get("quote") {
        let quote_type = text_of(quote, "msgtype").to_ascii_lowercase();
        let quoted = quote
            .pointer(&format!("/{quote_type}/content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !quoted.is_empty() {
            text = if text.is_empty() {
                quoted.to_owned()
            } else {
                format!("引用：{quoted}\n{text}")
            };
        }
    }
    text
}

fn strip_mention(text: &str) -> String {
    let text = text.trim_start();
    let Some(rest) = text.strip_prefix('@') else {
        return text.trim().to_owned();
    };
    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    rest[token_end..].trim_start().to_owned()
}

fn text_message(payload: String) -> Message {
    Message::Text(payload.into())
}

fn frame_text(message: Message) -> Option<String> {
    match message {
        Message::Text(text) => Some(text.to_string()),
        Message::Binary(bytes) => Some(String::from_utf8_lossy(bytes.as_ref()).into_owned()),
        _ => None,
    }
}

fn parse_payload(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(Value::is_object)
        .or_else(|| {
            let cleaned: String = text
                .chars()
                .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
                .collect();
            serde_json::from_str(&cleaned).ok().filter(Value::is_object)
        })
}

fn req_of(payload: &Value) -> String {
    payload
        .pointer("/headers/req_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn text_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn failure_code(value: &Value) -> Option<i64> {
    [value.get("errcode"), value.pointer("/body/errcode")]
        .into_iter()
        .flatten()
        .find_map(Value::as_i64)
        .filter(|code| *code != 0)
}

fn code_of(value: &Value) -> i64 {
    failure_code(value).unwrap_or(0)
}

fn is_kick(payload: &Value) -> bool {
    payload
        .pointer("/body/event_type")
        .and_then(Value::as_str)
        .or_else(|| payload.pointer("/body/event").and_then(Value::as_str))
        == Some("disconnected_event")
}

async fn set_status(
    status: &mpsc::Sender<StatusUpdate>,
    state: ConnectionState,
    message: impl Into<String>,
) {
    let _ = status
        .send(StatusUpdate {
            platform: ImPlatform::Wecom,
            state,
            message: message.into(),
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_without_req_id_cannot_send_proactively() {
        let error = plan_text(true, None, "hello group").unwrap_err();
        assert!(error.contains("req_id"));
        assert!(error.contains("不能主动"));
        assert!(matches!(
            plan_text(true, Some("req-1"), "hello").unwrap().as_slice(),
            [TextSend::Passive { .. }]
        ));
        assert!(matches!(
            plan_text(false, None, "hello").unwrap().as_slice(),
            [TextSend::Proactive { .. }]
        ));
    }

    #[test]
    fn stream_window_does_not_finish_and_group_does_not_go_proactive() {
        let mut book = StreamBook::default();
        book.push(0, "group-1", "bubble-1", Some("req-1"), true, "部分", false)
            .unwrap();
        let error = book
            .push(
                STREAM_SAFE_MS,
                "group-1",
                "bubble-1",
                Some("req-1"),
                true,
                "完整答案",
                true,
            )
            .unwrap_err();
        assert!(error.contains("不能主动"));
        assert!(book.finalized.contains("bubble-1"));

        let mut direct = StreamBook::default();
        direct
            .push(0, "dm-1", "bubble-2", Some("req-2"), false, "部分", false)
            .unwrap();
        let cmds = direct
            .push(
                STREAM_SAFE_MS,
                "dm-1",
                "bubble-2",
                Some("req-2"),
                false,
                "完整答案",
                true,
            )
            .unwrap();
        assert!(cmds
            .iter()
            .all(|cmd| !matches!(cmd, WireCmd::Stream { finish: true, .. })));
        assert!(matches!(cmds.as_slice(), [WireCmd::Proactive { .. }]));
    }

    #[test]
    fn duplicate_finalizer_emits_one_finish_frame() {
        let mut book = StreamBook::default();
        book.push(0, "dm-1", "bubble-3", Some("req-3"), false, "答案", false)
            .unwrap();
        let first = book
            .push(
                1_000,
                "dm-1",
                "bubble-3",
                Some("req-3"),
                false,
                "答案",
                true,
            )
            .unwrap();
        let finish: Vec<_> = first
            .into_iter()
            .filter(|cmd| matches!(cmd, WireCmd::Stream { finish: true, .. }))
            .collect();
        assert_eq!(finish.len(), 1);
        if let WireCmd::Stream { content, .. } = &finish[0] {
            assert!(content.starts_with("答案") && content != "答案");
        }
        assert!(book
            .push(
                2_000,
                "dm-1",
                "bubble-3",
                Some("req-3"),
                false,
                "答案",
                true
            )
            .unwrap()
            .is_empty());
        assert_eq!(classify_final_ack(6000, false), FinalAck::Delivered);
        assert_eq!(classify_final_ack(846608, false), FinalAck::Expired);
    }

    #[test]
    fn stream_truncation_keeps_unicode_boundaries() {
        assert_eq!(truncate_utf8("你你", 5), "你");
        assert!(truncate_utf8(&"你".repeat(10_000), MAX_STREAM_BYTES).len() <= MAX_STREAM_BYTES);
    }
}
