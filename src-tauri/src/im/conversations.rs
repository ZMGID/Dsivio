//! IM turns reuse the desktop chat pipeline. Identity mapping stays in the IM runtime.
//! This module creates or reuses the sidebar conversation and bridges one inbound turn.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, watch};

#[cfg(test)]
use crate::chat::protocol::im_visible_text;
use crate::chat::protocol::{truncate_stream_tail, ImVisibleText};
use crate::chat::storage::{assistant_snapshot, load_conversation, set_im_working_directory};
use crate::chat::ChatMessage;
use crate::state::AppState;

use super::types::{ImAgentConfig, InboundMessage, OutboundMessage};

/// Parent sends the visible failure. Shutdown and pipeline errors must not both
/// produce an IM message.
pub(crate) fn im_turn_result(shutdown: bool, failed: Option<String>) -> Result<bool, String> {
    if shutdown {
        return Ok(false);
    }
    match failed {
        Some(error) => Err(error),
        None => Ok(true),
    }
}

pub(crate) fn apply_visible_text(text: &mut String, update: &ImVisibleText) {
    match update {
        ImVisibleText::Append(delta) => text.push_str(delta),
        ImVisibleText::DiscardChars(chars) => truncate_stream_tail(text, *chars),
    }
}

pub(crate) fn remember_output_file(paths: &mut Vec<String>, path: &Path) {
    if !path.is_file() {
        return;
    }
    let value = path.to_string_lossy().into_owned();
    if paths.iter().any(|existing| existing == &value) {
        return;
    }
    paths.push(value);
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn configured_directory(config: &ImAgentConfig) -> Result<Option<String>, String> {
    let directory = config.working_directory.trim();
    if directory.is_empty() {
        return Ok(None);
    }
    if !Path::new(directory).is_dir() {
        return Err(format!("IM 工作目录不存在：{directory}"));
    }
    Ok(Some(directory.to_string()))
}

pub(crate) fn im_stream_id(streaming: bool, message_id: &str) -> Option<String> {
    streaming.then(|| format!("im:{message_id}"))
}

pub(crate) fn im_message(
    inbound: &InboundMessage,
    text: String,
    attachments: Vec<String>,
    stream_id: Option<String>,
    finished: bool,
) -> OutboundMessage {
    OutboundMessage {
        chat_id: inbound.chat_id.clone(),
        is_group: inbound.is_group,
        thread_id: inbound.thread_id.clone(),
        reply_token: inbound.reply_token.clone(),
        reply_to: Some(inbound.message_id.clone()),
        text,
        attachments,
        stream_id,
        finished,
    }
}

pub(crate) fn assistant_output_files(
    app: &AppHandle,
    conversation_id: &str,
    message: &ChatMessage,
) -> Vec<String> {
    let mut paths = Vec::new();
    let artifacts = message.artifacts.iter().chain(
        message
            .tool_calls
            .iter()
            .flat_map(|call| call.artifacts.iter()),
    );
    for artifact in artifacts {
        if let Ok(path) = crate::chat::artifacts::file_path(app, conversation_id, artifact) {
            remember_output_file(&mut paths, &path);
        }
    }
    for attachment in &message.attachments {
        if attachment.path.starts_with("memory://") {
            continue;
        }
        if let Ok(path) = crate::chat::attachments::resolve_attachment_file_path(
            app,
            Some(conversation_id),
            &attachment.path,
        ) {
            remember_output_file(&mut paths, &path);
        }
    }
    paths
}

fn prior_assistant_ids(messages: &[ChatMessage]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| message.role == "assistant")
        .map(|message| message.id.clone())
        .collect()
}

fn this_turn_assistants<'a>(
    messages: &'a [ChatMessage],
    prior: &HashSet<String>,
) -> Vec<&'a ChatMessage> {
    messages
        .iter()
        .filter(|message| message.role == "assistant" && !prior.contains(&message.id))
        .collect()
}

