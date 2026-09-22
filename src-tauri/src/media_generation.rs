//! The generation lifecycle shared by chat tools and Workbench. Provider modules own wire formats.
use crate::{comfyui, settings::ModelProvider, state::AppState, video_studio::providers};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaStatus {
    Running,
    Succeeded,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MediaOutput {
    pub path: String,
    pub mime: String,
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
    /// Who asked for this run, e.g. `workbench/main` or `chat`. Workbench pages list by it.
    #[serde(default)]
    pub origin: Option<String>,
    /// The prompt as submitted; kept so a history row can say what it was for.
    #[serde(default)]
    pub prompt: String,
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
        self.provider_id.as_ref().is_none_or(|p| *p == task.provider_id)
            && self.model.as_ref().is_none_or(|m| *m == task.model)
            && self.origin.as_ref().is_none_or(|o| task.origin.as_ref() == Some(o))
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
}
static ACTIVE: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
struct Active(String);
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}
fn claim(id: &str) -> Option<Active> {
    ACTIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
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
fn from_comfy(task: comfyui::ComfyTask) -> MediaTask {
    use comfyui::ComfyTaskStatus as S;
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
        origin: task.origin,
        prompt: String::new(),
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
                    origin: None,
                    prompt: String::new(),
                },
                base_url: if base.is_empty() {
                    provider.base_url.clone()
                } else {
                    base.into()
                },
                protocol: protocol.into(),
                download_url: old["remote"]["download_url"].as_str().map(str::to_owned),
                download_requires_auth: false,
            },
        )?;
    }
    Ok(())
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
    value["prompt"] = json!(request.prompt);
    // Images are explicit references; a caller chooses firstFrame/referenceImages for video.
    if !request.images.is_empty() {
        return Err("视频素材请明确指定为首帧或参考图".into());
    }
    serde_json::from_value(value)
        .map(|input| providers::input_with_defaults(&request.model, input))
        .map_err(|e| format!("视频参数无效：{e}"))
}
pub(crate) async fn start(app: &AppHandle, request: MediaRequest) -> Result<MediaTask, String> {
    let state = app.state::<AppState>();
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
        let task = from_comfy(
            comfyui::submit_to_store(
                &comfyui::root()?,
                p,
                flow,
                request.options,
                request.origin,
            )
            .await?,
        );
        if task.status == MediaStatus::Running {
            run_background(app.clone(), task.id.clone(), None);
        }
        return Ok(task);
    }
    if request.prompt.trim().is_empty() {
        return Err("请填写提示词".into());
    }
    let protocol = if request.kind == MediaKind::Video {
        let protocol = providers::selected(&p, &request.model)?;
        providers::prepare(
            protocol,
            &p.base_url,
            &request.model,
            &video_input(&request)?,
        )?;
        protocol.to_owned()
    } else {
        if !crate::chat::model_metadata::model_can_generate_images_directly(&p, &request.model) {
            return Err("此模型未启用图片生成".into());
        }
        image_inputs(&request.images)?;
        "image".into()
    };
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
            origin: request.origin.clone(),
            prompt: request.prompt.clone(),
        },
        base_url: p.base_url,
        protocol,
        download_url: None,
        download_requires_auth: false,
    };
    save(&root()?, &task)?;
    run_background(app.clone(), task.task.id.clone(), Some(request));
    Ok(task.task)
}
#[tauri::command]
pub async fn start_media_generation(
    app: AppHandle,
    request: MediaRequest,
) -> Result<MediaTask, String> {
    let allowed = {
        let state = app.state::<AppState>();
        let settings = state.settings_read();
        let pool = if request.kind == MediaKind::Image {
            &settings.workbench_media.image_models
        } else {
            &settings.workbench_media.video_models
        };
        pool.iter()
            .any(|m| m.provider_id == request.provider_id && m.model == request.model)
    };
    if !allowed {
        return Err("请先把模型加入媒体创作模型池".into());
    }
    start(&app, request).await
}
fn image_inputs(
    images: &[String],
) -> Result<Vec<crate::chat::image_generation::InputImage>, String> {
    if images.len() > 4 {
        return Err("最多使用 4 张参考图".into());
    }
    images
        .iter()
        .map(|image| {
            if image.starts_with("data:") {
                let (mime_type, base64) =
                    crate::chat::image_generation::parse_image_data_url(image)?;
                Ok(crate::chat::image_generation::InputImage { mime_type, base64 })
            } else {
                crate::chat::image_generation::load_input_images_from_paths(&[PathBuf::from(
                    image,
                )])?
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
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let result = async {
            if let Some(request) = request {
                submit_cloud_at(&root()?, &app.state::<AppState>(), &id, request).await?;
            }
            // Reading/downloading never submits. A restart resumes from the saved receipt.
            for _ in 0..28800 {
                let task = refresh_once(&app, &id).await?;
                if task.status != MediaStatus::Running {
                    return Ok::<_, String>(());
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Err("任务仍在供应商运行，可稍后继续查询".into())
        }
        .await;
        if let Err(error) = result {
            if let Ok(root) = root() {
                if let Ok(mut saved) = read(&root, &id) {
                    saved.task.status = MediaStatus::Failed;
                    saved.task.can_resume = saved.task.remote_id.is_some();
                    saved.task.error = Some(error);
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
    });
}
async fn submit_cloud_at(
    root: &Path,
    state: &AppState,
    id: &str,
    request: MediaRequest,
) -> Result<(), String> {
    let mut saved = read(root, id)?;
    let p = provider(state, &request.provider_id)?;
    if p.base_url != saved.base_url {
        return Err("供应商连接已变更，本次未提交".into());
    }
    if request.kind == MediaKind::Image {
        let mut args = serde_json::to_value(&request.options).map_err(|e| e.to_string())?;
        args["prompt"] = json!(request.prompt);
        let result = crate::chat::image_generation::generate_image_with_provider(
            &state,
            &p,
            &request.model,
            &args,
            &image_inputs(&request.images)?,
            0,
            "Media generation",
        )
        .await?;
        if result.artifacts.is_empty() {
            return Err(result.content);
        }
        for (i, artifact) in result.artifacts.iter().enumerate() {
            let (_, data) = artifact.data_url.split_once(',').ok_or("图片响应无效")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "图片编码无效")?;
            let ext = crate::chat::image_generation::extension_for_mime(&artifact.mime_type);
            let path = directory(&root, id)?.join(format!("output-{i}.{ext}"));
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            saved.task.outputs.push(MediaOutput {
                path: path.to_string_lossy().into_owned(),
                mime: artifact.mime_type.clone(),
            });
        }
        saved.task.status = MediaStatus::Succeeded;
    } else {
        let result = providers::submit_video_model_request(
            &state,
            p.id,
            request.model.clone(),
            video_input(&request)?,
        )
        .await
        .map_err(|e| format!("{e}；提交结果未确认时请勿重复生成"))?;
        apply_result(&mut saved, result);
    }
    save(&root, &saved)
}
fn apply_result(saved: &mut StoredTask, result: providers::VideoResult) {
    if !result.remote_id.is_empty() {
        saved.task.remote_id = Some(result.remote_id);
    }
    saved.download_url = result.download_url;
    saved.download_requires_auth = result.download_requires_auth;
    if matches!(result.status.as_str(), "failed" | "expired" | "cancelled") {
        saved.task.status = MediaStatus::Failed;
        saved.task.can_resume = false;
        saved.task.error = Some(result.error.unwrap_or("供应商生成失败".into()));
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
        apply_result(&mut saved, result);
        save(&root, &saved)?;
    }
    if let Some(url) = saved.download_url.clone() {
        let p = provider(&state, &saved.task.provider_id)?;
        let path = directory(&root, id)?.join("output.mp4");
        download(
            &p,
            &saved.base_url,
            &url,
            saved.download_requires_auth,
            &path,
        )
        .await?;
        saved.task.outputs = vec![MediaOutput {
            path: path.to_string_lossy().into_owned(),
            mime: "video/mp4".into(),
        }];
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
    auth: bool,
    path: &Path,
) -> Result<(), String> {
    let mut url = reqwest::Url::parse(url).map_err(|_| "结果地址无效")?;
    let original = reqwest::Url::parse(base).map_err(|_| "原服务地址无效")?;
    if auth && original.origin() != url.origin() {
        return Err("结果下载要求鉴权，但地址与原服务不一致，已阻止发送密钥".into());
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180));
    if !provider.request.use_system_proxy {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    for _ in 0..5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("结果地址必须是 HTTP(S)".into());
        }
        let mut request = client.get(url.clone());
        if auth && original.origin() == url.origin() {
            let key = provider
                .api_keys
                .get(provider.active_key_index)
                .or_else(|| provider.api_keys.first())
                .ok_or("缺少下载密钥")?;
            request = request.header("x-goog-api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if response.status().is_redirection() {
            let target = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("下载跳转缺少地址")?;
            let next = url.join(target).map_err(|_| "下载跳转地址无效")?;
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err("下载跳转不能降低 HTTPS 安全性".into());
            }
            url = next;
            continue;
        }
        return write_download(response, path).await;
    }
    Err("下载跳转次数过多".into())
}
async fn write_download(mut response: reqwest::Response, path: &Path) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    if !response.status().is_success() {
        return Err(format!(
            "成片下载 HTTP {}，可继续查询恢复下载",
            response.status().as_u16()
        ));
    }
    let temporary = path.with_extension("part");
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|e| e.to_string())?;
    let mut size = 0;
    while let Some(chunk) = response.chunk().await.map_err(|_| "下载中断，可恢复")? {
        size += chunk.len();
        if size > 512 * 1024 * 1024 {
            return Err("结果超过 512 MB".into());
        }
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    if size == 0 {
        return Err("供应商返回空文件".into());
    }
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(temporary, path)
        .await
        .map_err(|e| e.to_string())
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
            .unwrap_or_else(|e| e.into_inner())
            .contains(&id)
            && (task.status == MediaStatus::Running || task.can_resume)
        {
            task.status = MediaStatus::Running;
            task.can_resume = false;
            task.error = None;
        }
        return Ok(task);
    }
    let mut saved = read(&root, &id)?;
    let active = ACTIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&id);
    if !active && saved.task.status == MediaStatus::Running && saved.task.remote_id.is_none() {
        saved.task.status = MediaStatus::Failed;
        saved.task.error = Some("上次生成中断，结果未确认，请核查后再决定是否重新生成".into());
        save(&root, &saved)?;
    }
    if !active && resume == Some(true) && saved.task.can_resume {
        saved.task.status = MediaStatus::Running;
        saved.task.error = None;
        saved.task.can_resume = false;
        // Expired signed URLs are refreshed by querying the SAME remote task.
        saved.download_url = None;
        save(&root, &saved)?;
    }
    if saved.task.status == MediaStatus::Running && !active {
        run_background(app, id, None);
    }
    Ok(saved.task)
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
        filter.provider_id.as_ref().is_none_or(|p| *p == task.provider_id)
            && filter.model.as_ref().is_none_or(|m| *m == task.workflow_id)
            && filter.origin.as_ref().is_none_or(|o| task.origin.as_ref() == Some(o))
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
pub(crate) async fn wait(app: &AppHandle, id: &str, seconds: u64) -> Result<MediaTask, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let task = get_media_task(app.clone(), id.into(), None)?;
        if task.status != MediaStatus::Running || tokio::time::Instant::now() >= deadline {
            return Ok(task);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub(crate) async fn tool_call(
    app: &AppHandle,
    name: &str,
    args: Value,
) -> Result<crate::mcp::types::McpToolCallResult, String> {
    if name == "mixer_media_task" {
        let id = args["id"].as_str().ok_or("需要任务编号")?;
        get_media_task(app.clone(), id.into(), args["resume"].as_bool())?;
        return Ok(tool_result(
            wait(app, id, args["waitSeconds"].as_u64().unwrap_or(0).min(30)).await?,
        ));
    }
    let selection = app
        .state::<AppState>()
        .settings_read()
        .default_models
        .video_generation
        .clone();
    if selection.provider_id.is_empty() {
        return Err("请在模型分工中配置对话视频模型".into());
    }
    let mut options: BTreeMap<String, Value> =
        serde_json::from_value(args).map_err(|e| e.to_string())?;
    let prompt = options
        .remove("prompt")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("需要提示词")?;
    // Local chat attachments use the same validated image reader as image generation.
    for key in ["firstFrame", "lastFrame"] {
        if let Some(path) = options.get(key).and_then(Value::as_str) {
            if !path.starts_with("http://")
                && !path.starts_with("https://")
                && !path.starts_with("data:")
            {
                let image = image_inputs(&[path.into()])?.remove(0);
                options.insert(
                    key.into(),
                    json!(crate::chat::image_generation::data_url_for_input(&image)),
                );
            }
        }
    }
    if let Some(images) = options
        .get("referenceImages")
        .and_then(Value::as_array)
        .cloned()
    {
        let mut converted = vec![];
        for image in images {
            let value = image.as_str().ok_or("参考图必须是图片路径或 URL")?;
            converted.push(
                if value.starts_with("http://")
                    || value.starts_with("https://")
                    || value.starts_with("data:")
                {
                    value.to_owned()
                } else {
                    crate::chat::image_generation::data_url_for_input(
                        &image_inputs(&[value.into()])?.remove(0),
                    )
                },
            );
        }
        options.insert("referenceImages".into(), json!(converted));
    }
    let task = start(
        app,
        MediaRequest {
            provider_id: selection.provider_id,
            model: selection.model,
            kind: MediaKind::Video,
            prompt,
            images: vec![],
            options,
            origin: Some("chat".into()),
        },
    )
    .await?;
    Ok(tool_result(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
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
                origin: None,
                prompt: String::new(),
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
        };
        let request = MediaRequest {
            provider_id: "p".into(),
            model: model.into(),
            kind,
            prompt: "a product on a desk".into(),
            images: vec![],
            options: BTreeMap::new(),
            origin: None,
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
                (200, b"saved-video".to_vec()),
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
            b"saved-video"
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
    fn shared_defaults_and_receipt_storage_do_not_include_credentials() {
        let (settings, task, mut request) = fixture("https://example.test", MediaKind::Video);
        request.model = "MiniMax-H3".into();
        let input = video_input(&request).unwrap();
        assert_eq!(input.duration, Some(5));
        assert_eq!(input.resolution.as_deref(), Some("768P"));
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
            true,
            &temp.path().join("v.mp4"),
        )
        .await
        .unwrap_err();
        assert!(error.contains("已阻止发送密钥"));
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
