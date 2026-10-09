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

const EDIT_GAP: Duration = Duration::from_millis(200);

struct HeldPreview {
    message: OutboundMessage,
    not_before: tokio::time::Instant,
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
                    abort_join(worker).await;
                    return Ok(());
                }
                endpoint = runtime.api.endpoint() => endpoint,
            };
            let endpoint = match endpoint {
                Ok(endpoint) => endpoint,
                Err(error) => {
                    abort_join(worker).await;
                    return Err(error);
                }
            };
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
                        abort_join(worker).await;
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
                let _ = transport.await;
                let _ = worker.await;
                return Ok(());
            }
            ready = ready_rx => match ready {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    worker.abort();
                    transport.abort();
                    let _ = worker.await;
                    let _ = transport.await;
                    ctx.set_status(ImPlatform::Feishu, ConnectionState::Error, error.trim_start_matches("fatal:").trim()).await;
                    return Err(error);
                }
                Err(_) => {
                    worker.abort();
                    transport.abort();
                    let _ = worker.await;
                    let _ = transport.await;
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
    let sender_rt = runtime.clone();
    let sender_stop = ctx.shutdown.clone();
    let outbound = ctx.outbound;
    let sender = tokio::spawn(async move {
        pump(
            &sender_rt.api,
            &sender_rt.status,
            &sender_rt.fatal,
            outbound,
            sender_stop,
        )
        .await;
    });
    let mut transport_done = false;
    let result = loop {
        tokio::select! {
            biased;
            _ = stopped(&mut shutdown) => break Ok(()),
            error = fatal_rx.recv() => break Err(error.unwrap_or_else(|| "飞书连接已结束".into())),
            joined = &mut transport => {
                transport_done = true;
                break match joined {
                    Ok(result) => result,
                    Err(_) => Err("飞书连接任务已中断".into()),
                }
            }
        }
    };
    sender.abort();
    worker.abort();
    if !transport_done {
        transport.abort();
    }
    let _ = sender.await;
    let _ = worker.await;
    if !transport_done {
        let _ = transport.await;
    }
    result
}

async fn abort_join<T>(task: tokio::task::JoinHandle<T>) {
    task.abort();
    let _ = task.await;
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

async fn pump(
    api: &Api,
    status: &mpsc::Sender<StatusUpdate>,
    fatal: &mpsc::Sender<String>,
    mut out_rx: mpsc::Receiver<OutboundMessage>,
    mut stop: watch::Receiver<bool>,
) {
    let shutdown = stop.clone();
    let mut bubbles = HashMap::new();
    let mut held: HashMap<String, HeldPreview> = HashMap::new();
    let mut last_edit: HashMap<String, tokio::time::Instant> = HashMap::new();
    loop {
        let deadline = held.values().map(|item| item.not_before).min();
        let mut incoming = Vec::new();
        let mut closed = false;
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => break,
            _ = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            }, if deadline.is_some() => {}
            msg = out_rx.recv() => match msg {
                Some(msg) => incoming.push(msg),
                None => closed = true,
            },
        }
        while let Ok(msg) = out_rx.try_recv() {
            incoming.push(msg);
        }
        let now = tokio::time::Instant::now();
        let release_all = !incoming.is_empty() || closed;
        let mut batch = Vec::new();
        if release_all {
            batch.extend(held.drain().map(|(_, item)| item.message));
        } else {
            let due: Vec<String> = held
                .iter()
                .filter(|(_, item)| item.not_before <= now)
                .map(|(key, _)| key.clone())
                .collect();
            for key in due {
                if let Some(item) = held.remove(&key) {
                    batch.push(item.message);
                }
            }
        }
        batch.append(&mut incoming);
        if batch.is_empty() {
            if closed {
                break;
            }
            continue;
        }
        for msg in coalesce(batch) {
            if let Some(at) = hold_until(&msg, &last_edit, tokio::time::Instant::now()) {
                if let Some(stream) = msg.stream_id.clone() {
                    held.insert(
                        stream,
                        HeldPreview {
                            message: msg,
                            not_before: at,
                        },
                    );
                    continue;
                }
            }
            let stream = msg.stream_id.clone();
            let finished = msg.finished;
            let sent = deliver(api, &shutdown, status, fatal, &mut bubbles, msg).await;
            if let Some(stream) = stream {
                if finished {
                    last_edit.remove(&stream);
                } else if sent {
                    last_edit.insert(stream, tokio::time::Instant::now());
                }
            }
        }
        if closed {
            break;
        }
    }
}