/// Text and file sources for this inbound turn. Assistants that already existed
/// at admission are not part of the result, so a later desktop reply cannot be
/// forwarded and a turn that never generated does not replay history.
pub(crate) fn compose_turn_outbound<'a>(
    messages: &'a [ChatMessage],
    prior: &HashSet<String>,
    streamed: &str,
) -> (String, Vec<&'a ChatMessage>) {
    let fresh = this_turn_assistants(messages, prior);
    if fresh.is_empty() {
        return (streamed.to_string(), Vec::new());
    }
    let text = fresh
        .iter()
        .rev()
        .find_map(|message| (!message.content.is_empty()).then(|| message.content.clone()));
    (text.unwrap_or_else(|| streamed.to_string()), fresh)
}

struct TurnSnapshot {
    saw_assistant: bool,
    text: String,
    attachments: Vec<String>,
}

fn snapshot_this_turn(
    app: &AppHandle,
    conversation_id: &str,
    prior: &HashSet<String>,
) -> TurnSnapshot {
    let Ok(saved) = load_conversation(app, conversation_id) else {
        return TurnSnapshot {
            saw_assistant: false,
            text: String::new(),
            attachments: Vec::new(),
        };
    };
    let (text, fresh) = compose_turn_outbound(&saved.messages, prior, "");
    if fresh.is_empty() {
        return TurnSnapshot {
            saw_assistant: false,
            text: String::new(),
            attachments: Vec::new(),
        };
    }
    let mut attachments = Vec::new();
    for message in fresh {
        for path in assistant_output_files(app, conversation_id, message) {
            if !attachments.iter().any(|existing| existing == &path) {
                attachments.push(path);
            }
        }
    }
    TurnSnapshot {
        saw_assistant: true,
        text,
        attachments,
    }
}

fn pipeline_failure(outcome: &Result<serde_json::Value, String>) -> Option<String> {
    match outcome {
        Err(error) => Some(error.clone()),
        Ok(value) if value.get("success").and_then(|flag| flag.as_bool()) == Some(true) => None,
        Ok(value) => Some(
            value
                .get("error")
                .and_then(|error| error.as_str())
                .unwrap_or("发送失败")
                .to_string(),
        ),
    }
}

struct ImTurnGuard {
    app: AppHandle,
    conversation_id: String,
    armed: Arc<AtomicBool>,
}

impl Drop for ImTurnGuard {
    fn drop(&mut self) {
        if self.armed.load(Ordering::SeqCst) {
            let state = self.app.state::<AppState>();
            state
                .chat_protocol()
                .unbind_im_text_stream(&self.conversation_id);
            state.chat_runtime().end_im_turn(&self.conversation_id);
        }
    }
}

struct ClearImApproval {
    app: AppHandle,
    conversation_id: String,
    armed: Arc<AtomicBool>,
}

impl Drop for ClearImApproval {
    fn drop(&mut self) {
        let state = self.app.state::<AppState>();
        state
            .chat_protocol()
            .unbind_im_text_stream(&self.conversation_id);
        state.chat_runtime().end_im_turn(&self.conversation_id);
        self.armed.store(false, Ordering::SeqCst);
    }
}

/// Stop one admitted IM turn. A desktop run that never entered `begin_im_turn`
/// is left alone: the latch fails, and this does not cancel every generation.
fn stop_admitted_im_turn(state: &AppState, conversation_id: &str) -> bool {
    let Some(children) = state.chat_runtime().cancel_im_lineage(conversation_id) else {
        return false;
    };
    state.sub_agents.stop_lineage(conversation_id, &children);
    true
}

struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn send_outbound(
    progress: &mpsc::Sender<OutboundMessage>,
    shutdown: &mut watch::Receiver<bool>,
    message: OutboundMessage,
) -> Result<(), String> {
    if *shutdown.borrow() {
        return Ok(());
    }
    tokio::select! {
        biased;
        _ = shutdown.changed() => Ok(()),
        result = progress.send(message) => result.map_err(|_| "IM 出站通道已关闭".to_string()),
    }
}

