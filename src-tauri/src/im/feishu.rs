//! Feishu/Lark websocket and webhook transport.
//! Frame, decrypt, and event shapes follow lark-oapi 1.6.8 (Lark Technologies, MIT)
//! and Hermes Agent platform behavior (Nous Research, MIT). No Python runtime.

mod api;
mod crypto;
mod frame;
mod inbound;
mod outbound;
mod webhook;
mod ws;

use crate::im::common::{save_media, PlatformContext};
use crate::im::types::*;
use api::{Api, Uploaded};
use crypto::WebGuard;
use inbound::BotIdentity;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, watch};

struct Runtime {
    api: Api,
    bot: BotIdentity,
    require_mention: bool,
    app: tauri::AppHandle,
    inbound: mpsc::Sender<InboundMessage>,
    status: mpsc::Sender<StatusUpdate>,
    fatal: mpsc::Sender<String>,
    events: Mutex<WebGuard>,
    threads: Mutex<HashMap<String, ()>>,
    shutdown: watch::Receiver<bool>,
}

pub struct Hook {
    path: String,
    encrypt_key: String,
    verification_token: String,
    guard: Mutex<WebGuard>,
    runtime: Arc<Runtime>,
}

struct Bubble {
    message_id: String,
    rendered: String,
    tail_sent: bool,
    attachments_sent: bool,
}

enum SendKind {
    New,
    Append,
}