fn hold_until(
    msg: &OutboundMessage,
    last_edit: &HashMap<String, tokio::time::Instant>,
    now: tokio::time::Instant,
) -> Option<tokio::time::Instant> {
    if msg.finished {
        return None;
    }
    let stream = msg.stream_id.as_deref()?;
    let due = *last_edit.get(stream)? + EDIT_GAP;
    (now < due).then_some(due)
}

fn coalesce(batch: Vec<OutboundMessage>) -> Vec<OutboundMessage> {
    let mut out = Vec::new();
    let mut index = HashMap::<String, usize>::new();
    for msg in batch {
        let Some(stream) = msg.stream_id.clone() else {
            out.push(msg);
            continue;
        };
        if let Some(&slot) = index.get(&stream) {
            if out[slot].finished && !msg.finished {
                continue;
            }
            out[slot] = msg;
        } else {
            index.insert(stream, out.len());
            out.push(msg);
        }
    }
    out
}

async fn deliver(
    api: &Api,
    shutdown: &watch::Receiver<bool>,
    status: &mpsc::Sender<StatusUpdate>,
    fatal: &mpsc::Sender<String>,
    bubbles: &mut HashMap<String, Bubble>,
    msg: OutboundMessage,
) -> bool {
    if msg.chat_id.is_empty() || *shutdown.borrow() {
        return false;
    }
    let reply_to = msg.reply_to.clone().or(msg.reply_token.clone());
    let thread_id = msg
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    let in_thread = thread_id.is_some();
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
        if *shutdown.borrow() {
            return false;
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
            if let Err(error) = edit_text(api, id, &text).await {
                report(status, fatal, error);
                return false;
            }
            rendered = text;
            continue;
        }
        let reply = if in_thread || (matches!(kind, Some(SendKind::New)) && !replied) {
            reply_to.as_deref()
        } else {
            None
        };
        match send_text(
            api,
            shutdown,
            &msg.chat_id,
            reply,
            thread_id.as_deref(),
            &text,
        )
        .await
        {
            Ok(id) => {
                if message_id.is_none() {
                    message_id = Some(id);
                }
                rendered = text;
                replied = true;
            }
            Err(error) => {
                report(status, fatal, error);
                return false;
            }
        }
    }
    if (!streamed || msg.finished) && !files_sent {
        for path in &msg.attachments {
            if *shutdown.borrow() {
                return false;
            }
            let reply = if in_thread || !replied {
                reply_to.as_deref()
            } else {
                None
            };
            if let Err(error) = send_attachment(
                api,
                shutdown,
                &msg.chat_id,
                reply,
                thread_id.as_deref(),
                path,
            )
            .await
            {
                report(status, fatal, error);
                return false;
            }
            replied = true;
        }
        files_sent = true;
    }
    if streamed {
        if let Some(key) = msg.stream_id {
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
    }
    true
}

async fn send_text(
    api: &Api,
    shutdown: &watch::Receiver<bool>,
    chat_id: &str,
    reply_to: Option<&str>,
    thread_id: Option<&str>,
    text: &str,
) -> Result<String, String> {
    if text.is_empty() {
        return Err("空消息".into());
    }
    let (kind, payload) = outbound::payloads(text);
    match send_with_retry(api, shutdown, chat_id, reply_to, thread_id, kind, &payload).await {
        Err(error) if kind == "post" && outbound::post_rejected(&error) => {
            send_with_retry(
                api,
                shutdown,
                chat_id,
                reply_to,
                thread_id,
                "text",
                &outbound::plain_payload(text),
            )
            .await
        }
        other => other,
    }
}