/// Create a sidebar conversation or reuse the id the IM runtime already stored.
/// `/new` and `/reset` are handled by that runtime; a missing id creates the next chat.
pub async fn ensure_conversation(
    app: &AppHandle,
    config: &ImAgentConfig,
    inbound: &InboundMessage,
    existing: Option<&str>,
) -> Result<String, String> {
    let _ = inbound;
    let directory = configured_directory(config)?;
    if let Some(id) = existing.map(str::trim).filter(|id| !id.is_empty()) {
        if let Ok(conversation) = load_conversation(app, id) {
            if !conversation.agent_runtime.is_external() {
                align_conversation(app, config, id).await?;
                set_im_working_directory(app, id, directory.as_deref())?;
                return Ok(id.to_string());
            }
        }
    }
    let state = app.state::<AppState>();
    let conversation = crate::chat::commands::catalog::create_chat_conversation_internal(
        app,
        state.inner(),
        non_empty(&config.provider_id),
        non_empty(&config.model),
        None,
        None,
        None,
        non_empty(&config.assistant_id),
        false,
    )
    .await?;
    set_im_working_directory(app, &conversation.id, directory.as_deref())?;
    Ok(conversation.id)
}

pub(crate) fn resolved_im_model(
    provider_id: &str,
    model: &str,
    defaults: (String, String),
) -> (String, String) {
    match (non_empty(provider_id), non_empty(model)) {
        (Some(provider_id), Some(model)) => (provider_id, model),
        _ => defaults,
    }
}

pub(crate) fn apply_im_conversation_alignment(
    conversation: &mut crate::chat::Conversation,
    provider_id: &str,
    model: &str,
    assistant: Option<crate::chat::ChatAssistantSnapshot>,
) {
    conversation.provider_id = provider_id.to_string();
    conversation.model = model.to_string();
    match assistant {
        Some(snapshot) => {
            conversation.active_skill_id = None;
            conversation.assistant_id = Some(snapshot.id.clone());
            conversation.assistant_snapshot = Some(snapshot);
        }
        None => {
            conversation.active_skill_id = None;
            conversation.assistant_id = None;
            conversation.assistant_snapshot = None;
        }
    }
}

async fn align_conversation(
    app: &AppHandle,
    config: &ImAgentConfig,
    conversation_id: &str,
) -> Result<(), String> {
    let defaults = {
        let state = app.state::<AppState>();
        let settings = state.settings_read();
        settings.effective_chat_model()
    };
    let (provider_id, model) = resolved_im_model(&config.provider_id, &config.model, defaults);
    let assistant = match non_empty(&config.assistant_id) {
        Some(id) => Some(assistant_snapshot(app, &id)?),
        None => None,
    };
    crate::chat::repository::repository(app)
        .mutate(app, conversation_id, move |conversation| {
            apply_im_conversation_alignment(conversation, &provider_id, &model, assistant);
            Ok(())
        })
        .await
        .map_err(crate::chat::repository::repository_error)?;
    Ok(())
}