pub async fn run(
    config: FeishuConfig,
    credentials: CredentialInput,
    ctx: PlatformContext,
) -> Result<(), String> {
    if !*ctx.shutdown.borrow() && !config.enabled {
        ctx.set_status(ImPlatform::Feishu, ConnectionState::Disabled, "")
            .await;
        return Ok(());
    }
    if *ctx.shutdown.borrow() {
        return Ok(());
    }
    let app_id = config.app_id.trim();
    let secret = credentials.secret.trim();
    if app_id.is_empty() || secret.is_empty() {
        return fail(&ctx, "飞书缺少 App ID 或 App Secret").await;
    }
    if config.connection_mode == FeishuConnectionMode::Webhook
        && credentials.verification_token.trim().is_empty()
        && credentials.encrypt_key.trim().is_empty()
    {
        return fail(&ctx, "飞书 Webhook 需要 Verification Token 或 Encrypt Key").await;
    }
    let api = Api::new(base_url(config.domain), app_id, secret)?;
    let mut shutdown = ctx.shutdown.clone();
    let primed = tokio::select! {
        biased;
        _ = stopped(&mut shutdown) => return Ok(()),
        result = api.tenant_token() => result,
    };
    primed?;
    let bot = match api.bot_identity().await {
        Ok(bot) => bot,
        Err(error) if error.starts_with("fatal:") => {
            return fail(&ctx, error.trim_start_matches("fatal:").trim()).await
        }
        Err(_) => BotIdentity::default(),
    };
    let (fatal_tx, mut fatal_rx) = mpsc::channel(1);
    let runtime = Arc::new(Runtime {
        api,
        bot,
        require_mention: config.require_mention,
        app: ctx.app.clone(),
        inbound: ctx.inbound.clone(),
        status: ctx.status.clone(),
        fatal: fatal_tx,
        events: Mutex::new(WebGuard::default()),
        threads: Mutex::new(HashMap::new()),
        shutdown: ctx.shutdown.clone(),
    });
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (ack_tx, ack_rx) = mpsc::unbounded_channel();
    let worker = spawn_ws_worker(runtime.clone(), event_rx, ack_tx);
    let (ready_tx, ready_rx) = oneshot::channel();
    let transport = match config.connection_mode {
        FeishuConnectionMode::Websocket => {
            let endpoint = tokio::select! {
                biased;
                _ = stopped(&mut shutdown) => {
                    worker.abort();
                    return Ok(());
                }
                endpoint = runtime.api.endpoint() => endpoint,
            }?;
            let shutdown = ctx.shutdown.clone();
            tokio::spawn(async move {
                ws::connect(&endpoint, shutdown, event_tx, ack_rx, ready_tx).await
            })
        }
        FeishuConnectionMode::Webhook => {
            drop(event_tx);
            drop(ack_rx);
            drop(ready_tx);
            let host = if config.webhook.host.trim().is_empty() {
                "127.0.0.1".to_owned()
            } else {
                config.webhook.host.trim().to_owned()
            };
            let path = callback_path(&config.webhook.path);
            let listener =
                match tokio::net::TcpListener::bind((host.as_str(), config.webhook.port)).await {
                    Ok(listener) => listener,
                    Err(_) => {
                        worker.abort();
                        return Err(format!(
                            "飞书 Webhook 无法监听 {host}:{}",
                            config.webhook.port
                        ));
                    }
                };
            let hook = Arc::new(Hook {
                path,
                encrypt_key: credentials.encrypt_key.trim().to_owned(),
                verification_token: credentials.verification_token.trim().to_owned(),
                guard: Mutex::new(WebGuard::default()),
                runtime: runtime.clone(),
            });
            let shutdown = ctx.shutdown.clone();
            tokio::spawn(async move { webhook::serve(listener, hook, shutdown).await })
        }
    };
    tokio::pin!(transport);
    if config.connection_mode == FeishuConnectionMode::Websocket {
        tokio::select! {
            biased;
            _ = stopped(&mut shutdown) => {
                transport.abort();
                worker.abort();
                return Ok(());
            }
            ready = ready_rx => match ready {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    worker.abort();
                    let _ = (&mut transport).await;
                    ctx.set_status(ImPlatform::Feishu, ConnectionState::Error, error.trim_start_matches("fatal:").trim()).await;
                    return Err(error);
                }
                Err(_) => {
                    worker.abort();
                    return Err("飞书连接已中断".into());
                }
            }
        }
    }
    let note = if runtime.bot.open_id.is_empty() && config.require_mention {
        "飞书已连接（未取得机器人身份，群内仅响应 @所有人）"
    } else if config.connection_mode == FeishuConnectionMode::Webhook {
        "飞书 Webhook 已就绪"
    } else {
        "飞书长连接已就绪"
    };
    ctx.set_status(ImPlatform::Feishu, ConnectionState::Connected, note)
        .await;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let sender_rt = runtime.clone();
    let mut sender_stop = ctx.shutdown.clone();
    let sender = tokio::spawn(async move {
        let mut bubbles = HashMap::new();
        loop {
            tokio::select! {
                biased;
                _ = stopped(&mut sender_stop) => break,
                msg = out_rx.recv() => {
                    let Some(msg) = msg else { break };
                    deliver(&sender_rt, &mut bubbles, msg).await;
                }
            }
        }
    });
    let mut outbound = ctx.outbound;
    let result = loop {
        tokio::select! {
            biased;
            _ = stopped(&mut shutdown) => break Ok(()),
            error = fatal_rx.recv() => break Err(error.unwrap_or_else(|| "飞书连接已结束".into())),
            msg = outbound.recv() => match msg {
                Some(msg) => { let _ = out_tx.send(msg); }
                None => break Ok(()),
            },
            joined = &mut transport => break match joined {
                Ok(result) => result,
                Err(_) => Err("飞书连接任务已中断".into()),
            },
        }
    };
    drop(out_tx);
    sender.abort();
    worker.abort();
    transport.abort();
    result
}

fn spawn_ws_worker(
    rt: Arc<Runtime>,
    mut event_rx: mpsc::UnboundedReceiver<ws::Assembled>,
    ack_tx: mpsc::UnboundedSender<Vec<u8>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(item) = event_rx.recv().await {
            let value = serde_json::from_slice(&item.payload).unwrap_or(Value::Null);
            let code = accept(&rt, &value).await;
            let _ = ack_tx.send(frame::ack(&item.frame, code));
        }
    })
}