async fn edit_text(api: &Api, message_id: &str, text: &str) -> Result<(), String> {
    let (kind, payload) = outbound::payloads(text);
    match api.update_message(message_id, kind, &payload).await {
        Err(error) if kind == "post" && outbound::post_rejected(&error) => {
            api.update_message(message_id, "text", &outbound::plain_payload(text))
                .await
        }
        other => other,
    }
}

async fn send_attachment(
    api: &Api,
    shutdown: &watch::Receiver<bool>,
    chat_id: &str,
    reply_to: Option<&str>,
    thread_id: Option<&str>,
    path: &str,
) -> Result<String, String> {
    let uploaded = api.upload(path).await?;
    let (kind, key) = match uploaded {
        Uploaded::Image(key) => ("image", key),
        Uploaded::File { key, kind } => (kind, key),
    };
    send_with_retry(
        api,
        shutdown,
        chat_id,
        reply_to,
        thread_id,
        kind,
        &outbound::media_payload(kind, &key),
    )
    .await
}

async fn send_with_retry(
    api: &Api,
    shutdown: &watch::Receiver<bool>,
    chat_id: &str,
    reply_to: Option<&str>,
    thread_id: Option<&str>,
    kind: &str,
    payload: &str,
) -> Result<String, String> {
    let mut last = "飞书消息发送失败".to_string();
    for attempt in 0..3 {
        if *shutdown.borrow() {
            return Err("飞书连接已关闭".into());
        }
        match api
            .send_message(chat_id, reply_to, thread_id, kind, payload)
            .await
        {
            Ok(id) => return Ok(id),
            Err(error) if error.starts_with("fatal:") || outbound::post_rejected(&error) => {
                return Err(error)
            }
            Err(error) => last = error,
        }
        if attempt < 2 {
            let mut stop = shutdown.clone();
            tokio::select! {
                biased;
                _ = stopped(&mut stop) => return Err("飞书连接已关闭".into()),
                _ = tokio::time::sleep(Duration::from_millis(200 * (attempt as u64 + 1))) => {}
            }
        }
    }
    Err(last)
}

fn report(status: &mpsc::Sender<StatusUpdate>, fatal: &mpsc::Sender<String>, error: String) {
    if error.starts_with("fatal:") {
        let _ = fatal.try_send(error);
        return;
    }
    let _ = status.try_send(StatusUpdate {
        platform: ImPlatform::Feishu,
        state: ConnectionState::Connected,
        message: format!("飞书消息发送失败：{error}"),
    });
}

#[cfg(test)]
mod tests {
    use super::api::mock_http::{spawn_mock, MockHit, MockOpts};
    use super::api::Api;
    use super::{coalesce, hold_until, pump};
    use crate::im::types::OutboundMessage;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::sync::{mpsc, watch};

    fn outbound(chat: &str, text: &str, stream: Option<&str>, finished: bool) -> OutboundMessage {
        OutboundMessage {
            chat_id: chat.into(),
            is_group: true,
            thread_id: None,
            reply_token: None,
            reply_to: Some("om_user".into()),
            text: text.into(),
            attachments: Vec::new(),
            stream_id: stream.map(str::to_owned),
            finished,
        }
    }

