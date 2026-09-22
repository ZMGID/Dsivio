//! Agent host that never writes a sidebar conversation.
//! Automation (`auto_{id}`) and Workbench (`wb_{id}`) both use it: tool approval
//! and session consent are granted, and text is accumulated in memory.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::chat::agent::{AgentHost, AgentHostFuture, ToolExecutionContext, ToolExecutor, ToolExecutorFuture};
use crate::chat::ask_user::{AskUserPromptPayload, AskUserResponseResult};
use crate::chat::types::{ChatMessageSegment, ToolCallRecord};
use crate::mcp::ChatToolDefinition;
use crate::skills;
use crate::state::AppState;

pub(crate) const AI_TASK_DELTA_EVENT: &str = "ai-task-delta";

pub(crate) struct HeadlessAgentHost {
    app: AppHandle,
    text: Mutex<String>,
    /// When set, each text delta is also emitted as `ai-task-delta`.
    stream_task_id: Option<String>,
}

impl HeadlessAgentHost {
    pub(crate) fn silent(app: AppHandle) -> Self {
        Self { app, text: Mutex::new(String::new()), stream_task_id: None }
    }

    pub(crate) fn streaming(app: AppHandle, task_id: String) -> Self {
        Self { app, text: Mutex::new(String::new()), stream_task_id: Some(task_id) }
    }

    pub(crate) fn collected_text(&self) -> String {
        self.text.lock().map(|guard| guard.clone()).unwrap_or_default()
    }
}

impl AgentHost for HeadlessAgentHost {
    fn emit_stream_delta(
        &self,
        _conversation_id: &str,
        _run_id: &str,
        _message_id: &str,
        delta: &str,
        _reasoning_delta: Option<&str>,
        _segment: Option<&ChatMessageSegment>,
    ) {
        if delta.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.text.lock() {
            guard.push_str(delta);
        }
        if let Some(task_id) = &self.stream_task_id {
            let _ = self.app.emit(AI_TASK_DELTA_EVENT, json!({ "taskId": task_id, "delta": delta }));
        }
    }

    fn emit_tool_record(
        &self,
        _conversation_id: &str,
        _run_id: &str,
        _message_id: &str,
        _record: &ToolCallRecord,
    ) {
    }

    fn request_tool_approval<'a>(
        &'a self,
        _ctx: &'a ToolExecutionContext<'a>,
        _record: &'a ToolCallRecord,
    ) -> AgentHostFuture<'a, bool> {
        Box::pin(async { true })
    }

    fn request_session_consent<'a>(
        &'a self,
        _ctx: &'a ToolExecutionContext<'a>,
    ) -> AgentHostFuture<'a, bool> {
        Box::pin(async { true })
    }

    fn request_user_response<'a>(
        &'a self,
        _ctx: &'a ToolExecutionContext<'a>,
        _record: &'a ToolCallRecord,
        _prompt: AskUserPromptPayload,
    ) -> AgentHostFuture<'a, AskUserResponseResult> {
        Box::pin(async {
            AskUserResponseResult { phase: "cancelled".to_string(), answers: HashMap::new() }
        })
    }

    fn is_generation_active(&self, conversation_id: &str, generation: u64) -> bool {
        self.app.state::<AppState>().chat_runtime().is_generation_active(conversation_id, generation)
    }

    fn wait_for_generation_inactive<'a>(
        &'a self,
        conversation_id: &'a str,
        generation: u64,
    ) -> AgentHostFuture<'a, ()> {
        Box::pin(async move {
            loop {
                if !self.is_generation_active(conversation_id, generation) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    }
}

pub(crate) struct HeadlessToolExecutor {
    pub(crate) app: AppHandle,
}

impl ToolExecutor for HeadlessToolExecutor {
    fn prepare_result<'a>(
        &'a self,
        ctx: &ToolExecutionContext<'_>,
        tool: &ChatToolDefinition,
        arguments: &Value,
        output: crate::mcp::types::McpToolCallResult,
    ) -> ToolExecutorFuture<'a> {
        crate::chat::artifacts::prepare_output(
            &self.app,
            ctx.tool_conversation_id,
            ctx.message_id,
            tool,
            arguments,
            output,
        )
    }

    fn call<'a>(
        &'a self,
        ctx: &'a ToolExecutionContext<'a>,
        tool: &'a ChatToolDefinition,
        arguments: Value,
        skill_cache: Option<&'a mut skills::SkillRunCache>,
    ) -> ToolExecutorFuture<'a> {
        Box::pin(async move {
            let native_ctx = crate::mcp::registry::NativeToolContext {
                conversation_id: ctx.tool_conversation_id.to_string(),
                message_id: ctx.message_id.to_string(),
                tool_call_id: Some(ctx.tool_call_id.to_string()),
                run_id: ctx.run_id.to_string(),
                generation: ctx.generation,
                depth: ctx.depth,
            };
            crate::mcp::registry::call_tool(
                &self.app,
                &self.app.state::<AppState>(),
                tool,
                arguments,
                skill_cache,
                Some(native_ctx),
            )
            .await
        })
    }
}

/// Memory, automation control, and sub-agent tools stay off every headless run.
pub(crate) fn is_headless_forbidden_tool(tool: &ChatToolDefinition) -> bool {
    tool.name.starts_with("memory_")
        || tool.id.contains("memory_")
        || tool.name.starts_with("automation_")
        || tool.id.contains("automation_")
        || crate::chat::sub_agent::is_sub_agent_tool_name(&tool.name)
}

/// Keep tools the caller named, then drop the forbidden set.
/// An empty allow-list means no tools. A non-empty list that matches nothing is an error.
pub(crate) fn select_headless_tools(
    mut tools: Vec<ChatToolDefinition>,
    allow: &[String],
) -> Result<Vec<ChatToolDefinition>, String> {
    if allow.is_empty() {
        return Ok(Vec::new());
    }
    tools.retain(|tool| {
        !is_headless_forbidden_tool(tool)
            && allow.iter().any(|entry| crate::chat::agent::filter::entry_matches(tool, entry))
    });
    if tools.is_empty() {
        return Err("None of the selected tools are currently available".into());
    }
    Ok(tools)
}
