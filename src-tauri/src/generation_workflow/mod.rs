//! Workbench DAG execution owns immutable run snapshots and node checkpoints.
//! AI and media submission remain with their existing lifecycle owners.
mod graph;
#[cfg(test)]
mod tests;
pub mod types;
use crate::{
    media_generation::{self, MediaKind, MediaRequest, MediaStatus},
    state::AppState,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager};
use types::*;

struct Control {
    id: String,
    cancelled: Arc<AtomicBool>,
}
static ACTIVE: LazyLock<Mutex<HashMap<String, Control>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn root() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无法定位工作流记录")?
        .join("workflow-runs"))
}
fn directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效运行编号")?;
    Ok(root.join(id))
}
fn save(root: &Path, run: &WorkflowRun) -> Result<(), String> {
    let dir = directory(root, &run.id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(
        &dir.join("run.json"),
        &serde_json::to_string(run).map_err(|e| e.to_string())?,
        "workflow run",
    )
}
fn read(root: &Path, id: &str) -> Result<WorkflowRun, String> {
    serde_json::from_slice(
        &std::fs::read(directory(root, id)?.join("run.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn new_run(flow: GenerationWorkflow) -> WorkflowRun {
    let nodes = flow
        .nodes
        .iter()
        .map(|n| WorkflowNodeRun {
            node_id: n.id.clone(),
            status: WorkflowStatus::Pending,
            error: None,
            started_at: None,
            finished_at: None,
            media_task_id: None,
            outputs: BTreeMap::new(),
        })
        .collect();
    WorkflowRun {
        id: uuid::Uuid::new_v4().to_string(),
        workflow: flow,
        status: WorkflowStatus::Running,
        created_at: now(),
        updated_at: now(),
        error: None,
        nodes,
    }
}
fn recover(run: &mut WorkflowRun) {
    if run.status != WorkflowStatus::Running {
        return;
    }
    run.status = WorkflowStatus::Interrupted;
    run.error = Some(
        "应用关闭导致执行中断。继续运行会复用已完成节点，并查询已有媒体任务，不会自动重新生成。"
            .into(),
    );
    for node in &mut run.nodes {
        if node.status == WorkflowStatus::Running {
            node.status = WorkflowStatus::Interrupted;
        }
    }
    run.updated_at = now();
}
fn claim(run: &WorkflowRun, root: &Path) -> Result<Arc<AtomicBool>, String> {
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    claim_locked(run, root, &mut active)
}
fn claim_locked(
    run: &WorkflowRun,
    root: &Path,
    active: &mut HashMap<String, Control>,
) -> Result<Arc<AtomicBool>, String> {
    if active.contains_key(&run.workflow.id) {
        return Err("这个工作流正在运行，请先停止或等待完成".into());
    }
    save(root, run)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    active.insert(
        run.workflow.id.clone(),
        Control {
            id: run.id.clone(),
            cancelled: cancelled.clone(),
        },
    );
    Ok(cancelled)
}
struct Claim(String);
impl Drop for Claim {
    fn drop(&mut self) {
        ACTIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

#[tauri::command]
pub fn list_workflow_runs(workflow_id: String) -> Result<Vec<WorkflowRun>, String> {
    let root = root()?;
    let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    let mut runs = Vec::new();
    if !root.exists() {
        return Ok(runs);
    }
    for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok(mut run) = read(&root, &id) {
            if run.workflow.id != workflow_id {
                continue;
            }
            if run.status == WorkflowStatus::Running && !active.values().any(|c| c.id == run.id) {
                recover(&mut run);
                save(&root, &run)?;
            }
            runs.push(run);
        }
    }
    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    runs.truncate(50);
    Ok(runs)
}
#[tauri::command]
pub fn start_workflow_run(
    app: AppHandle,
    workflow: GenerationWorkflow,
) -> Result<WorkflowRun, String> {
    graph::validate(&workflow)?;
    validate_resources(&app, &workflow)?;
    let run = new_run(workflow);
    let root = root()?;
    let cancelled = claim(&run, &root)?;
    spawn(app, run.clone(), root, cancelled);
    Ok(run)
}
#[tauri::command]
pub fn resume_workflow_run(app: AppHandle, id: String) -> Result<WorkflowRun, String> {
    let root = root()?;
    // Hold the claim lock while reading: a finishing worker cannot be resumed from a stale checkpoint.
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    let mut run = read(&root, &id)?;
    if run.status == WorkflowStatus::Succeeded {
        return Err("该次运行已经完成".into());
    }
    graph::validate(&run.workflow)?;
    // Resuming uses the original snapshot and checkpoints, never the current edited draft.
    run.status = WorkflowStatus::Running;
    run.error = None;
    run.updated_at = now();
    let cancelled = claim_locked(&run, &root, &mut active)?;
    drop(active);
    spawn(app, run.clone(), root, cancelled);
    Ok(run)
}
#[tauri::command]
pub fn cancel_workflow_run(id: String) -> Result<(), String> {
    let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(control) = active.values().find(|c| c.id == id) {
        control.cancelled.store(true, Ordering::SeqCst);
    }
    Ok(())
}
fn spawn(app: AppHandle, mut run: WorkflowRun, root: PathBuf, cancelled: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        let _claim = Claim(run.workflow.id.clone());
        let result = execute(&mut run, &root, &cancelled, &DesktopExecutor(app)).await;
        if let Err(error) = result {
            run.status = if cancelled.load(Ordering::SeqCst) {
                WorkflowStatus::Cancelled
            } else {
                WorkflowStatus::Failed
            };
            run.error = Some(error);
        }
        run.updated_at = now();
        if let Err(error) = save(&root, &run) {
            eprintln!("workflow checkpoint failed: {error}");
        }
    });
}
fn asset_paths(config: &WorkflowConfig) -> &[WorkflowAsset] {
    match config {
        WorkflowConfig::Assets { assets }
        | WorkflowConfig::Understand { assets, .. }
        | WorkflowConfig::Generate { assets, .. } => assets,
        _ => &[],
    }
}
fn validate_resources(app: &AppHandle, flow: &GenerationWorkflow) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings_read();
    for node in &flow.nodes {
        let config = node.config.as_ref().ok_or("节点缺少配置")?;
        if matches!(config, WorkflowConfig::Assets { .. })
            || graph::incoming(flow, &node.id, "image").is_none()
        {
            for asset in asset_paths(config) {
                inspect_asset(
                    &asset.path,
                    if node.kind == "video.upload" {
                        "video"
                    } else {
                        "image"
                    },
                )
                .map_err(|e| format!("{}：{e}", node.title))?;
            }
        }
        let model = match config {
            WorkflowConfig::Understand { model, .. }
            | WorkflowConfig::Generate { model, .. }
            | WorkflowConfig::Text { model, .. }
                if node.kind != "text.join" =>
            {
                model.as_ref()
            }
            _ => None,
        };
        if let Some(model) = model {
            let provider = settings
                .get_provider(&model.provider_id)
                .ok_or("模型提供商不存在")?;
            if !provider.enabled
                || !provider.has_credentials()
                || !provider.enabled_models.contains(&model.model)
            {
                return Err(format!("{}：模型不可用，请重选", node.title));
            }
            if matches!(config, WorkflowConfig::Generate { .. }) {
                let pool = if node.kind == "image.generate" {
                    &settings.workbench_media.image_models
                } else {
                    &settings.workbench_media.video_models
                };
                if !pool
                    .iter()
                    .any(|m| m.provider_id == model.provider_id && m.model == model.model)
                {
                    return Err(format!("{}：模型不在媒体创作模型池", node.title));
                }
            }
        }
    }
    Ok(())
}
fn inspect_asset(path: &str, kind: &str) -> Result<(), String> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.is_file() {
        return Err("素材不可访问，请重新选择".into());
    }
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let supported = if kind == "image" {
        ["png", "jpg", "jpeg", "webp", "gif", "bmp"].contains(&extension.as_str())
    } else {
        ["mp4", "mov", "webm", "mkv", "m4v"].contains(&extension.as_str())
    };
    if !supported {
        return Err("素材类型不匹配".into());
    }
    Ok(())
}
fn input(run: &WorkflowRun, node: &WorkflowNode, handle: &str) -> Option<WorkflowValue> {
    let edge = graph::incoming(&run.workflow, &node.id, handle)?;
    run.nodes
        .iter()
        .find(|n| n.node_id == edge.source)?
        .outputs
        .get(edge.source_handle.as_deref()?)
        .cloned()
}
fn output(handle: &str, value: WorkflowValue) -> BTreeMap<String, WorkflowValue> {
    [(handle.into(), value)].into()
}
fn stopped(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("已停止工作流；已提交的媒体任务仍保留，可在继续运行时查询结果".into())
    } else {
        Ok(())
    }
}

