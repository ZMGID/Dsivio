//! One headless entry for Workbench AI calls.
//! `mode` only changes the tool-round cap, the timeout, and whether deltas stream.
//! The session id is `wb_{taskId}` and is never written to the sidebar.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tokio::time::timeout;
use ts_rs::TS;

use crate::chat::agent::headless_host::{select_headless_tools, HeadlessAgentHost, HeadlessToolExecutor};
use crate::chat::agent::prepare::{available_builtin_tool_names, build_chat_system_prompt};
use crate::chat::agent::{run_agent_loop, AgentRunConfig};
use crate::chat::model::ModelUsage;
use crate::chat::model_metadata::model_can_generate_images_directly;
use crate::chat::types::{ToolCallRecord, ToolCallStatus, WebSearchMode};
use crate::mcp::ChatToolDefinition;
use crate::settings::{ChatToolsConfig, ModelProvider, Settings};
use crate::state::AppState;

const MAX_IMAGES: usize = 16;
const ONCE_TIMEOUT_SECS: u64 = 60;
const AGENT_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AiTaskMode {
    Once,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AiTaskSlot {
    Chat,
    Vision,
    PromptOptimize,
    VideoAnalysis,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiTaskRequest {
    pub task_id: String,
    pub mode: AiTaskMode,
    #[serde(default)]
    pub system: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub images: Vec<String>,
    /// Local video inputs selected by a Workbench page; encoded by the shared video transport.
    #[serde(default)]
    #[ts(optional)]
    pub videos: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub slot: AiTaskSlot,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskToolCall {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskResult {
    pub text: String,
    pub tool_calls: Vec<AiTaskToolCall>,
    pub usage: Option<AiTaskUsage>,
}

fn localize(language: &str, zh: &str, en: &str) -> String {
    if language.starts_with("zh") { zh.to_string() } else { en.to_string() }
}

pub(crate) fn conversation_id(task_id: &str) -> String {
    format!("wb_{task_id}")
}

fn fallback_system(language: &str) -> String {
    localize(
        language,
        "你是工作台助手。只输出结果，不要解释。",
        "You are a workbench assistant. Output only the result, with no explanation.",
    )
}

fn validate(request: &AiTaskRequest, language: &str) -> Result<(), String> {
    if request.task_id.trim().is_empty() {
        return Err(localize(language, "缺少任务 id", "Missing task id"));
    }
    if request.prompt.trim().is_empty() {
        return Err(localize(language, "先输入要处理的内容", "Type something to run"));
    }
    if request.videos.as_ref().is_some_and(|videos| videos.len() > 1) {
        return Err(localize(language, "一次最多分析一个视频", "At most one video per analysis"));
    }
    if request.images.len() > MAX_IMAGES {
        return Err(localize(language, "一次最多 16 张参考图", "At most 16 reference images"));
    }
    if request.images.iter().any(|image| !image.starts_with("data:image/")) {
        return Err(localize(language, "参考图必须是图片 data URL", "Reference images must be image data URLs"));
    }
    Ok(())
}

/// Explicit provider+model wins. Images on the chat slot use the vision model.
pub(crate) fn resolve_model(settings: &Settings, request: &AiTaskRequest) -> Result<(String, String), String> {
    let language = crate::settings::resolve_chat_language(settings);
    let provider_id = nonempty(request.provider_id.as_deref());
    let model = nonempty(request.model.as_deref());
    let (provider_id, model) = match (provider_id, model) {
        (Some(provider_id), Some(model)) => (provider_id.to_string(), model.to_string()),
        (None, None) => {
            let slot = if request.videos.as_ref().is_some_and(|videos| !videos.is_empty()) {
                AiTaskSlot::VideoAnalysis
            } else if !request.images.is_empty() && request.slot == AiTaskSlot::Chat {
                AiTaskSlot::Vision
            } else {
                request.slot
            };
            match slot {
                AiTaskSlot::VideoAnalysis => (settings.default_models.video_analysis.provider_id.clone(), settings.default_models.video_analysis.model.clone()),
                AiTaskSlot::Chat => settings.effective_chat_model(),
                AiTaskSlot::Vision => settings.effective_vision_model(),
                AiTaskSlot::PromptOptimize => settings.effective_prompt_optimize_model_for_session(None),
            }
        }
        _ => {
            return Err(localize(
                &language,
                "模型和供应商必须同时指定，或同时留空",
                "Set both provider and model, or leave both empty",
            ))
        }
    };
    let Some(provider) = settings.get_provider(&provider_id) else {
        return Err(localize(&language, "未找到可用模型，请先在混音器或模型设置里配置", "No usable model. Configure one in Mixer or Models first."));
    };
    if !provider.enabled || !provider.has_credentials() || model.trim().is_empty() {
        return Err(localize(&language, "当前模型没有 API Key，请先在设置里填写", "This model has no API key. Add one in Settings."));
    }
    if model_can_generate_images_directly(provider, &model) {
        return Err(localize(&language, "生图模型不能用于这个调用，请另选模型", "Image models can't run this call. Pick another model."));
    }
    if request.videos.as_ref().is_some_and(|videos| !videos.is_empty()) { crate::chat::video::validate_model(provider, &model).map_err(|error| error.to_string())?; }
    Ok((provider_id, model))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

pub(crate) fn user_message(prompt: &str, images: &[String]) -> Value {
    if images.is_empty() {
        return json!({ "role": "user", "content": prompt });
    }
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    for url in images {
        content.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    json!({ "role": "user", "content": content })
}

pub(crate) fn apply_mode(tools: &mut ChatToolsConfig, mode: AiTaskMode) {
    tools.approval_policy = "auto".to_string();
    if mode == AiTaskMode::Once {
        tools.max_tool_rounds = Some(1);
    }
}

fn workdir_for(settings: &Settings, request: &AiTaskRequest) -> Option<PathBuf> {
    let raw = nonempty(request.cwd.as_deref())
        .unwrap_or(settings.chat_tools.native_tools.working_directory.trim());
    if raw.is_empty() { None } else { Some(PathBuf::from(raw)) }
}

fn system_prompt(app: &AppHandle, settings: &Settings, request: &AiTaskRequest, tools: &[ChatToolDefinition], workdir: Option<&std::path::Path>) -> String {
    let language = crate::settings::resolve_chat_language(settings);
    let caller = nonempty(request.system.as_deref());
    if tools.is_empty() {
        return caller.map(str::to_string).unwrap_or_else(|| fallback_system(&language));
    }
    let registry = crate::skills::build_registry_in(app, &settings.chat_tools.skill_scan_paths, workdir).unwrap_or_default();
    let builtin_names = available_builtin_tool_names(tools);
    let obsidian_vault_path = (!settings.obsidian_vault_path.trim().is_empty()).then_some(settings.obsidian_vault_path.as_str());
    let additional_directories: [crate::chat::types::AdditionalDirectory; 0] = [];
    let workdir_str = workdir.map(|path| path.to_string_lossy().into_owned());
    let mut prompt = build_chat_system_prompt(
        &language,
        !request.images.is_empty(),
        true,
        &registry,
        &settings.chat_tools,
        true,
        &builtin_names,
        None,
        None,
        None,
        None,
        &settings.chat.system_prompt,
        false,
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
    if let Some(extra) = caller {
        prompt.push_str("\n\n");
        prompt.push_str(extra);
    }
    prompt
}

fn assemble<'a>(
    state: &'a AppState,
    settings: Settings,
    request: &AiTaskRequest,
    system: String,
    tools: Vec<ChatToolDefinition>,
    provider: ModelProvider,
    model: String,
    generation: u64,
) -> AgentRunConfig<'a> {
    let language = crate::settings::resolve_chat_language(&settings);
    let workdir = workdir_for(&settings, request);
    let mut effective_chat_tools = settings.chat_tools.clone();
    apply_mode(&mut effective_chat_tools, request.mode);
    let retry_attempts = if settings.retry_enabled { settings.retry_attempts as usize } else { 1 };
    let max_output_tokens = settings.chat.max_output_tokens;
    let conversation_id = conversation_id(request.task_id.trim());
    let runtime_messages = vec![
        json!({ "role": "system", "content": &system }),
        user_message(&request.prompt, &request.images),
    ];
    AgentRunConfig {
        provider_runtime: state,
        conversation_id: conversation_id.clone(),
        tool_conversation_id: conversation_id,
        depth: 0,
        run_id: format!("wb-run-{}", request.task_id.trim()),
        message_id: format!("wb-msg-{}", request.task_id.trim()),
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
        web_search_mode: WebSearchMode::Off,
        max_output_tokens,
        retry_attempts,
        assistant_snapshot: None,
        provider_tools_fallback_system_prompt: system.clone(),
        initial_anchor_total_tokens: None,
        initial_anchor_trailing_estimate: 0,
        skill_project_cwd: workdir,
    }
}

fn tool_status(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::Running => "running",
        ToolCallStatus::Success => "success",
        ToolCallStatus::Error => "error",
        ToolCallStatus::Cancelled => "cancelled",
        ToolCallStatus::Skipped => "skipped",
    }
}

fn result_from(content: String, collected: String, records: &[ToolCallRecord], usage: Option<ModelUsage>, language: &str) -> Result<AiTaskResult, String> {
    let text = if content.trim().is_empty() { collected } else { content };
    if text.trim().is_empty() {
        return Err(localize(language, "模型没有返回内容", "The model returned an empty response"));
    }
    Ok(AiTaskResult {
        text,
        tool_calls: records.iter().map(|record| AiTaskToolCall { name: record.name.clone(), status: tool_status(&record.status).to_string() }).collect(),
        usage: usage.map(|usage| AiTaskUsage {
            input_tokens: usage.input_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
        }),
    })
}

pub(crate) fn abort_timed_out(state: &AppState, conversation_id: &str, language: &str) -> String {
    state.chat_runtime().cancel_conversation(conversation_id);
    localize(language, "AI 任务超时，请重试", "AI task timed out. Try again.")
}

fn timeout_secs(request: &AiTaskRequest) -> u64 {
    request.timeout_secs.map(u64::from).unwrap_or(match request.mode {
        AiTaskMode::Once => ONCE_TIMEOUT_SECS,
        AiTaskMode::Agent => AGENT_TIMEOUT_SECS,
    })
}

#[tauri::command]
pub(crate) async fn run_ai_task(app: AppHandle, state: State<'_, AppState>, request: AiTaskRequest) -> Result<AiTaskResult, String> {
    let settings = state.settings_read().clone();
    let language = crate::settings::resolve_chat_language(&settings);
    validate(&request, &language)?;
    let (provider_id, model) = resolve_model(&settings, &request)?;
    let provider = settings.get_provider(&provider_id).cloned().expect("resolve_model already checked the provider");
    let conversation_id = conversation_id(request.task_id.trim());
    state.chat_interactions().grant_session_consent(&conversation_id);
    let generation = state.chat_runtime().begin_generation(&conversation_id);
    let outcome = execute(&app, state.inner(), &settings, &request, provider, model, generation, &language).await;
    state.chat_runtime().end_generation(&conversation_id, generation);
    outcome
}

async fn execute(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    request: &AiTaskRequest,
    provider: ModelProvider,
    model: String,
    generation: u64,
    language: &str,
) -> Result<AiTaskResult, String> {
    let tools = if request.tools.is_empty() {
        Vec::new()
    } else {
        let catalog = crate::mcp::registry::list_enabled_tool_catalog(app, state).await;
        select_headless_tools(catalog.tools, &request.tools).map_err(|_| {
            localize(language, "所选工具当前不可用", "None of the selected tools are currently available")
        })?
    };
    let workdir = workdir_for(settings, request);
    let system = system_prompt(app, settings, request, &tools, workdir.as_deref());
    let mut config = assemble(state, settings.clone(), request, system, tools, provider, model, generation);
    if request.videos.as_ref().is_some_and(|videos| !videos.is_empty()) {
        let mut message = user_message(&request.prompt, &request.images);
        if message["content"].is_string() {
            message["content"] = json!([{"type":"text", "text":request.prompt}]);
        }
        let mut remaining = crate::chat::video::MAX_VIDEO_BYTES;
        for path in request.videos.as_deref().unwrap_or_default() {
            message["content"].as_array_mut().ok_or("Invalid media message")?.push(
                if path.starts_with("data:") { crate::chat::video::data_content_part(path, &mut remaining)? }
                else { crate::chat::video::content_part(std::path::Path::new(path), &mut remaining)? });
        }
        config.runtime_messages[1] = message;
    }
    let host = if request.stream {
        HeadlessAgentHost::streaming(app.clone(), request.task_id.trim().to_string())
    } else {
        HeadlessAgentHost::silent(app.clone())
    };
    let executor = HeadlessToolExecutor { app: app.clone() };
    let conversation_id = conversation_id(request.task_id.trim());
    match timeout(Duration::from_secs(timeout_secs(request)), run_agent_loop(config, &host, &executor)).await {
        Ok(Ok(result)) if !matches!(result.stream_outcome.as_str(), "completed" | "recovered") => Err(format!("AI task did not complete: {}",result.stream_outcome)),
        Ok(Ok(result)) => result_from(result.content, host.collected_text(), &result.tool_records, result.usage, language),
        Ok(Err(err)) if err == "cancelled" => Err("cancelled".to_string()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(abort_timed_out(state, &conversation_id, language)),
    }
}

#[tauri::command]
pub(crate) fn cancel_ai_task(state: State<'_, AppState>, task_id: String) -> Result<(), String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("Missing task id".to_string());
    }
    state.chat_runtime().cancel_conversation(&conversation_id(task_id));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::agent::{AgentHost, AgentHostFuture, ToolExecutionContext, ToolExecutor, ToolExecutorFuture};
    use crate::chat::ask_user::{AskUserPromptPayload, AskUserResponseResult};
    use crate::chat::types::ChatMessageSegment;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn request(mode: AiTaskMode) -> AiTaskRequest {
        AiTaskRequest {
            task_id: "task-1".into(),
            videos: None,
            mode,
            system: Some("只输出提示词".into()),
            prompt: "写一条短视频提示词".into(),
            images: Vec::new(),
            tools: Vec::new(),
            slot: AiTaskSlot::Chat,
            provider_id: None,
            model: None,
            cwd: None,
            timeout_secs: None,
            stream: false,
        }
    }

    fn test_app_state() -> AppState {
        let offline_models = crate::offline_models::OfflineModelManager::headless(reqwest::Client::new());
        AppState::base(
            Settings::default(),
            std::env::temp_dir().join(format!("kivio-ai-task-test-{}", uuid::Uuid::new_v4())),
            reqwest::Client::new(),
            #[cfg(target_os = "macos")]
            crate::macos_ocr::MacOcrClient::disabled(),
            offline_models.clone(),
            crate::rapidocr::RapidOcrClient::new(offline_models),
        )
    }

    fn provider(id: &str, base_url: &str) -> ModelProvider {
        ModelProvider {
            id: id.to_string(),
            name: id.to_string(),
            api_keys: vec!["test-key".to_string()],
            api_key_legacy: None,
            base_url: base_url.to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        }
    }

    fn tool(name: &str) -> ChatToolDefinition {
        ChatToolDefinition {
            id: name.to_string(),
            name: name.to_string(),
            description: name.to_string(),
            source: "native".to_string(),
            server_id: None,
            server_name: None,
            input_schema: json!({ "type": "object" }),
            sensitive: false,
            annotations: None,
            output_schema: None,
        }
    }

    struct ActiveHost;
    impl AgentHost for ActiveHost {
        fn emit_stream_delta(&self, _: &str, _: &str, _: &str, _: &str, _: Option<&str>, _: Option<&ChatMessageSegment>) {}
        fn emit_tool_record(&self, _: &str, _: &str, _: &str, _: &ToolCallRecord) {}
        fn request_tool_approval<'a>(&'a self, _: &'a ToolExecutionContext<'a>, _: &'a ToolCallRecord) -> AgentHostFuture<'a, bool> {
            Box::pin(async { true })
        }
        fn request_user_response<'a>(&'a self, _: &'a ToolExecutionContext<'a>, _: &'a ToolCallRecord, _: AskUserPromptPayload) -> AgentHostFuture<'a, AskUserResponseResult> {
            Box::pin(async { AskUserResponseResult { phase: "cancelled".into(), answers: std::collections::HashMap::new() } })
        }
        fn is_generation_active(&self, _: &str, _: u64) -> bool { true }
        fn wait_for_generation_inactive<'a>(&'a self, _: &'a str, _: u64) -> AgentHostFuture<'a, ()> {
            Box::pin(std::future::pending())
        }
    }

    struct UnusedExecutor;
    impl ToolExecutor for UnusedExecutor {
        fn call<'a>(&'a self, _: &'a ToolExecutionContext<'a>, _: &'a ChatToolDefinition, _: Value, _: Option<&'a mut crate::skills::SkillRunCache>) -> ToolExecutorFuture<'a> {
            Box::pin(async { Err("unused".into()) })
        }
    }

    fn start_sse_mock(events: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_thread = Arc::clone(&captured);
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            let header_end = loop {
                let Ok(n) = stream.read(&mut chunk) else { return };
                if n == 0 { return }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let headers = String::from_utf8_lossy(&buf[..header_end]).to_ascii_lowercase();
            let content_length = headers.lines().find_map(|line| line.strip_prefix("content-length:")).and_then(|value| value.trim().parse::<usize>().ok()).unwrap_or(0);
            while buf.len() < header_end + content_length {
                let Ok(n) = stream.read(&mut chunk) else { break };
                if n == 0 { break }
                buf.extend_from_slice(&chunk[..n]);
            }
            captured_thread.lock().unwrap_or_else(|err| err.into_inner()).push(String::from_utf8_lossy(&buf[header_end..]).into_owned());
            let sse: String = events.iter().map(|event| format!("data: {event}\n\n")).collect();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len());
            let _ = stream.write_all(response.as_bytes());
        });
        (format!("http://{addr}/v1"), captured)
    }

    #[test]
    fn conversation_id_is_not_a_sidebar_session() {
        assert_eq!(conversation_id("abc"), "wb_abc");
        assert!(!conversation_id("abc").starts_with("conv_"));
    }

    #[test]
    fn once_caps_tool_rounds_and_agent_keeps_the_setting() {
        let state = test_app_state();
        let mut settings = Settings::default();
        settings.chat_tools.max_tool_rounds = None;
        settings.providers = vec![provider("p", "http://127.0.0.1:9/v1")];
        settings.default_models.chat.provider_id = "p".into();
        settings.default_models.chat.model = "chat-model".into();
        let (provider_id, model) = resolve_model(&settings, &request(AiTaskMode::Once)).expect("model");
        let provider = settings.get_provider(&provider_id).cloned().unwrap();
        let once = assemble(&state, settings.clone(), &request(AiTaskMode::Once), "sys".into(), Vec::new(), provider.clone(), model.clone(), 1);
        assert_eq!(once.effective_chat_tools.max_tool_rounds, Some(1));
        assert_eq!(once.effective_chat_tools.approval_policy, "auto");
        assert_eq!(once.web_search_mode, WebSearchMode::Off);
        let agent = assemble(&state, settings, &request(AiTaskMode::Agent), "sys".into(), Vec::new(), provider, model, 1);
        assert_eq!(agent.effective_chat_tools.max_tool_rounds, None);
    }

    #[test]
    fn images_switch_the_chat_slot_to_vision_and_become_content_parts() {
        let mut settings = Settings::default();
        settings.providers = vec![provider("chat-p", "http://127.0.0.1:9/v1"), provider("vision-p", "http://127.0.0.1:9/v1")];
        settings.default_models.chat.provider_id = "chat-p".into();
        settings.default_models.chat.model = "chat-model".into();
        settings.default_models.vision.provider_id = "vision-p".into();
        settings.default_models.vision.model = "vision-model".into();
        let mut with_image = request(AiTaskMode::Once);
        with_image.images = vec!["data:image/png;base64,aaaa".into()];
        let (provider_id, model) = resolve_model(&settings, &with_image).expect("vision");
        assert_eq!((provider_id.as_str(), model.as_str()), ("vision-p", "vision-model"));
        let message = user_message(&with_image.prompt, &with_image.images);
        assert!(message["content"].is_array());
        assert!(message.to_string().contains("image_url"));
    }

    #[test]
    fn whitelist_drops_memory_and_rejects_unknown_names() {
        let tools = vec![tool("read"), tool("memory_save"), tool("present_artifacts")];
        let kept = select_headless_tools(tools.clone(), &["memory_save".into(), "read".into()]).expect("read remains");
        assert_eq!(kept.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["read"]);
        let err = select_headless_tools(tools, &["does_not_exist".into()]).unwrap_err();
        assert!(err.contains("currently available"), "{err}");
        assert!(select_headless_tools(vec![tool("read")], &[]).unwrap().is_empty());
    }

    #[test]
    fn one_sided_model_override_is_rejected() {
        let settings = Settings::default();
        let mut partial = request(AiTaskMode::Once);
        partial.provider_id = Some("p".into());
        assert!(resolve_model(&settings, &partial).is_err());
    }

    #[test]
    fn timeout_clears_the_generation() {
        let state = test_app_state();
        let id = conversation_id("slow");
        state.chat_runtime().begin_generation(&id);
        let message = abort_timed_out(&state, &id, "zh");
        assert!(message.contains("超时"));
        assert!(!state.chat_runtime().has_active_generation(&id));
    }

    #[tokio::test]
    async fn once_without_tools_streams_the_caller_system_prompt() {
        let (base_url, captured) = start_sse_mock(vec![
            r#"{"choices":[{"delta":{"content":"镜头推近产品"}}]}"#.to_string(),
            "[DONE]".to_string(),
        ]);
        let state = test_app_state();
        let mut settings = Settings::default();
        settings.providers = vec![provider("p", &base_url)];
        settings.default_models.chat.provider_id = "p".into();
        settings.default_models.chat.model = "chat-model".into();
        settings.retry_enabled = false;
        let task = request(AiTaskMode::Once);
        let (provider_id, model) = resolve_model(&settings, &task).expect("model");
        let provider = settings.get_provider(&provider_id).cloned().unwrap();
        let system = task.system.clone().unwrap();
        let config = assemble(&state, settings, &task, system, Vec::new(), provider, model, 1);
        let result = run_agent_loop(config, &ActiveHost, &UnusedExecutor).await.expect("loop");
        assert!(result.content.contains("镜头推近产品"));
        let bodies = captured.lock().unwrap_or_else(|err| err.into_inner()).clone();
        assert!(!bodies.is_empty(), "the model was called");
        let body = bodies.join("\n");
        assert!(body.contains("\"stream\":true"), "headless calls stream; body={body}");
        assert!(!body.contains("\"reasoning_effort\":\"none\""), "must not force thinking off; body={body}");
        assert!(body.contains("只输出提示词") && body.contains("chat-model"), "body={body}");
    }
}
