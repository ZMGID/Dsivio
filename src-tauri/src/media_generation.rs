//! The generation lifecycle shared by chat tools and Workbench. Provider modules own wire formats.
use crate::{comfyui, settings::ModelProvider, state::AppState};
pub(crate) mod image_providers;
pub mod video_providers;
use image_providers::async_task as image_gateway;
use video_providers as providers;
pub(crate) mod artifacts;
pub mod cli;
pub mod model_parameters;
pub mod speech_providers;
pub mod voices;
pub mod local_asr;
pub mod local_tts;
pub mod local_edit;
pub mod transcribe_providers;
mod request_evidence;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use ts_rs::TS;
use parking_lot::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
    Speech,
    Transcribe,
    Edit,
    Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaSubmissionState {
    Rejected,
    Uncertain,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaOutput {
    pub path: String,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CancelOutcome { Confirmed, Requested, Unsupported, TooLate }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CancelScope { Local, Remote, None }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ChargeFact { No, Maybe, Yes, Unknown }
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaCancellation {
    pub requested_at: String,
    pub scope: CancelScope,
    pub outcome: CancelOutcome,
    pub confirmed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaCancelResult {
    pub id: String,
    pub outcome: CancelOutcome,
    pub scope: CancelScope,
    pub charged: ChargeFact,
    pub task: MediaTask,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaTask {
    pub id: String,
    pub provider_id: String,
    pub model: String,
    pub kind: MediaKind,
    pub status: MediaStatus,
    pub created_at: String,
    pub error: Option<String>,
    pub remote_id: Option<String>,
    pub outputs: Vec<MediaOutput>,
    pub can_resume: bool,
    /// Missing on legacy records; missing receipts remain conservatively uncertain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub submission_state: Option<MediaSubmissionState>,
    /// Who asked for this run, e.g. `workbench/main` or `chat`. Workbench pages list by it.
    #[serde(default)]
    pub origin: Option<String>,
    /// The prompt as submitted; kept so a history row can say what it was for.
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    #[ts(type = "unknown | null")]
    pub result: Option<Value>,
    #[serde(default)]
    pub request_hash: Option<String>,
    #[serde(default)]
    pub cancellation: Option<MediaCancellation>,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaRequest {
    pub provider_id: String,
    pub model: String,
    pub kind: MediaKind,
    pub prompt: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub options: BTreeMap<String, Value>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description_revision: Option<String>,
}
/// UI projection of the route-owned facts. Validation remains in model_parameters.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaModelInfo {
    pub revision: String,
    pub complete: bool,
    pub parameters: Vec<MediaParameter>,
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaParameter {
    pub key: String,
    pub data_type: String,
    pub required: bool,
    #[ts(type = "Record<string, unknown>")]
    pub facts: BTreeMap<String, Value>,
}
#[tauri::command]
pub fn describe_media_model(app: AppHandle, provider_id: String, model: String, kind: MediaKind) -> Result<MediaModelInfo, String> {
    let state = app.state::<AppState>();
    let provider = provider(&state, &provider_id)?;
    let description = model_parameters::describe(&provider, &model, &kind);
    Ok(MediaModelInfo {
        revision: description.facts_revision,
        complete: description.facts_complete,
        parameters: description.arguments.into_iter().map(|(key, argument)| MediaParameter {
            key, data_type: serde_json::to_value(argument.data_type).unwrap().as_str().unwrap().to_owned(),
            required: argument.required, facts: argument.facts,
        }).collect(),
    })
}

/// Read the original input for an explicit new draft, never resubmit an old receipt.
#[tauri::command]
pub fn media_task_request(id: String) -> Result<MediaRequest, String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "无效任务编号")?;
    for root in [root()?, comfyui::root()?] {
        let path = root.join(&id).join("reuse-request.json");
        if path.exists() {
            return serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
    }
    Err("此历史任务未保存可复用参数，可复制提示词后重新选择模型".into())
}
fn save_reusable_request(root: &Path, id: &str, request: &MediaRequest) -> Result<(), String> {
    voices::atomic_bytes(&root.join(id).join("reuse-request.json"), &serde_json::to_vec(request).map_err(|e| e.to_string())?)
}

/// Cloud image options. ComfyUI uses declared workflow inputs instead.
#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaImageOptions {
    #[ts(optional)]
    pub size: Option<String>,
    #[serde(alias = "aspect_ratio")]
    #[ts(optional)]
    pub aspect_ratio: Option<String>,
    #[ts(optional)]
    pub quality: Option<String>,
    #[ts(optional)]
    pub n: Option<u32>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// Which saved tasks to list. Empty filter lists everything.
#[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaTaskFilter {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub origin: Option<String>,
}
impl MediaTaskFilter {
    fn matches(&self, task: &MediaTask) -> bool {
        self.provider_id
            .as_ref()
            .is_none_or(|p| *p == task.provider_id)
            && self.model.as_ref().is_none_or(|m| *m == task.model)
            && self
                .origin
                .as_ref()
                .is_none_or(|o| task.origin.as_ref() == Some(o))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTask {
    #[serde(flatten)]
    task: MediaTask,
    base_url: String,
    protocol: String,
    download_url: Option<String>,
    download_requires_auth: bool,
    /// When the provider returned the receipt. Missing on older records; `created_at` stands in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_at: Option<String>,
}
static ACTIVE: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static TASK_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
#[derive(Clone, Copy, PartialEq)]
enum ControlPhase { Pending, Local, Remote }
struct TaskControl {
    phase: ControlPhase,
    cancel: Arc<tokio::sync::Notify>,
    finished: Arc<tokio::sync::Notify>,
}
static CONTROLS: LazyLock<Mutex<BTreeMap<String, TaskControl>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
struct Active(String);
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .remove(&self.0);
    }
}
fn claim(id: &str) -> Option<Active> {
    ACTIVE
        .lock()
        .insert(id.into())
        .then(|| Active(id.into()))
}
fn root() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无法定位生成记录")?
        .join("media-tasks"))
}
fn directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    Ok(root.join(id))
}
fn save(root: &Path, task: &StoredTask) -> Result<(), String> {
    let _lock = TASK_LOCK.lock();
    if let Ok(existing) = read(root, &task.task.id) {
        if existing.task.status == MediaStatus::Cancelled { return Ok(()); }
        if task.task.cancellation.is_none() && existing.task.cancellation.is_some() {
            let mut merged = task.clone();
            merged.task.cancellation = existing.task.cancellation;
            return save_locked(root, &merged);
        }
    }
    save_locked(root, task)
}
fn save_locked(root: &Path, task: &StoredTask) -> Result<(), String> {
    let dir = directory(root, &task.task.id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(
        &dir.join("task.json"),
        &serde_json::to_string(task).map_err(|e| e.to_string())?,
        "media task",
    )
}
fn read(root: &Path, id: &str) -> Result<StoredTask, String> {
    serde_json::from_slice(
        &std::fs::read(directory(root, id)?.join("task.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn mime(path: &str) -> String {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "video/mp4",
    }
    .into()
}
pub(crate) fn from_comfy(task: comfyui::ComfyTask) -> MediaTask {
    use comfyui::ComfyTaskStatus as S;
    let request_hash = comfyui::root().ok().and_then(|root| std::fs::read_to_string(root.join(&task.id).join("request-hash.txt")).ok());
    MediaTask {
        id: task.id,
        provider_id: task.provider_id,
        model: task.workflow_id,
        kind: if task.kind == comfyui::ComfyMediaKind::Image {
            MediaKind::Image
        } else {
            MediaKind::Video
        },
        status: match task.status {
            S::Succeeded => MediaStatus::Succeeded,
            S::Failed | S::Uncertain | S::DownloadPending => MediaStatus::Failed,
            _ => MediaStatus::Running,
        },
        created_at: task.created_at,
        error: task.error,
        remote_id: task.prompt_id,
        outputs: task
            .outputs
            .into_iter()
            .filter_map(|o| {
                o.local_path.map(|path| MediaOutput {
                    mime: mime(&path),
                    path,
                })
            })
            .collect(),
        can_resume: task.status == S::DownloadPending,
        submission_state: match task.status {
            S::Failed => Some(MediaSubmissionState::Rejected),
            S::Uncertain => Some(MediaSubmissionState::Uncertain),
            _ => None,
        },
        origin: task.origin,
        prompt: task.prompt,
        result: None,
        request_hash,
        cancellation: None,
    }
}

pub(crate) fn import_legacy_at(
    root: &Path,
    settings: &crate::settings::Settings,
) -> Result<(), String> {
    let old_root = root.join("video-studio");
    let Ok(entries) = std::fs::read_dir(old_root.join("tasks")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(old) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let Some(id) = old["id"]
            .as_str()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        else {
            continue;
        };
        if root.join("media-tasks").join(id).join("task.json").exists()
            || root.join("comfy-tasks").join(id).join("task.json").exists()
        {
            continue;
        }
        let route = old["remote"]["route"]
            .as_str()
            .or(old["brief"]["route"].as_str())
            .unwrap_or("");
        let base = old["remote"]["base_url"].as_str().unwrap_or("");
        let Some(provider) = settings
            .providers
            .iter()
            .find(|p| p.base_url.trim_end_matches('/') == base.trim_end_matches('/'))
            .or_else(|| {
                settings
                    .providers
                    .iter()
                    .find(|p| p.id == format!("legacy-dsvideo-{route}"))
            })
        else {
            continue;
        };
        let remote_id = old["remote"]["id"].as_str().map(str::to_owned);
        let created_at = old["updatedAt"]
            .as_i64()
            .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();
        let output = old["output"]
            .as_str()
            .and_then(|p| Path::new(p).canonicalize().ok())
            .filter(|p| {
                old_root
                    .canonicalize()
                    .is_ok_and(|root| p.starts_with(root))
                    && p.is_file()
            });
        if route == "comfy" {
            // Older tasks were tied to a fixed workflow. Keep the original receipt; query all
            // output nodes for these historical records without creating a replacement workflow.
            let outputs = output
                .map(|p| comfyui::ComfyArtifact {
                    filename: p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    subfolder: String::new(),
                    folder_type: "output".into(),
                    local_path: Some(p.to_string_lossy().into_owned()),
                })
                .into_iter()
                .collect::<Vec<_>>();
            let status = if !outputs.is_empty() {
                comfyui::ComfyTaskStatus::Succeeded
            } else if remote_id.is_some() {
                comfyui::ComfyTaskStatus::DownloadPending
            } else {
                comfyui::ComfyTaskStatus::Uncertain
            };
            comfyui::save(
                &root.join("comfy-tasks"),
                &comfyui::ComfyTask {
                    id: id.into(),
                    provider_id: provider.id.clone(),
                    workflow_id: "legacy-comfy".into(),
                    workflow_name: "历史 ComfyUI 视频".into(),
                    kind: comfyui::ComfyMediaKind::Video,
                    base_url: if base.is_empty() {
                        provider.base_url.clone()
                    } else {
                        base.into()
                    },
                    output_nodes: vec![],
                    prompt_id: remote_id,
                    status,
                    error: Some("历史任务已保留，可按原编号继续查询；不会重新提交".into()),
                    outputs,
                    created_at,
                    origin: None,
                    prompt: String::new(),
                },
            )?;
            continue;
        }
        let protocol = match route {
            "grok" => "xai_video",
            "minimax" => "minimax_h3",
            _ => continue,
        };
        let model = old["requested"]["model"]
            .as_str()
            .or_else(|| provider.enabled_models.first().map(String::as_str))
            .unwrap_or("")
            .to_owned();
        let outputs = output
            .map(|p| MediaOutput {
                path: p.to_string_lossy().into_owned(),
                mime: "video/mp4".into(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        let status = if outputs.is_empty() {
            MediaStatus::Failed
        } else {
            MediaStatus::Succeeded
        };
        let can_resume = outputs.is_empty() && remote_id.is_some();
        save(
            &root.join("media-tasks"),
            &StoredTask {
                task: MediaTask {
                    id: id.into(),
                    provider_id: provider.id.clone(),
                    model,
                    kind: MediaKind::Video,
                    status,
                    created_at,
                    error: if can_resume {
                        Some("历史任务已保留，可继续查询；不会重新提交".into())
                    } else {
                        old["error"].as_str().map(str::to_owned)
                    },
                    remote_id,
                    outputs,
                    can_resume,
                    submission_state: None,
                    origin: None,
                    prompt: String::new(),
                    result: None,
                    request_hash: None,
                    cancellation: None,
                },
                base_url: if base.is_empty() {
                    provider.base_url.clone()
                } else {
                    base.into()
                },
                protocol: protocol.into(),
                download_url: old["remote"]["download_url"].as_str().map(str::to_owned),
                download_requires_auth: false,
                accepted_at: None,
            },
        )?;
    }
    Ok(())
}
/// Provider headers for a media request, plus a browser `User-Agent` when the provider sets none:
/// Cloudflare-fronted gateways reject default library agents (`error code: 1010`).
pub(crate) fn with_provider_headers(
    request: reqwest::RequestBuilder,
    provider: &ModelProvider,
    task_id: Option<&str>,
) -> reqwest::RequestBuilder {
    let has_agent = crate::provider_request::header_pairs(provider, task_id)
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"));
    let request = crate::provider_request::apply(request, provider, task_id);
    if has_agent {
        request
    } else {
        request.header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
    }
}
fn provider(state: &AppState, id: &str) -> Result<ModelProvider, String> {
    state
        .settings_read()
        .providers
        .iter()
        .find(|p| p.id == id && p.enabled)
        .cloned()
        .ok_or("供应商不存在或未启用".into())
}
fn video_input(request: &MediaRequest) -> Result<providers::VideoInput, String> {
    let mut value = serde_json::to_value(&request.options).map_err(|e| e.to_string())?;
    for name in ["firstFrame", "lastFrame"] {
        if let Some(value) = value.get_mut(name) {
            if value.is_array() {
                let sources = model_parameters::media_sources(value)?;
                if sources.len() != 1 { return Err(format!("{name} requires exactly one image")); }
                *value = json!(sources[0]);
            }
        }
    }
    for name in ["referenceImages", "referenceVideos", "referenceAudios", "voiceIds"] {
        if let Some(value) = value.get_mut(name) {
            *value = json!(model_parameters::media_sources(value)?);
        }
    }
    value["prompt"] = json!(request.prompt);
    // Images are explicit references; a caller chooses firstFrame/referenceImages for video.
    if !request.images.is_empty() {
        return Err("视频素材请明确指定为首帧或参考图".into());
    }
    let mut input: providers::VideoInput =
        serde_json::from_value(value).map_err(|e| format!("视频参数无效：{e}"))?;
    input.first_frame = input.first_frame.as_deref().map(video_image).transpose()?;
    input.last_frame = input.last_frame.as_deref().map(video_image).transpose()?;
    input.reference_images = input
        .reference_images
        .iter()
        .map(|v| video_image(v))
        .collect::<Result<_, _>>()?;
    input.reference_videos = input
        .reference_videos
        .iter()
        .map(|v| video_reference(v, false))
        .collect::<Result<_, _>>()?;
    input.reference_audios = input
        .reference_audios
        .iter()
        .map(|v| video_reference(v, true))
        .collect::<Result<_, _>>()?;
    Ok(input)
}
fn video_reference(value: &str, audio: bool) -> Result<String, String> {
    if value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("mm_file://")
        || value.starts_with("data:")
    {
        return Ok(value.into());
    }
    let path = Path::new(value);
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match (audio, extension.as_str()) {
        (true, "mp3") => "audio/mp3",
        (true, "wav") => "audio/wav",
        (false, "mp4") => "video/mp4",
        (false, "mov") => "video/quicktime",
        _ => return Err("参考媒体格式不支持".into()),
    };
    let limit = if audio { 15 } else { 50 } * 1024 * 1024;
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > limit {
        return Err("参考媒体文件超过大小限制".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err("参考媒体为空或过大".into());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
fn video_image(value: &str) -> Result<String, String> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(value.into());
    }
    Ok(image_providers::data_url_for_input(
        &image_inputs(&[value.into()])?.remove(0),
    ))
}
fn image_arguments(provider: &ModelProvider, request: &MediaRequest) -> Result<Value, String> {
    let mut args = request.options.clone();
    if args.insert("prompt".into(), json!(request.prompt)).is_some() {
        return Err("prompt must be supplied only at the request boundary".into());
    }
    Ok(json!(model_parameters::validate_and_resolve(provider, &request.model, &MediaKind::Image, args, request.description_revision.as_deref())?))
}
/// Bind standard media fields once, at the workflow boundary. Never guess positive/negative prompts.
fn comfy_values(
    provider: &ModelProvider,
    flow: &comfyui::ComfyWorkflow,
    request: &MediaRequest,
) -> Result<BTreeMap<String, Value>, String> {
    let mut values = request.options.clone();
    let mut prompt_used = request.prompt.is_empty();
    let mut images_used = HashSet::new();
    let mut parameters_used = HashSet::new();
    let mut array_items_used: BTreeMap<String, HashSet<usize>> = BTreeMap::new();
    for binding in &flow.inputs {
        let key = format!("{}:{}", binding.node_id, binding.input);
        let value = match &binding.source {
            Some(comfyui::ComfyInputSource::Prompt) if !request.prompt.is_empty() => {
                prompt_used = true;
                Some(json!(request.prompt))
            }
            Some(comfyui::ComfyInputSource::Image { index }) if !request.images.is_empty() => {
                let path = request
                    .images
                    .get(*index as usize)
                    .ok_or("工作流缺少绑定的参考图")?;
                images_used.insert(*index as usize);
                Some(json!(image_providers::data_url_for_input(
                    &image_inputs(&[path.clone()])?.remove(0)
                )))
            }
            Some(comfyui::ComfyInputSource::Parameter { name, index }) => {
                if let Some(value) = request.options.get(name) {
                    parameters_used.insert(name.clone());
                    let value = if let Some(index) = index {
                        array_items_used
                            .entry(name.clone())
                            .or_default()
                            .insert(*index as usize);
                        value
                            .as_array()
                            .and_then(|items| items.get(*index as usize))
                            .ok_or("工作流缺少绑定的数组参数")?
                    } else {
                        value
                    };
                    Some(if binding.kind == comfyui::ComfyInputKind::Image {
                        let path = value
                            .as_str()
                            .ok_or("工作流图片参数必须是本地路径或 data URL")?;
                        json!(image_providers::data_url_for_input(
                            &image_inputs(&[path.into()])?.remove(0)
                        ))
                    } else {
                        value.clone()
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(value) = value {
            if values.insert(key, value).is_some() {
                return Err("工作流输入同时收到通用字段和节点参数，请只传一处".into());
            }
        }
    }
    if !prompt_used || images_used.len() != request.images.len() {
        return Err(
            "工作流尚未绑定通用提示词或参考图，请在工作流输入中配置 source，或使用已声明的节点参数"
                .into(),
        );
    }
    for (name, indices) in array_items_used {
        if request.options[&name]
            .as_array()
            .is_some_and(|items| items.len() != indices.len())
        {
            return Err(format!("工作流没有绑定 {name} 中的全部素材"));
        }
    }
    for name in parameters_used {
        values.remove(&name);
    }
    let description = model_parameters::workflow_description(provider, flow, &request.kind);
    values = model_parameters::resolve(&description, values, request.description_revision.as_deref())?;
    comfyui::prepare(flow, &values)?;
    Ok(values)
}

/// Every caller enters here before a task exists or a paid request can be sent.
fn validate_request(provider: &ModelProvider, request: &mut MediaRequest) -> Result<(), String> {
    let mut args = model_parameters::normalize(std::mem::take(&mut request.options))?;
    let supplied: HashSet<String> = args.keys().cloned().collect();
    if args.insert("prompt".into(), json!(request.prompt)).is_some() {
        return Err("prompt must be supplied only at the request boundary".into());
    }
    if request.kind == MediaKind::Image {
        if args.insert("images".into(), json!(request.images)).is_some() {
            return Err("images must be supplied only at the request boundary".into());
        }
    } else {
        for name in ["firstFrame", "lastFrame"] {
            if let Some(value) = args.get_mut(name) {
                if value.is_string() { *value = json!([value.clone()]); }
            }
        }
    }
    let mut resolved = model_parameters::validate_and_resolve(provider, &request.model, &request.kind, args, request.description_revision.as_deref())?;
    resolved.remove("prompt");
    resolved.remove("images");
    // Keep supplied presence intact for the route's final validation/encoding. Defaults are
    // resolved there; serializing them here would turn an omitted field into user input.
    resolved.retain(|name, _| supplied.contains(name));
    request.options = resolved;
    Ok(())
}
pub(crate) async fn start(app: &AppHandle, mut request: MediaRequest) -> Result<MediaTask, String> {
    let state = app.state::<AppState>();
    if matches!(request.kind, MediaKind::Speech | MediaKind::Transcribe) {
        return start_audio_task(app, request).await;
    }
    if request.kind == MediaKind::Edit {
        return local_edit::start(app, request).await;
    }
    let request_hash = Some(request_hash(&request)?);
    let p = provider(&state, &request.provider_id)?;
    if !p.enabled_models.contains(&request.model) {
        return Err("模型尚未启用".into());
    }
    if let Some(config) = &p.request.comfy {
        let flow = config
            .workflows
            .iter()
            .find(|w| w.id == request.model)
            .ok_or("工作流不存在")?
            .clone();
        if (flow.kind == comfyui::ComfyMediaKind::Image) != (request.kind == MediaKind::Image) {
            return Err("工作流媒体类型不匹配".into());
        }
        let values = comfy_values(&p, &flow, &request)?;
        let id = uuid::Uuid::new_v4().to_string();
        let guard = claim(&id).ok_or("任务已运行")?;
        save_reusable_request(&comfyui::root()?, &id, &request)?;
        if let Some(hash) = &request_hash {
            voices::atomic_bytes(&comfyui::root()?.join(&id).join("request-hash.txt"),hash.as_bytes())?;
        }
        let task = from_comfy(
            comfyui::submit_to_store(
                &comfyui::root()?,
                id,
                p,
                flow,
                values,
                request.origin,
                request.prompt,
            )
            .await?,
        );
        if task.status == MediaStatus::Running {
            spawn_background(app.clone(), task.id.clone(), None, guard);
        }
        return Ok(task);
    }
    let mut reusable_request = request.clone();
    validate_request(&p, &mut request)?;
    if request.prompt.trim().is_empty() {
        return Err("请填写提示词".into());
    }
    let protocol = if request.kind == MediaKind::Video {
        let protocol = providers::selected(&p, &request.model)?;
        let input = video_input(&request)?;
        providers::prepare(protocol, &p.base_url, &request.model, &input)?;
        let mut normalized: BTreeMap<String, Value> =
            serde_json::from_value(json!(input)).map_err(|e| e.to_string())?;
        normalized.remove("prompt");
        request.options = normalized;
        protocol.to_owned()
    } else {
        if !crate::chat::model_metadata::model_can_generate_images_directly(&p, &request.model) {
            return Err("此模型未启用图片生成".into());
        }
        let images = image_inputs(&request.images)?;
        image_providers::validate_generation(
            &p,
            &request.model,
            &image_arguments(&p, &request)?,
            images.len(),
        )?;
        request.images = images
            .iter()
            .map(image_providers::data_url_for_input)
            .collect();
        "image".into()
    };
    // Preserve the submitted image bytes for reuse even if an original local file moves.
    reusable_request.images = request.images.clone();
    for key in ["firstFrame", "lastFrame", "referenceImages"] {
        if reusable_request.options.contains_key(key) {
            if let Some(value) = request.options.get(key).filter(|value| !value.is_null()) {
                reusable_request.options.insert(key.into(), if value.is_string() { json!([value]) } else { value.clone() });
            }
        }
    }
    let task = StoredTask {
        task: MediaTask {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: p.id,
            model: request.model.clone(),
            kind: request.kind.clone(),
            status: MediaStatus::Running,
            created_at: chrono::Utc::now().to_rfc3339(),
            error: None,
            remote_id: None,
            outputs: vec![],
            can_resume: false,
            submission_state: None,
            origin: request.origin.clone(),
            prompt: request.prompt.clone(),
            result: None,
            request_hash,
            cancellation: None,
        },
        base_url: p.base_url,
        protocol,
        download_url: None,
        download_requires_auth: false,
        accepted_at: None,
    };
    let guard = claim(&task.task.id).ok_or("任务已运行")?;
    save_reusable_request(&root()?, &task.task.id, &reusable_request)?;
    save(&root()?, &task)?;
    register_control(&task.task.id);
    spawn_background(app.clone(), task.task.id.clone(), Some(request), guard);
    Ok(task.task)
}
pub(crate) async fn start_configured(
    app: &AppHandle,
    request: MediaRequest,
) -> Result<MediaTask, String> {
    if request.kind == MediaKind::Edit {
        if request.provider_id == "local" && matches!(request.model.as_str(), "ffmpeg-subtitle" | "ffmpeg-edit") {
            return start(app, request).await;
        }
        return Err("本地剪辑只使用 local/ffmpeg-subtitle 或 local/ffmpeg-edit".into());
    }
    let allowed = {
        let state = app.state::<AppState>();
        let settings = state.settings_read();
        let pool = match request.kind {
            MediaKind::Image => &settings.workbench_media.image_models,
            MediaKind::Video => &settings.workbench_media.video_models,
            MediaKind::Speech => &settings.workbench_media.speech_models,
            MediaKind::Transcribe => &settings.workbench_media.transcribe_models,
            MediaKind::Edit => return Err("本地剪辑不走云模型池".into()),
            MediaKind::Text => return Err("文案记录不走图片模型池".into()),
        };
        pool.iter()
            .any(|m| m.provider_id == request.provider_id && m.model == request.model)
    };
    if !allowed && !(request.kind == MediaKind::Speech && request.provider_id == "local" && request.model == local_tts::MODEL && local_tts::available()) {
        return Err("请先把模型加入媒体创作模型池".into());
    }
    start(app, request).await
}

pub fn request_hash(request: &MediaRequest) -> Result<String, String> {
    request_evidence::hash(request, &BTreeMap::new())
}

pub fn local_provider() -> ModelProvider {
    serde_json::from_value(json!({"id":"local","name":"系统本地媒体","baseUrl":"","enabled":true,
        "availableModels":["whisperx-small","system-tts","ffmpeg-subtitle","ffmpeg-edit","record"],"enabledModels":["whisperx-small","system-tts","ffmpeg-subtitle","ffmpeg-edit","record"],"apiKeys":[]})).expect("local model provider")
}

pub(crate) fn text_record_description(provider: &ModelProvider, model: &str) -> model_parameters::ModelDescription {
    model_parameters::finish(provider, model, MediaKind::Text, BTreeMap::new(), vec![], true, Some(1), false)
}

pub(crate) fn transcribe_configured(provider: &ModelProvider, model: &str) -> bool {
    if provider.id == "local" { return model == "whisperx-small"; }
    model == "whisper-1" && provider.preferred_api_key().is_some() && provider.model_overrides.get(model).is_some_and(|m|
        m.transcribe_protocol.as_deref() == Some("openai_transcribe") && m.transcribe_base_url.as_deref().is_some_and(|s|
            reqwest::Url::parse(s).is_ok_and(|u|u.scheme()=="https" && u.host_str().is_some() && u.username().is_empty() && u.password().is_none() && u.query().is_none() && u.fragment().is_none())))
}

pub(crate) fn transcribe_description(provider: &ModelProvider, model: &str) -> model_parameters::ModelDescription {
    use model_parameters::{argument, DataType};
    let mut args = BTreeMap::new();
    let mut audio = argument(DataType::String, Some("--audio-file"), json!({"minLength":1,"resource":true,"locations":["local"]}));
    audio.required = true;
    args.insert("audioFile".into(), audio);
    let mut language = argument(DataType::String, Some("--language"), json!({"minLength":2,"maxLength":3,"lengthUnit":"unicodeCodePoint"}));
    language.required = true;
    args.insert("language".into(), language);
    args.insert("sampleFrames".into(), argument(DataType::Integer, Some("--sample-frames"), json!({"minimum":1,"maximum":9007199254740991u64})));
    args.insert("timestamps".into(), argument(DataType::String, Some("--timestamps"), json!({"allowed":["word","segment"],"defaultValue":"word"})));
    let mut description = model_parameters::finish(provider, model, MediaKind::Transcribe, args, vec![], true, Some(1), false);
    description.products["schema"] = json!("dsivio.media.transcript/1");
    description.products["wordAlignment"] = json!(true);
    description.arguments.get_mut("audioFile").unwrap().facts.insert("wavEvidence".into(), json!({"sampleRate":16000,"channels":1,"encoding":"pcm_s16le","maxBytes":if provider.id == "local" {512*1024*1024} else {25_000_000}}));
    model_parameters::rehash(&mut description);
    description
}

fn register_control(id: &str) {
    register_control_in_phase(id,ControlPhase::Pending);
}

fn register_control_in_phase(id:&str,phase:ControlPhase) {
    CONTROLS.lock().insert(id.into(), TaskControl {
        phase, cancel: Arc::new(tokio::sync::Notify::new()), finished: Arc::new(tokio::sync::Notify::new()),
    });
}

fn restore_speech_control(id:&str,task_dir:&Path,mode:&str)->Result<(),String> {
    let phase = match std::fs::read(task_dir.join("speech-state.json")) {
        Ok(bytes) => {
            let journal:voices::Journal = serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
            let submitted = journal.file_id.is_some() || journal.cloned_at.is_some()
                || journal.in_flight.is_some() || journal.rejected.is_some()
                || matches!(journal.step,voices::Step::Uploaded|voices::Step::Synthesized|voices::Step::Downloaded)
                || journal.step == voices::Step::Cloned && mode == "clone";
            if submitted {ControlPhase::Remote}else{ControlPhase::Pending}
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ControlPhase::Pending,
        Err(error) => return Err(error.to_string()),
    };
    register_control_in_phase(id,phase);
    Ok(())
}

/// The send boundary and cancellation compete under the same lock.
fn begin_execution(root: &Path, id: &str, phase: ControlPhase) -> Result<bool, String> {
    let _lock = TASK_LOCK.lock();
    if read(root, id)?.task.status != MediaStatus::Running { return Ok(false); }
    if let Some(control) = CONTROLS.lock().get_mut(id) {
        // Durable remote activity is irreversible; local preparation cannot make it unsent.
        if control.phase != ControlPhase::Remote {control.phase = phase;}
    }
    Ok(true)
}

enum AudioWork {
    Speech(speech_providers::SpeechInput),
    Transcribe(transcribe_providers::TranscribeInput, local_asr::LocalAsrConfig),
}

async fn start_audio_task(app: &AppHandle, mut request: MediaRequest) -> Result<MediaTask, String> {
    if !request.prompt.is_empty() || !request.images.is_empty() { return Err("语音/转写只接收 options 中声明的输入".into()); }
    let state = app.state::<AppState>();
    let provider = if request.provider_id == "local" && matches!(request.kind, MediaKind::Transcribe | MediaKind::Speech) { local_provider() } else { provider(&state, &request.provider_id)? };
    if !provider.enabled_models.contains(&request.model) { return Err("模型尚未启用".into()); }
    let original = request.clone();
    request.options = model_parameters::validate_and_resolve(&provider, &request.model, &request.kind, request.options, request.description_revision.as_deref())?;
    let data = crate::app_data::app_data_dir().ok_or("无法定位应用数据目录")?;
    let root = root()?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = directory(&root, &id)?;
    let mut snapshots = BTreeMap::new();
    let work = if request.kind == MediaKind::Speech {
        let input = speech_providers::validate_input(&provider, &request.model, &request.options, &data)?;
        snapshots = input.fingerprint();
        speech_providers::persist_input(&input,&dir)?;
        AudioWork::Speech(input)
    } else {
        if !transcribe_configured(&provider, &request.model) { return Err("转写协议/产品地址尚未显式配置".into()); }
        let mut input = transcribe_providers::validate_input(&request.options)?;
        let evidence = transcribe_providers::validate_wav(&input.audio_file, Some(input.sample_frames))?;
        if provider.id != "local" && evidence.bytes.len() > 25_000_000 {return Err("ASR_UPLOAD_TOO_LARGE: whisper-1 allows 25MB".into());}
        snapshots.insert(input.audio_file.to_string_lossy().into_owned(), request_evidence::bytes_hash(&evidence.bytes));
        std::fs::create_dir_all(dir.join("evidence")).map_err(|e| e.to_string())?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
        input.audio_file = dir.join("evidence/audio.wav");
        cli::write_private(&input.audio_file, &evidence.bytes)?;
        let config = state.settings_read().workbench_media.local_asr.clone();
        AudioWork::Transcribe(input, config)
    };
    let protocol = if request.kind == MediaKind::Speech && provider.id == "local" { "system_tts" } else if request.kind == MediaKind::Speech {
        provider.model_overrides.get(&request.model).and_then(|m|m.speech_protocol.as_deref()).ok_or("缺少语音协议")?
    } else if provider.id == "local" { "whisperx" } else { "openai_transcribe" };
    let base_url = if provider.id == "local" { String::new() } else if request.kind == MediaKind::Speech {
        provider.model_overrides.get(&request.model).and_then(|m|m.speech_base_url.clone()).ok_or("缺少语音产品地址")?
    } else if provider.id == "local" {String::new()} else {
        provider.model_overrides.get(&request.model).and_then(|m|m.transcribe_base_url.clone()).ok_or("缺少转写产品地址")?
    };
    let saved = StoredTask { task: MediaTask { id:id.clone(), provider_id:provider.id.clone(), model:request.model.clone(),kind:request.kind.clone(),
        status:MediaStatus::Running,created_at:chrono::Utc::now().to_rfc3339(),error:None,remote_id:None,outputs:vec![],can_resume:false,
        submission_state:None,origin:request.origin,prompt:String::new(),result:None,
        request_hash:Some(request_evidence::hash(&original,&snapshots)?),cancellation:None },
        base_url,protocol:protocol.into(),download_url:None,download_requires_auth:false,accepted_at:None };
    let guard = claim(&id).ok_or("任务已运行")?;
    save(&root, &saved)?;
    register_control(&id);
    spawn_audio_task(root, data, provider, saved.clone(), work, guard);
    Ok(saved.task)
}

fn spawn_audio_task(root: PathBuf, data: PathBuf, provider: ModelProvider, mut saved: StoredTask, work: AudioWork, guard: Active) {
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let id = saved.task.id.clone();
        let local_asr_task = saved.protocol == "whisperx";
        let local = local_asr_task || saved.protocol == "system_tts";
        let phase = if local { ControlPhase::Local } else { ControlPhase::Pending };
        let control = CONTROLS.lock().get(&id).map(|c|(c.cancel.clone(),c.finished.clone()));
        let Some((cancel, finished)) = control else { return; };
        let result: Result<(), String> = async {
            if !begin_execution(&root,&id,phase)? { return Ok(()); }
            let before_send = || {
                if begin_execution(&root,&id,ControlPhase::Remote)? {Ok(())} else {Err("MEDIA_CANCELLED: 请求发送前已取消".into())}
            };
            let future = async {
                match work {
                    AudioWork::Speech(input) => {
                        let outputs = speech_providers::execute(artifacts::download_client(&provider),&provider,&saved.task.model,&input,&id,&directory(&root,&id)?,&data,&before_send)
                            .await.map_err(|e| {
                                saved.task.submission_state = if speech_providers::is_resumable(&directory(&root,&id).unwrap_or_default()) {None}
                                    else {Some(if e.uncertain || input.mode == "clone" {MediaSubmissionState::Uncertain} else {MediaSubmissionState::Rejected})};
                                let _ = save(&root,&saved);
                                e.message
                            })?;
                        saved.task.outputs = outputs;
                    },
                    AudioWork::Transcribe(input,config) => {
                        let value = if local {
                            local_asr::transcribe(&id,&input.audio_file,&input.language,input.sample_frames,&input.timestamps,config).await?
                        } else {
                            let info = provider.model_overrides.get(&saved.task.model).ok_or("缺少转写配置")?;
                            let base = info.transcribe_base_url.as_deref().ok_or("缺少转写产品地址")?;
                            let key = provider.api_keys.get(provider.active_key_index).or_else(||provider.api_keys.first()).filter(|s|!s.trim().is_empty()).ok_or("缺少语音产品 API key")?;
                            transcribe_providers::transcribe_openai(artifacts::download_client(&provider),base,key,&input,&before_send).await.map_err(|error| {
                                saved.task.submission_state = Some(image_submission_state(&error));
                                let _ = save(&root,&saved);
                                error
                            })?
                        };
                        saved.task.outputs = vec![artifacts::save_transcript(&value,&directory(&root,&id)?.join("transcript.json"))?];
                        if serde_json::to_vec(&value).map_err(|e|e.to_string())?.len() <= 3*1024*1024 { saved.task.result = Some(value); }
                    },
                }
                saved.task.status = MediaStatus::Succeeded;
                save(&root,&saved)
            };
            if local {
                tokio::pin!(future);
                tokio::select! {
                    biased;
                    _ = cancel.notified() => {
                        if local_asr_task { local_asr::cancel(&id).await?; }
                        Ok(())
                    },
                    result = &mut future => result,
                }
            } else { future.await }
        }.await;
        if let Err(error) = result {
            if let Ok(mut current) = read(&root,&id) {
                record_background_error(&mut current,error);
                if !local && current.task.kind == MediaKind::Speech && speech_providers::is_resumable(&directory(&root,&id).unwrap_or_default()) {
                    current.task.status = MediaStatus::Failed;
                    current.task.can_resume = true;
                } else if !local && current.task.submission_state.is_none() { current.task.submission_state = Some(MediaSubmissionState::Uncertain); }
                let _ = save(&root,&current);
            }
        }
        if local {
            let _lock = TASK_LOCK.lock();
            if let Ok(mut current) = read(&root,&id) {
                if current.task.status == MediaStatus::Running && current.task.cancellation.as_ref().is_some_and(|c|c.outcome == CancelOutcome::Requested) {
                    current.task.status = MediaStatus::Cancelled;
                    current.task.can_resume = false;
                    if let Some(c) = &mut current.task.cancellation { c.outcome = CancelOutcome::Confirmed;c.confirmed_at=Some(chrono::Utc::now().to_rfc3339()); }
                    let _ = save_locked(&root,&current);
                }
            }
        }
        finished.notify_waiters();
        CONTROLS.lock().remove(&id);
    });
}

#[tauri::command]
pub async fn cancel_media_task(app: AppHandle, id: String) -> Result<MediaCancelResult, String> {
    let root = root()?;
    directory(&root,&id)?;
    if !directory(&root,&id)?.join("task.json").exists() {
        let task = from_comfy(comfyui::read_task(&comfyui::root()?,&id)?);
        return Ok(MediaCancelResult {id,outcome:if task.status == MediaStatus::Running {CancelOutcome::Unsupported}else{CancelOutcome::TooLate},
            scope:CancelScope::None,charged:ChargeFact::Unknown,task});
    }
    let (signal, finished, immediate) = cancel_at(&root,&id)?;
    if immediate.outcome != CancelOutcome::Requested {return Ok(immediate);}
    if let (Some(signal),Some(finished)) = (signal,finished) {
        let notified = finished.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        signal.notify_one();
        let _ = tokio::time::timeout(Duration::from_secs(30),notified).await;
    }
    let task = get_media_task(app,id.clone(),None)?;
    let outcome = if task.status == MediaStatus::Cancelled {CancelOutcome::Confirmed} else if task.status == MediaStatus::Running {CancelOutcome::Requested} else {CancelOutcome::TooLate};
    Ok(MediaCancelResult {id,outcome,scope:CancelScope::Local,charged:ChargeFact::No,task})
}

type CancelDecision = (Option<Arc<tokio::sync::Notify>>, Option<Arc<tokio::sync::Notify>>, MediaCancelResult);
fn cancel_at(root:&Path,id:&str)->Result<CancelDecision,String> {
    let _lock = TASK_LOCK.lock();
    let mut saved = read(root,id)?;
    if saved.task.status == MediaStatus::Cancelled {
        return Ok((None,None,MediaCancelResult {id:id.into(),outcome:CancelOutcome::Confirmed,scope:CancelScope::Local,charged:ChargeFact::No,task:saved.task}));
    }
    if saved.task.status != MediaStatus::Running {
        return Ok((None,None,MediaCancelResult {id:id.into(),outcome:CancelOutcome::TooLate,scope:CancelScope::None,charged:ChargeFact::Unknown,task:saved.task}));
    }
    let controls = CONTROLS.lock();
    let phase = controls.get(id).map(|c|c.phase).unwrap_or(ControlPhase::Remote);
    let outcome = match phase {ControlPhase::Pending=>CancelOutcome::Confirmed,ControlPhase::Local=>CancelOutcome::Requested,ControlPhase::Remote=>CancelOutcome::Unsupported};
    let scope = if phase == ControlPhase::Remote {CancelScope::Remote}else{CancelScope::Local};
    let now = chrono::Utc::now().to_rfc3339();
    let requested_at = saved.task.cancellation.as_ref().map(|c|c.requested_at.clone()).unwrap_or_else(||now.clone());
    saved.task.cancellation = Some(MediaCancellation {requested_at,scope:scope.clone(),outcome:outcome.clone(),confirmed_at:if outcome == CancelOutcome::Confirmed {Some(now)}else{None}});
    if outcome == CancelOutcome::Confirmed {saved.task.status=MediaStatus::Cancelled;saved.task.can_resume=false;}
    save_locked(root,&saved)?;
    let control = controls.get(id);
    let result = MediaCancelResult {id:id.into(),outcome,scope,charged:if phase == ControlPhase::Remote {ChargeFact::Maybe}else{ChargeFact::No},task:saved.task};
    Ok((control.map(|c|c.cancel.clone()),control.map(|c|c.finished.clone()),result))
}

#[tauri::command]
pub fn list_media_voices() -> Result<Vec<voices::VoiceReference>,String> {
    voices::list(&crate::app_data::app_data_dir().ok_or("无法定位声音引用")?)
}
#[tauri::command]
pub fn delete_media_voice(id:String) -> Result<(),String> {
    voices::delete(&crate::app_data::app_data_dir().ok_or("无法定位声音引用")?,&id)
}
#[tauri::command]
pub fn register_media_voice(app:AppHandle,provider_id:String,model:String,voice_id:String,consent_attestation:String) -> Result<voices::VoiceReference,String> {
    if consent_attestation.trim().is_empty() {return Err("VOICE_CONSENT_REQUIRED: 必须提供用户对既有合法声音的明确授权声明".into());}
    let provider = provider(&app.state::<AppState>(),&provider_id)?;
    if !speech_providers::configured(&provider,&model) || provider.model_overrides.get(&model).and_then(|m|m.speech_protocol.as_deref()) != Some("openai_tts") {return Err("只有显式配置的 OpenAI TTS 可登记既有 custom voice；不会上传或克隆".into());}
    voices::register_custom(&crate::app_data::app_data_dir().ok_or("无法定位声音引用")?,&provider_id,&voice_id,&request_evidence::bytes_hash(consent_attestation.as_bytes()))
}
#[tauri::command]
pub async fn check_media_speech_connection(app:AppHandle,provider_id:String,model:String)->Result<Value,String> {
    let provider = provider(&app.state::<AppState>(),&provider_id)?;
    speech_providers::connection_check(&provider,&model).await
}
#[tauri::command]
pub async fn list_local_speech_voices() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(local_tts::voices).await.map_err(|error| error.to_string())
}
#[tauri::command]
pub async fn get_local_asr_status()->Result<local_asr::LocalAsrStatus,String> {local_asr::status().await}
#[tauri::command]
pub async fn install_local_asr(config:local_asr::LocalAsrConfig)->Result<local_asr::LocalAsrStatus,String> {local_asr::install(config).await}
#[tauri::command]
pub async fn cancel_local_asr_install(operation_id:String)->Result<local_asr::LocalAsrStatus,String> {local_asr::cancel_install(&operation_id).await}
#[tauri::command]
pub async fn stop_local_asr()->Result<local_asr::LocalAsrStopResult,String> {local_asr::stop().await}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordOutputRequest {
    pub origin: String,
    pub title: String,
    pub text: String,
    pub prompt: Option<String>,
}

pub(crate) fn record_text_output(
    _app: &AppHandle,
    request: RecordOutputRequest,
) -> Result<MediaTask, String> {
    record_text_at(&root()?, request)
}

fn record_text_at(root: &Path, request: RecordOutputRequest) -> Result<MediaTask, String> {
    if request.origin.trim().is_empty() || request.title.trim().is_empty() || request.text.trim().is_empty() {
        return Err("文案记录缺少来源、标题或正文".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = directory(root, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("output.md");
    std::fs::write(&path, request.text.as_bytes()).map_err(|e| e.to_string())?;
    let task = StoredTask {
        task: MediaTask {
            id,
            provider_id: "local".into(),
            model: "record".into(),
            kind: MediaKind::Text,
            status: MediaStatus::Succeeded,
            created_at: chrono::Utc::now().to_rfc3339(),
            error: None,
            remote_id: None,
            outputs: vec![MediaOutput {
                path: path.to_string_lossy().into_owned(),
                mime: "text/markdown".into(),
            }],
            can_resume: false,
            submission_state: None,
            origin: Some(request.origin),
            prompt: request.prompt.unwrap_or_default(),
            result: Some(json!({ "title": request.title })),
            request_hash: None,
            cancellation: None,
        },
        base_url: String::new(),
        protocol: "local".into(),
        download_url: None,
        download_requires_auth: false,
        accepted_at: None,
    };
    save(root, &task)?;
    Ok(task.task)
}

#[tauri::command]
pub fn record_media_output(app: AppHandle, request: RecordOutputRequest) -> Result<MediaTask, String> {
    record_text_output(&app, request)
}

fn classify_import(source: &Path) -> Result<(MediaKind, String), String> {
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (kind, mime) = match ext.as_str() {
        "png" => (MediaKind::Image, "image/png"),
        "jpg" | "jpeg" => (MediaKind::Image, "image/jpeg"),
        "webp" => (MediaKind::Image, "image/webp"),
        "gif" => (MediaKind::Image, "image/gif"),
        "mp4" => (MediaKind::Video, "video/mp4"),
        "webm" => (MediaKind::Video, "video/webm"),
        "mov" => (MediaKind::Video, "video/quicktime"),
        "mp3" => (MediaKind::Speech, "audio/mpeg"),
        "wav" => (MediaKind::Speech, "audio/wav"),
        "m4a" => (MediaKind::Speech, "audio/mp4"),
        "srt" => (MediaKind::Transcribe, "application/x-subrip"),
        "md" | "markdown" | "txt" => (MediaKind::Text, "text/markdown"),
        _ => return Err(format!("不支持导入这种文件：{ext}")),
    };
    Ok((kind, mime.into()))
}

fn import_title(source: &Path, title: Option<String>) -> String {
    if let Some(title) = title {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("导入文件")
        .to_string()
}

fn import_artifact_at(root: &Path, origin: String, source: &Path, title: Option<String>) -> Result<MediaTask, String> {
    if origin.trim().is_empty() {
        return Err("导入缺少来源".into());
    }
    if !source.is_absolute() {
        return Err("只能导入绝对路径上的文件".into());
    }
    if !source.is_file() {
        return Err("找不到要导入的文件".into());
    }
    let (kind, mime) = classify_import(source)?;
    let title = import_title(source, title);
    if kind == MediaKind::Text {
        let text = std::fs::read_to_string(source).map_err(|error| format!("文案不是 UTF-8：{error}"))?;
        return record_text_at(root, RecordOutputRequest {
            origin,
            title,
            text,
            prompt: Some(source.display().to_string()),
        });
    }
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or("文件名无效")?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = directory(root, &id)?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let dest = dir.join(file_name);
    std::fs::copy(source, &dest).map_err(|error| error.to_string())?;
    let task = StoredTask {
        task: MediaTask {
            id,
            provider_id: "local".into(),
            model: "import".into(),
            kind,
            status: MediaStatus::Succeeded,
            created_at: chrono::Utc::now().to_rfc3339(),
            error: None,
            remote_id: None,
            outputs: vec![MediaOutput {
                path: dest.to_string_lossy().into_owned(),
                mime,
            }],
            can_resume: false,
            submission_state: None,
            origin: Some(origin),
            prompt: file_name.to_string(),
            result: Some(json!({ "title": title })),
            request_hash: None,
            cancellation: None,
        },
        base_url: String::new(),
        protocol: "local".into(),
        download_url: None,
        download_requires_auth: false,
        accepted_at: None,
    };
    save(root, &task)?;
    Ok(task.task)
}

#[cfg(test)]
fn export_output_at(root: &Path, id: &str, destination: &Path) -> Result<String, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    export_task_output(&read(root, id)?.task, destination, 0)
}
fn export_task_output(task: &MediaTask, destination: &Path, index: usize) -> Result<String, String> {
    if ACTIVE.lock().contains(&task.id) || task.status == MediaStatus::Running {
        return Err("任务仍在运行，不能导出".into());
    }
    let output = task.outputs.get(index).ok_or("没有可导出的文件")?;
    let source = PathBuf::from(&output.path);
    if !source.is_file() {
        return Err("导出的文件不存在".into());
    }
    if !destination.is_absolute() {
        return Err("导出位置必须是绝对路径".into());
    }
    if destination == source {
        return Ok(source.to_string_lossy().into_owned());
    }
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    std::fs::copy(&source, destination).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn import_media_artifact(origin: String, path: String, title: Option<String>) -> Result<MediaTask, String> {
    import_artifact_at(&root()?, origin, Path::new(&path), title)
}

#[tauri::command]
pub fn export_media_output(app: AppHandle, id: String, destination: String, index: Option<usize>) -> Result<String, String> {
    let task = get_media_task(app, id, None)?;
    export_task_output(&task, Path::new(destination.trim()), index.unwrap_or(0))
}

#[tauri::command]
pub fn delete_media_task(id: String) -> Result<(), String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "无效任务编号")?;
    if ACTIVE.lock().contains(&id) {
        return Err("任务仍在运行，不能删除".into());
    }
    let media = read(&root()?, &id).ok();
    if media.as_ref().is_some_and(|task| task.task.status == MediaStatus::Running) {
        return Err("任务仍在运行，不能删除".into());
    }
    let comfy = comfyui::root().ok().and_then(|root| comfyui::read_task(&root, &id).ok());
    if comfy.as_ref().is_some_and(|task| {
        matches!(
            task.status,
            comfyui::ComfyTaskStatus::Submitting
                | comfyui::ComfyTaskStatus::Queued
                | comfyui::ComfyTaskStatus::Running
                | comfyui::ComfyTaskStatus::DownloadPending
        )
    }) {
        return Err("任务仍在运行，不能删除".into());
    }
    if media.is_none() && comfy.is_none() {
        return Err(format!("找不到任务 {id}"));
    }
    if media.is_some() {
        let data_root = root()?;
        crate::chat::artifacts::delete_media_job_in(&data_root.with_file_name("artifacts"), &data_root, &id)?;
    }
    if comfy.is_some() {
        if let Ok(comfy_root) = comfyui::root() {
            let dir = comfy_root.join(&id);
            if dir.exists() {
                let data_root = root()?;
                crate::chat::artifacts::delete_media_job_in(&data_root.with_file_name("artifacts"), &comfy_root, &id)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn delete_saved(root: &Path, id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    if ACTIVE.lock().contains(id) {
        return Err("任务仍在运行，不能删除".into());
    }
    let saved = read(root, id)?;
    if saved.task.status == MediaStatus::Running {
        return Err("任务仍在运行，不能删除".into());
    }
    crate::chat::artifacts::delete_media_job_in(&root.join("artifacts"), root, id)
}

#[tauri::command]
pub async fn start_media_generation(
    app: AppHandle,
    request: MediaRequest,
) -> Result<MediaTask, String> {
    start_configured(&app, request).await.map_err(|error| {
        // IPC keeps the existing human-readable String error; CLI retains the structured error.
        serde_json::from_str::<Value>(&error).ok()
            .filter(|value| value.get("code").and_then(Value::as_str).is_some_and(|code| code.starts_with("MODEL_")))
            .and_then(|value| value.get("message").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or(error)
    })
}
fn image_inputs(images: &[String]) -> Result<Vec<image_providers::InputImage>, String> {
    if images.len() > 16 {
        return Err("最多使用 16 张参考图".into());
    }
    images
        .iter()
        .map(|image| {
            if image.starts_with("data:") {
                let (mime_type, base64) = image_providers::parse_image_data_url(image)?;
                Ok(image_providers::InputImage { mime_type, base64 })
            } else {
                image_providers::load_input_images_from_paths(&[PathBuf::from(image)])?
                    .into_iter()
                    .next()
                    .ok_or("参考图无法读取".into())
            }
        })
        .collect()
}
fn run_background(app: AppHandle, id: String, request: Option<MediaRequest>) {
    let Some(guard) = claim(&id) else {
        return;
    };
    spawn_background(app, id, request, guard);
}
const POLL_PAUSE: Duration = Duration::from_secs(3);
/// Non-pending read errors (e.g. authorization/download failures) stop this worker
/// after a bounded number of tries. Expected pending HTTP responses are handled by the protocol.
const MAX_CONSECUTIVE_READ_ERRORS: u32 = 10;
// One worker performs at most 30 minutes of scheduled waits. Callers can continue
// querying the same receipt after this bound; it never authorizes another POST.
const MAX_POLL_READS: u32 = 600;
/// Query the saved receipt until the task settles. Never submits, so retrying a read is free.
async fn poll_until_settled<F, Fut>(mut refresh: F, pause: Duration) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<MediaTask, String>>,
{
    let mut consecutive_errors = 0;
    for _ in 0..MAX_POLL_READS {
        match refresh().await {
            Ok(task) if task.status != MediaStatus::Running => return Ok(()),
            Ok(_) => consecutive_errors = 0,
            Err(error) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                    return Err(error);
                }
            }
        }
        tokio::time::sleep(pause).await;
    }
    Err("查询等待达到上限，生成结果尚未确认；请继续查询同一任务，勿重新提交".into())
}
fn spawn_background(app: AppHandle, id: String, request: Option<MediaRequest>, guard: Active) {
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let result = async {
            if let Some(request) = request {
                submit_cloud_at(&root()?, &app.state::<AppState>(), &id, request).await?;
            }
            // Reading/downloading never submits. A restart resumes from the saved receipt.
            poll_until_settled(|| refresh_once(&app, &id), POLL_PAUSE).await
        }
        .await;
        if let Err(error) = result {
            if let Ok(root) = root() {
                if let Ok(mut saved) = read(&root, &id) {
                    record_background_error(&mut saved, error);
                    let _ = save(&root, &saved);
                } else if let Ok(comfy_root) = comfyui::root() {
                    if let Ok(mut task) = comfyui::read_task(&comfy_root, &id) {
                        task.status = comfyui::ComfyTaskStatus::DownloadPending;
                        task.error = Some(error);
                        let _ = comfyui::save(&comfy_root, &task);
                    }
                }
            }
        }
        if let Some(control) = CONTROLS.lock().remove(&id) { control.finished.notify_waiters(); }
    });
}
fn record_background_error(saved: &mut StoredTask, error: String) {
    // A provider-confirmed failure or completed output is already authoritative.
    if saved.task.status != MediaStatus::Running {
        return;
    }
    saved.task.can_resume = saved.task.remote_id.is_some() || saved.download_url.is_some();
    if !saved.task.can_resume {
        saved.task.status = MediaStatus::Failed;
    }
    saved.task.error = Some(error);
}
/// How long an accepted receipt may stay invisible at the provider before the task stops
/// waiting. Observed gateways lose some receipts outright; others lag briefly after acceptance.
const RECEIPT_GRACE: chrono::TimeDelta = chrono::TimeDelta::minutes(10);
fn receipt_lost(saved: &StoredTask, now: chrono::DateTime<chrono::Utc>) -> bool {
    let since = saved
        .accepted_at
        .as_deref()
        .unwrap_or(&saved.task.created_at);
    chrono::DateTime::parse_from_rfc3339(since)
        .is_ok_and(|at| now.signed_duration_since(at) > RECEIPT_GRACE)
}
/// Settle a read that did not return the receipt: keep waiting within the grace period, then
/// stop with the receipt kept so the user can query it once more after checking with the provider.
fn apply_unseen_receipt(
    saved: &mut StoredTask,
    detail: String,
    now: chrono::DateTime<chrono::Utc>,
) {
    if !receipt_lost(saved, now) {
        saved.task.error = Some(detail);
        return;
    }
    let receipt = saved.task.remote_id.as_deref().unwrap_or("");
    saved.task.status = MediaStatus::Failed;
    saved.task.can_resume = true;
    saved.task.error = Some(format!(
        "供应商受理后一直查不到该任务（回执 {receipt}），可能已扣费。请凭回执联系供应商核实；确认未生成后再重新生成"
    ));
}
async fn submit_cloud_at(
    root: &Path,
    state: &AppState,
    id: &str,
    request: MediaRequest,
) -> Result<(), String> {
    if !begin_execution(root,id,ControlPhase::Remote)? {return Ok(());}
    let mut saved = read(root, id)?;
    let p = provider(state, &request.provider_id)?;
    if p.base_url != saved.base_url {
        return Err("供应商连接已变更，本次未提交".into());
    }
    if request.kind == MediaKind::Image {
        let submitted = submit_image(root, state, &p, &mut saved, &request).await;
        if let Err(error) = &submitted {
            // A task with a receipt or any saved output is past submission; the poller owns it.
            if saved.task.remote_id.is_none()
                && saved.download_url.is_none()
                && saved.task.outputs.is_empty()
            {
                saved.task.submission_state = Some(image_submission_state(error));
                saved.task.status = MediaStatus::Failed;
                saved.task.can_resume = false;
                saved.task.error = Some(error.clone());
                save(root, &saved)?;
            }
        }
        return submitted;
    }
    if providers::selected(&p, &request.model)? != saved.protocol {
        return Err("供应商协议已变更，本次未提交".into());
    }
    let result = providers::submit_video_model_request(
        &state,
        &p,
        request.model.clone(),
        video_input(&request)?,
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            saved.task.submission_state = Some(if error.rejected {
                MediaSubmissionState::Rejected
            } else {
                MediaSubmissionState::Uncertain
            });
            saved.task.status = MediaStatus::Failed;
            saved.task.can_resume = false;
            saved.task.error = Some(error.to_string());
            save(root, &saved)?;
            return Err(error.to_string());
        }
    };
    // Persist the receipt first. Failure to write diagnostics must never lose a paid task.
    let diagnostics = [
        ("submission-request.json", result.submission_request.clone()),
        (
            "submission-response.json",
            result.submission_response.clone(),
        ),
    ];
    apply_result(&mut saved, result);
    save(root, &saved)?;
    for (name, value) in diagnostics {
        let Some(value) = value else { continue };
        let mut text = value.to_string();
        for key in p.api_keys.iter().filter(|k| !k.is_empty()) {
            text = text.replace(key, "[redacted]");
        }
        let _ = std::fs::write(directory(root, id)?.join(name), text);
    }
    Ok(())
}
/// Image submission failures carry an HTTP status when the provider answered. A definite 4xx
/// (not timeout/rate limit) was refused before generation; anything else may have been charged.
fn image_submission_state(error: &str) -> MediaSubmissionState {
    match crate::api::extract_status_code(error) {
        Some(code) if (400..500).contains(&code) && !matches!(code, 408 | 429) => {
            MediaSubmissionState::Rejected
        }
        _ => MediaSubmissionState::Uncertain,
    }
}
async fn submit_image(
    root: &Path,
    state: &AppState,
    p: &ModelProvider,
    saved: &mut StoredTask,
    request: &MediaRequest,
) -> Result<(), String> {
    let id = saved.task.id.clone();
    if image_providers::resolve_image_route(p, &request.model)
        == image_providers::ImageRoute::AsyncTask
    {
        let value = image_gateway::submit(state, p, &id, request).await?;
        saved.protocol = "image-async".into();
        saved.task.remote_id = image_gateway::remote_task_id(&value);
        saved.download_url = image_gateway::image_download_url(&value).map(str::to_owned);
        if saved.task.remote_id.is_some() || saved.download_url.is_some() {
            saved.accepted_at = Some(chrono::Utc::now().to_rfc3339());
            // Persist the receipt before any download that may fail.
            return save(root, saved);
        }
        let bytes = image_gateway::immediate_image(p, &value).await?;
        saved.task.outputs =
            vec![artifacts::save_image(&bytes, &directory(root, &id)?.join("output-0")).await?];
    } else {
        let args = image_arguments(p, request)?;
        let batch = image_providers::generate_image_with_provider(
            state,
            p,
            &request.model,
            &args,
            &image_inputs(&request.images)?,
            0,
            "Media generation",
        )
        .await?;
        for (i, bytes) in batch.images.iter().enumerate() {
            let path = directory(root, &id)?.join(format!("output-{i}"));
            saved
                .task
                .outputs
                .push(artifacts::save_image(bytes, &path).await?);
            // A later output may fail; keep the images already saved rather than losing a paid result.
            save(root, saved)?;
        }
        if let Some(note) = batch.note {
            saved.task.error = Some(note);
        }
    }
    saved.task.status = MediaStatus::Succeeded;
    save(root, saved)
}
fn apply_result(saved: &mut StoredTask, result: providers::VideoResult) {
    saved.task.submission_state = None;
    if !result.remote_id.is_empty() && saved.task.remote_id.is_none() {
        saved.accepted_at = Some(chrono::Utc::now().to_rfc3339());
    }
    if !result.remote_id.is_empty() {
        saved.task.remote_id = Some(result.remote_id);
    }
    saved.download_url = result.download_url;
    saved.download_requires_auth = result.download_requires_auth;
    if matches!(result.status.as_str(), "failed" | "expired" | "cancelled") {
        saved.task.status = MediaStatus::Failed;
        saved.task.can_resume = false;
        saved.task.error = Some(result.error.unwrap_or("供应商生成失败".into()));
    } else {
        saved.task.error = result.error;
    }
}
async fn refresh_once(app: &AppHandle, id: &str) -> Result<MediaTask, String> {
    let root = root()?;
    if !directory(&root, id)?.join("task.json").exists() {
        return Ok(from_comfy(
            comfyui::refresh_at(&comfyui::root()?, id).await?,
        ));
    }
    refresh_cloud_at(&app.state::<AppState>(), &root, id).await
}
async fn refresh_cloud_at(state: &AppState, root: &Path, id: &str) -> Result<MediaTask, String> {
    let mut saved = read(root, id)?;
    if saved.task.status != MediaStatus::Running {
        return Ok(saved.task);
    }
    if saved.task.kind == MediaKind::Image && saved.protocol == "image-async" {
        let p = provider(state, &saved.task.provider_id)?;
        if p.base_url.trim_end_matches('/') != saved.base_url.trim_end_matches('/') {
            return Err("原任务连接已变更，请恢复原供应商地址后查询".into());
        }
        let bytes = if let Some(url) = saved.download_url.as_deref() {
            image_gateway::download(&p, url).await?
        } else if let Some(remote) = saved.task.remote_id.clone() {
            let cfg = image_gateway::ReceiptConfig {
                provider_id: p.id,
                model: saved.task.model.clone(),
                protocol: "async".into(),
            };
            match image_gateway::poll(state, &cfg, id, &remote).await? {
                image_gateway::ReceiptRead::Ready(bytes) => bytes,
                image_gateway::ReceiptRead::Pending => return Ok(saved.task),
                image_gateway::ReceiptRead::Unseen(code) => {
                    let detail = format!("{}（HTTP {code}）", providers::RECEIPT_UNSEEN);
                    apply_unseen_receipt(&mut saved, detail, chrono::Utc::now());
                    save(root, &saved)?;
                    return Ok(saved.task);
                }
                image_gateway::ReceiptRead::Failed(error) => {
                    saved.task.status = MediaStatus::Failed;
                    saved.task.can_resume = false;
                    saved.task.error = Some(error);
                    save(root, &saved)?;
                    return Ok(saved.task);
                }
            }
        } else {
            return Ok(saved.task);
        };
        saved.task.outputs =
            vec![artifacts::save_image(&bytes, &directory(root, id)?.join("output-0")).await?];
        saved.task.status = MediaStatus::Succeeded;
        saved.task.can_resume = false;
        saved.task.error = None;
        save(root, &saved)?;
        return Ok(saved.task);
    }
    let Some(remote) = saved.task.remote_id.clone() else {
        return Ok(saved.task);
    };
    if saved.download_url.is_none() {
        let result = providers::query_video_model_request(
            &state,
            saved.task.provider_id.clone(),
            saved.task.model.clone(),
            saved.protocol.clone(),
            saved.base_url.clone(),
            remote,
        )
        .await?;
        let unseen = result.status == "running"
            && result
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with(providers::RECEIPT_UNSEEN));
        if unseen {
            let detail = result.error.unwrap_or_default();
            apply_unseen_receipt(&mut saved, detail, chrono::Utc::now());
        } else {
            apply_result(&mut saved, result);
        }
        save(&root, &saved)?;
    }
    if let Some(url) = saved
        .download_url
        .clone()
        .filter(|_| saved.task.status == MediaStatus::Running)
    {
        let p = provider(&state, &saved.task.provider_id)?;
        if p.base_url.trim_end_matches('/') != saved.base_url.trim_end_matches('/') {
            return Err("原任务连接已变更，请恢复原供应商地址后下载".into());
        }
        let path = directory(&root, id)?.join("output.mp4");
        let output = download(
            &p,
            &saved.base_url,
            &url,
            &saved.protocol,
            saved.download_requires_auth,
            &path,
        )
        .await?;
        saved.task.outputs = vec![output];
        saved.task.status = MediaStatus::Succeeded;
        saved.task.can_resume = false;
        saved.task.error = None;
        save(&root, &saved)?;
    }
    Ok(saved.task)
}
async fn download(
    provider: &ModelProvider,
    base: &str,
    url: &str,
    protocol: &str,
    auth: bool,
    path: &Path,
) -> Result<MediaOutput, String> {
    let origin = reqwest::Url::parse(base).map_err(|_| "原服务地址无效")?;
    // A provider-relative media path belongs to its authenticated API. Absolute CDN URLs
    // remain unauthenticated unless the protocol explicitly requires credentials.
    let auth = auth || (url.starts_with('/') && !url.starts_with("//"));
    let key = provider
        .api_keys
        .get(provider.active_key_index)
        .or_else(|| provider.api_keys.first());
    let credential = match (auth, key) {
        (false, _) => artifacts::DownloadAuth::None,
        (true, None) => return Err("缺少下载密钥".into()),
        (true, Some(key)) => match providers::download_auth(protocol)? {
            "google" => artifacts::DownloadAuth::Google(key),
            "token" => artifacts::DownloadAuth::Token(key),
            _ => artifacts::DownloadAuth::Bearer(key),
        },
    };
    let response = artifacts::fetch(
        artifacts::download_client(provider),
        &origin,
        url,
        credential,
        Duration::from_secs(180),
    )
    .await?;
    artifacts::save_response(response, path, &MediaKind::Video).await
}
#[tauri::command]
pub fn get_media_task(
    app: AppHandle,
    id: String,
    resume: Option<bool>,
) -> Result<MediaTask, String> {
    let root = root()?;
    if !directory(&root, &id)?.join("task.json").exists() {
        let mut task = from_comfy(comfyui::read_task(&comfyui::root()?, &id)?);
        if task.status == MediaStatus::Running || resume == Some(true) && task.can_resume {
            run_background(app, id.clone(), None);
        }
        if ACTIVE
            .lock()
            .contains(&id)
            && (task.status == MediaStatus::Running || task.can_resume)
        {
            task.status = MediaStatus::Running;
            task.can_resume = false;
            task.error = None;
        }
        return Ok(task);
    }
    let Some(guard) = claim(&id) else {
        return Ok(read(&root, &id)?.task);
    };
    let mut saved = read(&root, &id)?;
    recover_task(&root, &mut saved, resume == Some(true))?;
    if saved.task.status == MediaStatus::Running {
        if saved.task.kind == MediaKind::Speech {
            let prepared = (|| {
                let data = crate::app_data::app_data_dir().ok_or("无法定位声音引用")?;
                let provider = provider(&app.state::<AppState>(),&saved.task.provider_id)?;
                let info = provider.model_overrides.get(&saved.task.model).ok_or("语音连接已移除")?;
                if info.speech_base_url.as_deref() != Some(saved.base_url.as_str()) || info.speech_protocol.as_deref() != Some(saved.protocol.as_str()) {
                    return Err("语音连接已变更；恢复原产品连接后可恢复同一任务".to_owned());
                }
                let input = speech_providers::load_input(&directory(&root,&id)?)?;
                Ok((data,provider,input))
            })();
            let (data,provider,input) = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    saved.task.status = MediaStatus::Failed;
                    saved.task.can_resume = speech_providers::is_resumable(&directory(&root,&id)?);
                    saved.task.error = Some(error);
                    save(&root,&saved)?;
                    return Ok(saved.task);
                },
            };
            restore_speech_control(&id,&directory(&root,&id)?,&input.mode)?;
            spawn_audio_task(root,data,provider,saved.clone(),AudioWork::Speech(input),guard);
        } else {
            spawn_background(app, id, None, guard);
        }
    }
    Ok(saved.task)
}
// Caller holds the task claim. Recovery never repeats an uncertain or completed paid POST.
fn recover_task(root: &Path, saved: &mut StoredTask, resume: bool) -> Result<(), String> {
    if local_edit::is_local_edit_protocol(&saved.protocol) && !matches!(saved.task.status, MediaStatus::Succeeded | MediaStatus::Cancelled) {
        if saved.task.status == MediaStatus::Running {
            saved.task.status = MediaStatus::Failed;
            saved.task.error = Some("本地处理已中断，可重新开始；未调用云服务".into());
            saved.task.submission_state = Some(MediaSubmissionState::Rejected);
        }
        saved.task.can_resume = false;
        save(root,saved)?;
        return Ok(());
    }
    if saved.protocol == "system_tts" && !matches!(saved.task.status, MediaStatus::Succeeded | MediaStatus::Cancelled) {
        if saved.task.status == MediaStatus::Running {
            saved.task.status = MediaStatus::Failed;
            saved.task.error = Some("本地配音执行已中断，可重新生成；未调用云服务".into());
        }
        saved.task.can_resume = false;
        save(root,saved)?;
        return Ok(());
    }
    if saved.task.kind == MediaKind::Speech && saved.task.status != MediaStatus::Succeeded && saved.task.status != MediaStatus::Cancelled {
        let can_resume = speech_providers::is_resumable(&directory(root,&saved.task.id)?);
        saved.task.can_resume = can_resume;
        if resume && can_resume {
            saved.task.status = MediaStatus::Running;
            saved.task.error = None;
            saved.task.can_resume = false;
        } else if saved.task.status == MediaStatus::Running {
            saved.task.status = MediaStatus::Failed;
            saved.task.error = Some(if can_resume {"语音步骤已保存；可恢复同一任务，不会重复已完成步骤"} else {"语音步骤结果不确定；只能核实，不能重复提交"}.into());
            if !can_resume {saved.task.submission_state=Some(MediaSubmissionState::Uncertain);}
        }
        save(root,saved)?;
        return Ok(());
    }
    if saved.task.status == MediaStatus::Running
        && saved.task.remote_id.is_none()
        && saved.download_url.is_none()
    {
        saved.task.status = MediaStatus::Failed;
        saved.task.can_resume = false;
        saved.task.error = Some("上次生成中断，结果未确认，请核查后再决定是否重新生成".into());
        save(root, saved)?;
    }
    // An interrupted worker (Running + resumable) continues on its own; a stopped task
    // (Failed + resumable, e.g. a lost receipt) is queried again only on an explicit resume.
    if saved.task.can_resume && (resume || saved.task.status == MediaStatus::Running) {
        saved.task.status = MediaStatus::Running;
        saved.task.error = None;
        saved.task.can_resume = false;
        if resume && saved.task.remote_id.is_some() {
            // Refresh expired signed URLs with the SAME receipt, and give it a fresh grace period.
            saved.download_url = None;
            saved.accepted_at = Some(chrono::Utc::now().to_rfc3339());
        }
        save(root, saved)?;
    }
    Ok(())
}
#[tauri::command]
pub fn list_media_tasks(app: AppHandle, filter: MediaTaskFilter) -> Result<Vec<MediaTask>, String> {
    let mut tasks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root()?) {
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if let Ok(saved) = read(&root()?, &id) {
                if filter.matches(&saved.task) {
                    tasks.push(get_media_task(app.clone(), id, None)?);
                }
            }
        }
    }
    for task in comfyui::list_comfy_tasks(|task| {
        filter
            .provider_id
            .as_ref()
            .is_none_or(|p| *p == task.provider_id)
            && filter.model.as_ref().is_none_or(|m| *m == task.workflow_id)
            && filter
                .origin
                .as_ref()
                .is_none_or(|o| task.origin.as_ref() == Some(o))
    })? {
        tasks.push(get_media_task(app.clone(), task.id, None)?);
    }
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tasks)
}
pub(crate) fn tool_result(task: MediaTask) -> crate::mcp::types::McpToolCallResult {
    let artifacts = task
        .outputs
        .iter()
        .map(|o| crate::mcp::types::ChatToolArtifact {
            id: None,
            name: Path::new(&o.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            mime_type: o.mime.clone(),
            data_url: String::new(),
            size_bytes: std::fs::metadata(&o.path).ok().map(|m| m.len()),
            path: Some(o.path.clone()),
        })
        .collect();
    crate::mcp::types::McpToolCallResult {
        content: serde_json::to_string(&task).unwrap_or_default(),
        is_error: task.status == MediaStatus::Failed,
        raw: json!(task),
        artifacts,
        ..Default::default()
    }
}
/// Read a task until it settles, the deadline passes (`None` waits indefinitely) or `stop` is set.
/// Reading never submits; the background worker owns the provider queries.
pub(crate) async fn wait(
    app: &AppHandle,
    id: &str,
    deadline: Option<Duration>,
    stop: &(dyn Fn() -> bool + Sync),
) -> Result<MediaTask, String> {
    let until = deadline.map(|d| tokio::time::Instant::now() + d);
    loop {
        let task = get_media_task(app.clone(), id.into(), None)?;
        let expired = until.is_some_and(|at| tokio::time::Instant::now() >= at);
        if task.status != MediaStatus::Running || expired || stop() {
            return Ok(task);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Chat tools take the first usable pool member they can drive. ComfyUI workflows are skipped:
/// their inputs are user-defined bindings, not the chat tool's prompt/size schema.
pub(crate) fn chat_model(settings: &crate::settings::Settings, kind: &MediaKind) -> Option<(String, String)> {
    cli::pool(settings, kind)
        .iter()
        .filter(|entry| cli::usable(settings, entry, kind))
        .find(|entry| settings.get_provider(&entry.provider_id).is_some_and(|p| p.request.comfy.is_none()))
        .map(|entry| (entry.provider_id.clone(), entry.model.clone()))
}

pub(crate) async fn tool_call(
    app: &AppHandle,
    name: &str,
    args: Value,
) -> Result<crate::mcp::types::McpToolCallResult, String> {
    if name == "mixer_process_video" {
        let operation = args.get("operation").and_then(Value::as_str).ok_or("需要 operation")?;
        let (model, options) = match operation {
            "subtitle" => {
                let mut options = BTreeMap::new();
                options.insert("video".into(), json!(args.get("video").and_then(Value::as_str).ok_or("需要 video")?));
                options.insert("language".into(), json!(args.get("language").and_then(Value::as_str).ok_or("需要 language")?));
                if let Some(burn) = args.get("burn") {
                    options.insert("burn".into(), burn.clone());
                }
                (local_edit::MODEL_SUBTITLE, options)
            }
            "edit" => {
                let mut options = BTreeMap::new();
                options.insert("plan".into(), args.get("plan").cloned().ok_or("需要 plan")?);
                (local_edit::MODEL_EDIT, options)
            }
            _ => return Err("operation 只能是 subtitle 或 edit".into()),
        };
        let task = start(
            app,
            MediaRequest {
                provider_id: "local".into(),
                model: model.into(),
                kind: MediaKind::Edit,
                prompt: String::new(),
                images: vec![],
                options,
                origin: Some("chat".into()),
                description_revision: None,
            },
        )
        .await?;
        return Ok(tool_result(task));
    }
    if name == "mixer_media_task" {
        let id = args["id"].as_str().ok_or("需要任务编号")?;
        get_media_task(app.clone(), id.into(), args["resume"].as_bool())?;
        return Ok(tool_result(
            wait(
                app,
                id,
                Some(Duration::from_secs(
                    args["waitSeconds"].as_u64().unwrap_or(0).min(30),
                )),
                &|| false,
            )
            .await?,
        ));
    }
    let (provider_id, model) = chat_model(&app.state::<AppState>().settings_read(), &MediaKind::Video)
        .ok_or("请先在「设置 > 媒体创作」的视频模型池开启模型")?;
    let mut options: BTreeMap<String, Value> =
        serde_json::from_value(args).map_err(|e| e.to_string())?;
    let prompt = options
        .remove("prompt")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("需要提示词")?;
    let task = start(
        app,
        MediaRequest {
            provider_id,
            model,
            kind: MediaKind::Video,
            prompt,
            images: vec![],
            options,
            origin: Some("chat".into()),
            description_revision: None,
        },
    )
    .await?;
    Ok(tool_result(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn workbench_local_text_record_create_list_delete() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let task = record_text_at(root, RecordOutputRequest {
            origin: "workbench/posts".into(),
            title: "上新标题".into(),
            text: "正文与标签".into(),
            prompt: Some("brief".into()),
        }).unwrap();
        assert_eq!(task.kind, MediaKind::Text);
        assert_eq!(task.status, MediaStatus::Succeeded);
        assert_eq!(task.result.as_ref().unwrap()["title"], "上新标题");
        assert_eq!(std::fs::read_to_string(root.join(&task.id).join("output.md")).unwrap(), "正文与标签");
        let listed: Vec<_> = std::fs::read_dir(root).unwrap().flatten().filter_map(|entry| read(root, &entry.file_name().to_string_lossy()).ok()).filter(|saved| MediaTaskFilter { origin: Some("workbench/posts".into()), ..Default::default() }.matches(&saved.task)).collect();
        assert_eq!(listed.len(), 1);
        let mut running = read(root, &task.id).unwrap();
        running.task.id = uuid::Uuid::new_v4().to_string();
        running.task.status = MediaStatus::Running;
        save(root, &running).unwrap();
        assert!(delete_saved(root, &running.task.id).unwrap_err().contains("运行"));
        assert!(read(root, &running.task.id).is_ok());
        delete_saved(root, &task.id).unwrap();
        assert!(read(root, &task.id).is_err());
    }
    #[test]
    fn reusable_media_input_keeps_canonical_parameters_without_provider_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let request = MediaRequest {
            provider_id: "provider".into(), model: "image-model".into(), kind: MediaKind::Image,
            prompt: "product photo".into(), images: vec!["/tmp/reference.png".into()],
            options: BTreeMap::from([("aspectRatio".into(), json!("16:9"))]),
            origin: Some("media-station".into()), description_revision: Some("revision".into()),
        };
        save_reusable_request(dir.path(), &id, &request).unwrap();
        let stored: Value = serde_json::from_slice(&std::fs::read(dir.path().join(id).join("reuse-request.json")).unwrap()).unwrap();
        assert_eq!(stored, serde_json::to_value(&request).unwrap());
        assert!(stored.get("apiKey").is_none());
        assert!(stored.get("baseUrl").is_none());
        assert!(media_task_request("../invalid".into()).is_err());
    }
    #[test]
    fn export_selects_requested_output_and_rejects_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.png");
        let second = dir.path().join("second.png");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();
        let mut task = import_artifact_at(dir.path(), "media-station".into(), &first, None).unwrap();
        task.outputs.push(MediaOutput { path: second.to_string_lossy().into(), mime: "image/png".into() });
        let destination = dir.path().join("exported.png");
        export_task_output(&task, &destination, 1).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"second");
        assert!(export_task_output(&task, &destination, 2).is_err());
        assert!(export_task_output(&task, Path::new("relative.png"), 0).is_err());
    }
    #[test]
    fn workbench_local_import_and_export() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let source = dir.path().join("photo.png");
        std::fs::write(&source, b"png-bytes").unwrap();
        assert!(import_artifact_at(root, "  ".into(), &source, None).is_err());
        assert!(import_artifact_at(root, "workbench/assets".into(), Path::new("photo.png"), None).is_err());
        let task = import_artifact_at(root, "workbench/assets".into(), &source, None).unwrap();
        assert_eq!(task.kind, MediaKind::Image);
        assert_eq!(task.status, MediaStatus::Succeeded);
        assert_eq!(task.model, "import");
        assert_eq!(task.origin.as_deref(), Some("workbench/assets"));
        assert_eq!(std::fs::read(&task.outputs[0].path).unwrap(), b"png-bytes");
        let dest = dir.path().join("out").join("copy.png");
        let exported = export_output_at(root, &task.id, &dest).unwrap();
        assert_eq!(std::fs::read(exported).unwrap(), b"png-bytes");
        let mut running = read(root, &task.id).unwrap();
        running.task.status = MediaStatus::Running;
        save(root, &running).unwrap();
        assert!(export_output_at(root, &task.id, &dir.path().join("blocked.png")).unwrap_err().contains("运行"));
        let note = dir.path().join("note.md");
        std::fs::write(&note, "# 标题\n正文").unwrap();
        let text = import_artifact_at(root, "workbench/assets".into(), &note, Some("标题".into())).unwrap();
        assert_eq!(text.kind, MediaKind::Text);
        assert_eq!(text.model, "record");
        assert_eq!(std::fs::read_to_string(root.join(&text.id).join("output.md")).unwrap(), "# 标题\n正文");
        let bin = dir.path().join("payload.exe");
        std::fs::write(&bin, b"x").unwrap();
        assert!(import_artifact_at(root, "workbench/assets".into(), &bin, None).unwrap_err().contains("不支持导入"));
        assert!(import_artifact_at(root, "workbench/assets".into(), &dir.path().join("missing.png"), None).is_err());
    }
    #[test]
    fn workbench_local_edit_recovery_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media-tasks");
        let (_, mut saved, _) = fixture("http://127.0.0.1:1", MediaKind::Edit);
        saved.protocol = "ffmpeg_edit".into();
        saved.task.status = MediaStatus::Running;
        saved.task.model = "ffmpeg-edit".into();
        save(&root, &saved).unwrap();
        recover_task(&root, &mut saved, false).unwrap();
        assert_eq!(saved.task.status, MediaStatus::Failed);
        assert_eq!(saved.task.submission_state, Some(MediaSubmissionState::Rejected));
        assert!(!saved.task.can_resume);
        assert!(saved.task.error.unwrap().contains("本地处理已中断"));
    }
    #[test]
    fn resumed_synthesized_speech_cannot_be_cancelled_as_unsent_or_lose_paid_output() {
        let data=tempfile::tempdir().unwrap();
        let root=data.path().join("media-tasks");
        let (_,mut saved,_)=fixture("https://example.invalid",MediaKind::Speech);
        saved.task.model="gpt-4o-mini-tts".into();
        saved.protocol="openai_tts".into();
        let provider:ModelProvider=serde_json::from_value(json!({"id":"p","name":"speech","baseUrl":"https://example.invalid",
            "apiKeys":["synthetic-test-key"],"modelOverrides":{"gpt-4o-mini-tts":{"speechProtocol":"openai_tts","speechBaseUrl":"https://example.invalid/v1"}}})).unwrap();
        let input=speech_providers::validate_input(&provider,&saved.task.model,&BTreeMap::from([("text".into(),json!("paid speech")),("voice".into(),json!("alloy"))]),data.path()).unwrap();
        let directory=directory(&root,&saved.task.id).unwrap();
        speech_providers::persist_input(&input,&directory).unwrap();
        let mut journal=voices::Journal::load_or_new(&directory,&saved.task.id,"captured","alloy",false).unwrap();
        journal.step=voices::Step::Synthesized;
        journal.save(&directory).unwrap();
        saved.task.status=MediaStatus::Failed;
        save(&root,&saved).unwrap();
        recover_task(&root,&mut saved,true).unwrap();
        restore_speech_control(&saved.task.id,&directory,&input.mode).unwrap();
        assert!(begin_execution(&root,&saved.task.id,ControlPhase::Pending).unwrap());
        let cancelled=cancel_at(&root,&saved.task.id).unwrap().2;
        assert_eq!(cancelled.outcome,CancelOutcome::Unsupported);
        assert_eq!(cancelled.charged,ChargeFact::Maybe);
        assert_eq!(cancelled.task.status,MediaStatus::Running);
        saved.task.status=MediaStatus::Succeeded;
        saved.task.outputs=vec![MediaOutput {path:"paid.wav".into(),mime:"audio/wav".into()}];
        save(&root,&saved).unwrap();
        let completed=read(&root,&saved.task.id).unwrap().task;
        assert_eq!(completed.status,MediaStatus::Succeeded);
        assert_eq!(completed.outputs[0].path,"paid.wav");
        CONTROLS.lock().remove(&saved.task.id);
    }
    #[test]
    fn resumed_tts_without_a_submission_receipt_remains_safely_cancellable() {
        let root=tempfile::tempdir().unwrap();
        let (_,saved,_)=fixture("https://example.invalid",MediaKind::Speech);
        save(root.path(),&saved).unwrap();
        restore_speech_control(&saved.task.id,&directory(root.path(),&saved.task.id).unwrap(),"tts").unwrap();
        assert!(begin_execution(root.path(),&saved.task.id,ControlPhase::Pending).unwrap());
        let cancelled=cancel_at(root.path(),&saved.task.id).unwrap().2;
        assert_eq!(cancelled.outcome,CancelOutcome::Confirmed);
        assert_eq!(cancelled.charged,ChargeFact::No);
        assert_eq!(cancelled.task.status,MediaStatus::Cancelled);
        assert!(!begin_execution(root.path(),&saved.task.id,ControlPhase::Remote).unwrap());
        CONTROLS.lock().remove(&saved.task.id);
    }
    #[test]
    fn speech_recovery_requires_explicit_resume_and_never_repeats_uncertain_step() {
        let data=tempfile::tempdir().unwrap();
        let root=data.path().join("media-tasks");
        let (_,mut saved,_)=fixture("https://example.invalid",MediaKind::Speech);
        saved.task.model="gpt-4o-mini-tts".into();
        saved.protocol="openai_tts".into();
        let provider:ModelProvider=serde_json::from_value(json!({"id":"p","name":"speech","baseUrl":"https://example.invalid",
            "apiKeys":["synthetic-test-key"],"modelOverrides":{"gpt-4o-mini-tts":{"speechProtocol":"openai_tts","speechBaseUrl":"https://example.invalid/v1"}}})).unwrap();
        let input=speech_providers::validate_input(&provider,&saved.task.model,&BTreeMap::from([("text".into(),json!("test")),("voice".into(),json!("alloy"))]),data.path()).unwrap();
        let directory=directory(&root,&saved.task.id).unwrap();
        speech_providers::persist_input(&input,&directory).unwrap();
        save(&root,&saved).unwrap();
        recover_task(&root,&mut saved,false).unwrap();
        assert_eq!(saved.task.status,MediaStatus::Failed);
        assert!(saved.task.can_resume);
        recover_task(&root,&mut saved,true).unwrap();
        assert_eq!(saved.task.status,MediaStatus::Running);
        let mut journal=voices::Journal::load_or_new(&directory,&saved.task.id,"captured","alloy",false).unwrap();
        journal.begin(&directory,"tts").unwrap();
        recover_task(&root,&mut saved,false).unwrap();
        assert_eq!(saved.task.status,MediaStatus::Failed);
        assert!(!saved.task.can_resume);
        assert_eq!(saved.task.submission_state,Some(MediaSubmissionState::Uncertain));
        recover_task(&root,&mut saved,true).unwrap();
        assert_eq!(saved.task.status,MediaStatus::Failed);
    }
    #[test]
    fn pending_cancel_blocks_send_and_late_success_cannot_overwrite_terminal_record() {
        let root = tempfile::tempdir().unwrap();
        let (_,saved,_) = fixture("https://example.invalid",MediaKind::Video);
        save(root.path(),&saved).unwrap();
        register_control(&saved.task.id);
        let first = cancel_at(root.path(),&saved.task.id).unwrap().2;
        assert_eq!(first.outcome,CancelOutcome::Confirmed);
        assert_eq!(first.charged,ChargeFact::No);
        assert!(!begin_execution(root.path(),&saved.task.id,ControlPhase::Remote).unwrap());
        let mut late = saved.clone();
        late.task.status=MediaStatus::Succeeded;
        late.task.outputs=vec![MediaOutput {path:"retained.mp4".into(),mime:"video/mp4".into()}];
        save(root.path(),&late).unwrap();
        let terminal=read(root.path(),&saved.task.id).unwrap();
        assert_eq!(terminal.task.status,MediaStatus::Cancelled);
        assert!(terminal.task.outputs.is_empty());
        let second=cancel_at(root.path(),&saved.task.id).unwrap().2;
        assert_eq!(second.outcome,CancelOutcome::Confirmed);
        assert_eq!(first.task.cancellation.unwrap().confirmed_at,second.task.cancellation.unwrap().confirmed_at);
        CONTROLS.lock().remove(&saved.task.id);
    }

    #[test]
    fn issued_cloud_cancel_is_unsupported_and_completion_becomes_too_late_without_deleting_outputs() {
        let root=tempfile::tempdir().unwrap();
        let (_,mut saved,_)=fixture("https://example.invalid",MediaKind::Video);
        saved.task.remote_id=Some("paid-receipt".into());
        save(root.path(),&saved).unwrap();
        register_control(&saved.task.id);
        assert!(begin_execution(root.path(),&saved.task.id,ControlPhase::Remote).unwrap());
        let unsupported=cancel_at(root.path(),&saved.task.id).unwrap().2;
        assert_eq!(unsupported.outcome,CancelOutcome::Unsupported);
        assert_eq!(unsupported.charged,ChargeFact::Maybe);
        assert_eq!(unsupported.task.status,MediaStatus::Running);
        assert_eq!(unsupported.task.remote_id.as_deref(),Some("paid-receipt"));
        saved.task.status=MediaStatus::Succeeded;
        saved.task.outputs=vec![MediaOutput {path:"paid.mp4".into(),mime:"video/mp4".into()}];
        save(root.path(),&saved).unwrap();
        let late=cancel_at(root.path(),&saved.task.id).unwrap().2;
        assert_eq!(late.outcome,CancelOutcome::TooLate);
        assert_eq!(late.task.outputs[0].path,"paid.mp4");
        assert_eq!(read(root.path(),&saved.task.id).unwrap().task.remote_id.as_deref(),Some("paid-receipt"));
        CONTROLS.lock().remove(&saved.task.id);
    }

    #[test]
    fn completion_races_cancel_at_a_single_durable_boundary() {
        for _ in 0..12 {
            let root=tempfile::tempdir().unwrap();
            let (_,saved,_)=fixture("https://example.invalid",MediaKind::Image);
            save(root.path(),&saved).unwrap();
            register_control(&saved.task.id);
            let barrier=Arc::new(std::sync::Barrier::new(2));
            let path=root.path().to_owned();
            let id=saved.task.id.clone();
            let gate=barrier.clone();
            let cancellation=std::thread::spawn(move|| {gate.wait();cancel_at(&path,&id).unwrap().2.outcome});
            barrier.wait();
            let mut completed=saved.clone();
            completed.task.status=MediaStatus::Succeeded;
            completed.task.outputs=vec![MediaOutput {path:"result.png".into(),mime:"image/png".into()}];
            save(root.path(),&completed).unwrap();
            let outcome=cancellation.join().unwrap();
            let final_task=read(root.path(),&saved.task.id).unwrap().task;
            match outcome {
                CancelOutcome::Confirmed=>assert_eq!(final_task.status,MediaStatus::Cancelled),
                CancelOutcome::TooLate=>{assert_eq!(final_task.status,MediaStatus::Succeeded);assert_eq!(final_task.outputs[0].path,"result.png");},
                other=>panic!("unexpected {other:?}"),
            }
            CONTROLS.lock().remove(&saved.task.id);
        }
    }
    fn fixture(
        base: &str,
        kind: MediaKind,
    ) -> (crate::settings::Settings, StoredTask, MediaRequest) {
        let model = if kind == MediaKind::Video {
            "grok-imagine-video"
        } else {
            "gpt-image-1"
        };
        let mut settings = crate::settings::Settings::default();
        settings.providers.push(serde_json::from_value(json!({"id":"p","name":"test","baseUrl":base,"enabled":true,"enabledModels":[model],"apiKeys":["test-key"],"request":{"useSystemProxy":false},"modelOverrides":{model:{"capabilities":{"imageGeneration":kind==MediaKind::Image,"videoGeneration":kind==MediaKind::Video}}}})).unwrap());
        let task = StoredTask {
            task: MediaTask {
                id: uuid::Uuid::new_v4().to_string(),
                provider_id: "p".into(),
                model: model.into(),
                kind: kind.clone(),
                status: MediaStatus::Running,
                created_at: "now".into(),
                remote_id: None,
                error: None,
                outputs: vec![],
                can_resume: false,
                submission_state: None,
                origin: None,
                prompt: String::new(),
                result: None,
                request_hash: None,
                cancellation: None,
            },
            base_url: base.into(),
            protocol: if kind == MediaKind::Video {
                "xai_video"
            } else {
                "image"
            }
            .into(),
            download_url: None,
            download_requires_auth: false,
            accepted_at: None,
        };
        let request = MediaRequest {
            provider_id: "p".into(),
            model: model.into(),
            kind,
            prompt: "a product on a desk".into(),
            images: vec![],
            options: BTreeMap::new(),
            origin: None,
            description_revision: None,
        };
        (settings, task, request)
    }
    fn server(
        responses: impl FnOnce(&str) -> Vec<(u16, Vec<u8>)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let responses = responses(&base);
        let handle = std::thread::spawn(move || {
            let mut requests = vec![];
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut buf = [0; 8192];
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(split) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let h = String::from_utf8_lossy(&bytes[..split]).to_lowercase();
                        let len = h
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= split + 4 + len {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8_lossy(&bytes).into_owned());
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
            requests
        });
        (base, handle)
    }
    #[test]
    fn filter_matches_by_origin_independently_of_model_and_round_trips_untagged_records() {
        let (_, mut saved, _) = fixture("http://127.0.0.1:1", MediaKind::Image);
        saved.task.origin = Some("workbench/main".into());
        saved.task.prompt = "白底主图".into();
        let by_origin = MediaTaskFilter {
            origin: Some("workbench/main".into()),
            ..Default::default()
        };
        assert!(by_origin.matches(&saved.task));
        let other_model = MediaTaskFilter {
            model: Some("other".into()),
            ..Default::default()
        };
        assert!(!other_model.matches(&saved.task));
        assert!(MediaTaskFilter::default().matches(&saved.task));
        // Records written before `origin`/`prompt` existed still load and never match an origin filter.
        let mut legacy = serde_json::to_value(&saved).unwrap();
        legacy.as_object_mut().unwrap().remove("origin");
        legacy.as_object_mut().unwrap().remove("prompt");
        let legacy: StoredTask = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.task.origin, None);
        assert_eq!(legacy.task.prompt, "");
        assert!(!by_origin.matches(&legacy.task));
    }
    #[tokio::test]
    async fn grok_generation_modes_reach_http_with_their_distinct_image_fields() {
        // Capture the full submission path, including local-image conversion and reqwest JSON,
        // rather than assuming that a correct preview is what the provider receives.
        for mode in ["text", "first-frame", "reference"] {
            let (base, server) = server(|_| vec![(200, br#"{"request_id":"captured"}"#.to_vec())]);
            let root = tempfile::tempdir().unwrap();
            let image_path = root.path().join("reference.png");
            let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=").unwrap();
            std::fs::write(&image_path, &png).unwrap();
            let (mut settings, mut task, mut request) = fixture(&base, MediaKind::Video);
            let model = "grok-imagine-video-1.5";
            settings.providers[0].enabled_models = vec![model.into()];
            task.task.model = model.into();
            request.model = model.into();
            request.prompt = "A subtle slow camera move. Keep the scene consistent.".into();
            request.options =
                serde_json::from_value(json!({"duration":1,"resolution":"480p","ratio":"16:9"}))
                    .unwrap();
            match mode {
                "first-frame" => {
                    request
                        .options
                        .insert("firstFrame".into(), json!(image_path));
                }
                "reference" => {
                    request
                        .options
                        .insert("referenceImages".into(), json!([image_path]));
                }
                _ => {}
            }
            let state = AppState::new_headless(settings, root.path().join("usage"));
            save(root.path(), &task).unwrap();
            submit_cloud_at(root.path(), &state, &task.task.id, request)
                .await
                .unwrap();
            let requests = server.join().unwrap();
            assert_eq!(requests.len(), 1, "one paid submission per mode");
            assert!(requests[0].starts_with("POST /videos/generations HTTP/1.1"));
            let (headers, body) = requests[0].split_once("\r\n\r\n").unwrap();
            assert!(headers
                .to_ascii_lowercase()
                .contains("content-type: application/json"));
            let body: Value = serde_json::from_str(body).unwrap();
            let mut expected = json!({"model":model,"prompt":"A subtle slow camera move. Keep the scene consistent.","duration":1,"resolution":"480p","aspect_ratio":"16:9"});
            let data = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&png)
            );
            match mode {
                "first-frame" => expected["image"] = json!({"url":data}),
                "reference" => expected["reference_images"] = json!([{"url":data}]),
                _ => {}
            }
            assert_eq!(body, expected, "wire JSON differs for {mode}");
            let diagnostic: Value = serde_json::from_slice(
                &std::fs::read(
                    directory(root.path(), &task.task.id)
                        .unwrap()
                        .join("submission-request.json"),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(diagnostic["body"], body);
            assert_eq!(diagnostic["url"], format!("{base}/videos/generations"));
            assert!(!diagnostic.to_string().contains("test-key"));
            assert_eq!(
                read(root.path(), &task.task.id)
                    .unwrap()
                    .task
                    .remote_id
                    .as_deref(),
                Some("captured")
            );
        }
    }
    #[tokio::test]
    async fn cloud_receipt_survives_download_failure_and_reopening_without_another_post() {
        let (base, server) = server(|base| {
            vec![
                (200, br#"{"request_id":"r1"}"#.to_vec()),
                (
                    200,
                    serde_json::to_vec(
                        &json!({"status":"done","video":{"url":format!("{base}/output.mp4")}}),
                    )
                    .unwrap(),
                ),
                (503, b"unavailable".to_vec()),
                (200, b"\x00\x00\x00\x18ftypisom-saved-video".to_vec()),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task, request) = fixture(&base, MediaKind::Video);
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap();
        assert_eq!(
            read(root.path(), &task.task.id)
                .unwrap()
                .task
                .remote_id
                .as_deref(),
            Some("r1")
        );
        assert!(refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap_err()
            .contains("503"));
        let saved = read(root.path(), &task.task.id).unwrap();
        assert!(saved.download_url.is_some());
        let complete = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(complete.status, MediaStatus::Succeeded);
        assert_eq!(
            std::fs::read(&complete.outputs[0].path).unwrap(),
            b"\x00\x00\x00\x18ftypisom-saved-video"
        );
        let requests = server.join().unwrap();
        assert_eq!(
            requests.iter().filter(|s| s.starts_with("POST ")).count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|s| s.starts_with("GET /videos/"))
                .count(),
            1
        );
        assert!(requests[0].contains("test-key"));
        assert!(!requests[2].contains("test-key"));
    }
    #[tokio::test]
    async fn relative_video_result_downloads_from_original_provider_with_auth() {
        let (base, server) = server(|_| {
            vec![
                (200, br#"{"request_id":"r1"}"#.to_vec()),
                (
                    200,
                    br#"{"status":"done","video":{"url":"/videos/r1/content"}}"#.to_vec(),
                ),
                (200, b"\x00\x00\x00\x18ftypisom-saved-video".to_vec()),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task, request) = fixture(&base, MediaKind::Video);
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap();
        let complete = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(complete.status, MediaStatus::Succeeded);
        assert!(Path::new(&complete.outputs[0].path).is_file());
        let requests = server.join().unwrap();
        assert_eq!(
            requests.iter().filter(|s| s.starts_with("POST ")).count(),
            1
        );
        assert!(requests[2].starts_with("GET /videos/r1/content "));
        assert!(requests[2]
            .to_ascii_lowercase()
            .contains("authorization: bearer test-key"));
    }
    #[tokio::test]
    async fn relative_video_result_uses_each_protocols_auth_header() {
        for (protocol, header) in [
            ("vidu", "authorization: token test-key"),
            ("veo", "x-goog-api-key: test-key"),
        ] {
            let (base, server) =
                server(|_| vec![(200, b"\x00\x00\x00\x18ftypisom-saved-video".to_vec())]);
            let root = tempfile::tempdir().unwrap();
            let (settings, _, _) = fixture(&base, MediaKind::Video);
            let path = root.path().join("output.mp4");
            download(
                &settings.providers[0],
                &base,
                "/video.mp4",
                protocol,
                false,
                &path,
            )
            .await
            .unwrap();
            let requests = server.join().unwrap();
            assert!(requests[0].starts_with("GET /video.mp4 "));
            assert!(requests[0].to_ascii_lowercase().contains(header));
        }
    }
    #[tokio::test]
    async fn chat_image_count_creates_distinct_requests_and_preserves_partial_success() {
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=";
        for partial in [false, true] {
            let reply = serde_json::to_vec(&json!({"choices":[{"message":{"images":[{"image_url":{"url":format!("data:image/png;base64,{png}")}}]}}]})).unwrap();
            let (base, server) = server(|_| {
                vec![
                    (200, reply.clone()),
                    if partial {
                        (400, br#"{"error":{"message":"invalid request"}}"#.to_vec())
                    } else {
                        (200, reply)
                    },
                ]
            });
            let root = tempfile::tempdir().unwrap();
            let (settings, _, _) = fixture(&base, MediaKind::Image);
            let state = AppState::new_headless(settings, root.path().join("usage"));
            let result = image_providers::generate_image_with_provider(
                &state,
                &provider(&state, "p").unwrap(),
                "gemini-3-pro-image-preview",
                &json!({"prompt":"a paper boat", "n":2}),
                &[],
                1,
                "chat count regression",
            )
            .await
            .unwrap();
            assert_eq!(result.images.len(), if partial { 1 } else { 2 });
            let requests = server.join().unwrap();
            assert_eq!(requests.len(), 2);
            assert!(requests
                .iter()
                .all(|r| r.starts_with("POST /chat/completions ")));
        }
    }

    const STATIC_FILE_404: &[u8] =
        br#"{"error":{"message":"Failed to read static file.","type":"not_found_error"}}"#;
    fn accepted(task: &mut StoredTask, minutes_ago: i64) {
        task.task.remote_id = Some("accepted-receipt".into());
        task.accepted_at =
            Some((chrono::Utc::now() - chrono::TimeDelta::minutes(minutes_ago)).to_rfc3339());
    }

    #[tokio::test]
    async fn an_unseen_receipt_keeps_waiting_within_the_grace_period() {
        let (base, server) = server(|_| vec![(404, STATIC_FILE_404.to_vec())]);
        let root = tempfile::tempdir().unwrap();
        let (settings, mut task, _) = fixture(&base, MediaKind::Video);
        accepted(&mut task, 1);
        save(root.path(), &task).unwrap();
        let state = AppState::new_headless(settings, root.path().join("usage"));
        let pending = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(pending.status, MediaStatus::Running);
        assert!(pending
            .error
            .unwrap()
            .starts_with(providers::RECEIPT_UNSEEN));
        assert_eq!(pending.remote_id.as_deref(), Some("accepted-receipt"));
        assert!(server.join().unwrap()[0].starts_with("GET /videos/accepted-receipt "));
    }

    #[tokio::test]
    async fn a_lost_receipt_stops_after_the_grace_period_and_resumes_only_when_asked() {
        let png = b"\x00\x00\x00\x18ftypisom-recovered".to_vec();
        let (base, server) = server(|base| {
            vec![
                (404, STATIC_FILE_404.to_vec()),
                (
                    200,
                    serde_json::to_vec(
                        &json!({"status":"done","video":{"url":format!("{base}/output.mp4")}}),
                    )
                    .unwrap(),
                ),
                (200, png.clone()),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, mut task, _) = fixture(&base, MediaKind::Video);
        accepted(&mut task, 11);
        save(root.path(), &task).unwrap();
        let state = AppState::new_headless(settings, root.path().join("usage"));
        let lost = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(lost.status, MediaStatus::Failed);
        assert!(lost.can_resume);
        assert!(lost.error.as_deref().unwrap().contains("accepted-receipt"));
        // Reopening without an explicit resume leaves it stopped: no background reads.
        let mut reopened = read(root.path(), &task.task.id).unwrap();
        recover_task(root.path(), &mut reopened, false).unwrap();
        assert_eq!(reopened.task.status, MediaStatus::Failed);
        // The user resumes after checking with the provider: the same receipt is read again.
        recover_task(root.path(), &mut reopened, true).unwrap();
        assert_eq!(reopened.task.status, MediaStatus::Running);
        let done = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(done.status, MediaStatus::Succeeded);
        let requests = server.join().unwrap();
        assert!(requests.iter().all(|r| r.starts_with("GET ")));
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET /videos/accepted-receipt "))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn provider_confirmed_video_outcomes_are_final_and_never_resumed() {
        for (body, expected) in [
            (
                json!({"status":"failed","error":{"code":"invalid_argument","message":"blocked"}}),
                "blocked",
            ),
            (json!({"status":"expired"}), "expired"),
            (
                json!({"status":"done","video":{"respect_moderation":false}}),
                "内容审核",
            ),
            (json!({"status":"mystery"}), "mystery"),
        ] {
            let (base, server) = server(|_| vec![(200, serde_json::to_vec(&body).unwrap())]);
            let root = tempfile::tempdir().unwrap();
            let (settings, mut task, _) = fixture(&base, MediaKind::Video);
            accepted(&mut task, 1);
            save(root.path(), &task).unwrap();
            let state = AppState::new_headless(settings, root.path().join("usage"));
            let settled = refresh_cloud_at(&state, root.path(), &task.task.id)
                .await
                .unwrap();
            assert_eq!(settled.status, MediaStatus::Failed, "{body}");
            assert!(!settled.can_resume, "{body}");
            assert!(
                settled.error.as_deref().unwrap().contains(expected),
                "{body}: {:?}",
                settled.error
            );
            let mut reopened = read(root.path(), &task.task.id).unwrap();
            recover_task(root.path(), &mut reopened, true).unwrap();
            assert_eq!(reopened.task.status, MediaStatus::Failed);
            // Already final: no further read reaches the provider.
            assert_eq!(
                refresh_cloud_at(&state, root.path(), &task.task.id)
                    .await
                    .unwrap()
                    .status,
                MediaStatus::Failed
            );
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn async_image_receipt_failure_is_final_and_lost_receipt_is_bounded() {
        for (status, body, final_status, resumable) in [
            (
                200,
                br#"{"status":"failed","error":{"message":"policy"}}"#.to_vec(),
                MediaStatus::Failed,
                false,
            ),
            (404, STATIC_FILE_404.to_vec(), MediaStatus::Failed, true),
        ] {
            let (base, server) = server(|_| vec![(status, body)]);
            let root = tempfile::tempdir().unwrap();
            let (settings, mut task, _) = fixture(&base, MediaKind::Image);
            task.protocol = "image-async".into();
            accepted(&mut task, 11);
            save(root.path(), &task).unwrap();
            let state = AppState::new_headless(settings, root.path().join("usage"));
            let settled = refresh_cloud_at(&state, root.path(), &task.task.id)
                .await
                .unwrap();
            assert_eq!(settled.status, final_status);
            assert_eq!(settled.can_resume, resumable);
            assert_eq!(settled.remote_id.as_deref(), Some("accepted-receipt"));
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn image_submission_failures_are_classified_before_reopening() {
        for (status, expected) in [
            (400, MediaSubmissionState::Rejected),
            (401, MediaSubmissionState::Rejected),
            (429, MediaSubmissionState::Uncertain),
            (502, MediaSubmissionState::Uncertain),
        ] {
            let (base, server) =
                server(|_| vec![(status, br#"{"error":{"message":"no"}}"#.to_vec())]);
            let root = tempfile::tempdir().unwrap();
            let (settings, task, request) = fixture(&base, MediaKind::Image);
            let state = AppState::new_headless(settings, root.path().join("usage"));
            save(root.path(), &task).unwrap();
            submit_cloud_at(root.path(), &state, &task.task.id, request)
                .await
                .unwrap_err();
            let mut saved = read(root.path(), &task.task.id).unwrap();
            assert_eq!(saved.task.status, MediaStatus::Failed, "HTTP {status}");
            assert_eq!(saved.task.submission_state, Some(expected), "HTTP {status}");
            recover_task(root.path(), &mut saved, true).unwrap();
            assert_eq!(saved.task.status, MediaStatus::Failed);
            assert!(!saved.task.can_resume);
            assert!(server.join().unwrap().len() >= 1);
        }
    }

    #[tokio::test]
    async fn async_image_gateway_reads_the_camel_case_ratio() {
        let (base, server) = server(|_| vec![(200, br#"{"task_id":"receipt"}"#.to_vec())]);
        let root = tempfile::tempdir().unwrap();
        let (settings, task, mut request) = fixture(&base, MediaKind::Image);
        request.model = "gpt-image-2".into();
        request.options.insert("aspectRatio".into(), json!("16:9"));
        request.options.insert("size".into(), json!("1K"));
        let state = AppState::new_headless(settings, root.path().join("usage"));
        image_gateway::submit(
            &state,
            &provider(&state, "p").unwrap(),
            &task.task.id,
            &request,
        )
        .await
        .unwrap();
        let captured = server.join().unwrap();
        let body: Value =
            serde_json::from_str(captured[0].split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["size"], "1536x864", "{body}");
    }

    #[tokio::test]
    async fn luma_missing_receipt_keeps_waiting_without_losing_or_resubmitting_it() {
        let (base, server) = server(|_| vec![(404, br#"{"error":"not found"}"#.to_vec())]);
        let root = tempfile::tempdir().unwrap();
        let (mut settings, mut task, _) = fixture(&base, MediaKind::Video);
        settings.providers[0].enabled_models.push("ray-3.2".into());
        task.task.model = "ray-3.2".into();
        task.protocol = "luma".into();
        task.task.remote_id = Some("original-receipt".into());
        save(root.path(), &task).unwrap();
        let state = AppState::new_headless(settings, root.path().join("usage"));
        let pending = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert!(pending.error.as_deref().unwrap().contains("404"));
        let saved = read(root.path(), &task.task.id).unwrap();
        assert_eq!(saved.task.remote_id, task.task.remote_id);
        assert_eq!(saved.task.status, MediaStatus::Running);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET "));
    }

    #[tokio::test]
    async fn async_image_submission_preserves_explicit_quality_and_reference_bytes() {
        for quality in [None, Some("low"), Some("max")] {
            for reference in [false, true] {
                let (base, server) = server(|_| vec![(200, br#"{"task_id":"receipt"}"#.to_vec())]);
                let root = tempfile::tempdir().unwrap();
                let (settings, task, mut request) = fixture(&base, MediaKind::Image);
                request.model = "gpt-image-2.5-flare".into();
                request.options.insert("size".into(), json!("2K"));
                if let Some(quality) = quality {
                    request.options.insert("quality".into(), json!(quality));
                }
                if reference {
                    request.images.push("data:image/png;base64,aGVsbG8=".into());
                }
                let state = AppState::new_headless(settings, root.path().join("usage"));
                image_gateway::submit(
                    &state,
                    &provider(&state, "p").unwrap(),
                    &task.task.id,
                    &request,
                )
                .await
                .unwrap();
                let captured = server.join().unwrap();
                assert_eq!(captured.len(), 1);
                let body = captured[0].split_once("\r\n\r\n").unwrap().1;
                if reference {
                    assert!(captured[0].starts_with("POST /v1/images/edits/async "));
                    assert!(body.contains("name=\"image[]\""));
                    assert!(body.contains("hello"));
                    assert!(body.contains("2048x2048"));
                    assert_eq!(body.contains("name=\"quality\""), quality.is_some());
                    if let Some(quality) = quality {
                        assert!(body.contains(&format!("\r\n\r\n{quality}\r\n")));
                    }
                } else {
                    assert!(captured[0].starts_with("POST /v1/images/generations/async "));
                    let body: Value = serde_json::from_str(body).unwrap();
                    assert_eq!(body["size"], "2048x2048");
                    assert_eq!(body.get("quality").and_then(Value::as_str), quality);
                }
            }
        }
    }

    #[tokio::test]
    async fn async_image_gateway_recovers_receipt_without_resubmission() {
        let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=";
        let (base, server) = server(|_| {
            vec![
                (200, br#"{"task_id":"receipt"}"#.to_vec()),
                (503, b"temporary failure".to_vec()),
                (
                    200,
                    serde_json::to_vec(&json!({"status":"succeeded","data":[{"b64_json":png}]}))
                        .unwrap(),
                ),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, mut task, request) = fixture(&base, MediaKind::Image);
        let state = AppState::new_headless(settings, root.path().join("usage"));
        let p = provider(&state, "p").unwrap();
        let response = image_gateway::submit(&state, &p, &task.task.id, &request)
            .await
            .unwrap();
        task.protocol = "image-async".into();
        task.task.remote_id = image_gateway::remote_task_id(&response);
        save(root.path(), &task).unwrap();
        let pending = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(pending.status, MediaStatus::Running);
        assert_eq!(pending.remote_id.as_deref(), Some("receipt"));
        let complete = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(complete.status, MediaStatus::Succeeded);
        assert!(Path::new(&complete.outputs[0].path).is_file());
        let requests = server.join().unwrap();
        assert_eq!(
            requests.iter().filter(|s| s.starts_with("POST ")).count(),
            1
        );
        assert!(requests[0].starts_with("POST /v1/images/generations/async "));
        assert!(requests[1..]
            .iter()
            .all(|s| s.starts_with("GET /v1/images/tasks/receipt ")));
    }
    #[tokio::test]
    async fn cloud_image_uses_existing_transport_and_persists_an_output() {
        let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=";
        let (base, server) = server(|_| {
            vec![(
                200,
                serde_json::to_vec(&json!({"data":[{"b64_json":png}]})).unwrap(),
            )]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task, request) = fixture(&base, MediaKind::Image);
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap();
        let done = read(root.path(), &task.task.id).unwrap().task;
        assert_eq!(done.status, MediaStatus::Succeeded);
        assert_eq!(done.outputs.len(), 1);
        assert!(std::fs::read(&done.outputs[0].path)
            .unwrap()
            .starts_with(b"\x89PNG"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn partial_image_outputs_survive_a_later_invalid_image() {
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=";
        let (base, server) = server(|_| {
            vec![(
                200,
                serde_json::to_vec(
                    &json!({"data":[{"b64_json":png},{"b64_json":"PGh0bWw+ZXJyb3I8L2h0bWw+"}]}),
                )
                .unwrap(),
            )]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task, mut request) = fixture(&base, MediaKind::Image);
        request.options.insert("n".into(), json!(2));
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        assert!(submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap_err()
            .contains("不是支持的媒体"));
        let saved = read(root.path(), &task.task.id).unwrap();
        assert_eq!(saved.task.outputs.len(), 1);
        assert!(Path::new(&saved.task.outputs[0].path).exists());
        assert!(!directory(root.path(), &task.task.id)
            .unwrap()
            .join("output-1.part")
            .exists());
        assert_eq!(server.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn submission_failure_classification_survives_reopening_without_reposting() {
        for (status, body, expected) in [
            (400, b"{}".to_vec(), MediaSubmissionState::Rejected),
            (401, b"{}".to_vec(), MediaSubmissionState::Rejected),
            (408, b"{}".to_vec(), MediaSubmissionState::Uncertain),
            (502, b"{}".to_vec(), MediaSubmissionState::Uncertain),
            (
                200,
                b"invalid json".to_vec(),
                MediaSubmissionState::Uncertain,
            ),
            (200, b"{}".to_vec(), MediaSubmissionState::Uncertain),
        ] {
            let (base, server) = server(|_| vec![(status, body)]);
            let root = tempfile::tempdir().unwrap();
            let (settings, task, request) = fixture(&base, MediaKind::Video);
            let state = AppState::new_headless(settings, root.path().join("usage"));
            save(root.path(), &task).unwrap();
            assert!(submit_cloud_at(root.path(), &state, &task.task.id, request)
                .await
                .is_err());
            let mut saved = read(root.path(), &task.task.id).unwrap();
            assert_eq!(saved.task.submission_state, Some(expected), "HTTP {status}");
            assert_eq!(saved.task.status, MediaStatus::Failed);
            recover_task(root.path(), &mut saved, true).unwrap();
            assert_eq!(saved.task.status, MediaStatus::Failed);
            assert!(!saved.task.can_resume);
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn uncertain_paid_submission_is_not_replayed() {
        let (base, server) = server(|_| vec![(502, b"{}".to_vec())]);
        let root = tempfile::tempdir().unwrap();
        let (settings, task, request) = fixture(&base, MediaKind::Video);
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        assert!(submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .is_err());
        assert!(read(root.path(), &task.task.id)
            .unwrap()
            .task
            .remote_id
            .is_none());
        assert!(refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap()
            .outputs
            .is_empty());
        assert_eq!(server.join().unwrap().len(), 1);
    }
    #[test]
    fn all_video_callers_accept_local_reference_images() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("reference.png");
        std::fs::write(&path, base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=").unwrap()).unwrap();
        let (_, _, mut request) = fixture("http://127.0.0.1:1", MediaKind::Video);
        request.options.insert("firstFrame".into(), json!(path));
        request.options.insert(
            "referenceImages".into(),
            json!([path, "https://example.test/ref.png"]),
        );
        let input = video_input(&request).unwrap();
        assert!(input
            .first_frame
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(input.reference_images[0].starts_with("data:image/png;base64,"));
        assert_eq!(input.reference_images[1], "https://example.test/ref.png");
        request
            .options
            .insert("firstFrame".into(), json!("/nonexistent/dsivio-test.png"));
        assert!(video_input(&request).is_err());
    }

    #[test]
    fn image_options_are_typed_and_rejected_before_submission() {
        let (settings, _, mut request) = fixture("https://example.test", MediaKind::Image);
        for invalid in [
            json!({"n": 0}),
            json!({"n": 5}),
            json!({"n": "2"}),
            json!({"typo": true}),
            json!({"size": 1024}),
        ] {
            request.options = serde_json::from_value(invalid).unwrap();
            assert!(image_arguments(&settings.providers[0], &request).is_err());
        }
        request.options = serde_json::from_value(json!({"aspectRatio":"3:2", "n":2})).unwrap();
        let args = image_arguments(&settings.providers[0], &request).unwrap();
        image_providers::validate_generation(&settings.providers[0], &request.model, &args, 0)
            .unwrap();
        request.options.insert("size".into(), json!("not-a-size"));
        assert!(image_arguments(&settings.providers[0], &request).is_err());
    }

    #[test]
    fn comfy_standard_fields_require_explicit_bindings_and_preserve_node_options() {
        let (settings, _, mut request) = fixture("http://localhost:8188", MediaKind::Image);
        let mut flow: comfyui::ComfyWorkflow = serde_json::from_value(json!({
            "id":"wf", "name":"image", "kind":"image",
            "graph":{"1":{"class_type":"CLIPTextEncode","inputs":{"text":"original"}},"2":{"class_type":"SaveImage","inputs":{"images":["1",0]}}},
            "inputs":[{"nodeId":"1","input":"text","label":"Prompt","kind":"text"}],"outputNodes":["2"]
        })).unwrap();
        assert!(comfy_values(&settings.providers[0], &flow, &request)
            .unwrap_err()
            .contains("尚未绑定"));
        flow.inputs[0].source = Some(comfyui::ComfyInputSource::Prompt);
        let values = comfy_values(&settings.providers[0], &flow, &request).unwrap();
        assert_eq!(
            comfyui::prepare(&flow, &values).unwrap()["1"]["inputs"]["text"],
            request.prompt
        );
        assert_eq!(flow.graph["1"]["inputs"]["text"], "original");
        request
            .options
            .insert("1:text".into(), json!("conflicting"));
        assert!(comfy_values(&settings.providers[0], &flow, &request).is_err());
        request.prompt.clear();
        assert_eq!(
            comfy_values(&settings.providers[0], &flow, &request).unwrap()["1:text"],
            "conflicting"
        );
    }

    #[test]
    fn comfy_video_options_map_once_in_the_workflow_instead_of_each_feature() {
        let (settings, _, mut request) = fixture("http://localhost:8188", MediaKind::Video);
        request.prompt.clear();
        request.options = serde_json::from_value(
            json!({"duration":5,"firstFrame":"data:image/png;base64,aW1hZ2U="}),
        )
        .unwrap();
        let flow: comfyui::ComfyWorkflow = serde_json::from_value(json!({
            "id":"wf", "name":"video", "kind":"video",
            "graph":{"1":{"class_type":"VideoNode","inputs":{"seconds":1,"image":"old.png"}}},
            "inputs":[{"nodeId":"1","input":"seconds","label":"Duration","kind":"number","source":{"type":"parameter","name":"duration"}}, {"nodeId":"1","input":"image","label":"First frame","kind":"image","source":{"type":"parameter","name":"firstFrame"}}],"outputNodes":["1"]
        })).unwrap();
        let graph = comfyui::prepare(&flow, &comfy_values(&settings.providers[0], &flow, &request).unwrap()).unwrap();
        assert_eq!(graph["1"]["inputs"]["seconds"], 5);
        assert!(graph["1"]["inputs"]["image"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        request.options.insert("unsupported".into(), json!(true));
        assert!(comfy_values(&settings.providers[0], &flow, &request).is_err());
    }

    #[tokio::test]
    async fn comfy_common_request_uploads_binds_persists_and_returns_shared_task() {
        let (base, server) = server(|_| {
            vec![
            (200, br#"{"name":"uploaded.png","subfolder":"inputs"}"#.to_vec()),
            (200, br#"{"prompt_id":"comfy-receipt"}"#.to_vec()),
            (200, br#"{"comfy-receipt":{"status":{"completed":true},"outputs":{"3":{"images":[{"filename":"result.png","type":"output"}]}}}}"#.to_vec()),
            (200, b"\x89PNG\r\n\x1a\nresult".to_vec()),
        ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, _, mut request) = fixture(&base, MediaKind::Image);
        request.origin = Some("workbench/test".into());
        request.images = vec!["data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO6sAAAAASUVORK5CYII=".into()];
        let flow: comfyui::ComfyWorkflow = serde_json::from_value(json!({
            "id":"wf", "name":"image", "kind":"image",
            "graph":{"1":{"class_type":"CLIPTextEncode","inputs":{"text":"original"}},"2":{"class_type":"LoadImage","inputs":{"image":"old.png"}},"3":{"class_type":"SaveImage","inputs":{"images":["2",0]}}},
            "inputs":[{"nodeId":"1","input":"text","label":"Prompt","kind":"text","source":{"type":"prompt"}}, {"nodeId":"2","input":"image","label":"Reference","kind":"image","source":{"type":"image","index":0}}],"outputNodes":["3"]
        })).unwrap();
        let values = comfy_values(&settings.providers[0], &flow, &request).unwrap();
        let task = comfyui::submit_to_store(
            root.path(),
            uuid::Uuid::new_v4().to_string(),
            settings.providers[0].clone(),
            flow,
            values,
            request.origin.clone(),
            request.prompt.clone(),
        )
        .await
        .unwrap();
        let done = from_comfy(comfyui::refresh_at(root.path(), &task.id).await.unwrap());
        assert_eq!(done.status, MediaStatus::Succeeded);
        assert_eq!(done.prompt, request.prompt);
        assert_eq!(done.origin, request.origin);
        assert_eq!(done.outputs[0].mime, "image/png");
        assert!(std::path::Path::new(&done.outputs[0].path).exists());
        let calls = server.join().unwrap();
        assert!(calls[0].starts_with("POST /upload/image "));
        assert!(calls[1].starts_with("POST /prompt "));
        assert!(calls[1].contains(&request.prompt));
        assert!(calls[1].contains("inputs/uploaded.png"));
        assert!(!calls[1].contains("base64"));
    }

    #[tokio::test]
    async fn changed_protocol_is_rejected_before_a_paid_submission() {
        let root = tempfile::tempdir().unwrap();
        let (settings, mut task, request) = fixture("http://127.0.0.1:1", MediaKind::Video);
        task.protocol = "minimax_h3".into();
        save(root.path(), &task).unwrap();
        let state = AppState::new_headless(settings, root.path().join("usage"));
        assert!(submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap_err()
            .contains("协议已变更"));
        assert!(read(root.path(), &task.task.id)
            .unwrap()
            .task
            .remote_id
            .is_none());
    }

    #[test]
    fn recovery_preserves_receipt_and_never_restarts_uncertain_submission() {
        let root = tempfile::tempdir().unwrap();
        let (_, mut saved, _) = fixture("http://localhost:8188", MediaKind::Video);
        save(root.path(), &saved).unwrap();
        recover_task(root.path(), &mut saved, true).unwrap();
        assert_eq!(saved.task.status, MediaStatus::Failed);
        assert!(!saved.task.can_resume);
        saved.task.remote_id = Some("original-receipt".into());
        saved.task.can_resume = true;
        saved.download_url = Some("https://example.test/expired".into());
        recover_task(root.path(), &mut saved, true).unwrap();
        let reopened = read(root.path(), &saved.task.id).unwrap();
        assert_eq!(reopened.task.status, MediaStatus::Running);
        assert_eq!(reopened.task.remote_id.as_deref(), Some("original-receipt"));
        assert_eq!(reopened.download_url, None);
    }

    #[tokio::test]
    async fn bad_download_never_becomes_an_output_and_cleans_partial_file() {
        let (base, server) = server(|_| {
            vec![
                (200, b"<html>gateway error</html>".to_vec()),
                (200, b"\x1a\x45\xdf\xa3-webm".to_vec()),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, _, _) = fixture(&base, MediaKind::Video);
        let path = root.path().join("output.mp4");
        assert!(download(
            &settings.providers[0],
            &base,
            &base,
            "xai_video",
            false,
            &path
        )
        .await
        .is_err());
        assert!(!path.exists());
        assert!(!path.with_extension("part").exists());
        assert_eq!(
            download(
                &settings.providers[0],
                &base,
                &base,
                "xai_video",
                false,
                &path
            )
            .await
            .unwrap()
            .mime,
            "video/webm"
        );
        assert!(path.with_extension("webm").exists());
        server.join().unwrap();
    }

    #[test]
    fn receipt_storage_does_not_include_credentials() {
        let (settings, task, _) = fixture("https://example.test", MediaKind::Video);
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains(&settings.providers[0].api_keys[0]));
        assert!(directory(Path::new("/tmp"), "../bad").is_err());
    }
    #[test]
    fn migration_preserves_receipts_outputs_and_original_files_idempotently() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("video-studio");
        std::fs::create_dir_all(old.join("tasks")).unwrap();
        std::fs::create_dir_all(old.join("outputs")).unwrap();
        let output = old.join("outputs/old.mp4");
        std::fs::write(&output, b"movie").unwrap();
        let (settings, task, _) = fixture("https://example.test", MediaKind::Video);
        let record = json!({"id":task.task.id,"remote":{"route":"grok","base_url":"https://example.test","id":"original-task"},"requested":{"model":"grok-imagine-video"},"output":output,"status":"succeeded"});
        let file = old.join("tasks").join(format!("{}.json", task.task.id));
        std::fs::write(&file, record.to_string()).unwrap();
        import_legacy_at(root.path(), &settings).unwrap();
        import_legacy_at(root.path(), &settings).unwrap();
        let saved = read(&root.path().join("media-tasks"), &task.task.id).unwrap();
        assert_eq!(saved.task.status, MediaStatus::Succeeded);
        assert_eq!(saved.task.remote_id.as_deref(), Some("original-task"));
        assert!(file.exists());
        assert!(output.exists());
    }
    #[tokio::test]
    async fn authenticated_download_rejects_unrelated_origin_before_sending() {
        let (settings, _, _) = fixture("https://example.test", MediaKind::Video);
        let temp = tempfile::tempdir().unwrap();
        let error = download(
            &settings.providers[0],
            "https://example.test",
            "https://unrelated.test/video",
            "veo",
            true,
            &temp.path().join("v.mp4"),
        )
        .await
        .unwrap_err();
        assert!(error.contains("已阻止发送密钥"));
    }
    fn running_task() -> MediaTask {
        fixture("http://127.0.0.1:1", MediaKind::Video).1.task
    }
    #[tokio::test]
    async fn reference_video_survives_more_than_ten_pending_404s_without_resubmission() {
        let (base, server) = server(|base| {
            let mut replies = vec![(
                200,
                br#"{"request_id":"reference-receipt","status":"pending"}"#.to_vec(),
            )];
            replies.extend((0..15).map(|_| (404, br#"{"error":"not ready yet"}"#.to_vec())));
            replies.push((200, serde_json::to_vec(&json!({"request_id":"reference-receipt","status":"done","video":{"url":format!("{base}/output.mp4")}})).unwrap()));
            replies.push((200, b"\x00\x00\x00\x18ftypisom-reference-video".to_vec()));
            replies
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task, mut request) = fixture(&base, MediaKind::Video);
        request.options.insert(
            "referenceImages".into(),
            json!(["data:image/png;base64,aW1hZ2U="]),
        );
        let state = AppState::new_headless(settings, root.path().join("usage"));
        save(root.path(), &task).unwrap();
        submit_cloud_at(root.path(), &state, &task.task.id, request)
            .await
            .unwrap();
        for _ in 0..15 {
            let pending = refresh_cloud_at(&state, root.path(), &task.task.id).await;
            assert!(
                pending.is_ok(),
                "404 while awaiting an accepted task must remain pending: {pending:?}"
            );
            assert_eq!(pending.unwrap().status, MediaStatus::Running);
        }
        let done = refresh_cloud_at(&state, root.path(), &task.task.id)
            .await
            .unwrap();
        assert_eq!(done.status, MediaStatus::Succeeded);
        assert_eq!(done.remote_id.as_deref(), Some("reference-receipt"));
        assert!(done.error.is_none());
        assert_eq!(done.outputs.len(), 1);
        let requests = server.join().unwrap();
        assert_eq!(
            requests.iter().filter(|r| r.starts_with("POST ")).count(),
            1
        );
        assert!(requests[0].contains("reference_images"));
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET /videos/reference-receipt "))
                .count(),
            16
        );
    }

    #[test]
    fn interrupted_query_keeps_the_receipt_pending_but_confirmed_failure_stays_failed() {
        let root = tempfile::tempdir().unwrap();
        let (_, mut saved, _) = fixture("http://localhost:8188", MediaKind::Video);
        saved.task.remote_id = Some("paid-receipt".into());
        record_background_error(&mut saved, "查询等待超时".into());
        assert_eq!(saved.task.status, MediaStatus::Running);
        assert!(saved.task.can_resume);
        assert_eq!(saved.task.remote_id.as_deref(), Some("paid-receipt"));
        save(root.path(), &saved).unwrap();
        recover_task(root.path(), &mut saved, false).unwrap();
        assert_eq!(saved.task.status, MediaStatus::Running);
        assert!(!saved.task.can_resume);
        apply_result(
            &mut saved,
            providers::VideoResult {
                remote_id: "paid-receipt".into(),
                status: "failed".into(),
                download_url: None,
                file_id: None,
                error: Some("provider confirmed failure".into()),
                download_requires_auth: false,
                submission_response: None,
                submission_request: None,
            },
        );
        record_background_error(&mut saved, "another error".into());
        assert_eq!(saved.task.status, MediaStatus::Failed);
        assert!(!saved.task.can_resume);
        assert_eq!(
            saved.task.error.as_deref(),
            Some("provider confirmed failure")
        );
    }
    #[tokio::test]
    async fn concurrent_reference_tasks_keep_independent_receipts_through_pending_404s() {
        let (base, server) = server(|base| {
            let mut replies = vec![
                (
                    200,
                    br#"{"request_id":"receipt-a","status":"pending"}"#.to_vec(),
                ),
                (
                    200,
                    br#"{"request_id":"receipt-b","status":"pending"}"#.to_vec(),
                ),
            ];
            replies.extend((0..24).map(|_| (404, br#"{"error":"not ready yet"}"#.to_vec())));
            for _ in 0..2 {
                replies.push((
                    200,
                    serde_json::to_vec(
                        &json!({"status":"done","video":{"url":format!("{base}/output.mp4")}}),
                    )
                    .unwrap(),
                ));
                replies.push((200, b"\x00\x00\x00\x18ftypisom-concurrent-video".to_vec()));
            }
            replies
        });
        let root = tempfile::tempdir().unwrap();
        let (settings, task_a, mut request_a) = fixture(&base, MediaKind::Video);
        let (_, task_b, mut request_b) = fixture(&base, MediaKind::Video);
        assert_ne!(task_a.task.id, task_b.task.id);
        for request in [&mut request_a, &mut request_b] {
            request.options.insert(
                "referenceImages".into(),
                json!(["data:image/png;base64,aW1hZ2U="]),
            );
        }
        let state = AppState::new_headless(settings, root.path().join("usage"));
        for task in [&task_a, &task_b] {
            save(root.path(), task).unwrap();
        }
        let (a, b) = tokio::join!(
            submit_cloud_at(root.path(), &state, &task_a.task.id, request_a),
            submit_cloud_at(root.path(), &state, &task_b.task.id, request_b),
        );
        a.unwrap();
        b.unwrap();
        let receipt_a = read(root.path(), &task_a.task.id).unwrap().task.remote_id;
        let receipt_b = read(root.path(), &task_b.task.id).unwrap().task.remote_id;
        assert_ne!(receipt_a, receipt_b);
        for _ in 0..12 {
            let (a, b) = tokio::join!(
                refresh_cloud_at(&state, root.path(), &task_a.task.id),
                refresh_cloud_at(&state, root.path(), &task_b.task.id),
            );
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(a.status, MediaStatus::Running);
            assert_eq!(b.status, MediaStatus::Running);
            assert_eq!(a.remote_id, receipt_a);
            assert_eq!(b.remote_id, receipt_b);
        }
        for task in [&task_a, &task_b] {
            let done = refresh_cloud_at(&state, root.path(), &task.task.id)
                .await
                .unwrap();
            assert_eq!(done.status, MediaStatus::Succeeded);
            assert_eq!(done.outputs.len(), 1);
            assert!(done.outputs[0].path.contains(&task.task.id));
        }
        let requests = server.join().unwrap();
        assert_eq!(
            requests.iter().filter(|r| r.starts_with("POST ")).count(),
            2
        );
        assert!(requests[..2].iter().all(|r| r.contains("reference_images")));
    }
    #[tokio::test]
    async fn a_provider_404_right_after_acceptance_does_not_fail_the_task() {
        let mut reads = 0;
        let result = poll_until_settled(
            || {
                reads += 1;
                let step = reads;
                async move {
                    match step {
                        1 | 2 => Err("视频接口 HTTP 404；请检查密钥、权限与请求参数".to_string()),
                        3 => Ok(running_task()),
                        _ => Ok(MediaTask {
                            status: MediaStatus::Succeeded,
                            ..running_task()
                        }),
                    }
                }
            },
            Duration::ZERO,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(reads, 4);
    }
    #[tokio::test]
    async fn a_read_that_keeps_failing_is_given_up_after_a_bounded_number_of_tries() {
        let mut reads = 0;
        let result = poll_until_settled(
            || {
                reads += 1;
                async {
                    Err::<MediaTask, _>("视频接口 HTTP 401；请检查密钥、权限与请求参数".to_string())
                }
            },
            Duration::ZERO,
        )
        .await;
        assert!(result.unwrap_err().contains("401"));
        assert_eq!(reads, MAX_CONSECUTIVE_READ_ERRORS);
    }
    #[tokio::test]
    async fn pending_receipt_has_a_bounded_wait_and_can_be_queried_again() {
        let mut reads = 0;
        let result = poll_until_settled(
            || {
                reads += 1;
                async { Ok(running_task()) }
            },
            Duration::ZERO,
        )
        .await;
        assert!(result.unwrap_err().contains("勿重新提交"));
        assert_eq!(reads, MAX_POLL_READS);
    }
    #[tokio::test]
    async fn successful_reads_reset_the_error_budget() {
        let mut reads = 0u32;
        let result = poll_until_settled(
            || {
                reads += 1;
                let step = reads;
                async move {
                    // Alternating error/running never reaches the consecutive limit; finish after 40 reads.
                    match step {
                        40 => Ok(MediaTask {
                            status: MediaStatus::Succeeded,
                            ..running_task()
                        }),
                        n if n % 2 == 1 => Err("HTTP 502".to_string()),
                        _ => Ok(running_task()),
                    }
                }
            },
            Duration::ZERO,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(reads, 40);
    }
    #[test]
    fn one_worker_owns_each_task_and_release_allows_recovery() {
        let id = uuid::Uuid::new_v4().to_string();
        let first = claim(&id).unwrap();
        assert!(claim(&id).is_none());
        drop(first);
        assert!(claim(&id).is_some());
    }
}

/// Attach a missing receipt to an uncertain task. This only permits future GETs, never resubmits.
pub(crate) fn attach_receipt(_app: &AppHandle, id: &str, remote: &str) -> Result<(), String> {
    let remote = remote.trim();
    if remote.is_empty()
        || remote.len() > 160
        || !remote
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("请输入有效的远程任务编号".into());
    }
    let _guard = claim(id).ok_or("任务仍在运行，请稍后查询")?;
    let base = root()?;
    if directory(&base, id)?.join("task.json").exists() {
        let mut saved = read(&base, id)?;
        if saved.task.remote_id.is_some() || !saved.task.outputs.is_empty() {
            return Err("任务已有回执或成片，不能覆盖".into());
        }
        saved.task.remote_id = Some(remote.into());
        saved.task.can_resume = true;
        saved.task.status = MediaStatus::Failed;
        save(&base, &saved)
    } else {
        let base = comfyui::root()?;
        let mut saved = comfyui::read_task(&base, id)?;
        if saved.prompt_id.is_some() || !saved.outputs.is_empty() {
            return Err("任务已有回执或成片，不能覆盖".into());
        }
        saved.prompt_id = Some(remote.into());
        saved.status = comfyui::ComfyTaskStatus::DownloadPending;
        comfyui::save(&base, &saved)
    }
}
