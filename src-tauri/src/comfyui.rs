//! Self-hosted ComfyUI workflows. One typed contract for settings, validation and Workbench.
use crate::settings::ModelProvider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ComfyConfig {
    pub workflows: Vec<ComfyWorkflow>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComfyWorkflow {
    pub id: String,
    pub name: String,
    pub kind: ComfyMediaKind,
    #[ts(
        type = "Record<string, { class_type: string; inputs: Record<string, unknown>; _meta?: { title?: string } }>"
    )]
    pub graph: Value,
    pub inputs: Vec<ComfyInput>,
    pub output_nodes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ComfyMediaKind {
    Image,
    Video,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ComfyInputKind {
    Text,
    Number,
    Image,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComfyInput {
    pub node_id: String,
    pub input: String,
    pub label: String,
    pub kind: ComfyInputKind,
    /// Optional mapping from the common generation request; old node-key inputs still work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<ComfyInputSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ComfyInputSource {
    Prompt,
    Image { index: u32 },
    /// Named common options such as duration, firstFrame, size or referenceImages[index].
    Parameter {
        name: String,
        #[serde(default)]
        #[ts(optional)]
        index: Option<u32>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComfyConnection {
    pub devices: Vec<String>,
    pub missing_nodes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComfyArtifact {
    pub filename: String,
    pub subfolder: String,
    pub folder_type: String,
    pub local_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ComfyTask {
    pub id: String,
    pub provider_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub kind: ComfyMediaKind,
    pub base_url: String,
    pub output_nodes: Vec<String>,
    pub prompt_id: Option<String>,
    pub status: ComfyTaskStatus,
    pub error: Option<String>,
    pub outputs: Vec<ComfyArtifact>,
    pub created_at: String,
    /// Who asked for this run (e.g. `workbench/main`); lets each Workbench page list its own history.
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub prompt: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ComfyTaskStatus {
    Submitting,
    Queued,
    Running,
    Succeeded,
    Failed,
    Uncertain,
    DownloadPending,
}

pub fn validate_workflow(workflow: &ComfyWorkflow) -> Result<(), String> {
    if workflow.id.trim().is_empty() || workflow.name.trim().is_empty() {
        return Err("工作流名称不能为空".into());
    }
    let graph = workflow
        .graph
        .as_object()
        .filter(|g| !g.is_empty())
        .ok_or("请导入 ComfyUI 的 API 格式 JSON")?;
    if graph.contains_key("nodes") || graph.contains_key("links") {
        return Err("这是画布格式，请在 ComfyUI 中导出 API 格式 JSON".into());
    }
    for (id, node) in graph {
        if node["class_type"]
            .as_str()
            .filter(|v| !v.is_empty())
            .is_none()
            || !node["inputs"].is_object()
        {
            return Err(format!(
                "节点 {id} 缺少 class_type 或 inputs，请使用 API 格式"
            ));
        }
        for value in node["inputs"].as_object().unwrap().values() {
            if let Some(link) = value.as_array() {
                if link.len() != 2
                    || !link[0].as_str().is_some_and(|id| graph.contains_key(id))
                    || link[1].as_u64().is_none()
                {
                    return Err(format!("节点 {id} 包含无效连接"));
                }
            }
        }
    }
    if workflow.output_nodes.is_empty()
        || workflow
            .output_nodes
            .iter()
            .any(|id| !graph.contains_key(id))
    {
        return Err("请选择有效的图片或视频保存节点".into());
    }
    let mut seen = HashSet::new();
    for binding in &workflow.inputs {
        if let Some(ComfyInputSource::Parameter { name, .. }) = &binding.source {
            if name.trim().is_empty() || name.contains(':') {
                return Err("通用参数名不能为空或包含节点分隔符".into());
            }
        }
        if matches!(&binding.source, Some(ComfyInputSource::Prompt))
            && binding.kind != ComfyInputKind::Text
            || matches!(&binding.source, Some(ComfyInputSource::Image { .. }))
                && binding.kind != ComfyInputKind::Image
        {
            return Err("通用输入 source 与工作流输入类型不匹配".into());
        }
        if binding.label.trim().is_empty() || !seen.insert((&binding.node_id, &binding.input)) {
            return Err("输入名称为空或重复绑定".into());
        }
        let value = &workflow.graph[&binding.node_id]["inputs"][&binding.input];
        if !(match binding.kind {
            ComfyInputKind::Number => value.is_number(),
            _ => value.is_string(),
        }) {
            return Err(format!(
                "输入 {} 必须绑定到匹配类型的固定值，不能覆盖节点连接",
                binding.label
            ));
        }
    }
    Ok(())
}

pub fn normalize_provider(provider: &mut ModelProvider) {
    if let Some(config) = &provider.request.comfy {
        // Workflow IDs, names and capabilities have one owner; no duplicate model overrides.
        provider.enabled_models = config.workflows.iter().map(|w| w.id.clone()).collect();
        provider.available_models = provider.enabled_models.clone();
    }
}

fn endpoint(base: &str, route: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base.trim()).map_err(|_| "ComfyUI 服务地址无效")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("请填写不含账号、参数或片段的 HTTP(S) 服务地址".into());
    }
    url.set_path(&format!(
        "{}/{}",
        url.path().trim_end_matches('/'),
        route.trim_start_matches('/')
    ));
    Ok(url)
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}
async fn response_json(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let data: Value = response.json().await.map_err(|_| "ComfyUI 返回无效 JSON")?;
    if !status.is_success() {
        let message = data["error"]["message"]
            .as_str()
            .unwrap_or("请检查服务及工作流配置");
        return Err(format!(
            "ComfyUI HTTP {}: {}",
            status.as_u16(),
            message.chars().take(300).collect::<String>()
        ));
    }
    Ok(data)
}
async fn get(base: &str, route: &str) -> Result<Value, String> {
    response_json(
        client()?
            .get(endpoint(base, route)?)
            .send()
            .await
            .map_err(|e| format!("无法连接 ComfyUI: {}", e.without_url()))?,
    )
    .await
}
#[tauri::command]
pub fn validate_comfy_workflow(workflow: ComfyWorkflow) -> Result<(), String> {
    validate_workflow(&workflow)
}
#[tauri::command]
pub async fn test_comfy_connection(
    base_url: String,
    workflow: Option<ComfyWorkflow>,
) -> Result<ComfyConnection, String> {
    let stats = get(&base_url, "system_stats").await?;
    if !stats["system"].is_object() {
        return Err("该地址未返回 ComfyUI 系统信息".into());
    }
    let mut missing = vec![];
    if let Some(workflow) = workflow {
        validate_workflow(&workflow)?;
        let nodes = get(&base_url, "object_info").await?;
        for node in workflow.graph.as_object().unwrap().values() {
            let class = node["class_type"].as_str().unwrap();
            if nodes.get(class).is_none() && !missing.contains(&class.to_string()) {
                missing.push(class.to_string());
            }
        }
    }
    Ok(ComfyConnection {
        devices: stats["devices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect(),
        missing_nodes: missing,
    })
}
pub(crate) fn root() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("comfy-tasks"))
}
fn task_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    Ok(root.join(id))
}
pub(crate) fn save(root: &Path, task: &ComfyTask) -> Result<(), String> {
    let dir = task_dir(root, &task.id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(
        &dir.join("task.json"),
        &serde_json::to_string_pretty(task).map_err(|e| e.to_string())?,
        "ComfyUI task",
    )
}
pub(crate) fn read_task(root: &Path, id: &str) -> Result<ComfyTask, String> {
    serde_json::from_slice(
        &std::fs::read(task_dir(root, id)?.join("task.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub(crate) fn list_comfy_tasks(
    keep: impl Fn(&ComfyTask) -> bool,
) -> Result<Vec<ComfyTask>, String> {
    let mut tasks = vec![];
    if let Ok(dirs) = std::fs::read_dir(root()?) {
        for dir in dirs.flatten() {
            if let Ok(mut task) = read_task(&root()?, &dir.file_name().to_string_lossy()) {
                if !keep(&task) {
                    continue;
                }
                if task.status == ComfyTaskStatus::Submitting {
                    task.status = ComfyTaskStatus::Uncertain;
                }
                tasks.push(task);
            }
        }
    }
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tasks)
}
pub(crate) fn prepare(
    workflow: &ComfyWorkflow,
    values: &BTreeMap<String, Value>,
) -> Result<Value, String> {
    validate_workflow(workflow)?;
    let keys: HashSet<_> = workflow
        .inputs
        .iter()
        .map(|b| format!("{}:{}", b.node_id, b.input))
        .collect();
    if let Some(key) = values.keys().find(|k| !keys.contains(*k)) {
        return Err(format!("工作流未声明输入：{key}"));
    }
    let mut graph = workflow.graph.clone();
    for binding in &workflow.inputs {
        let key = format!("{}:{}", binding.node_id, binding.input);
        if let Some(value) = values.get(&key) {
            if !(match binding.kind {
                ComfyInputKind::Number => value.is_number(),
                _ => value.is_string(),
            }) {
                return Err(format!("{} 的输入类型无效", binding.label));
            }
            graph[&binding.node_id]["inputs"][&binding.input] = value.clone();
        }
    }
    Ok(graph)
}
// Serialize refreshes of one task; unrelated jobs never wait for its network I/O.
static TASK_LOCKS: LazyLock<
    std::sync::Mutex<BTreeMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>,
> = LazyLock::new(|| std::sync::Mutex::new(BTreeMap::new()));
fn task_lock(id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    let mut locks = TASK_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(id).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(id.to_owned(), std::sync::Arc::downgrade(&lock));
    lock
}
pub(crate) async fn submit_to_store(
    root: &Path,
    id: String,
    provider: ModelProvider,
    workflow: ComfyWorkflow,
    values: BTreeMap<String, Value>,
    origin: Option<String>,
    prompt: String,
) -> Result<ComfyTask, String> {
    let mut graph = prepare(&workflow, &values)?;
    let submit_url = endpoint(&provider.base_url, "prompt")?;
    for binding in &workflow.inputs {
        if binding.kind != ComfyInputKind::Image {
            continue;
        }
        let value = &mut graph[&binding.node_id]["inputs"][&binding.input];
        let data = value.as_str().unwrap_or_default();
        if data.starts_with("data:") {
            use base64::Engine;
            let (header, encoded) = data.split_once(',').ok_or("图片内容无效")?;
            if !header.starts_with("data:image/")
                || !header.ends_with(";base64")
                || encoded.len() > 40 * 1024 * 1024
            {
                return Err("请选择 30 MB 以内的图片".into());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| "图片编码无效")?;
            let mime = header
                .trim_start_matches("data:")
                .trim_end_matches(";base64");
            let ext = match mime {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => return Err("仅支持 PNG、JPEG、WebP 参考图".into()),
            };
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(format!("dsivio-{}.{}", uuid::Uuid::new_v4(), ext))
                .mime_str(mime)
                .map_err(|e| e.to_string())?;
            let response = client()?
                .post(endpoint(&provider.base_url, "upload/image")?)
                .multipart(
                    reqwest::multipart::Form::new()
                        .part("image", part)
                        .text("type", "input"),
                )
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?;
            let uploaded = response_json(response).await?;
            let name = uploaded["name"].as_str().ok_or("上传未返回文件名")?;
            let folder = uploaded["subfolder"].as_str().unwrap_or_default();
            *value = json!(if folder.is_empty() {
                name.to_owned()
            } else {
                format!("{folder}/{name}")
            });
        }
    }
    let mut task = ComfyTask {
        id,
        provider_id: provider.id,
        workflow_id: workflow.id,
        workflow_name: workflow.name,
        kind: workflow.kind,
        base_url: provider.base_url,
        output_nodes: workflow.output_nodes,
        prompt_id: None,
        status: ComfyTaskStatus::Submitting,
        error: None,
        outputs: vec![],
        created_at: chrono::Utc::now().to_rfc3339(),
        origin,
        prompt,
    };
    save(root, &task)?; // Persist before POST. Never replay a submission after an uncertain response.
    let result = client()?.post(submit_url).json(&json!({"prompt": graph, "client_id": task.id, "extra_data": {"dsivio_task_id":task.id}})).send().await;
    match result {
        Ok(response) => {
            let definitive_rejection = response.status().is_client_error();
            match response_json(response).await {
                Ok(data) if data["prompt_id"].is_string() => {
                    task.prompt_id = data["prompt_id"].as_str().map(str::to_owned);
                    task.status = ComfyTaskStatus::Queued;
                }
                result => {
                    task.status = if definitive_rejection {
                        ComfyTaskStatus::Failed
                    } else {
                        ComfyTaskStatus::Uncertain
                    };
                    task.error = Some(result.err().unwrap_or(
                        "提交未返回任务编号，请在 ComfyUI 队列中核查，勿重复提交".into(),
                    ));
                }
            }
        }
        Err(_) => {
            task.status = ComfyTaskStatus::Uncertain;
            task.error =
                Some("提交连接中断，结果不确定。请在 ComfyUI 队列中核查，勿重复提交。".into());
        }
    }
    save(root, &task)?;
    Ok(task)
}

fn decode_history(task: &mut ComfyTask, history: &Value) {
    if history["status"]["status_str"] == "error" {
        task.status = ComfyTaskStatus::Failed;
        task.error = Some("ComfyUI 工作流执行失败，请检查服务中的节点错误。".into());
        return;
    }
    if history["status"]["completed"] != true {
        return;
    }
    task.outputs.clear();
    let output_nodes = if task.output_nodes.is_empty() {
        history["outputs"].as_object().map(|v|v.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()
    } else { task.output_nodes.clone() };
    for id in &output_nodes {
        for key in ["images", "gifs", "videos"] {
            for item in history["outputs"][id][key].as_array().into_iter().flatten() {
                if let Some(filename) = item["filename"].as_str() {
                    let ext = filename
                        .rsplit('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let accepted = if task.kind == ComfyMediaKind::Video {
                        ["mp4", "webm", "mov", "gif"].contains(&ext.as_str())
                    } else {
                        ["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str())
                    };
                    if accepted {
                        task.outputs.push(ComfyArtifact {
                            filename: filename.into(),
                            subfolder: item["subfolder"].as_str().unwrap_or_default().into(),
                            folder_type: item["type"].as_str().unwrap_or("output").into(),
                            local_path: None,
                        });
                    }
                }
            }
        }
    }
    task.status = if task.outputs.is_empty() {
        ComfyTaskStatus::Failed
    } else {
        ComfyTaskStatus::DownloadPending
    };
    if task.outputs.is_empty() {
        task.error = Some("选定输出节点未返回可读取的媒体文件，请检查输出节点和媒体类型。".into());
    }
}
pub(crate) async fn refresh_at(root: &Path, id: &str) -> Result<ComfyTask, String> {
    let lock = task_lock(&id);
    let _lock = lock.lock().await;
    let mut task = read_task(root, id)?;
    if matches!(
        task.status,
        ComfyTaskStatus::Succeeded | ComfyTaskStatus::Failed | ComfyTaskStatus::Uncertain
    ) {
        return Ok(task);
    }
    let Some(prompt_id) = &task.prompt_id else {
        task.status = ComfyTaskStatus::Uncertain;
        save(root, &task)?;
        return Ok(task);
    };
    if task.outputs.is_empty() {
        let mut url = endpoint(&task.base_url, "history")?;
        url.path_segments_mut()
            .map_err(|_| "无效地址")?
            .push(prompt_id);
        let history = response_json(
            client()?
                .get(url)
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?,
        )
        .await?;
        if let Some(entry) = history.get(prompt_id) {
            decode_history(&mut task, entry);
        } else {
            let queue = get(&task.base_url, "queue").await?;
            let running = queue["queue_running"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|row| row[1].as_str() == Some(prompt_id));
            let pending = queue["queue_pending"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|row| row[1].as_str() == Some(prompt_id));
            task.status = if running {
                ComfyTaskStatus::Running
            } else {
                ComfyTaskStatus::Queued
            };
            task.error = if !running && !pending {
                Some("任务暂未出现在队列或历史中，可稍后刷新；请勿重复提交。".into())
            } else {
                None
            };
        }
        save(root, &task)?;
    }
    if !task.outputs.is_empty() {
        for (index, output) in task.outputs.iter_mut().enumerate() {
            if output.local_path.is_some() {
                continue;
            }
            let mut url = endpoint(&task.base_url, "view")?;
            url.query_pairs_mut()
                .append_pair("filename", &output.filename)
                .append_pair("subfolder", &output.subfolder)
                .append_pair("type", &output.folder_type);
            let download: Result<String, String> = async {
                let response = client()?
                    .get(url)
                    .timeout(Duration::from_secs(180))
                    .send()
                    .await
                    .map_err(|e| e.without_url().to_string())?
                    .error_for_status()
                    .map_err(|e| e.without_url().to_string())?;
                let path = task_dir(root, &task.id)?.join(format!("output-{index}"));
                let kind = if task.kind == ComfyMediaKind::Image {
                    crate::media_generation::MediaKind::Image
                } else {
                    crate::media_generation::MediaKind::Video
                };
                Ok(
                    crate::media_generation::artifacts::save_response(response, &path, &kind)
                        .await?
                        .path,
                )
            }
            .await;
            match download {
                Ok(path) => output.local_path = Some(path),
                Err(error) => {
                    task.error = Some(format!("生成已完成，下载待恢复：{error}"));
                    break;
                }
            }
        }
        if task.outputs.iter().all(|o| o.local_path.is_some()) {
            task.status = ComfyTaskStatus::Succeeded;
            task.error = None;
        }
        save(root, &task)?;
    }
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn workflow() -> ComfyWorkflow {
        serde_json::from_value(json!({"id":"wf", "name":"Product image", "kind":"image", "graph":{"1":{"class_type":"CLIPTextEncode","inputs":{"text":"original"}},"2":{"class_type":"SaveImage","inputs":{"images":["1",0]}}}, "inputs":[{"nodeId":"1","input":"text","label":"Prompt","kind":"text"}], "outputNodes":["2"]})).unwrap()
    }
    fn provider(base: &str) -> ModelProvider {
        serde_json::from_value(json!({"id":"local", "name":"ComfyUI", "baseUrl":base, "request":{"comfy":{"workflows":[workflow()]}}})).unwrap()
    }
    fn server(responses: Vec<(u16, Vec<u8>)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = vec![];
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut buffer = [0; 8192];
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(split) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..split]).to_lowercase();
                        let size = headers
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= split + 4 + size {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8_lossy(&bytes).into_owned());
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
            listener.set_nonblocking(true).unwrap();
            assert!(listener.accept().is_err(), "unexpected retry");
            requests
        });
        (base, handle)
    }
    #[test]
    fn validates_api_graph_and_only_replaces_bound_inputs() {
        let original = workflow();
        let updated = prepare(
            &original,
            &BTreeMap::from([("1:text".into(), json!("new prompt"))]),
        )
        .unwrap();
        assert_eq!(updated["1"]["inputs"]["text"], "new prompt");
        assert_eq!(updated["2"]["inputs"]["images"], json!(["1", 0]));
        assert_eq!(original.graph["1"]["inputs"]["text"], "original");
        let mut invalid = original.clone();
        invalid.graph = json!({"nodes":[],"links":[]});
        assert!(validate_workflow(&invalid)
            .unwrap_err()
            .contains("画布格式"));
        let mut invalid = original.clone();
        invalid.inputs[0].node_id = "2".into();
        invalid.inputs[0].input = "images".into();
        assert!(validate_workflow(&invalid).is_err());
        let mut invalid = original;
        invalid.output_nodes = vec!["missing".into()];
        assert!(validate_workflow(&invalid).is_err());
    }
    #[test]
    fn unbound_inputs_are_rejected_instead_of_silently_ignored() {
        assert!(prepare(
            &workflow(),
            &BTreeMap::from([("typo:text".into(), json!("ignored"))])
        )
        .is_err());
    }

    #[test]
    fn canonical_settings_preserve_workflows_and_keep_them_out_of_chat() {
        let mut settings = crate::settings::Settings::default();
        settings.providers = vec![provider("http://127.0.0.1:8188")];
        settings.default_models.chat = crate::settings::DefaultModelSelection {
            provider_id: "local".into(),
            model: "wf".into(),
        };
        settings.workbench_media.image_models = vec![crate::settings::DefaultModelSelection {
            provider_id: "local".into(),
            model: "wf".into(),
        }];
        let saved = crate::settings::sanitize_settings(settings);
        assert_eq!(saved.providers[0].enabled_models, vec!["wf"]);
        assert_eq!(saved.workbench_media.image_models.len(), 1);
        assert!(saved.default_models.chat.provider_id.is_empty());
        assert_ne!(saved.translator_provider_id, "local");
        let reopened: crate::settings::Settings =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(
            reopened.providers[0]
                .request
                .comfy
                .as_ref()
                .unwrap()
                .workflows[0]
                .name,
            "Product image"
        );
    }
    #[tokio::test]
    async fn official_http_flow_persists_receipt_and_recovers_download_without_resubmitting() {
        let history = json!({"remote-1":{"status":{"completed":true,"status_str":"success"},"outputs":{"2":{"images":[{"filename":"final image.png","subfolder":"nested","type":"output"}]}}}});
        let (base, server) = server(vec![
            (200, br#"{"prompt_id":"remote-1"}"#.to_vec()),
            (200, serde_json::to_vec(&history).unwrap()),
            (503, b"{}".to_vec()),
            (200, b"\x89PNG\r\n\x1a\nimage-bytes".to_vec()),
        ]);
        let root = tempfile::tempdir().unwrap();
        let task = submit_to_store(
            root.path(),
            uuid::Uuid::new_v4().to_string(),
            provider(&base),
            workflow(),
            BTreeMap::from([("1:text".into(), json!("new product"))]),
            None,
            String::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            read_task(root.path(), &task.id)
                .unwrap()
                .prompt_id
                .as_deref(),
            Some("remote-1")
        );
        let pending = refresh_at(root.path(), &task.id).await.unwrap();
        assert_eq!(pending.status, ComfyTaskStatus::DownloadPending);
        let complete = refresh_at(root.path(), &task.id).await.unwrap();
        assert_eq!(complete.status, ComfyTaskStatus::Succeeded);
        assert_eq!(
            std::fs::read(complete.outputs[0].local_path.as_ref().unwrap()).unwrap(),
            b"\x89PNG\r\n\x1a\nimage-bytes"
        );
        let requests = server.join().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("POST /prompt "))
                .count(),
            1
        );
        assert!(requests[0].contains("new product"));
        assert!(requests[1].starts_with("GET /history/remote-1 "));
        assert!(requests[2].contains("filename=final+image.png&subfolder=nested&type=output"));
        assert!(requests[3].starts_with("GET /view?"));
    }
    #[tokio::test]
    async fn connection_check_reports_missing_custom_nodes() {
        let (base, server) = server(vec![
            (200, br#"{"system":{},"devices":[{"name":"GPU"}]}"#.to_vec()),
            (200, br#"{"SaveImage":{}}"#.to_vec()),
        ]);
        let check = test_comfy_connection(base, Some(workflow())).await.unwrap();
        assert_eq!(check.devices, vec!["GPU"]);
        assert_eq!(check.missing_nodes, vec!["CLIPTextEncode"]);
        server.join().unwrap();
    }
    #[test]
    fn endpoint_validation_preserves_prefix_and_rejects_credential_urls() {
        assert_eq!(
            endpoint("http://localhost:8188/comfy/", "prompt")
                .unwrap()
                .as_str(),
            "http://localhost:8188/comfy/prompt"
        );
        assert!(endpoint("file:///tmp", "prompt").is_err());
        assert!(endpoint("http://token@localhost:8188", "prompt").is_err());
    }
    #[tokio::test]
    async fn server_error_leaves_an_uncertain_receipt_and_is_never_replayed() {
        let (base, server) = server(vec![(502, b"{}".to_vec())]);
        let root = tempfile::tempdir().unwrap();
        let task = submit_to_store(root.path(), uuid::Uuid::new_v4().to_string(), provider(&base), workflow(), BTreeMap::new(), None, String::new())
            .await
            .unwrap();
        assert_eq!(task.status, ComfyTaskStatus::Uncertain);
        assert_eq!(
            refresh_at(root.path(), &task.id).await.unwrap().status,
            ComfyTaskStatus::Uncertain
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reference_image_is_uploaded_before_its_server_filename_is_bound() {
        let (base, server) = server(vec![
            (
                200,
                br#"{"name":"reference.png","subfolder":"uploads"}"#.to_vec(),
            ),
            (200, br#"{"prompt_id":"image-task"}"#.to_vec()),
        ]);
        let mut flow = workflow();
        flow.graph["1"]["class_type"] = json!("LoadImage");
        flow.inputs[0].kind = ComfyInputKind::Image;
        let root = tempfile::tempdir().unwrap();
        submit_to_store(
            root.path(),
            uuid::Uuid::new_v4().to_string(),
            provider(&base),
            flow,
            BTreeMap::from([("1:text".into(), json!("data:image/png;base64,aW1hZ2U="))]),
            None,
            String::new(),
        )
        .await
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /upload/image "));
        assert!(requests[0].contains("name=\"image\""));
        assert!(requests[1].contains("uploads/reference.png"));
        assert!(!requests[1].contains("base64"));
    }

    #[test]
    fn video_outputs_respect_selected_nodes_and_report_missing_media() {
        let mut task: ComfyTask = serde_json::from_value(json!({"id":"task", "providerId":"local", "workflowId":"wf", "workflowName":"video", "kind":"video", "baseUrl":"http://localhost:8188", "outputNodes":["9"], "promptId":"remote", "status":"running", "error":null, "outputs":[], "createdAt":"now"})).unwrap();
        decode_history(
            &mut task,
            &json!({"status":{"completed":true}, "outputs":{"2":{"images":[{"filename":"preview.png"}]}, "9":{"gifs":[{"filename":"video.mp4","type":"output"}]}}}),
        );
        assert_eq!(task.status, ComfyTaskStatus::DownloadPending);
        assert_eq!(task.outputs.len(), 1);
        assert_eq!(task.outputs[0].filename, "video.mp4");
        decode_history(
            &mut task,
            &json!({"status":{"completed":true}, "outputs":{}}),
        );
        assert_eq!(task.status, ComfyTaskStatus::Failed);
    }
}