    fn bodies(hits: &[MockHit]) -> String {
        hits.iter()
            .map(|hit| hit.body.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn wait_for(hits: &std::sync::Arc<parking_lot::Mutex<Vec<MockHit>>>, needle: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if hits.lock().iter().any(|hit| hit.body.contains(needle)) {
                return;
            }
            if tokio::time::Instant::now() > deadline {
                let snapshot = hits.lock().clone();
                let joined = bodies(&snapshot);
                panic!("timed out waiting for {needle}\n{joined}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn coalesce_keeps_final_attachments_and_other_sessions() {
        let mut stale = Vec::new();
        for i in 0..5 {
            stale.push(outbound(
                "oc_a",
                &format!("old-version-{i}"),
                Some("stream-a"),
                false,
            ));
        }
        let mut final_msg = outbound("oc_a", "final-marker-unique", Some("stream-a"), true);
        final_msg.attachments = vec!["/tmp/answer.txt".into()];
        final_msg.thread_id = Some("omt_topic".into());
        let mut later = outbound("oc_a", "stale-after-final", Some("stream-a"), false);
        later.attachments = vec!["/tmp/should-not-replace".into()];
        let side = outbound("oc_b", "side-session-marker", None, true);
        let mut batch = stale;
        batch.push(final_msg);
        batch.push(side);
        batch.push(later);
        let planned = coalesce(batch);
        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].text, "final-marker-unique");
        assert_eq!(planned[0].attachments, vec!["/tmp/answer.txt".to_string()]);
        assert_eq!(planned[0].thread_id.as_deref(), Some("omt_topic"));
        assert!(planned[0].finished);
        assert_eq!(planned[1].text, "side-session-marker");
        assert_eq!(planned[1].chat_id, "oc_b");
    }

    #[test]
    fn edit_gap_defers_preview_but_not_final() {
        let mut last = HashMap::new();
        let now = tokio::time::Instant::now();
        last.insert("stream-a".into(), now);
        let preview = outbound("oc_a", "next", Some("stream-a"), false);
        let final_msg = outbound("oc_a", "done", Some("stream-a"), true);
        assert!(hold_until(&preview, &last, now).is_some());
        assert!(hold_until(&final_msg, &last, now).is_none());
        let side = outbound("oc_b", "side", None, true);
        assert!(hold_until(&side, &last, now).is_none());
    }

    #[tokio::test]
    async fn backlog_final_does_not_wait_for_every_old_edit() {
        let (base, mock) = spawn_mock(MockOpts {
            message_delay: Duration::from_millis(200),
            fail_reply_once: None,
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        let (status_tx, _status_rx) = mpsc::channel(8);
        let (fatal_tx, _fatal_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, out_rx) = mpsc::channel(32);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"attach-bytes-final").unwrap();
        for i in 0..20 {
            out_tx
                .send(outbound(
                    "oc_a",
                    &format!("old-version-{i}"),
                    Some("stream-a"),
                    false,
                ))
                .await
                .unwrap();
        }
        let mut final_msg = outbound("oc_a", "final-marker-unique", Some("stream-a"), true);
        final_msg.attachments = vec![file.to_string_lossy().into_owned()];
        out_tx.send(final_msg).await.unwrap();
        out_tx
            .send(outbound("oc_b", "side-session-marker", None, true))
            .await
            .unwrap();
        drop(out_tx);
        let started = std::time::Instant::now();
        pump(&api, &status_tx, &fatal_tx, out_rx, stop_rx).await;
        let elapsed = started.elapsed();
        let hits = mock.hits.lock().clone();
        let joined = bodies(&hits);
        assert!(joined.contains("final-marker-unique"), "{joined}");
        assert!(joined.contains("side-session-marker"), "{joined}");
        assert!(!joined.contains("old-version-0"), "{joined}");
        assert!(joined.contains("attach-bytes-final"), "{joined}");
        assert!(joined.contains("fk_mock"), "{joined}");
        assert!(
            elapsed < Duration::from_millis(2500),
            "final waited on the backlog: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn edit_gap_does_not_block_final_or_another_session() {
        let (base, mock) = spawn_mock(MockOpts {
            message_delay: Duration::ZERO,
            fail_reply_once: None,
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        let (status_tx, _status_rx) = mpsc::channel(8);
        let (fatal_tx, _fatal_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, out_rx) = mpsc::channel(32);
        let pump_task = tokio::spawn(async move {
            pump(&api, &status_tx, &fatal_tx, out_rx, stop_rx).await;
        });
        out_tx
            .send(outbound("oc_a", "preview-one", Some("stream-a"), false))
            .await
            .unwrap();
        wait_for(&mock.hits, "preview-one").await;
        let marked = std::time::Instant::now();
        out_tx
            .send(outbound("oc_a", "preview-two", Some("stream-a"), false))
            .await
            .unwrap();
        out_tx
            .send(outbound("oc_b", "side-session-marker", None, true))
            .await
            .unwrap();
        wait_for(&mock.hits, "side-session-marker").await;
        assert!(
            marked.elapsed() < Duration::from_millis(500),
            "{:?}",
            marked.elapsed()
        );
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"attach-bytes-final").unwrap();
        let mut final_msg = outbound("oc_a", "final-after-gap", Some("stream-a"), true);
        final_msg.attachments = vec![file.to_string_lossy().into_owned()];
        out_tx.send(final_msg).await.unwrap();
        wait_for(&mock.hits, "final-after-gap").await;
        wait_for(&mock.hits, "attach-bytes-final").await;
        assert!(
            marked.elapsed() < Duration::from_millis(700),
            "{:?}",
            marked.elapsed()
        );
        drop(out_tx);
        tokio::time::timeout(Duration::from_secs(2), pump_task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn thread_chunks_and_attachment_keep_reply_destination() {
        let (base, mock) = spawn_mock(MockOpts {
            message_delay: Duration::ZERO,
            fail_reply_once: None,
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        let (status_tx, _status_rx) = mpsc::channel(4);
        let (fatal_tx, _fatal_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, out_rx) = mpsc::channel(32);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"thread-file-bytes").unwrap();
        let mut text = "A".repeat(12_000);
        text.push('\n');
        text.push_str("TAIL-CHUNK-MARKER");
        let mut msg = outbound("oc_group", &text, None, true);
        msg.thread_id = Some("omt_topic".into());
        msg.reply_to = Some("om_root".into());
        msg.attachments = vec![file.to_string_lossy().into_owned()];
        out_tx.send(msg).await.unwrap();
        drop(out_tx);
        pump(&api, &status_tx, &fatal_tx, out_rx, stop_rx).await;
        let hits = mock.hits.lock().clone();
        let messages: Vec<_> = hits
            .iter()
            .filter(|hit| hit.path.contains("/im/v1/messages"))
            .collect();
        assert!(messages.len() >= 2);
        assert!(messages
            .iter()
            .all(|hit| hit.path.contains("/messages/om_root/reply")));
        assert!(messages
            .iter()
            .all(|hit| hit.body.contains("\"reply_in_thread\":true")));
        assert!(messages.iter().all(|hit| !hit.body.contains("oc_group")));
        let joined = bodies(&hits);
        assert!(joined.contains("TAIL-CHUNK-MARKER"), "{joined}");
        assert!(joined.contains("thread-file-bytes"), "{joined}");
        assert!(joined.contains("fk_mock"), "{joined}");
    }

    #[tokio::test]
    async fn thread_without_reply_target_is_not_a_group_send() {
        let (base, mock) = spawn_mock(MockOpts {
            message_delay: Duration::ZERO,
            fail_reply_once: None,
        })
        .await;
        let api = Api::new(&base, "cli_app", "secret").unwrap();
        let (status_tx, _status_rx) = mpsc::channel(4);
        let (fatal_tx, _fatal_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, out_rx) = mpsc::channel(32);
        let mut text = "B".repeat(12_000);
        text.push('\n');
        text.push_str("THREAD-TAIL");
        let mut msg = outbound("oc_group", &text, None, true);
        msg.thread_id = Some("omt_topic".into());
        msg.reply_to = None;
        msg.reply_token = None;
        out_tx.send(msg).await.unwrap();
        drop(out_tx);
        pump(&api, &status_tx, &fatal_tx, out_rx, stop_rx).await;
        let hits = mock.hits.lock().clone();
        let messages: Vec<_> = hits
            .iter()
            .filter(|hit| hit.path.contains("/im/v1/messages"))
            .collect();
        assert!(!messages.is_empty());
        for hit in messages {
            assert!(
                hit.path.contains("receive_id_type=thread_id"),
                "{}",
                hit.path
            );
            assert!(hit.body.contains("\"receive_id\":\"omt_topic\""));
            assert!(!hit.body.contains("oc_group"), "{}", hit.body);
        }
        assert!(bodies(&hits).contains("THREAD-TAIL"));
    }
}
