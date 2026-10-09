//! IM turns reuse the desktop chat pipeline. Identity mapping stays in the IM runtime.
//! This module creates or reuses the sidebar conversation and bridges one inbound turn.

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
        let state = self.app.state::<AppState>();
        if self.armed.load(Ordering::SeqCst) {
            state.chat_runtime().end_im_turn(&self.conversation_id);
        }
        state
            .chat_protocol()
            .unbind_im_text_stream(&self.conversation_id);
    }
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

async fn align_conversation(
    app: &AppHandle,
    config: &ImAgentConfig,
    conversation_id: &str,
) -> Result<(), String> {
    let provider_id = non_empty(&config.provider_id);
    let model = non_empty(&config.model);
    let assistant = match non_empty(&config.assistant_id) {
        Some(id) => Some(assistant_snapshot(app, &id)?),
        None => None,
    };
    crate::chat::repository::repository(app)
        .mutate(app, conversation_id, move |conversation| {
            if let Some(provider_id) = provider_id {
                conversation.provider_id = provider_id;
            }
            if let Some(model) = model {
                conversation.model = model;
            }
            if let Some(snapshot) = assistant {
                conversation.active_skill_id = None;
                conversation.assistant_id = Some(snapshot.id.clone());
                conversation.assistant_snapshot = Some(snapshot);
            }
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
    let app_for_turn = app.clone();
    let turn_id = conversation_id.to_string();
    let armed = Arc::clone(&guard.armed);
    let on_admitted = move || {
        let state = app_for_turn.state::<AppState>();
        state.chat_runtime().begin_im_turn(&turn_id);
        state
            .chat_protocol()
            .bind_im_text_stream(&turn_id, updates.clone());
        armed.store(true, Ordering::SeqCst);
    };

    let app_for_cancel = app.clone();
    let cancel_id = conversation_id.to_string();
    let mut cancel_shutdown = shutdown.clone();
    let canceller = AbortOnDrop(tokio::spawn(async move {
        loop {
            if *cancel_shutdown.borrow() {
                let state = app_for_cancel.state::<AppState>();
                if state.chat_runtime().im_turn_active(&cancel_id) {
                    state.cancel_chat_generation(&cancel_id);
                }
                break;
            }
            if cancel_shutdown.changed().await.is_err() {
                break;
            }
        }
    }));

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
    )
    .await;
    drop(canceller);
    // Drop the admitted sender before waiting, otherwise the forwarder never sees EOF.
    app.state::<AppState>()
        .chat_protocol()
        .unbind_im_text_stream(conversation_id);

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

    let saved = match load_conversation(app, conversation_id) {
        Ok(conversation) => conversation,
        Err(error) => {
            if *shutdown.borrow() {
                return Ok(());
            }
            return Err(error);
        }
    };
    let assistant = saved
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant");
    let text = assistant
        .map(|message| message.content.clone())
        .filter(|content| !content.is_empty())
        .unwrap_or(streamed);
    let attachments = assistant
        .map(|message| assistant_output_files(app, conversation_id, message))
        .unwrap_or_default();
    drop(guard);
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
}