/// Run one inbound turn through the normal send reservation, user-message write,
/// and assistant reply. Tool approval is limited to this turn, including sub-agents.
/// Progress is cumulative visible text. The caller reports pipeline failures.
pub async fn send(
    app: &AppHandle,
    config: &ImAgentConfig,
    inbound: &InboundMessage,
    conversation_id: &str,
    progress: mpsc::Sender<OutboundMessage>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    if *shutdown.borrow() {
        return Ok(());
    }
    let (updates, mut update_rx) = mpsc::unbounded_channel();
    let guard = ImTurnGuard {
        app: app.clone(),
        conversation_id: conversation_id.to_string(),
        armed: Arc::new(AtomicBool::new(false)),
    };
    let prior_assistants = Arc::new(Mutex::new(None));
    let turn_snapshot = Arc::new(Mutex::new(None));
    let app_for_turn = app.clone();
    let turn_id = conversation_id.to_string();
    let armed = Arc::clone(&guard.armed);
    let prior_for_admit = Arc::clone(&prior_assistants);
    let admit_shutdown = shutdown.clone();
    let watcher: Arc<Mutex<Option<AbortOnDrop<()>>>> = Arc::new(Mutex::new(None));
    let watcher_for_admit = Arc::clone(&watcher);
    let app_for_watch = app.clone();
    let watch_id = conversation_id.to_string();
    let mut watch_shutdown = shutdown.clone();
    let on_admitted = move || {
        // Arm before any await-free work so a panic still clears the turn.
        armed.store(true, Ordering::SeqCst);
        let state = app_for_turn.state::<AppState>();
        state.chat_runtime().begin_im_turn(&turn_id);
        state
            .chat_protocol()
            .bind_im_text_stream(&turn_id, updates.clone());
        let known = load_conversation(&app_for_turn, &turn_id)
            .ok()
            .map(|conversation| prior_assistant_ids(&conversation.messages));
        *prior_for_admit.lock() = known;
        // Stop can already be latched before this turn owns a generation.
        // This only runs after the reservation is held, so a queued turn never
        // cancels the desktop generation that still owns the conversation.
        if *admit_shutdown.borrow() {
            stop_admitted_im_turn(&state, &turn_id);
        }
        let handle = tokio::spawn(async move {
            loop {
                if *watch_shutdown.borrow() {
                    let _ = stop_admitted_im_turn(&app_for_watch.state::<AppState>(), &watch_id);
                    break;
                }
                if watch_shutdown.changed().await.is_err() {
                    break;
                }
            }
        });
        *watcher_for_admit.lock() = Some(AbortOnDrop(handle));
    };

    let streaming = config.streaming;
    let stream_id = im_stream_id(streaming, &inbound.message_id);
    let inbound_for_progress = inbound.clone();
    let progress_for_stream = progress.clone();
    let mut stream_shutdown = shutdown.clone();
    let mut forwarder = tokio::spawn(async move {
        let mut text = String::new();
        while let Some(update) = update_rx.recv().await {
            apply_visible_text(&mut text, &update);
            if !streaming || *stream_shutdown.borrow() {
                continue;
            }
            let message = im_message(
                &inbound_for_progress,
                text.clone(),
                Vec::new(),
                stream_id.clone(),
                false,
            );
            tokio::select! {
                biased;
                _ = stream_shutdown.changed() => {}
                _ = progress_for_stream.send(message) => {}
            }
        }
        text
    });

    let snapshot_for_release = Arc::clone(&turn_snapshot);
    let prior_for_release = Arc::clone(&prior_assistants);
    let app_for_release = app.clone();
    let release_id = conversation_id.to_string();
    let armed_for_release = Arc::clone(&guard.armed);
    let release_shutdown = shutdown.clone();
    let before_release = move || {
        // Watcher, approval, and the reservation end together. Abort first so a
        // late stop cannot run after the desktop turn has taken the conversation.
        drop(watcher.lock().take());
        if *release_shutdown.borrow() {
            stop_admitted_im_turn(&app_for_release.state::<AppState>(), &release_id);
        }
        // Drop clears approval before the send reservation is released, including
        // when the snapshot panics. The reservation is still held while we read.
        let clear = ClearImApproval {
            app: app_for_release.clone(),
            conversation_id: release_id.clone(),
            armed: Arc::clone(&armed_for_release),
        };
        let prior = prior_for_release.lock().take();
        let snapshot = match &prior {
            Some(ids) => snapshot_this_turn(&app_for_release, &release_id, ids),
            None => TurnSnapshot {
                saw_assistant: false,
                text: String::new(),
                attachments: Vec::new(),
            },
        };
        *snapshot_for_release.lock() = Some(snapshot);
        drop(clear);
    };

    let mut wait_shutdown = shutdown.clone();
    let outcome = crate::chat::commands::send::send_attached_when_idle(
        app,
        conversation_id,
        inbound.text.clone(),
        inbound.attachments.clone(),
        async move {
            if *wait_shutdown.borrow() {
                return "cancelled".to_string();
            }
            let _ = wait_shutdown.changed().await;
            "cancelled".to_string()
        },
        on_admitted,
        before_release,
    )
    .await;
    if *shutdown.borrow() {
        forwarder.abort();
        return Ok(());
    }
    let streamed = tokio::select! {
        biased;
        _ = shutdown.changed() => {
            forwarder.abort();
            return Ok(());
        }
        joined = &mut forwarder => joined.unwrap_or_default(),
    };
    if let Err(error) = im_turn_result(*shutdown.borrow(), pipeline_failure(&outcome)) {
        return Err(error);
    }

    let snapshot = turn_snapshot.lock().take().unwrap_or(TurnSnapshot {
        saw_assistant: false,
        text: String::new(),
        attachments: Vec::new(),
    });
    let attachments = if snapshot.saw_assistant {
        snapshot.attachments
    } else {
        Vec::new()
    };
    let text = if snapshot.saw_assistant && !snapshot.text.is_empty() {
        snapshot.text
    } else {
        streamed
    };
    let final_message = im_message(
        inbound,
        text,
        attachments,
        im_stream_id(config.streaming, &inbound.message_id),
        true,
    );
    send_outbound(&progress, &mut shutdown, final_message).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::protocol::ChatRunEvent;

    #[test]
    fn visible_stream_omits_private_events_and_rolls_back_unicode() {
        let hidden = [
            ChatRunEvent::ReasoningDelta {
                delta: "思考".into(),
                segment: None,
            },
            ChatRunEvent::TextDelta {
                delta: "你".into(),
                segment: None,
            },
            ChatRunEvent::SessionConsentRequested,
            ChatRunEvent::StatusNoteUpdated {
                note: Some("retry".into()),
            },
            ChatRunEvent::TextDelta {
                delta: "好".into(),
                segment: None,
            },
            ChatRunEvent::StreamAttemptDiscarded {
                text_chars: 1,
                reasoning_chars: 4,
                segment_ids: Vec::new(),
                tool_ids: vec!["tool-secret".into()],
            },
        ];
        let updates: Vec<_> = hidden.iter().filter_map(im_visible_text).collect();
        assert_eq!(
            updates,
            vec![
                ImVisibleText::Append("你".into()),
                ImVisibleText::Append("好".into()),
                ImVisibleText::DiscardChars(1),
            ]
        );
        let mut text = String::new();
        for update in &updates {
            apply_visible_text(&mut text, update);
        }
        assert_eq!(text, "你");
    }

    #[test]
    fn output_files_keep_real_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("answer.txt");
        std::fs::write(&file, "ok").unwrap();
        let mut paths = Vec::new();
        remember_output_file(&mut paths, &file);
        remember_output_file(&mut paths, &file);
        remember_output_file(&mut paths, dir.path());
        remember_output_file(&mut paths, &dir.path().join("missing.txt"));
        assert_eq!(paths, vec![file.to_string_lossy().into_owned()]);
    }

    #[test]
    fn working_directory_mapping_is_isolated_per_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("im-working-directories.json");
        crate::chat::storage::write_im_working_directory_at(&path, "conv_a", Some("/work/a"))
            .unwrap();
        crate::chat::storage::write_im_working_directory_at(&path, "conv_b", Some("/work/b"))
            .unwrap();
        assert_eq!(
            crate::chat::storage::read_im_working_directory_at(&path, "conv_a").as_deref(),
            Some("/work/a")
        );
        crate::chat::storage::write_im_working_directory_at(&path, "conv_a", None).unwrap();
        assert!(crate::chat::storage::read_im_working_directory_at(&path, "conv_a").is_none());
        assert_eq!(
            crate::chat::storage::read_im_working_directory_at(&path, "conv_b").as_deref(),
            Some("/work/b")
        );
    }

    fn message(id: &str, role: &str, content: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "role": role,
            "content": content,
            "timestamp": 1
        }))
        .unwrap()
    }

    fn inbound(is_group: bool, thread_id: Option<&str>) -> InboundMessage {
        InboundMessage {
            platform: crate::im::types::ImPlatform::Feishu,
            message_id: "in_1".into(),
            chat_id: "chat_1".into(),
            user_id: "user_1".into(),
            user_name: "Ada".into(),
            is_group,
            thread_id: thread_id.map(str::to_string),
            reply_token: Some("token".into()),
            text: "hello".into(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn new_turn_does_not_replay_an_old_assistant_or_its_files() {
        let old = message("old", "assistant", "OLD SECRET");
        let user = message("user", "user", "new question");
        let prior = prior_assistant_ids(std::slice::from_ref(&old));

        let without_reply = [old.clone(), user.clone()];
        let (text, fresh) = compose_turn_outbound(&without_reply, &prior, "");
        assert!(
            fresh.is_empty(),
            "a turn that never generated must not select the previous assistant"
        );
        assert_eq!(text, "");
        assert_ne!(text, old.content);

        let fresh_assistant = message("new", "assistant", "this turn");
        let with_reply = [old.clone(), user, fresh_assistant];
        let (text, fresh) = compose_turn_outbound(&with_reply, &prior, "streamed tail");
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "new");
        assert_eq!(text, "this turn");
        assert!(!text.contains("OLD SECRET"));
    }

    #[test]
    fn cleared_shared_model_and_assistant_follow_current_defaults() {
        let mut conversation: crate::chat::Conversation =
            serde_json::from_value(serde_json::json!({
                "id": "conv_align",
                "title": "im",
                "provider_id": "shared-provider",
                "model": "shared-model",
                "messages": [],
                "created_at": 1,
                "updated_at": 1,
                "active_skill_id": "skill-from-assistant",
                "assistant_id": "asst_old",
                "assistant_snapshot": {
                    "id": "asst_old",
                    "name": "Old",
                    "provider_id": "assistant-provider",
                    "model": "assistant-model",
                    "system_prompt": "stay in character"
                }
            }))
            .unwrap();
        let defaults = (
            "settings-provider".to_string(),
            "settings-model".to_string(),
        );

        let (provider_id, model) = resolved_im_model("  ", "", defaults.clone());
        apply_im_conversation_alignment(&mut conversation, &provider_id, &model, None);
        assert_eq!(conversation.provider_id, "settings-provider");
        assert_eq!(conversation.model, "settings-model");
        assert_ne!(conversation.model, "shared-model");
        assert_ne!(conversation.model, "assistant-model");
        assert!(conversation.assistant_id.is_none());
        assert!(conversation.assistant_snapshot.is_none());
        assert!(conversation.active_skill_id.is_none());

        let (provider_id, model) = resolved_im_model("only-provider", " ", defaults.clone());
        apply_im_conversation_alignment(&mut conversation, &provider_id, &model, None);
        assert_eq!(
            (
                conversation.provider_id.as_str(),
                conversation.model.as_str()
            ),
            ("settings-provider", "settings-model"),
            "half-cleared shared model must not keep the leftover provider"
        );

        let snapshot = crate::chat::ChatAssistantSnapshot {
            id: "asst_new".into(),
            name: "New".into(),
            description: String::new(),
            source: String::new(),
            system_prompt: String::new(),
            provider_id: "assistant-provider".into(),
            model: "assistant-model".into(),
            mcp_server_ids: Vec::new(),
            skill_ids: vec!["skill-from-assistant".into()],
        };
        conversation.active_skill_id = Some("skill-from-assistant".into());
        let (provider_id, model) = resolved_im_model("custom-provider", "custom-model", defaults);
        apply_im_conversation_alignment(&mut conversation, &provider_id, &model, Some(snapshot));
        assert_eq!(conversation.provider_id, "custom-provider");
        assert_eq!(conversation.model, "custom-model");
        assert_eq!(conversation.assistant_id.as_deref(), Some("asst_new"));
        assert!(conversation.active_skill_id.is_none());
    }

    /// In-process stand-in for `conversations::send`: same reservation primitive as
    /// `send_attached_when_idle` (`try_reserve_send` / `reserve_send_when_idle`).
    /// A full `send` smoke still needs a Tauri `AppHandle` so the repository and
    /// protocol can persist the turn; wire that entry when a test app exists.
    #[tokio::test]
    async fn busy_reservation_stops_before_generation_and_does_not_approve_desktop() {
        let state = crate::state::test_app_state();
        let id = "conv_im_smoke";
        let queued_desktop = state.chat_runtime().begin_generation(id);
        assert!(
            !stop_admitted_im_turn(&state, id),
            "stop while the IM turn is still queued must not cancel a desktop generation"
        );
        assert!(state
            .chat_runtime()
            .is_generation_active(id, queued_desktop));
        state.chat_runtime().end_generation(id, queued_desktop);

        assert!(
            state.chat_runtime().try_reserve_send(id, "im-send"),
            "send reservation is the interface desktop must wait on"
        );
        state.chat_runtime().begin_im_turn(id);
        assert!(
            !state.chat_runtime().try_reserve_send(id, "desktop-during"),
            "desktop cannot enter while this IM send still holds the conversation"
        );

        let im_generation = state
            .chat_runtime()
            .begin_generation_unless_im_stopped(id)
            .expect("admitted turn can start one generation");
        let desktop_generation = state.chat_runtime().begin_generation(id);
        assert_eq!(
            state
                .chat_runtime()
                .note_im_child(id, im_generation, "im-child"),
            crate::chat::runtime_state::ImChild::Inherited
        );
        assert_eq!(
            state
                .chat_runtime()
                .note_im_child(id, desktop_generation, "desktop-child"),
            crate::chat::runtime_state::ImChild::NotIm
        );
        let mut model_requests = 0;
        let mut tool_runs = 0;
        assert!(stop_admitted_im_turn(&state, id));
        if state
            .chat_runtime()
            .begin_generation_unless_im_stopped(id)
            .is_some()
        {
            model_requests += 1;
            tool_runs += 1;
        }
        assert_eq!(model_requests, 0);
        assert_eq!(tool_runs, 0);
        assert!(!state.chat_runtime().is_generation_active(id, im_generation));
        assert!(
            state
                .chat_runtime()
                .is_generation_active(id, desktop_generation),
            "stopping this IM lineage must leave the desktop generation running"
        );
        state.chat_runtime().end_generation(id, desktop_generation);

        let old = message("old", "assistant", "OLD SECRET");
        let prior = prior_assistant_ids(std::slice::from_ref(&old));
        let (text, fresh) = compose_turn_outbound(std::slice::from_ref(&old), &prior, "");
        assert!(fresh.is_empty());
        assert_eq!(text, "");

        state.chat_runtime().end_im_turn(id);
        assert!(
            !crate::chat::runtime_state::im_turn_allows_tools(
                state.chat_runtime().im_turn_active(id)
            ),
            "approval clears before the IM reservation is released"
        );
        assert!(!state
            .chat_runtime()
            .try_reserve_send(id, "desktop-still-busy"));
        state.chat_runtime().end_reply(id, "im-send");

        assert!(state.chat_runtime().try_reserve_send(id, "desktop"));
        let desktop = state.chat_runtime().begin_generation(id);
        assert!(
            !stop_admitted_im_turn(&state, id),
            "stopping a finished IM turn must not latch the desktop generation"
        );
        assert!(state.chat_runtime().is_generation_active(id, desktop));
        assert!(!crate::chat::runtime_state::im_session_consent(
            state.chat_runtime().im_turn_active(id),
            false
        ));

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let desktop_text = "desktop reply that IM must not forward";
        tx.send(desktop_text).await.unwrap();
        let blocked = tokio::spawn(async move {
            tx.send(desktop_text).await.unwrap();
        });
        tokio::task::yield_now().await;
        assert_ne!(text, desktop_text);
        assert!(!crate::chat::runtime_state::im_turn_allows_tools(
            state.chat_runtime().im_turn_active(id)
        ));
        assert_eq!(rx.recv().await.unwrap(), desktop_text);
        blocked.await.unwrap();
        state.chat_runtime().end_reply(id, "desktop");
    }
}
