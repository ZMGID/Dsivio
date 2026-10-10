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
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
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
    controls: BTreeMap<String, Arc<SessionControl>>,
}
pub struct ImRuntime {
    dir: PathBuf,
    state: Mutex<State>,
    changed: Notify,
    stop: watch::Sender<bool>,
    runner: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    setup: onboarding::SetupFlows,
    credentials: credentials::CredentialStore,
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
        let credentials = credentials::CredentialStore::file(dir.join("credentials.json"));
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
                controls: BTreeMap::new(),
            }),
            changed: Notify::new(),
            stop,
            runner: Mutex::new(None),
            setup: onboarding::SetupFlows::new(credentials.clone()),
            credentials,
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

impl ImRuntime {
    /// Revoke only IM work, including preparation before a chat generation exists.
    pub fn cancel_user_turns(
        &self,
        _app: &tauri::AppHandle,
        platform: ImPlatform,
        identity: &str,
        user_id: &str,
    ) {
        let mut prefix = key(&[platform.key(), identity]);
        prefix.pop();
        prefix.push(',');
        let controls: Vec<_> = self
            .state
            .lock()
            .controls
            .iter()
            .filter(|(route, _)| route.starts_with(&prefix))
            .map(|(_, control)| Arc::clone(control))
            .collect();
        for control in controls {
            control.cancel_user(user_id);
        }
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
    let text = if run.status == crate::scheduled_tasks::types::RunStatus::Interrupted {
        format!("定时任务「{name}」已中断：{}", run.error.as_deref().unwrap_or("运行未完成"))
    } else if let Some(error) = &run.error {
        format!("定时任务「{name}」失败：{error}")
    } else if run.status == crate::scheduled_tasks::types::RunStatus::Succeeded {
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
    } else {
        return;
    };
    for (platform, chat_id) in channels {
        let output = {
            let state = runtime.state.lock();
            state
                .status
                .get(&platform)
                .filter(|s| s.state == ConnectionState::Connected)
                .and_then(|_| state.outputs.get(&platform).cloned())
        };
        if let Some(output) = output {
            let _ = output
                .send(OutboundMessage {
                    chat_id,
                    is_group: false,
                    thread_id: None,
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
        is_group: msg.is_group,
        thread_id: msg.thread_id.clone(),
        reply_token: msg.reply_token.clone(),
        reply_to: Some(msg.message_id.clone()),
        text,
        attachments: Vec::new(),
        stream_id: None,
        finished: true,
    }
}

struct ClaimedTurn {
    notice: Option<String>,
    enqueue: bool,
}

/// Attachment failures are a finished user reply. They are a model turn only
/// when the callback also has text or a saved attachment.
fn claimed_turn(msg: &InboundMessage) -> ClaimedTurn {
    let notice = (!msg.attachment_failures.is_empty()).then(|| msg.attachment_failures.join("\n"));
    let enqueue = !msg.text.trim().is_empty() || !msg.attachments.is_empty();
    ClaimedTurn { notice, enqueue }
}

#[derive(Default)]
struct SessionControl {
    inner: Mutex<SessionControlState>,
}
#[derive(Default)]
struct SessionControlState {
    revision: u64,
    active: Option<(String, watch::Sender<bool>)>,
}
impl SessionControl {
    fn revision(&self) -> u64 {
        self.inner.lock().revision
    }
    fn begin(self: &Arc<Self>, revision: u64, user: &str) -> Option<TurnControl> {
        let mut inner = self.inner.lock();
        if inner.revision != revision {
            return None;
        }
        let (stop, shutdown) = watch::channel(false);
        inner.active = Some((user.to_owned(), stop));
        Some(TurnControl {
            control: Arc::clone(self),
            shutdown,
        })
    }
    fn cancel(&self) {
        let mut inner = self.inner.lock();
        inner.revision = inner.revision.wrapping_add(1);
        if let Some((_, stop)) = &inner.active {
            stop.send_replace(true);
        }
    }
    fn cancel_user(&self, user: &str) {
        let mut inner = self.inner.lock();
        if inner
            .active
            .as_ref()
            .is_some_and(|(owner, _)| owner == user)
        {
            inner.revision = inner.revision.wrapping_add(1);
            if let Some((_, stop)) = &inner.active {
                stop.send_replace(true);
            }
        }
    }
}
struct TurnControl {
    control: Arc<SessionControl>,
    shutdown: watch::Receiver<bool>,
}
impl Drop for TurnControl {
    fn drop(&mut self) {
        self.control.inner.lock().active = None;
    }
}
struct QueuedMessage {
    message: InboundMessage,
    revision: u64,
}
struct SessionWorker {
    queue: mpsc::Sender<QueuedMessage>,
    control: Arc<SessionControl>,
    task: tauri::async_runtime::JoinHandle<()>,
}

/// Only unsent output lives here. Intermediate versions coalesce; final results do not.
#[derive(Default)]
struct PendingOutput {
    queue: VecDeque<OutboundMessage>,
}
impl PendingOutput {
    const CAPACITY: usize = 128;
    fn push(&mut self, message: OutboundMessage) -> bool {
        if let Some(stream) = message.stream_id.as_deref() {
            if message.finished {
                self.queue
                    .retain(|old| old.stream_id.as_deref() != Some(stream) || old.finished);
            } else if let Some(old) = self
                .queue
                .iter_mut()
                .find(|old| old.stream_id.as_deref() == Some(stream))
            {
                if !old.finished {
                    *old = message;
                }
                return true;
            }
        }
        if self.queue.len() == Self::CAPACITY {
            if let Some(index) = self.queue.iter().position(|old| !old.finished) {
                self.queue.remove(index);
            } else {
                return false;
            }
        }
        self.queue.push_back(message);
        true
    }
    fn can_receive(&self) -> bool {
        self.queue.len() < Self::CAPACITY || self.queue.iter().any(|message| !message.finished)
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
                let store = task_app.state::<ImRuntime>().credentials.clone();
                let loaded =
                    tauri::async_runtime::spawn_blocking(move || store.load(platform, &identity))
                        .await
                        .unwrap_or_else(|_| Err("IM 凭证文件读取失败".into()));
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
struct Transport {
    output: mpsc::Sender<OutboundMessage>,
    stop: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<Result<(), String>>,
}

fn start_transport(
    app: &tauri::AppHandle,
    config: &ImConfig,
    platform: ImPlatform,
    secret: CredentialInput,
    incoming: mpsc::Sender<InboundMessage>,
    status: mpsc::Sender<StatusUpdate>,
) -> Transport {
    let (output, outgoing) = mpsc::channel(128);
    let (stop, shutdown) = watch::channel(false);
    let ctx = common::PlatformContext {
        app: app.clone(),
        inbound: incoming,
        outbound: outgoing,
        status,
        shutdown,
    };
    let task = match platform {
        ImPlatform::Feishu => {
            tauri::async_runtime::spawn(feishu::run(config.feishu.clone(), secret, ctx))
        }
        ImPlatform::Wecom => {
            tauri::async_runtime::spawn(wecom::run(config.wecom.clone(), secret, ctx))
        }
        ImPlatform::WecomCallback => tauri::async_runtime::spawn(wecom_callback::run(
            config.wecom_callback.clone(),
            secret,
            ctx,
        )),
    };
    Transport { output, stop, task }
}

async fn stop_transport(mut transport: Transport) {
    transport.stop.send_replace(true);
    if tokio::time::timeout(Duration::from_secs(5), &mut transport.task)
        .await
        .is_err()
    {
        transport.task.abort();
        let _ = transport.task.await;
    }
}

enum SupervisorEvent {
    Shutdown,
    Connect,
    Ended(Result<(), String>),
    Status(StatusUpdate),
    Inbound(InboundMessage),
    Output(OutboundMessage),
    Send(Option<mpsc::OwnedPermit<OutboundMessage>>),
}

async fn supervise(
    app: tauri::AppHandle,
    config: ImConfig,
    platform: ImPlatform,
    secret: CredentialInput,
    epoch: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let config = Arc::new(config);
    let (incoming, mut inbound) = mpsc::channel(128);
    let (outbound, mut outgoing) = mpsc::channel(128);
    let (status_tx, mut status_rx) = mpsc::channel(32);
    app.state::<ImRuntime>()
        .state
        .lock()
        .outputs
        .insert(platform, outbound.clone());
    let mut workers: BTreeMap<String, SessionWorker> = BTreeMap::new();
    let mut pending = PendingOutput::default();
    let mut transport: Option<Transport> = None;
    let mut connected = false;
    let mut attempt = 0usize;
    let mut retry_at = tokio::time::Instant::now();
    loop {
        if *shutdown.borrow() {
            break;
        }
        let output = transport
            .as_ref()
            .filter(|_| connected)
            .map(|t| t.output.clone());
        let disconnected = transport.is_none();
        let event = {
            let connection = async {
                match transport.as_mut() {
                    Some(connection) => (&mut connection.task)
                        .await
                        .unwrap_or_else(|_| Err("IM 连接任务中断".into())),
                    None => std::future::pending().await,
                }
            };
            let send = async {
                match output {
                    Some(output) => output.reserve_owned().await.ok(),
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = shutdown.changed() => SupervisorEvent::Shutdown,
                result = connection => SupervisorEvent::Ended(result),
                _ = tokio::time::sleep_until(retry_at), if disconnected => SupervisorEvent::Connect,
                Some(status) = status_rx.recv() => SupervisorEvent::Status(status),
                Some(message) = inbound.recv() => SupervisorEvent::Inbound(message),
                permit = send, if connected && !pending.queue.is_empty() => SupervisorEvent::Send(permit),
                Some(message) = outgoing.recv(), if pending.can_receive() => SupervisorEvent::Output(message),
            }
        };
        match event {
            SupervisorEvent::Shutdown => break,
            SupervisorEvent::Connect => {
                connected = false;
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
                            "正在连接机器人"
                        } else {
                            "连接已断开，正在重连"
                        }
                        .into(),
                    },
                );
                transport = Some(start_transport(
                    &app,
                    &config,
                    platform,
                    secret.clone(),
                    incoming.clone(),
                    status_tx.clone(),
                ));
            }
            SupervisorEvent::Ended(outcome) => {
                transport = None;
                connected = false;
                while status_rx.try_recv().is_ok() {}
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
                retry_at = tokio::time::Instant::now() + Duration::from_secs(delay);
                attempt = attempt.saturating_add(1);
            }
            SupervisorEvent::Status(status) => {
                if status.state == ConnectionState::Connected {
                    connected = true;
                }
                app.state::<ImRuntime>().update_status(&app, epoch, status);
            }
            SupervisorEvent::Output(message) => {
                pending.push(message);
            }
            SupervisorEvent::Send(Some(permit)) => {
                if let Some(message) = pending.queue.pop_front() {
                    permit.send(message);
                }
            }
            SupervisorEvent::Send(None) => connected = false,
            SupervisorEvent::Inbound(msg) => {
                if msg.platform != platform {
                    continue;
                }
                let runtime = app.state::<ImRuntime>();
                if !runtime.authorize(&config, &msg) {
                    if !msg.is_group && config.access(platform).dm_policy == DmPolicy::Pairing {
                        match runtime.pair(&config, &msg) {
                            Ok(Some(request)) => {
                                let _ = app.emit("im-pairing-changed", ());
                                queue_response(&app, epoch, &mut pending, &msg, format!("需要本机主人批准访问。配对码：{}（10 分钟有效）。请在 Dsivio 设置 → IM 中批准。", request.code));
                            }
                            Ok(None) => {}
                            Err(message) => runtime.update_status(
                                &app,
                                epoch,
                                StatusUpdate {
                                    platform,
                                    state: ConnectionState::Error,
                                    message,
                                },
                            ),
                        }
                    }
                    continue;
                }
                let session = session_key(&config, &msg);
                if msg.text.trim() == "/stop" {
                    if let Some(worker) = workers.get(&session) {
                        worker.control.cancel();
                    }
                    queue_response(
                        &app,
                        epoch,
                        &mut pending,
                        &msg,
                        "已停止当前回复和等待中的消息。".into(),
                    );
                    continue;
                }
                if msg.text.trim() == "/help" {
                    queue_response(&app, epoch, &mut pending, &msg, "直接发送消息即可与 Dsivio Agent 对话。\n/new 或 /reset：开始新对话，保留历史。\n/stop：停止当前回复和等待中的消息。\nIM 对话自动批准工具，请仅授权可信用户。".into());
                    continue;
                }
                match runtime.claim(&config, &msg) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(message) => {
                        runtime.update_status(
                            &app,
                            epoch,
                            StatusUpdate {
                                platform,
                                state: ConnectionState::Error,
                                message,
                            },
                        );
                        continue;
                    }
                }
                runtime.touch(&app, platform);
                let claimed = claimed_turn(&msg);
                if let Some(notice) = claimed.notice {
                    queue_response(&app, epoch, &mut pending, &msg, notice);
                }
                if !claimed.enqueue {
                    continue;
                }
                if !workers.contains_key(&session)
                    || workers
                        .get(&session)
                        .is_some_and(|worker| worker.queue.is_closed())
                {
                    if let Some(old) = workers.remove(&session) {
                        let _ = old.task.await;
                    }
                    let (queue, receiver) = mpsc::channel(32);
                    let control = Arc::new(SessionControl::default());
                    runtime
                        .state
                        .lock()
                        .controls
                        .insert(session.clone(), Arc::clone(&control));
                    let task_app = app.clone();
                    let task_config = Arc::clone(&config);
                    let task_output = outbound.clone();
                    let task_stop = shutdown.clone();
                    let task_control = Arc::clone(&control);
                    let key = session.clone();
                    let task = tauri::async_runtime::spawn(async move {
                        session_worker(
                            task_app,
                            task_config,
                            key,
                            receiver,
                            task_output,
                            task_stop,
                            task_control,
                        )
                        .await;
                    });
                    workers.insert(
                        session.clone(),
                        SessionWorker {
                            queue,
                            control,
                            task,
                        },
                    );
                }
                if let Some(worker) = workers.get(&session) {
                    let revision = worker.control.revision();
                    if let Err(error) = worker.queue.try_send(QueuedMessage {
                        message: msg,
                        revision,
                    }) {
                        let msg = error.into_inner().message;
                        let claim =
                            key(&[platform.key(), config.identity(platform), &msg.message_id]);
                        let _ = runtime.transact(|store| {
                            store.seen.remove(&claim);
                            Ok(())
                        });
                        queue_response(
                            &app,
                            epoch,
                            &mut pending,
                            &msg,
                            "当前会话排队已满，请稍后重试".into(),
                        );
                    }
                }
            }
        }
    }
    {
        let runtime = app.state::<ImRuntime>();
        let mut state = runtime.state.lock();
        state.outputs.remove(&platform);
        for (session, worker) in &workers {
            worker.control.cancel();
            if state
                .controls
                .get(session)
                .is_some_and(|control| Arc::ptr_eq(control, &worker.control))
            {
                state.controls.remove(session);
            }
        }
    }
    drop(outgoing);
    drop(inbound);
    drop(status_rx);
    if let Some(transport) = transport {
        stop_transport(transport).await;
    }
    for (_, worker) in workers {
        drop(worker.queue);
        let _ = worker.task.await;
    }
}

fn queue_response(
    app: &tauri::AppHandle,
    epoch: u64,
    pending: &mut PendingOutput,
    msg: &InboundMessage,
    text: String,
) {
    if !pending.push(response(msg, text)) {
        app.state::<ImRuntime>().update_status(
            app,
            epoch,
            StatusUpdate {
                platform: msg.platform,
                state: ConnectionState::Error,
                message: "IM 发送排队已满，请等待连接恢复".into(),
            },
        );
    }
}

async fn send_worker_output(
    output: &mpsc::Sender<OutboundMessage>,
    message: OutboundMessage,
    shutdown: &mut watch::Receiver<bool>,
) {
    if *shutdown.borrow() {
        return;
    }
    tokio::select! {
        biased;
        _ = shutdown.changed() => {}
        _ = output.send(message) => {}
    }
}

async fn session_worker(
    app: tauri::AppHandle,
    config: Arc<ImConfig>,
    session: String,
    mut queue: mpsc::Receiver<QueuedMessage>,
    output: mpsc::Sender<OutboundMessage>,
    mut shutdown: watch::Receiver<bool>,
    control: Arc<SessionControl>,
) {
    loop {
        if *shutdown.borrow() {
            break;
        }
        let queued = tokio::select! {biased; _ = shutdown.changed() => break, msg = queue.recv() => match msg { Some(msg) => msg, None => break }};
        let msg = queued.message;
        let Some(mut turn) = control.begin(queued.revision, &msg.user_id) else {
            continue;
        };
        let runtime = app.state::<ImRuntime>();
        if *shutdown.borrow() || !runtime.authorize(&config, &msg) {
            continue;
        }
        if matches!(msg.text.trim(), "/new" | "/reset") {
            let text = match runtime.transact(|store| {
                store.sessions.remove(&session);
                Ok(())
            }) {
                Ok(()) => "已开始新对话，历史记录仍保留在 Dsivio。".into(),
                Err(error) => error,
            };
            send_worker_output(&output, response(&msg, text), &mut turn.shutdown).await;
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
                send_worker_output(
                    &output,
                    response(&msg, format!("无法创建 IM 对话：{error}")),
                    &mut turn.shutdown,
                )
                .await;
                continue;
            }
        };
        if *turn.shutdown.borrow() {
            continue;
        }
        if let Err(error) = runtime.transact(|store| {
            store.sessions.insert(session.clone(), conversation.clone());
            Ok(())
        }) {
            send_worker_output(&output, response(&msg, error), &mut turn.shutdown).await;
            continue;
        }
        if let Err(error) = conversations::send(
            &app,
            &config.agent,
            &msg,
            &conversation,
            output.clone(),
            turn.shutdown.clone(),
        )
        .await
        {
            send_worker_output(
                &output,
                response(&msg, format!("本次回复失败：{error}")),
                &mut turn.shutdown,
            )
            .await;
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
            attachment_failures: Vec::new(),
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

    #[test]
    fn stop_discards_queued_work_and_cancels_preparation_but_allows_a_new_turn() {
        let control = Arc::new(SessionControl::default());
        let before_stop = control.revision();
        let turn = control.begin(before_stop, "alice").unwrap();
        control.cancel();
        assert!(*turn.shutdown.borrow());
        drop(turn);
        assert!(control.begin(before_stop, "alice").is_none());
        let next = control.begin(control.revision(), "alice").unwrap();
        assert!(!*next.shutdown.borrow());
    }

    #[test]
    fn revoking_one_sender_does_not_cancel_another_senders_active_turn() {
        let control = Arc::new(SessionControl::default());
        let turn = control.begin(control.revision(), "alice").unwrap();
        control.cancel_user("bob");
        assert!(!*turn.shutdown.borrow());
        control.cancel_user("alice");
        assert!(*turn.shutdown.borrow());
    }

    #[test]
    fn disconnected_outbox_replaces_previews_but_keeps_final_files_and_other_chats() {
        let mut pending = PendingOutput::default();
        let mut preview = response(&message(), "first".into());
        preview.stream_id = Some("a".into());
        preview.finished = false;
        pending.push(preview.clone());
        preview.text = "latest".into();
        pending.push(preview.clone());
        let mut other = response(&message(), "other-chat".into());
        other.chat_id = "other".into();
        other.stream_id = Some("b".into());
        pending.push(other);
        let mut final_message = preview.clone();
        final_message.finished = true;
        final_message.text = "final".into();
        final_message.attachments = vec!["final.pdf".into()];
        pending.push(final_message);
        preview.text = "late-stale".into();
        pending.push(preview);
        let other = pending.queue.pop_front().unwrap();
        assert_eq!(
            (other.chat_id.as_str(), other.text.as_str()),
            ("other", "other-chat")
        );
        let final_message = pending.queue.pop_front().unwrap();
        assert_eq!(final_message.text, "final");
        assert_eq!(final_message.attachments, ["final.pdf"]);
        assert!(final_message.finished);
        assert!(pending.queue.is_empty());
    }

    #[test]
    fn a_full_outbox_backpressures_without_dropping_accepted_final_results() {
        let mut pending = PendingOutput::default();
        for index in 0..PendingOutput::CAPACITY {
            assert!(pending.push(response(&message(), index.to_string())));
        }
        assert!(!pending.can_receive());
        assert!(!pending.push(response(&message(), "overflow".into())));
        for index in 0..PendingOutput::CAPACITY {
            assert_eq!(pending.queue.pop_front().unwrap().text, index.to_string());
        }
        assert!(pending.can_receive());
    }

    #[test]
    fn empty_inbound_is_not_a_model_turn() {
        let mut msg = message();
        msg.text.clear();
        let claimed = claimed_turn(&msg);
        assert!(!claimed.enqueue);
        assert!(claimed.notice.is_none());
    }
}
