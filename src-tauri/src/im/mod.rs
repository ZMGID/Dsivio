//! Native messaging gateway. Protocol adaptations: Hermes Agent, Nous Research, MIT.
pub mod commands;
pub mod common;
mod conversations;
mod credentials;
mod feishu;
mod onboarding;
pub mod types;
mod wecom;
mod wecom_callback;
mod wecom_crypto;
mod wecom_media;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tauri::{Emitter, Listener, Manager};
use tokio::sync::{mpsc, watch, Notify};
use types::*;

const PLATFORMS: [ImPlatform; 3] = [
    ImPlatform::Feishu,
    ImPlatform::Wecom,
    ImPlatform::WecomCallback,
];
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn key(parts: &[&str]) -> String {
    serde_json::to_string(parts).expect("string arrays serialize")
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    approved: Vec<ImApprovedUser>,
    pending: Vec<PendingPair>,
    sessions: BTreeMap<String, String>,
    seen: BTreeMap<String, i64>,
}
#[derive(Clone, Serialize, Deserialize)]
struct PendingPair {
    identity: String,
    request: ImPairingRequest,
}
struct State {
    store: Store,
    status: BTreeMap<ImPlatform, ImStatus>,
    epoch: BTreeMap<ImPlatform, u64>,
    revisions: BTreeMap<ImPlatform, u64>,
    outputs: BTreeMap<ImPlatform, mpsc::Sender<OutboundMessage>>,
}
pub struct ImRuntime {
    dir: PathBuf,
    state: Mutex<State>,
    changed: Notify,
    stop: watch::Sender<bool>,
    runner: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    setup: onboarding::SetupFlows,
}
impl ImRuntime {
    pub fn load(dir: PathBuf) -> Result<Self, String> {
        let path = dir.join("state.json");
        let store = match std::fs::read(&path) {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|_| "IM 状态文件损坏，请备份后修复；未开放机器人访问")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(_) => return Err("无法读取 IM 状态文件".into()),
        };
        let (stop, _) = watch::channel(false);
        let status = PLATFORMS
            .into_iter()
            .map(|platform| {
                (
                    platform,
                    ImStatus {
                        platform,
                        state: ConnectionState::Disabled,
                        message: String::new(),
                        credentials_configured: false,
                        webhook_url: String::new(),
                        last_message_at: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            dir,
            state: Mutex::new(State {
                store,
                status,
                epoch: BTreeMap::new(),
                revisions: BTreeMap::new(),
                outputs: BTreeMap::new(),
            }),
            changed: Notify::new(),
            stop,
            runner: Mutex::new(None),
            setup: Default::default(),
        })
    }
    fn transact<T>(
        &self,
        action: impl FnOnce(&mut Store) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.state.lock();
        let mut next = state.store.clone();
        let result = action(&mut next)?;
        let data = serde_json::to_string(&next).map_err(|_| "无法编码 IM 状态")?;
        crate::chat::storage::atomic_write(&self.dir.join("state.json"), &data, "IM state")?;
        state.store = next;
        Ok(result)
    }
    fn update_status(&self, app: &tauri::AppHandle, epoch: u64, change: StatusUpdate) {
        let mut state = self.state.lock();
        if state.epoch.get(&change.platform) != Some(&epoch) {
            return;
        }
        if let Some(status) = state.status.get_mut(&change.platform) {
            status.state = change.state;
            status.message = change.message;
        }
        drop(state);
        let _ = app.emit("im-status-changed", ());
    }
    fn touch(&self, app: &tauri::AppHandle, platform: ImPlatform) {
        if let Some(status) = self.state.lock().status.get_mut(&platform) {
            status.last_message_at = Some(now());
        }
        let _ = app.emit("im-status-changed", ());
    }
    pub fn reconnect(&self, platform: ImPlatform) {
        let mut state = self.state.lock();
        *state.revisions.entry(platform).or_default() += 1;
        drop(state);
        self.changed.notify_one();
    }
    pub async fn shutdown(&self) {
        self.stop.send_replace(true);
        self.changed.notify_waiters();
        let runner = self.runner.lock().take();
        if let Some(runner) = runner {
            let _ = runner.await;
        }
    }
    fn claim(&self, config: &ImConfig, msg: &InboundMessage) -> Result<bool, String> {
        if msg.message_id.is_empty() || msg.user_id.is_empty() || msg.chat_id.is_empty() {
            return Err("IM 消息缺少可信身份或消息 ID".into());
        }
        let id = key(&[
            msg.platform.key(),
            config.identity(msg.platform),
            &msg.message_id,
        ]);
        let ttl = if msg.platform == ImPlatform::Feishu {
            86400
        } else {
            300
        };
        self.transact(|s| {
            s.seen.retain(|_, expiry| *expiry > now());
            if s.seen.contains_key(&id) {
                return Ok(false);
            }
            if s.seen.len() >= 10000 {
                return Err("IM 去重队列已满，请稍后重试".into());
            }
            s.seen.insert(id, now() + ttl);
            Ok(true)
        })
    }
    fn authorize(&self, config: &ImConfig, msg: &InboundMessage) -> bool {
        let state = self.state.lock();
        authorized(config, msg, &state.store.approved)
    }
    fn pair(
        &self,
        config: &ImConfig,
        msg: &InboundMessage,
    ) -> Result<Option<ImPairingRequest>, String> {
        let identity = config.identity(msg.platform);
        self.transact(|s| {
            s.pending.retain(|p| p.request.expires_at > now());
            if s.pending.iter().any(|p| {
                p.identity == identity
                    && p.request.platform == msg.platform
                    && p.request.user_id == msg.user_id
            }) {
                return Ok(None);
            }
            if s.pending.len() >= 100 {
                return Err("配对请求已满，请在 IM 设置中处理".into());
            }
            let code = uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase();
            let request = ImPairingRequest {
                platform: msg.platform,
                code,
                user_id: msg.user_id.clone(),
                user_name: msg.user_name.clone(),
                created_at: now(),
                expires_at: now() + 600,
            };
            s.pending.push(PendingPair {
                identity: identity.to_owned(),
                request: request.clone(),
            });
            Ok(Some(request))
        })
    }
}

/// Deliver opted-in task notifications through the existing live socket, never another login.
pub async fn notify_task_finished(
    app: &tauri::AppHandle,
    name: &str,
    run: &crate::scheduled_tasks::types::TaskRun,
) {
    let Some(runtime) = app.try_state::<ImRuntime>() else {
        return;
    };
    let config = app
        .state::<crate::state::AppState>()
        .settings_read()
        .im
        .clone();
    let channels: Vec<_> = PLATFORMS
        .into_iter()
        .filter_map(|p| {
            let home = match p {
                ImPlatform::Feishu => &config.feishu.home_channel,
                ImPlatform::Wecom => &config.wecom.home_channel,
                ImPlatform::WecomCallback => &config.wecom_callback.home_channel,
            };
            (config.enabled(p) && !home.trim().is_empty()).then(|| (p, home.clone()))
        })
        .collect();
    if channels.is_empty() {
        return;
    }
    let text = if let Some(error) = &run.error {
        format!("定时任务「{name}」失败：{error}")
    } else {
        let Some(id) = &run.conversation_id else {
            return;
        };
        let Ok(conversation) = crate::chat::storage::load_conversation(app, id) else {
            return;
        };
        let Some(message) = conversation
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
        else {
            return;
        };
        format!("定时任务「{name}」已完成：\n{}", message.content)
    };
    for (platform, chat_id) in channels {
        let output = runtime.state.lock().outputs.get(&platform).cloned();
        if let Some(output) = output {
            let _ = output
                .send(OutboundMessage {
                    chat_id,
                    reply_token: None,
                    reply_to: None,
                    text: text.clone(),
                    attachments: Vec::new(),
                    stream_id: None,
                    finished: true,
                })
                .await;
        }
    }
}
fn contains(ids: &[String], id: &str) -> bool {
    ids.iter().any(|v| v == "*" || v.eq_ignore_ascii_case(id))
}
fn authorized(config: &ImConfig, msg: &InboundMessage, approved: &[ImApprovedUser]) -> bool {
    let access = config.access(msg.platform);
    let granted = approved.iter().any(|a| {
        a.platform == msg.platform
            && a.identity == config.identity(msg.platform)
            && a.user_id == msg.user_id
    });
    if !msg.is_group {
        return match access.dm_policy {
            DmPolicy::Disabled => false,
            DmPolicy::Open => true,
            DmPolicy::Allowlist => contains(&access.allowed_users, &msg.user_id),
            DmPolicy::Pairing => granted || contains(&access.allowed_users, &msg.user_id),
        };
    }
    let group = match access.group_policy {
        GroupPolicy::Disabled => false,
        GroupPolicy::Open => true,
        GroupPolicy::Allowlist => contains(&access.allowed_groups, &msg.chat_id),
    };
    if !group {
        return false;
    }
    if let Some(users) = access
        .group_users
        .get(&msg.chat_id)
        .or_else(|| access.group_users.get("*"))
    {
        return contains(users, &msg.user_id);
    }
    access.allowed_users.is_empty() || contains(&access.allowed_users, &msg.user_id) || granted
}
fn session_key(config: &ImConfig, msg: &InboundMessage) -> String {
    let user = if msg.is_group && !config.agent.group_sessions_per_user {
        "*"
    } else {
        &msg.user_id
    };
    key(&[
        msg.platform.key(),
        config.identity(msg.platform),
        &msg.chat_id,
        msg.thread_id.as_deref().unwrap_or(""),
        user,
    ])
}
fn response(msg: &InboundMessage, text: String) -> OutboundMessage {
    OutboundMessage {
        chat_id: msg.chat_id.clone(),
        reply_token: msg.reply_token.clone(),
        reply_to: Some(msg.message_id.clone()),
        text,
        attachments: Vec::new(),
        stream_id: None,
        finished: true,
    }
}

struct Running {
    signature: String,
    stop: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}
async fn stop_running(running: Running) {
    let _ = running.stop.send(true);
    let _ = running.task.await;
}
pub fn start(app: tauri::AppHandle) {
    let listener_app = app.clone();
    let listener = app.listen(crate::settings::SETTINGS_CHANGED_EVENT, move |_| {
        if let Some(runtime) = listener_app.try_state::<ImRuntime>() {
            runtime.changed.notify_one();
        }
    });
    let runtime = app.state::<ImRuntime>();
    let task_app = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut running: BTreeMap<ImPlatform, Running> = BTreeMap::new();
        let mut stopped = task_app.state::<ImRuntime>().stop.subscribe();
        let mut epoch = 0u64;
        loop {
            if *stopped.borrow() {
                break;
            }
            let config = task_app
                .state::<crate::state::AppState>()
                .settings_read()
                .im
                .clone();
            for platform in PLATFORMS {
                let revision = *task_app
                    .state::<ImRuntime>()
                    .state
                    .lock()
                    .revisions
                    .get(&platform)
                    .unwrap_or(&0);
                let specific = match platform {
                    ImPlatform::Feishu => serde_json::to_value(&config.feishu),
                    ImPlatform::Wecom => serde_json::to_value(&config.wecom),
                    ImPlatform::WecomCallback => serde_json::to_value(&config.wecom_callback),
                }
                .expect("IM config serializes");
                let signature = serde_json::to_string(&(specific, &config.agent, revision))
                    .expect("IM config serializes");
                if running
                    .get(&platform)
                    .is_some_and(|r| r.signature == signature)
                {
                    continue;
                }
                epoch += 1;
                {
                    let runtime = task_app.state::<ImRuntime>();
                    let mut state = runtime.state.lock();
                    state.epoch.insert(platform, epoch);
                }
                if let Some(old) = running.remove(&platform) {
                    stop_running(old).await;
                }
                let identity = config.identity(platform).to_owned();
                let loaded = tauri::async_runtime::spawn_blocking(move || {
                    credentials::load(platform, &identity)
                })
                .await
                .unwrap_or_else(|_| Err("系统凭证库读取失败".into()));
                let configured = loaded.is_ok();
                {
                    let runtime = task_app.state::<ImRuntime>();
                    let mut state = runtime.state.lock();
                    if let Some(status) = state.status.get_mut(&platform) {
                        status.credentials_configured = configured;
                        status.webhook_url = match platform {
                            ImPlatform::Feishu
                                if config.feishu.connection_mode
                                    == FeishuConnectionMode::Webhook =>
                            {
                                format!(
                                    "http://{}:{}{}",
                                    config.feishu.webhook.host,
                                    config.feishu.webhook.port,
                                    config.feishu.webhook.path
                                )
                            }
                            ImPlatform::WecomCallback => format!(
                                "http://{}:{}{}",
                                config.wecom_callback.webhook.host,
                                config.wecom_callback.webhook.port,
                                config.wecom_callback.webhook.path
                            ),
                            _ => String::new(),
                        };
                    }
                }
                let (stop, rx) = watch::channel(false);
                let task = if !config.enabled(platform) {
                    task_app.state::<ImRuntime>().update_status(
                        &task_app,
                        epoch,
                        StatusUpdate {
                            platform,
                            state: ConnectionState::Disabled,
                            message: String::new(),
                        },
                    );
                    tauri::async_runtime::spawn(async {})
                } else if let Ok(secret) = loaded {
                    let app = task_app.clone();
                    let cfg = config.clone();
                    tauri::async_runtime::spawn(async move {
                        supervise(app, cfg, platform, secret, epoch, rx).await;
                    })
                } else {
                    task_app.state::<ImRuntime>().update_status(
                        &task_app,
                        epoch,
                        StatusUpdate {
                            platform,
                            state: ConnectionState::Error,
                            message: loaded.err().unwrap_or_default(),
                        },
                    );
                    tauri::async_runtime::spawn(async {})
                };
                running.insert(
                    platform,
                    Running {
                        signature,
                        stop,
                        task,
                    },
                );
            }
            let notified = task_app.state::<ImRuntime>();
            tokio::select! { _=notified.changed.notified()=>{}, _=stopped.changed()=>break }
            if *stopped.borrow() {
                break;
            }
        }
        for (_, old) in running {
            stop_running(old).await;
        }
        task_app.unlisten(listener);
    });
    *runtime.runner.lock() = Some(task);
}
async fn supervise(
    app: tauri::AppHandle,
    config: ImConfig,
    platform: ImPlatform,
    secret: CredentialInput,
    epoch: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut attempt = 0usize;
    loop {
        if *shutdown.borrow() {
            break;
        }
        app.state::<ImRuntime>().update_status(
            &app,
            epoch,
            StatusUpdate {
                platform,
                state: if attempt == 0 {
                    ConnectionState::Connecting
                } else {
                    ConnectionState::Retrying
                },
                message: if attempt == 0 {
                    "正在连接机器人".into()
                } else {
                    "连接已断开，正在重连".into()
                },
            },
        );
        let outcome = run_connection(
            &app,
            &config,
            platform,
            secret.clone(),
            epoch,
            shutdown.clone(),
        )
        .await;
        if *shutdown.borrow() {
            break;
        }
        let error = outcome.err().unwrap_or_else(|| "IM 连接已结束".into());
        let fatal = error.starts_with("fatal:");
        app.state::<ImRuntime>().update_status(
            &app,
            epoch,
            StatusUpdate {
                platform,
                state: if fatal {
                    ConnectionState::Error
                } else {
                    ConnectionState::Retrying
                },
                message: error.trim_start_matches("fatal:").to_owned(),
            },
        );
        if fatal {
            break;
        }
        let delay = [2, 5, 10, 30, 60][attempt.min(4)];
        attempt = attempt.saturating_add(1);
        tokio::select! {_=tokio::time::sleep(Duration::from_secs(delay))=>{},_=shutdown.changed()=>break}
    }
}
async fn run_connection(
    app: &tauri::AppHandle,
    config: &ImConfig,
    platform: ImPlatform,
    secret: CredentialInput,
    epoch: u64,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let (incoming, mut inbound) = mpsc::channel(128);
    let (outbound, outgoing) = mpsc::channel(128);
    let (status_tx, mut status_rx) = mpsc::channel(32);
    {
        let runtime = app.state::<ImRuntime>();
        runtime
            .state
            .lock()
            .outputs
            .insert(platform, outbound.clone());
    }
    let (connection_stop, connection_shutdown) = watch::channel(false);
    let ctx = common::PlatformContext {
        app: app.clone(),
        inbound: incoming,
        outbound: outgoing,
        status: status_tx,
        shutdown: connection_shutdown.clone(),
    };
    let transport = async {
        match platform {
            ImPlatform::Feishu => feishu::run(config.feishu.clone(), secret, ctx).await,
            ImPlatform::Wecom => wecom::run(config.wecom.clone(), secret, ctx).await,
            ImPlatform::WecomCallback => {
                wecom_callback::run(config.wecom_callback.clone(), secret, ctx).await
            }
        }
    };
    tokio::pin!(transport);
    let mut shutdown = shutdown;
    let mut workers: BTreeMap<String, mpsc::Sender<InboundMessage>> = BTreeMap::new();
    let mut tasks = Vec::new();
    let result = loop {
        tokio::select! {
            result=&mut transport=>break result,
            _=shutdown.changed()=>{
                connection_stop.send_replace(true);
                break loop {
                    tokio::select! {
                        result=&mut transport=>break result,
                        Some(status)=status_rx.recv()=>app.state::<ImRuntime>().update_status(app,epoch,status),
                    }
                };
            },
            Some(status)=status_rx.recv()=>app.state::<ImRuntime>().update_status(app,epoch,status),
            Some(msg)=inbound.recv()=>{
                if msg.platform!=platform {continue;}
                let runtime=app.state::<ImRuntime>();
                if !runtime.authorize(config,&msg) {
                    if !msg.is_group&&config.access(platform).dm_policy==DmPolicy::Pairing {
                        match runtime.pair(config,&msg) {Ok(Some(request))=>{let _=app.emit("im-pairing-changed",());let _=outbound.try_send(response(&msg,format!("需要本机主人批准访问。配对码：{}（10 分钟有效）。请在 Dsivio 设置 → IM 中批准。",request.code)));},Ok(None)=>{},Err(error)=>runtime.update_status(app,epoch,StatusUpdate{platform,state:ConnectionState::Error,message:error})}
                    }
                    continue;
                }
                if msg.text.trim()=="/stop" {
                    let session=session_key(config,&msg);
                    let id=runtime.state.lock().store.sessions.get(&session).cloned();
                    if let Some(id)=id {app.state::<crate::state::AppState>().cancel_chat_generation(&id);}
                    let _=outbound.try_send(response(&msg,"已停止当前回复。".into()));
                    continue;
                }
                if msg.text.trim()=="/help" {
                    let _=outbound.try_send(response(&msg,"直接发送消息即可与 Dsivio Agent 对话。\n/new 或 /reset：开始新对话，保留历史。\n/stop：停止当前回复。\nIM 对话自动批准工具，请仅授权可信用户。".into()));
                    continue;
                }
                match runtime.claim(config,&msg) {Ok(true)=>{},Ok(false)=>continue,Err(message)=>{runtime.update_status(app,epoch,StatusUpdate{platform,state:ConnectionState::Error,message});continue;}}
                runtime.touch(app,platform);
                let session=session_key(config,&msg);
                if !workers.contains_key(&session)||workers.get(&session).is_some_and(|q|q.is_closed()) {
                    let (tx,rx)=mpsc::channel(32);workers.insert(session.clone(),tx);
                    let app=app.clone();let cfg=config.clone();let output=outbound.clone();let stop=connection_shutdown.clone();let key=session.clone();
                    tasks.push(tauri::async_runtime::spawn(async move {session_worker(app,cfg,key,rx,output,stop).await;}));
                }
                if let Some(queue)=workers.get(&session) {
                    if let Err(error)=queue.try_send(msg) {
                        let msg=error.into_inner();
                        let claim=key(&[platform.key(),config.identity(platform),&msg.message_id]);
                        let _=runtime.transact(|s|{s.seen.remove(&claim);Ok(())});
                        let _=outbound.try_send(response(&msg,"当前会话排队已满，请稍后重试".into()));
                    }
                }
            },
        }
    };
    {
        let runtime = app.state::<ImRuntime>();
        runtime.state.lock().outputs.remove(&platform);
    }
    let _ = connection_stop.send(true);
    drop(workers);
    for task in tasks {
        let _ = task.await;
    }
    result
}
async fn session_worker(
    app: tauri::AppHandle,
    config: ImConfig,
    session: String,
    mut queue: mpsc::Receiver<InboundMessage>,
    output: mpsc::Sender<OutboundMessage>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let msg = tokio::select! {biased;_=shutdown.changed()=>break,msg=queue.recv()=>match msg {Some(msg)=>msg,None=>break}};
        if *shutdown.borrow() {
            break;
        }
        let runtime = app.state::<ImRuntime>();
        if !runtime.authorize(&config, &msg) {
            continue;
        }
        let reset = matches!(msg.text.trim(), "/new" | "/reset");
        if reset {
            match runtime.transact(|store| {
                store.sessions.remove(&session);
                Ok(())
            }) {
                Ok(()) => {
                    let _ = output
                        .send(response(
                            &msg,
                            "已开始新对话，历史记录仍保留在 Dsivio。".into(),
                        ))
                        .await;
                }
                Err(error) => {
                    let _ = output.send(response(&msg, error)).await;
                }
            }
            continue;
        }
        let existing = runtime.state.lock().store.sessions.get(&session).cloned();
        let conversation = match conversations::ensure_conversation(
            &app,
            &config.agent,
            &msg,
            existing.as_deref(),
        )
        .await
        {
            Ok(id) => id,
            Err(error) => {
                let _ = output
                    .send(response(&msg, format!("无法创建 IM 对话：{error}")))
                    .await;
                continue;
            }
        };
        if let Err(error) = runtime.transact(|s| {
            s.sessions.insert(session.clone(), conversation.clone());
            Ok(())
        }) {
            let _ = output.send(response(&msg, error)).await;
            continue;
        }
        if let Err(error) = conversations::send(
            &app,
            &config.agent,
            &msg,
            &conversation,
            output.clone(),
            shutdown.clone(),
        )
        .await
        {
            if !*shutdown.borrow() {
                let _ = output
                    .send(response(&msg, format!("本次回复失败：{error}")))
                    .await;
            }
        }
    }
}