fn callback_path(path: &str) -> String {
    let path = path.trim();
    let path = if path.is_empty() {
        "/feishu/webhook"
    } else {
        path
    };
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

async fn fail(ctx: &PlatformContext, message: &str) -> Result<(), String> {
    ctx.set_status(ImPlatform::Feishu, ConnectionState::Error, message)
        .await;
    Err(format!("fatal: {message}"))
}

async fn stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

fn base_url(domain: FeishuDomain) -> &'static str {
    match domain {
        FeishuDomain::Feishu => "https://open.feishu.cn",
        FeishuDomain::Lark => "https://open.larksuite.com",
    }
}

async fn accept(rt: &Runtime, event: &Value) -> u16 {
    if *rt.shutdown.borrow() {
        return 200;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if rt
        .events
        .lock()
        .allow_event(&inbound::event_id(event), now)
        .is_err()
    {
        return 200;
    }
    let Some(mut draft) = inbound::draft(event, &rt.bot, rt.require_mention) else {
        return 200;
    };
    if draft.message.thread_id.is_some() {
        let mut threads = rt.threads.lock();
        if threads.len() >= 1024 {
            threads.clear();
        }
        threads.insert(draft.message.message_id.clone(), ());
    }
    let mut saved = 0usize;
    let mut failed = 0usize;
    for item in draft.downloads {
        if *rt.shutdown.borrow() {
            return 200;
        }
        let downloaded = tokio::time::timeout(
            Duration::from_secs(30),
            rt.api.download_resource(&draft.message.message_id, &item),
        )
        .await;
        match downloaded {
            Ok(Ok(bytes)) => match save_media(
                &rt.app,
                ImPlatform::Feishu,
                &draft.message.message_id,
                &item.name,
                &bytes,
            ) {
                Ok(path) => {
                    draft.message.attachments.push(path);
                    saved += 1;
                }
                Err(_) => failed += 1,
            },
            _ => failed += 1,
        }
    }
    let Some(text) = inbound::compose_text(&draft.message.text, saved, failed) else {
        return 200;
    };
    draft.message.text = text;
    let event_id = inbound::event_id(event);
    match rt.inbound.try_send(draft.message) {
        Ok(()) => 200,
        Err(_) => {
            rt.events.lock().forget_event(&event_id);
            500
        }
    }
}

async fn deliver(rt: &Runtime, bubbles: &mut HashMap<String, Bubble>, msg: OutboundMessage) {
    if msg.chat_id.is_empty() || *rt.shutdown.borrow() {
        return;
    }
    let reply_to = msg.reply_to.clone().or(msg.reply_token.clone());
    let in_thread = reply_to
        .as_deref()
        .is_some_and(|id| rt.threads.lock().contains_key(id));
    let streamed = msg.stream_id.is_some();
    let key = msg.stream_id.clone().unwrap_or_default();
    let (plan, mut message_id, mut rendered, mut files_sent, mut replied) = {
        let current = streamed.then(|| bubbles.get(&key)).flatten();
        let plan = outbound::steps(
            current.map(|bubble| bubble.rendered.as_str()),
            current.is_some_and(|bubble| bubble.tail_sent),
            &msg.text,
            msg.finished,
            streamed,
            outbound::CHUNK_BYTES,
        );
        let message_id = current.map(|bubble| bubble.message_id.clone());
        let rendered = current
            .map(|bubble| bubble.rendered.clone())
            .unwrap_or_default();
        let files_sent = current.is_some_and(|bubble| bubble.attachments_sent);
        let replied = message_id.is_some();
        (plan, message_id, rendered, files_sent, replied)
    };
    for step in plan.steps {
        if *rt.shutdown.borrow() {
            return;
        }
        let (text, kind) = match step {
            outbound::Step::Replace(text) => (text, None),
            outbound::Step::New(text) => (text, Some(SendKind::New)),
            outbound::Step::Append(text) => (text, Some(SendKind::Append)),
        };
        if kind.is_none() {
            let Some(id) = message_id.as_deref() else {
                continue;
            };
            if let Err(error) = edit_text(rt, id, &text).await {
                report(rt, error);
                return;
            }
            rendered = text;
            continue;
        }
        let reply = if matches!(kind, Some(SendKind::New)) && !replied {
            reply_to.as_deref()
        } else {
            None
        };
        match send_text(rt, &msg.chat_id, reply, in_thread, &text).await {
            Ok(id) => {
                if message_id.is_none() {
                    message_id = Some(id);
                }
                rendered = text;
                replied = true;
            }
            Err(error) => {
                report(rt, error);
                return;
            }
        }
    }
    if (!streamed || msg.finished) && !files_sent {
        for path in &msg.attachments {
            if *rt.shutdown.borrow() {
                return;
            }
            let reply = if !replied { reply_to.as_deref() } else { None };
            if let Err(error) = send_attachment(rt, &msg.chat_id, reply, in_thread, path).await {
                report(rt, error);
                return;
            }
            replied = true;
        }
        files_sent = true;
    }
    if !streamed {
        return;
    }
    let Some(key) = msg.stream_id else {
        return;
    };
    if plan.clear {
        bubbles.remove(&key);
    } else if let Some(id) = message_id {
        bubbles.insert(
            key,
            Bubble {
                message_id: id,
                rendered,
                tail_sent: plan.tail_sent,
                attachments_sent: files_sent,
            },
        );
    }
}

async fn send_text(
    rt: &Runtime,
    chat_id: &str,
    reply_to: Option<&str>,
    in_thread: bool,
    text: &str,
) -> Result<String, String> {
    if text.is_empty() {
        return Err("空消息".into());
    }
    let (kind, payload) = outbound::payloads(text);
    match send_with_retry(rt, chat_id, reply_to, in_thread, kind, &payload).await {
        Err(error) if kind == "post" && outbound::post_rejected(&error) => {
            send_with_retry(
                rt,
                chat_id,
                reply_to,
                in_thread,
                "text",
                &outbound::plain_payload(text),
            )
            .await
        }
        other => other,
    }
}

async fn edit_text(rt: &Runtime, message_id: &str, text: &str) -> Result<(), String> {
    let (kind, payload) = outbound::payloads(text);
    match rt.api.update_message(message_id, kind, &payload).await {
        Err(error) if kind == "post" && outbound::post_rejected(&error) => {
            rt.api
                .update_message(message_id, "text", &outbound::plain_payload(text))
                .await
        }
        other => other,
    }
}

async fn send_attachment(
    rt: &Runtime,
    chat_id: &str,
    reply_to: Option<&str>,
    in_thread: bool,
    path: &str,
) -> Result<String, String> {
    let uploaded = rt.api.upload(path).await?;
    let (kind, key) = match uploaded {
        Uploaded::Image(key) => ("image", key),
        Uploaded::File { key, kind } => (kind, key),
    };
    send_with_retry(
        rt,
        chat_id,
        reply_to,
        in_thread,
        kind,
        &outbound::media_payload(kind, &key),
    )
    .await
}

async fn send_with_retry(
    rt: &Runtime,
    chat_id: &str,
    reply_to: Option<&str>,
    in_thread: bool,
    kind: &str,
    payload: &str,
) -> Result<String, String> {
    let mut last = "飞书消息发送失败".to_string();
    for attempt in 0..3 {
        if *rt.shutdown.borrow() {
            return Err("飞书连接已关闭".into());
        }
        match rt
            .api
            .send_message(chat_id, reply_to, in_thread, kind, payload)
            .await
        {
            Ok(id) => return Ok(id),
            Err(error) if error.starts_with("fatal:") || outbound::post_rejected(&error) => {
                return Err(error)
            }
            Err(error) => last = error,
        }
        if attempt < 2 {
            let mut stop = rt.shutdown.clone();
            tokio::select! {
                biased;
                _ = stopped(&mut stop) => return Err("飞书连接已关闭".into()),
                _ = tokio::time::sleep(Duration::from_millis(200 * (attempt as u64 + 1))) => {}
            }
        }
    }
    Err(last)
}

fn report(rt: &Runtime, error: String) {
    if error.starts_with("fatal:") {
        let _ = rt.fatal.try_send(error);
        return;
    }
    let _ = rt.status.try_send(StatusUpdate {
        platform: ImPlatform::Feishu,
        state: ConnectionState::Connected,
        message: format!("飞书消息发送失败：{error}"),
    });
}