// The executor seam isolates network/desktop I/O. Tests run the same checkpointed scheduler.
trait Executor {
    fn run<'a>(
        &'a self,
        node: &'a WorkflowNode,
        run: &'a mut WorkflowRun,
        root: &'a Path,
        index: usize,
        cancelled: &'a AtomicBool,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BTreeMap<String, WorkflowValue>, String>>
                + Send
                + 'a,
        >,
    >;
}
async fn execute(
    run: &mut WorkflowRun,
    root: &Path,
    cancelled: &AtomicBool,
    executor: &impl Executor,
) -> Result<(), String> {
    let order = graph::validate(&run.workflow)?;
    for id in order {
        stopped(cancelled)?;
        let index = run
            .nodes
            .iter()
            .position(|n| n.node_id == id)
            .ok_or("节点检查点缺失")?;
        if run.nodes[index].status == WorkflowStatus::Succeeded {
            continue;
        }
        let node = run
            .workflow
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("节点不存在")?
            .clone();
        run.nodes[index].status = WorkflowStatus::Running;
        run.nodes[index].started_at = Some(now());
        run.nodes[index].finished_at = None;
        run.nodes[index].error = None;
        run.updated_at = now();
        save(root, run)?;
        match executor.run(&node, run, root, index, cancelled).await {
            Ok(outputs) => {
                run.nodes[index].outputs = outputs;
                run.nodes[index].status = WorkflowStatus::Succeeded;
            }
            Err(error) => {
                run.nodes[index].status = if cancelled.load(Ordering::SeqCst) {
                    WorkflowStatus::Cancelled
                } else {
                    WorkflowStatus::Failed
                };
                run.nodes[index].error = Some(error.clone());
                run.nodes[index].finished_at = Some(now());
                run.updated_at = now();
                save(root, run)?;
                return Err(format!("{}：{error}", node.title));
            }
        }
        run.nodes[index].finished_at = Some(now());
        run.updated_at = now();
        save(root, run)?;
    }
    stopped(cancelled)?;
    run.status = WorkflowStatus::Succeeded;
    run.error = None;
    Ok(())
}
struct DesktopExecutor(AppHandle);
impl Executor for DesktopExecutor {
    fn run<'a>(
        &'a self,
        node: &'a WorkflowNode,
        run: &'a mut WorkflowRun,
        root: &'a Path,
        index: usize,
        cancelled: &'a AtomicBool,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BTreeMap<String, WorkflowValue>, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { execute_node(&self.0, node, run, root, index, cancelled).await })
    }
}
async fn execute_node(
    app: &AppHandle,
    node: &WorkflowNode,
    run: &mut WorkflowRun,
    root: &Path,
    index: usize,
    cancelled: &AtomicBool,
) -> Result<BTreeMap<String, WorkflowValue>, String> {
    let config = node.config.as_ref().ok_or("节点配置缺失")?;
    match config {
        WorkflowConfig::Prompt { text } => Ok(output(
            "text",
            WorkflowValue {
                text: Some(text.clone()),
                files: vec![],
            },
        )),
        WorkflowConfig::Assets { assets } => {
            let kind = if node.kind == "video.upload" {
                "video"
            } else {
                "image"
            };
            for asset in assets {
                inspect_asset(&asset.path, kind)?;
            }
            Ok(output(
                kind,
                WorkflowValue {
                    text: None,
                    files: assets.iter().map(|a| a.path.clone()).collect(),
                },
            ))
        }
        WorkflowConfig::Output => {
            let handle = if node.kind.starts_with("text.") {
                "text"
            } else if node.kind.starts_with("video.") {
                "video"
            } else {
                "image"
            };
            let value = input(run, node, handle).ok_or("上游没有输出")?;
            if node.kind == "image.downloadZip" {
                let path = directory(root, &run.id)?.join(format!("output-{index}.zip"));
                zip_files(&value.files, &path)?;
                Ok(output(
                    "file",
                    WorkflowValue {
                        text: None,
                        files: vec![path.to_string_lossy().into_owned()],
                    },
                ))
            } else {
                Ok(output(handle, value))
            }
        }
        WorkflowConfig::Understand {
            instruction,
            model,
            assets,
        } => {
            let prompt = input(run, node, "prompt")
                .and_then(|v| v.text)
                .unwrap_or_else(|| instruction.clone());
            let images = input(run, node, "image")
                .map(|v| v.files)
                .unwrap_or_else(|| assets.iter().map(|a| a.path.clone()).collect());
            ai(
                app,
                &format!("{}-{index}", run.id),
                model.as_ref(),
                prompt,
                images,
                crate::chat::ai_task::AiTaskSlot::Vision,
                cancelled,
            )
            .await
        }
        WorkflowConfig::Text {
            instruction,
            text,
            model,
        } => {
            let text = input(run, node, "text")
                .and_then(|v| v.text)
                .unwrap_or_else(|| text.clone());
            let prompt = if instruction.trim().is_empty() {
                text
            } else {
                format!("{instruction}\n\n{text}")
            };
            if node.kind == "text.join" {
                return Ok(output(
                    "text",
                    WorkflowValue {
                        text: Some(prompt),
                        files: vec![],
                    },
                ));
            }
            ai(
                app,
                &format!("{}-{index}", run.id),
                model.as_ref(),
                prompt,
                vec![],
                if node.kind == "prompt.optimize" {
                    crate::chat::ai_task::AiTaskSlot::PromptOptimize
                } else {
                    crate::chat::ai_task::AiTaskSlot::Chat
                },
                cancelled,
            )
            .await
        }
        WorkflowConfig::Generate {
            prompt,
            assets,
            model,
            options,
        } => {
            let model = model.as_ref().ok_or("请选择生成模型")?;
            let origin = format!(
                "workbench/workflows/{}/{}/{}",
                run.workflow.id, run.id, node.id
            );
            // A process can stop between media submission and saving its receipt here.
            // Reconcile by this node's unique origin before considering a new submission.
            if run.nodes[index].media_task_id.is_none() {
                let tasks = media_generation::list_media_tasks(
                    app.clone(),
                    media_generation::MediaTaskFilter {
                        origin: Some(origin.clone()),
                        ..Default::default()
                    },
                )?;
                if let Some(task) = tasks.first() {
                    run.nodes[index].media_task_id = Some(task.id.clone());
                    save(root, run)?;
                }
            }
            let task_id = if let Some(id) = &run.nodes[index].media_task_id {
                id.clone()
            } else {
                let prompt = input(run, node, "prompt")
                    .and_then(|v| v.text)
                    .unwrap_or_else(|| prompt.clone());
                let mut images: Vec<String> = input(run, node, "image")
                    .map(|v| v.files)
                    .unwrap_or_else(|| assets.iter().map(|a| a.path.clone()).collect());
                let mut media_options = media_options(app, model, options, &node.kind)?;
                let is_comfy = app
                    .state::<AppState>()
                    .settings_read()
                    .get_provider(&model.provider_id)
                    .is_some_and(|p| p.request.comfy.is_some());
                bind_video_images(
                    &node.kind,
                    is_comfy,
                    options,
                    &mut images,
                    &mut media_options,
                );
                let request = MediaRequest {
                    provider_id: model.provider_id.clone(),
                    model: model.model.clone(),
                    kind: if node.kind == "image.generate" {
                        MediaKind::Image
                    } else {
                        MediaKind::Video
                    },
                    prompt,
                    images,
                    options: media_options,
                    origin: Some(origin),
                };
                let task = media_generation::start_media_generation(app.clone(), request).await?;
                run.nodes[index].media_task_id = Some(task.id.clone());
                run.updated_at = now();
                save(root, run)?;
                task.id
            };
            // Recovery only queries/resumes the saved media receipt. A failed receipt is never silently resubmitted.
            let mut task =
                media_generation::get_media_task(app.clone(), task_id.clone(), Some(true))?;
            loop {
                stopped(cancelled)?;
                match task.status {
                    MediaStatus::Succeeded => {
                        let files: Vec<_> = task.outputs.into_iter().map(|o| o.path).collect();
                        if files.is_empty() {
                            return Err("生成任务没有返回文件".into());
                        }
                        // Single-file ports intentionally use the first artifact; all artifacts remain in MediaTask.
                        return Ok(output(
                            if node.kind == "image.generate" {
                                "image"
                            } else {
                                "video"
                            },
                            WorkflowValue {
                                text: None,
                                files: files.into_iter().take(1).collect(),
                            },
                        ));
                    }
                    MediaStatus::Failed => {
                        return Err(task.error.unwrap_or_else(|| {
                            "媒体生成失败；可继续查询，或新建一次运行重新生成".into()
                        }))
                    }
                    MediaStatus::Running => {}
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                task = media_generation::get_media_task(app.clone(), task_id.clone(), None)?;
            }
        }
        WorkflowConfig::Placeholder => Err("节点尚未实现".into()),
    }
}
async fn ai(
    app: &AppHandle,
    task_id: &str,
    model: Option<&WorkflowModelChoice>,
    prompt: String,
    images: Vec<String>,
    slot: crate::chat::ai_task::AiTaskSlot,
    cancelled: &AtomicBool,
) -> Result<BTreeMap<String, WorkflowValue>, String> {
    use crate::chat::ai_task::{cancel_ai_task, run_ai_task, AiTaskMode, AiTaskRequest};
    let images = images
        .iter()
        .map(|p| crate::chat::attachments::read_attachment_as_data_url(Path::new(p)))
        .collect::<Result<Vec<_>, _>>()?;
    let request = AiTaskRequest {
        task_id: task_id.into(),
        mode: AiTaskMode::Once,
        system: None,
        prompt,
        images,
        videos: None,
        tools: vec![],
        slot,
        provider_id: model.map(|m| m.provider_id.clone()),
        model: model.map(|m| m.model.clone()),
        cwd: None,
        timeout_secs: None,
        stream: false,
    };
    let future = run_ai_task(app.clone(), app.state::<AppState>(), request);
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result.map(|result| output("text", WorkflowValue { text: Some(result.text), files: vec![] })),
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                if cancelled.load(Ordering::SeqCst) { cancel_ai_task(app.state::<AppState>(), task_id.into())?; let _ = future.await; return Err("AI 节点已停止".into()); }
            }
        }
    }
}
fn media_options(
    app: &AppHandle,
    model: &WorkflowModelChoice,
    options: &BTreeMap<String, String>,
    kind: &str,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let state = app.state::<AppState>();
    let settings = state.settings_read();
    let provider = settings
        .get_provider(&model.provider_id)
        .ok_or("模型提供商不存在")?;
    let mut result = BTreeMap::new();
    if let Some(comfy) = &provider.request.comfy {
        let workflow = comfy
            .workflows
            .iter()
            .find(|w| w.id == model.model)
            .ok_or("ComfyUI 工作流不存在")?;
        for binding in &workflow.inputs {
            if matches!(
                binding.source,
                Some(
                    crate::comfyui::ComfyInputSource::Prompt
                        | crate::comfyui::ComfyInputSource::Image { .. }
                )
            ) {
                continue;
            }
            let key = format!("{}:{}", binding.node_id, binding.input);
            if let Some(value) = options.get(&key) {
                let value = if binding.kind == crate::comfyui::ComfyInputKind::Number {
                    serde_json::json!(value.parse::<f64>().map_err(|_| "数字参数无效")?)
                } else {
                    serde_json::json!(value)
                };
                match &binding.source {
                    Some(crate::comfyui::ComfyInputSource::Parameter { name, index: None }) => {
                        result.insert(name.clone(), value);
                    }
                    Some(crate::comfyui::ComfyInputSource::Parameter {
                        name,
                        index: Some(index),
                    }) => {
                        let array = result
                            .entry(name.clone())
                            .or_insert_with(|| serde_json::json!([]))
                            .as_array_mut()
                            .ok_or("参数类型冲突")?;
                        array.resize(
                            (*index as usize + 1).max(array.len()),
                            serde_json::Value::Null,
                        );
                        array[*index as usize] = value;
                    }
                    _ => {
                        result.insert(key, value);
                    }
                }
            }
        }
    } else if kind == "image.generate" {
        if let Some(value) = options.get("output").filter(|s| !s.is_empty()) {
            let (resolution, ratio) = value.split_once(':').ok_or("图片尺寸无效")?;
            result.insert("size".into(), serde_json::json!(resolution.to_uppercase()));
            result.insert("aspect_ratio".into(), serde_json::json!(ratio));
        }
        result.insert("n".into(), serde_json::json!(1));
    } else {
        for (key, value) in options
            .iter()
            .filter(|(k, v)| k.as_str() != "imageMode" && !v.is_empty())
        {
            let value = match key.as_str() {
                "duration" => {
                    serde_json::json!(value.parse::<u32>().map_err(|_| "视频时长无效")?)
                }
                "audio" => serde_json::json!(value.parse::<bool>().map_err(|_| "声音参数无效")?),
                _ => serde_json::json!(value),
            };
            result.insert(
                if key == "audio" {
                    "generateAudio".into()
                } else {
                    key.clone()
                },
                value,
            );
        }
    }
    Ok(result)
}
fn bind_video_images(
    kind: &str,
    comfy: bool,
    options: &BTreeMap<String, String>,
    images: &mut Vec<String>,
    target: &mut BTreeMap<String, serde_json::Value>,
) {
    if kind != "video.generate" || comfy || images.is_empty() {
        return;
    }
    if options
        .get("imageMode")
        .is_some_and(|mode| mode == "reference")
    {
        target.insert("referenceImages".into(), serde_json::json!(images));
    } else {
        target.insert("firstFrame".into(), serde_json::json!(images[0]));
    }
    images.clear();
}
fn zip_files(files: &[String], path: &Path) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    for (index, path) in files.iter().enumerate() {
        let name = Path::new(path)
            .file_name()
            .ok_or("文件名无效")?
            .to_string_lossy();
        zip.start_file(
            format!("{}-{name}", index + 1),
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Existing debug probe, deliberately restricted to deterministic local nodes.
#[cfg(debug_assertions)]
pub(crate) async fn probe_local_workflow(
    app: &AppHandle,
    workflow: GenerationWorkflow,
) -> Result<WorkflowRun, String> {
    if workflow.nodes.iter().any(|node| {
        !matches!(
            node.kind.as_str(),
            "prompt.input" | "text.join" | "text.preview"
        )
    }) {
        return Err("工作流探针只允许运行本地文本节点".into());
    }
    let run = start_workflow_run(app.clone(), workflow)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(current) = list_workflow_runs(run.workflow.id.clone())?
            .into_iter()
            .find(|r| r.id == run.id)
        {
            if current.status != WorkflowStatus::Running {
                return Ok(current);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            cancel_workflow_run(run.id)?;
            return Err("探针超时".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
