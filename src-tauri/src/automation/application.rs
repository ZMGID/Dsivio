//! Application coordinator for `action.agent` nodes.
//! Auto-approves tools (unattended schedule/hotkey cannot prompt). Built-in
//! and Chat runs do not write a sidebar conversation (`auto_{id}` workspace
//! only). External CLI needs a `conv_` session, so it uses an archived
//! `conv_auto_{id}` conversation that stays off the default sidebar.

use std::collections::HashSet;
use std::path::Path;

use tauri::{AppHandle, Manager};

use crate::chat::agent::headless_host::{is_headless_forbidden_tool, HeadlessAgentHost, HeadlessToolExecutor};
use crate::chat::agent::prepare::{available_builtin_tool_names, build_chat_system_prompt};
use crate::chat::agent::{run_agent_loop, AgentRunConfig};
use crate::chat::types::{AgentRuntimeKind, WebSearchMode};
use crate::mcp::ChatToolDefinition;
use crate::skills;
use crate::state::AppState;

use super::types::{AgentNodeRequest, NodeOutput};
use super::workspace;

pub(crate) fn begin_run(app: &AppHandle, automation_id: &str, run_id: &str) -> Result<(), String> {
    app.state::<AppState>()
        .automation_runs
        .begin(automation_id, run_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn finish_run_slot(app: &AppHandle, automation_id: &str, run_id: &str) {
    app.state::<AppState>()
        .automation_runs
        .finish(automation_id, run_id);
}

pub(crate) fn active_automation_ids(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>()
        .automation_runs
        .active_automation_ids()
}

pub(crate) fn automation_runs_empty(app: &AppHandle) -> bool {
    app.state::<AppState>().automation_runs.is_empty()
}

pub(crate) fn mark_run_cancelled(app: &AppHandle, automation_id: &str) -> bool {
    app.state::<AppState>()
        .automation_runs
        .mark_cancelled(automation_id)
        .is_some()
}

pub(crate) fn is_run_cancelled(app: &AppHandle, run_id: &str) -> bool {
    app.state::<AppState>().automation_runs.is_cancelled(run_id)
}

trait ChatCancelPort {
    fn cancel_generation(&self, conversation_id: &str);
}

trait AutomationActivityPort {
    fn chat_generation_active(&self, conversation_id: &str, generation: u64) -> bool;
    fn automation_run_active(&self, automation_id: &str, run_id: &str) -> bool;
}

impl AutomationActivityPort for AppState {
    fn chat_generation_active(&self, conversation_id: &str, generation: u64) -> bool {
        self.chat_runtime()
            .is_generation_active(conversation_id, generation)
    }

    fn automation_run_active(&self, automation_id: &str, run_id: &str) -> bool {
        self.automation_runs.is_active(automation_id, run_id)
    }
}

fn chat_owner_active_with(
    port: &dyn AutomationActivityPort,
    chat_generation: Option<(&str, u64)>,
) -> bool {
    chat_generation
        .map(|(conversation_id, generation)| {
            port.chat_generation_active(conversation_id, generation)
        })
        .unwrap_or(true)
}

pub(crate) fn chat_owner_active(app: &AppHandle, chat_generation: Option<(&str, u64)>) -> bool {
    let state = app.state::<AppState>();
    chat_owner_active_with(&*state, chat_generation)
}

pub(crate) fn automation_run_active(app: &AppHandle, automation_id: &str, run_id: &str) -> bool {
    let state = app.state::<AppState>();
    automation_run_active_with(&*state, automation_id, run_id)
}

fn automation_run_active_with(
    port: &dyn AutomationActivityPort,
    automation_id: &str,
    run_id: &str,
) -> bool {
    port.automation_run_active(automation_id, run_id)
}

impl ChatCancelPort for AppState {
    fn cancel_generation(&self, conversation_id: &str) {
        self.cancel_chat_generation(conversation_id);
    }
}

fn cancel_agent_generations_with(
    port: &dyn ChatCancelPort,
    automation_id: &str,
    agent_node_ids: impl IntoIterator<Item = String>,
) {
    port.cancel_generation(&workspace::conversation_id(automation_id));
    port.cancel_generation(&workspace::external_conversation_id(automation_id, ""));
    for node_id in agent_node_ids {
        port.cancel_generation(&workspace::external_conversation_id(
            automation_id,
            &node_id,
        ));
    }
}

pub(crate) fn cancel_agent_generations(
    app: &AppHandle,
    automation_id: &str,
    agent_node_ids: impl IntoIterator<Item = String>,
) {
    let state = app.state::<AppState>();
    cancel_agent_generations_with(&*state, automation_id, agent_node_ids);
}

pub(crate) fn settings_language(app: &AppHandle) -> String {
    app.state::<AppState>()
        .settings_read()
        .settings_language
        .clone()
        .unwrap_or_else(|| "zh".to_string())
}

pub(crate) fn automation_working_directory(app: &AppHandle) -> String {
    app.state::<AppState>()
        .settings_read()
        .chat_tools
        .native_tools
        .working_directory
        .clone()
}

pub(crate) fn http_client(app: &AppHandle) -> reqwest::Client {
    app.state::<AppState>().http.clone()
}

pub(crate) async fn run_captured_command(
    app: &AppHandle,
    command: &str,
    cwd: std::path::PathBuf,
    timeout_ms: u64,
) -> Result<crate::native_tools::CapturedCommand, String> {
    let state = app.state::<AppState>();
    crate::native_tools::run_captured_command(command, cwd, timeout_ms, Some(&*state)).await
}

pub(crate) async fn run_agent_node(
    app: &AppHandle,
    request: AgentNodeRequest,
) -> Result<NodeOutput, String> {
    let spec = AgentSpec::from_json(&request.spec);
    let prompt = spec.prompt.trim();
    if prompt.is_empty() {
        return Err("Agent prompt is empty".to_string());
    }
    if spec.runtime_kind == AgentRuntimeKind::External {
        return Err("Local CLI support has been removed; select Dsivio Agent.".into());
    }
    run_builtin_agent_node(app, &request.automation_id, &request.run_id, prompt, &spec).await
}

struct AgentSpec {
    prompt: String,
    runtime_kind: AgentRuntimeKind,
    external_agent_id: Option<String>,
    external_model: Option<String>,
    provider_id: Option<String>,
    model: Option<String>,
    tool_ids: Vec<String>,
    skill_ids: Vec<String>,
}

impl AgentSpec {
    fn from_json(value: &serde_json::Value) -> Self {
        let str_field = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let mut skill_ids = json_string_list(value.get("skillIds"));
        if let Some(legacy) = str_field("skillId") {
            if !skill_ids.iter().any(|id| id == &legacy) {
                skill_ids.insert(0, legacy);
            }
        }
        let runtime_kind = match str_field("runtimeKind").as_deref() {
            Some("chat") => AgentRuntimeKind::Builtin,
            Some("external") => AgentRuntimeKind::External,
            _ => AgentRuntimeKind::Builtin,
        };
        Self {
            prompt: value
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            runtime_kind,
            external_agent_id: str_field("externalAgentId"),
            external_model: str_field("externalModel"),
            provider_id: str_field("providerId"),
            model: str_field("model"),
            tool_ids: json_string_list(value.get("toolIds")),
            skill_ids,
        }
    }
}

fn json_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for item in items {
        let Some(id) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        if seen.insert(id.to_string()) {
            ids.push(id.to_string());
        }
    }
    ids
}

async fn run_builtin_agent_node(
    app: &AppHandle,
    automation_id: &str,
    run_id: &str,
    prompt: &str,
    spec: &AgentSpec,
) -> Result<NodeOutput, String> {
    let state = app.state::<AppState>();
    let state: &AppState = &state;
    let settings = state.settings_read().clone();
    let (provider_id, model) = resolve_kivio_model(&settings, spec)?;
    let provider = settings
        .get_provider(&provider_id)
        .filter(|p| p.enabled && p.has_credentials())
        .cloned()
        .ok_or_else(|| {
            "Configure a chat provider and model in Settings before running an Agent step"
                .to_string()
        })?;

    let conversation_id = workspace::conversation_id(automation_id);
    state
        .chat_interactions()
        .grant_session_consent(&conversation_id);
    let generation = state.chat_runtime().begin_generation(&conversation_id);
    let message_id = format!("auto-msg-{run_id}");

    let catalog = crate::mcp::registry::list_enabled_tool_catalog(app, state).await;
    let mut tools = catalog.tools;
    let is_chat = spec.runtime_kind == AgentRuntimeKind::Chat;
    if is_chat {
        crate::chat::commands::apply_chat_mode_tool_filter(
            &mut tools,
            true,
            &settings.chat.chat_mode,
        );
    }
    apply_agent_tool_whitelist(&mut tools, &spec.tool_ids)?;
    strip_workflow_forbidden_tools(&mut tools);
    if !is_chat {
        ensure_skill_activate_tool(&mut tools);
    }
    let builtin_names = available_builtin_tool_names(&tools);

    let workdir = workspace::workbench_dir(
        &settings.chat_tools.native_tools.working_directory,
        automation_id,
    );
    let workdir_str = workdir.as_ref().map(|p| p.to_string_lossy().into_owned());
    let registry = skills::build_registry_in(
        app,
        &settings.chat_tools.skill_scan_paths,
        workdir.as_deref(),
    )
    .unwrap_or_default();
    let active_skill_id = spec.skill_ids.first().map(|id| id.as_str());
    let active_skill_detail = active_skill_id.and_then(|id| {
        skills::read_skill_detail_in(
            app,
            &settings.chat_tools.skill_scan_paths,
            id,
            workdir.as_deref(),
        )
        .ok()
    });

    let mut effective_chat_tools = settings.chat_tools.clone();
    effective_chat_tools.approval_policy = "auto".to_string();
    let tools_available = !tools.is_empty();
    let language = settings
        .settings_language
        .clone()
        .unwrap_or_else(|| "zh".to_string());
    let custom_system_prompt = settings.chat.system_prompt.clone();
    let obsidian_vault_path = (!settings.obsidian_vault_path.trim().is_empty())
        .then_some(settings.obsidian_vault_path.as_str());
    let additional_directories: [crate::chat::types::AdditionalDirectory; 0] = [];
    let mut system_prompt = build_chat_system_prompt(
        &language,
        false,
        true,
        &registry,
        &effective_chat_tools,
        tools_available,
        &builtin_names,
        active_skill_id,
        active_skill_detail.as_ref(),
        None,
        None,
        &custom_system_prompt,
        is_chat,
        None,
        None,
        None,
        None,
        None,
        workdir_str.as_deref(),
        None,
        obsidian_vault_path,
        &additional_directories,
    );
    if let Some(extra) = extra_skill_bodies(
        app,
        &settings.chat_tools.skill_scan_paths,
        workdir.as_deref(),
        &spec.skill_ids,
    ) {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&extra);
    }

    let runtime_messages = vec![
        serde_json::json!({ "role": "system", "content": system_prompt }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];

    let host = HeadlessAgentHost::silent(app.clone());
    let executor = HeadlessToolExecutor { app: app.clone() };
    let retry_attempts = if settings.retry_enabled {
        settings.retry_attempts as usize
    } else {
        1
    };
    let max_output_tokens = settings.chat.max_output_tokens;
    let web_search_mode = WebSearchMode::resolve(None, &settings);

    let config = AgentRunConfig {
        provider_runtime: state,
        conversation_id: conversation_id.clone(),
        tool_conversation_id: conversation_id.clone(),
        depth: 0,
        run_id: format!("auto-run-{run_id}"),
        message_id,
        generation,
        provider,
        model,
        runtime_messages,
        tools,
        blocked_tool_calls: Vec::new(),
        settings,
        effective_chat_tools,
        language,
        thinking_enabled: true,
        thinking_level: None,
        web_search_mode,
        max_output_tokens,
        retry_attempts,
        assistant_snapshot: None,
        provider_tools_fallback_system_prompt: system_prompt,
        initial_anchor_total_tokens: None,
        initial_anchor_trailing_estimate: 0,
        skill_project_cwd: workdir,
    };

    let outcome = run_agent_loop(config, &host, &executor).await;
    state
        .chat_runtime()
        .end_generation(&conversation_id, generation);
    match outcome {
        Ok(result) => {
            let text = if result.content.trim().is_empty() {
                host.collected_text()
            } else {
                result.content
            };
            if text.trim().is_empty() {
                Err("Agent returned an empty response".to_string())
            } else {
                Ok(NodeOutput::from_text(text))
            }
        }
        Err(err) if err == "cancelled" => Err("cancelled".to_string()),
        Err(err) => Err(err),
    }
}

fn resolve_kivio_model(
    settings: &crate::settings::Settings,
    spec: &AgentSpec,
) -> Result<(String, String), String> {
    match (&spec.provider_id, &spec.model) {
        (Some(provider_id), Some(model)) => Ok((provider_id.clone(), model.clone())),
        (None, None) => Ok(settings.effective_chat_model()),
        _ => Err(
            "Agent model is incomplete: set both provider and model, or leave both empty".into(),
        ),
    }
}

fn apply_agent_tool_whitelist(
    tools: &mut Vec<ChatToolDefinition>,
    ids: &[String],
) -> Result<(), String> {
    // Read-only tools and the `skill` loader are always mounted so the Skill
    // slot can load bodies and the model can still `read` / search. `toolIds`
    // only opts in write/side-effect tools. Memory never mounts.
    tools.retain(|tool| {
        if is_headless_forbidden_tool(tool) {
            return false;
        }
        if is_always_on_automation_tool(tool) {
            return true;
        }
        ids.iter()
            .any(|entry| crate::chat::agent::filter::entry_matches(tool, entry))
    });
    if !ids.is_empty() && tools.is_empty() {
        return Err("None of the selected tools are currently available".into());
    }
    Ok(())
}

fn is_skill_activate_tool(tool: &ChatToolDefinition) -> bool {
    tool.source == "skill" || tool.name == "skill"
}

fn is_always_on_automation_tool(tool: &ChatToolDefinition) -> bool {
    !is_headless_forbidden_tool(tool) && (is_skill_activate_tool(tool) || tool.is_read_only_tool())
}

fn ensure_skill_activate_tool(tools: &mut Vec<ChatToolDefinition>) {
    if tools.iter().any(is_skill_activate_tool) {
        return;
    }
    tools.push(crate::mcp::types::native_skill_activate_tool());
}

fn strip_workflow_forbidden_tools(tools: &mut Vec<ChatToolDefinition>) {
    tools.retain(|tool| !is_headless_forbidden_tool(tool));
}

fn extra_skill_bodies(
    app: &AppHandle,
    scan_paths: &[String],
    workdir: Option<&Path>,
    skill_ids: &[String],
) -> Option<String> {
    if skill_ids.len() <= 1 {
        return None;
    }
    let mut parts = Vec::new();
    for id in skill_ids.iter().skip(1) {
        let Ok(detail) = skills::read_skill_detail_in(app, scan_paths, id, workdir) else {
            continue;
        };
        let name = if detail.meta.name.trim().is_empty() {
            id.as_str()
        } else {
            detail.meta.name.as_str()
        };
        let body = detail.body.trim();
        if body.is_empty() {
            continue;
        }
        parts.push(format!("## Skill: {name}\n\n{body}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!(
            "Additional mounted skills (read-only context):\n\n{}",
            parts.join("\n\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeChatCancelPort {
        cancelled: Mutex<Vec<String>>,
    }

    struct FakeActivityPort {
        chat_active: bool,
        run_active: bool,
    }

    impl AutomationActivityPort for FakeActivityPort {
        fn chat_generation_active(&self, _conversation_id: &str, _generation: u64) -> bool {
            self.chat_active
        }

        fn automation_run_active(&self, _automation_id: &str, _run_id: &str) -> bool {
            self.run_active
        }
    }

    impl ChatCancelPort for FakeChatCancelPort {
        fn cancel_generation(&self, conversation_id: &str) {
            self.cancelled
                .lock()
                .unwrap()
                .push(conversation_id.to_string());
        }
    }

    #[test]
    fn cancellation_crosses_the_chat_port_for_every_agent_workspace() {
        let port = FakeChatCancelPort::default();
        cancel_agent_generations_with(
            &port,
            "daily",
            ["summarize".to_string(), "publish".to_string()],
        );
        assert_eq!(
            *port.cancelled.lock().unwrap(),
            vec![
                workspace::conversation_id("daily"),
                workspace::external_conversation_id("daily", ""),
                workspace::external_conversation_id("daily", "summarize"),
                workspace::external_conversation_id("daily", "publish"),
            ]
        );
    }

    #[test]
    fn tool_activity_checks_are_owned_by_the_application_port() {
        let active = FakeActivityPort {
            chat_active: true,
            run_active: true,
        };
        assert!(chat_owner_active_with(&active, None));
        assert!(chat_owner_active_with(&active, Some(("conversation", 7))));
        assert!(automation_run_active_with(&active, "automation", "run"));

        let cancelled = FakeActivityPort {
            chat_active: false,
            run_active: false,
        };
        assert!(!chat_owner_active_with(
            &cancelled,
            Some(("conversation", 7))
        ));
        assert!(!automation_run_active_with(&cancelled, "automation", "run"));
    }

    #[test]
    fn automation_runner_has_no_app_state_dependency() {
        let runner = include_str!("runner.rs");
        assert!(!runner.contains("AppState"));
        assert!(!runner.contains("state::<"));
        let tools = include_str!("tools.rs");
        assert!(!tools.contains("AppState"));
        assert!(!tools.contains("state::<"));
    }

    #[test]
    fn spec_defaults_to_builtin_and_merges_legacy_skill() {
        let spec = AgentSpec::from_json(&json!({
            "prompt": "  hello  ",
            "skillId": "pdf",
            "skillIds": ["docx", "pdf"],
        }));
        assert_eq!(spec.runtime_kind, AgentRuntimeKind::Builtin);
        assert_eq!(spec.prompt, "  hello  ");
        assert_eq!(spec.skill_ids, vec!["docx", "pdf"]);
        assert!(spec.tool_ids.is_empty());
    }

    #[test]
    fn spec_parses_external_runtime_and_tool_whitelist() {
        let spec = AgentSpec::from_json(&json!({
            "runtimeKind": "external",
            "externalAgentId": "claude",
            "externalModel": "sonnet",
            "toolIds": ["read", "read", "", "glob"],
            "skillIds": ["pdf"],
        }));
        assert_eq!(spec.runtime_kind, AgentRuntimeKind::External);
        assert_eq!(spec.external_agent_id.as_deref(), Some("claude"));
        assert_eq!(spec.external_model.as_deref(), Some("sonnet"));
        assert_eq!(spec.tool_ids, vec!["read", "glob"]);
    }

    #[test]
    fn spec_parses_chat_runtime_and_model_override() {
        let spec = AgentSpec::from_json(&json!({
            "runtimeKind": "chat",
            "providerId": "openai",
            "model": "gpt-4.1",
        }));
        assert_eq!(spec.runtime_kind, AgentRuntimeKind::Builtin);
        assert_eq!(spec.provider_id.as_deref(), Some("openai"));
        assert_eq!(spec.model.as_deref(), Some("gpt-4.1"));
    }

    fn tool(id: &str, name: &str) -> ChatToolDefinition {
        ChatToolDefinition {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            source: "native".into(),
            server_id: None,
            server_name: None,
            input_schema: json!({}),
            sensitive: false,
            annotations: None,
            output_schema: None,
        }
    }

    fn skill_tool() -> ChatToolDefinition {
        let mut tool = tool("skill__activate", "skill");
        tool.source = "skill".into();
        tool
    }

    #[test]
    fn empty_tool_ids_keeps_read_only_and_skill() {
        let mut tools = vec![
            tool("native__read", "read"),
            tool("native__run_command", "bash"),
            tool("native__memory_read", "memory_read"),
            skill_tool(),
        ];
        apply_agent_tool_whitelist(&mut tools, &[]).unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["read", "skill"]);
    }

    #[test]
    fn write_tools_are_opt_in_on_top_of_read_only() {
        let mut tools = vec![
            tool("native__read", "read"),
            tool("native__run_command", "bash"),
            tool("native__write_file", "write"),
            skill_tool(),
        ];
        apply_agent_tool_whitelist(&mut tools, &["bash".into()]).unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["read", "bash", "skill"]);
    }

    #[test]
    fn memory_tools_are_stripped_even_when_selected() {
        let mut tools = vec![
            tool("native__read", "read"),
            tool("native__memory_read", "memory_read"),
            tool("native__memory_search", "memory_search"),
            tool("native__memory_modify", "memory_modify"),
        ];
        apply_agent_tool_whitelist(
            &mut tools,
            &["native__read".into(), "native__memory_read".into()],
        )
        .unwrap();
        strip_workflow_forbidden_tools(&mut tools);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read");
    }

    #[test]
    fn automation_control_tools_are_stripped_even_when_read_only() {
        let mut tools = vec![
            tool("native__read", "read"),
            tool("native__automation_list", "automation_list"),
            tool("native__automation_run", "automation_run"),
        ];
        apply_agent_tool_whitelist(&mut tools, &["automation_run".into()]).unwrap();
        strip_workflow_forbidden_tools(&mut tools);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read");
    }

    #[test]
    fn sub_agent_tool_is_stripped_even_when_selected() {
        let mut tools = vec![tool("native__read", "read"), tool("native__agent", "agent")];
        apply_agent_tool_whitelist(&mut tools, &["agent".into()]).unwrap();
        strip_workflow_forbidden_tools(&mut tools);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read");
    }

    #[test]
    fn ensure_skill_activate_tool_appends_when_missing() {
        let mut tools = vec![tool("native__read", "read")];
        ensure_skill_activate_tool(&mut tools);
        assert!(tools.iter().any(is_skill_activate_tool));
        let count = tools.len();
        ensure_skill_activate_tool(&mut tools);
        assert_eq!(tools.len(), count);
    }
}