pub fn sanitize_config(config: &mut ImConfig) {
    fn strings(values: &mut Vec<String>) {
        for value in values.iter_mut() {
            *value = value.trim().to_owned();
        }
        values.retain(|v| !v.is_empty());
        values.sort();
        values.dedup();
    }
    fn access(config: &mut ImAccessConfig) {
        strings(&mut config.allowed_users);
        strings(&mut config.allowed_groups);
        for list in config.group_users.values_mut() {
            strings(list);
        }
    }
    config.feishu.app_id = config.feishu.app_id.trim().to_owned();
    config.wecom.bot_id = config.wecom.bot_id.trim().to_owned();
    config.wecom_callback.corp_id = config.wecom_callback.corp_id.trim().to_owned();
    config.wecom_callback.agent_id = config.wecom_callback.agent_id.trim().to_owned();
    config.wecom.websocket_url = config.wecom.websocket_url.trim().to_owned();
    if config.wecom.websocket_url.is_empty() {
        config.wecom.websocket_url = WecomConfig::default().websocket_url;
    }
    access(&mut config.feishu.access);
    access(&mut config.wecom.access);
    access(&mut config.wecom_callback.access);
    for webhook in [
        &mut config.feishu.webhook,
        &mut config.wecom_callback.webhook,
    ] {
        webhook.host = webhook.host.trim().to_owned();
        webhook.path = webhook.path.trim().to_owned();
        if webhook.host.is_empty() {
            webhook.host = "127.0.0.1".into();
        }
    }
    config.agent.assistant_id = config.agent.assistant_id.trim().to_owned();
    config.agent.provider_id = config.agent.provider_id.trim().to_owned();
    config.agent.model = config.agent.model.trim().to_owned();
    config.agent.working_directory = config.agent.working_directory.trim().to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message() -> InboundMessage {
        InboundMessage {
            platform: ImPlatform::Wecom,
            message_id: "1".into(),
            chat_id: "chat".into(),
            user_id: "alice".into(),
            user_name: String::new(),
            is_group: false,
            thread_id: None,
            reply_token: None,
            text: "hello".into(),
            attachments: Vec::new(),
        }
    }
    #[test]
    fn authorization_is_bot_scoped_and_group_rules_are_sender_scoped() {
        let mut config = ImConfig::default();
        config.wecom.bot_id = "bot-a".into();
        let mut msg = message();
        let grant = ImApprovedUser {
            platform: msg.platform,
            identity: "bot-a".into(),
            user_id: msg.user_id.clone(),
            user_name: String::new(),
            approved_at: 0,
        };
        assert!(!authorized(&config, &msg, &[]));
        assert!(authorized(&config, &msg, &[grant.clone()]));
        config.wecom.bot_id = "bot-b".into();
        assert!(!authorized(&config, &msg, &[grant]));
        msg.is_group = true;
        config.wecom.access.allowed_groups = vec![msg.chat_id.clone()];
        config
            .wecom
            .access
            .group_users
            .insert(msg.chat_id.clone(), vec!["bob".into()]);
        assert!(!authorized(&config, &msg, &[]));
        msg.user_id = "bob".into();
        assert!(authorized(&config, &msg, &[]));
    }
    #[test]
    fn session_routes_do_not_collide_across_bots_users_or_threads() {
        let mut config = ImConfig::default();
        config.wecom.bot_id = "a".into();
        let mut msg = message();
        msg.is_group = true;
        let a = session_key(&config, &msg);
        msg.user_id = "bob".into();
        assert_ne!(a, session_key(&config, &msg));
        config.agent.group_sessions_per_user = false;
        let shared = session_key(&config, &msg);
        msg.user_id = "alice".into();
        assert_eq!(shared, session_key(&config, &msg));
        msg.thread_id = Some("thread".into());
        assert_ne!(shared, session_key(&config, &msg));
    }
    #[test]
    fn admitted_messages_remain_deduplicated_after_restart_and_bot_change_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = ImRuntime::load(dir.path().to_owned()).unwrap();
        let mut config = ImConfig::default();
        config.wecom.bot_id = "first".into();
        let msg = message();
        assert!(runtime.claim(&config, &msg).unwrap());
        drop(runtime);
        let runtime = ImRuntime::load(dir.path().to_owned()).unwrap();
        assert!(!runtime.claim(&config, &msg).unwrap());
        config.wecom.bot_id = "second".into();
        assert!(runtime.claim(&config, &msg).unwrap());
    }
    #[test]
    fn failed_state_commit_does_not_grant_pairing_or_claim_message() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = ImRuntime::load(dir.path().to_owned()).unwrap();
        std::fs::write(dir.path().join("state.json"), "not JSON").unwrap();
        assert!(ImRuntime::load(dir.path().to_owned()).is_err());
        // A non-directory parent makes the atomic commit fail.
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "x").unwrap();
        let runtime = ImRuntime {
            dir: blocked,
            ..runtime
        };
        assert!(runtime.claim(&ImConfig::default(), &message()).is_err());
        assert!(runtime.state.lock().store.seen.is_empty());
    }
}
