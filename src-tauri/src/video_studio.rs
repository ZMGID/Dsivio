//! Video UI and bundled chat plugin share the same Python workspace service.
pub(crate) mod config;
mod planning;
use crate::{
    image_studio::{agent, types::StudioConfig},
    plugins::packages,
    state::AppState,
};
use base64::Engine;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{atomic::AtomicBool, Arc},
};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;

pub(crate) mod runtime;
pub(crate) const PACKAGE_ID: &str = "42df724b-34e1-47b8-aa2c-6c738b09d280";

// Public worker operations shared by the chat tool and the video page.
// Submission bookkeeping (comfy_submitted, uncertain, etc.) stays host-only.
const WORKER_ACTIONS: &[&str] = &[
    "bootstrap",
    "get",
    "create",
    "save",
    "plan_result",
    "analysis_result",
    "approve",
    "prompt_result",
    "quote",
    "template_save",
    "template_import",
    "install_comfy",
    "recover",
];
const HOST_ACTIONS: &[&str] = &[
    "config",
    "image_preview",
    "open",
    "preview",
    "plan",
    "prepare",
    "analyze",
    "revise",
    "submit",
    "poll",
    "wait",
];

pub(crate) fn public_actions() -> Vec<&'static str> {
    WORKER_ACTIONS.iter().chain(HOST_ACTIONS).copied().collect()
}

fn unknown_action(action: &str) -> String {
    format!(
        "未知视频操作 {action:?}；支持的操作：{}。请按 studio 工具说明调用；若文档中的操作仍不可用，请报告接口版本不一致并停止，不要搜索或修改安装目录，也不要绕过工具提交。",
        public_actions().join(", ")
    )
}

fn source(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(runtime::resource_directory(app)?.join("plugins/dsvideo-plugin"))
}
pub fn initialize(app: &AppHandle) -> Result<(), String> {
    runtime::initialize(app)?;
    packages::ensure_builtin(PACKAGE_ID, &source(app)?)?;
    Ok(())
}

pub fn sync_settings(settings: &mut crate::settings::Settings) {
    let Ok(p) = plugin() else {
        return;
    };
    sync_builtin_servers(settings, p.servers);
}

fn sync_builtin_servers(
    settings: &mut crate::settings::Settings,
    servers: Vec<crate::settings::ChatMcpServer>,
) {
    for mut server in servers {
        if let Some(previous) = settings
            .chat_tools
            .servers
            .iter_mut()
            .find(|s| s.id == server.id)
        {
            // Preserve user switches while upgrading old npx/system-Python paths,
            // including when the application bundle itself has moved.
            server.enabled = previous.enabled;
            server.enabled_tools = previous.enabled_tools.clone();
            for (key, value) in &previous.env {
                server
                    .env
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            *previous = server;
        } else {
            settings.chat_tools.servers.push(server);
        }
    }
}

fn plugin() -> Result<packages::Resolved, String> {
    packages::active()
        .into_iter()
        .find(|p| p.package.id == PACKAGE_ID)
        .ok_or("内置 dsvideo 插件已关闭，请在扩展中启用".into())
}

fn image_url(path: &str) -> Result<String, String> {
    let path = PathBuf::from(path);
    let mime = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => return Err("不支持的图片格式".into()),
    };
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 30 * 1024 * 1024 {
        return Err("单张参考图片需小于 30 MB".into());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD
            .encode(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}

async fn worker(app: &AppHandle, action: &str, input: Value) -> Result<Value, String> {
    let script = source(app)?.join("scripts/studio.py");
    let environment = runtime::environment()?;
    let mut command = tokio::process::Command::new(&environment["DSVIDEO_PYTHON"]);
    command
        .envs(&environment)
        .args(["-s", "-B", "-X", "utf8"])
        .arg(script)
        .arg(action)
        .env(
            "DSVIDEO_STUDIO_ROOT",
            crate::app_data::app_data_dir()
                .ok_or("无应用数据目录")?
                .join("video-studio"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|e| format!("内置视频运行环境无法启动，请重新安装 dsivio：{e}"))?;
    child
        .stdin
        .take()
        .ok_or("无输入管道")?
        .write_all(input.to_string().as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "视频步骤超时；如已提交请恢复查询，勿重复生成")?
    .map_err(|e| e.to_string())?;
    let result = worker_result(&output.stdout)?;
    if !matches!(action, "get" | "preflight") {
        if let Some(id) = result["id"].as_str() {
            crate::studio::wait::changed("video", id);
        }
    }
    Ok(result)
}

fn worker_result(stdout: &[u8]) -> Result<Value, String> {
    let result: Value =
        serde_json::from_slice(stdout).map_err(|_| "视频服务未返回有效结果，请检查 Python 版本")?;
    // Task records carry their own error field (including an empty string on
    // success). Only the worker's standalone error envelope is an IPC failure.
    if result.get("id").is_none() {
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            return Err(if error.trim().is_empty() {
                "视频服务返回了空错误".into()
            } else {
                error.into()
            });
        }
    }
    Ok(result)
}

async fn mcp(
    app: &AppHandle,
    key: &str,
    name: &str,
    input: Value,
    url: Option<&str>,
) -> Result<Value, String> {
    let package = plugin()?;
    let mut server = package
        .servers
        .into_iter()
        .find(|s| s.name.ends_with(key))
        .ok_or("内置 MCP 不存在")?;
    // The task's original endpoint remains authoritative during recovery.
    if let Some(url) = url {
        server.env.insert("COMFYUI_URL".into(), url.into());
    }
    let result = app
        .state::<AppState>()
        .mcp_call_tool(Some(app), &server, name, input)
        .await?;
    if result.is_error {
        return Err(format!("{key} 执行失败：{}", result.content));
    }
    Ok(result.raw)
}

fn find_string(value: &Value, key: &str) -> Option<String> {
    if let Some(s) = value.get(key).and_then(Value::as_str) {
        return Some(s.into());
    }
    match value {
        Value::Array(a) => a.iter().find_map(|v| find_string(v, key)),
        Value::Object(o) => o.values().find_map(|v| find_string(v, key)),
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .and_then(|v| find_string(&v, key)),
        _ => None,
    }
}

async fn direct(app: &AppHandle, action: &str, input: Value) -> Result<Value, String> {
    let mut preflight = input.clone();
    preflight["operation"] = json!(action);
    let t = worker(app, "preflight", preflight).await?;
    let id = t["id"].as_str().ok_or("无任务编号")?;
    let b = &t["brief"];
    if action == "prepare" {
        if t["approved"] != true {
            return Err("请先确认当前剧本".into());
        }
        let mut save = input;
        save["prompt"] = t["script"].clone();
        return worker(app, "prompt_result", save).await;
    }
    let planner = if action == "plan" {
        Some(planning::select(
            crate::chat::storage::load_assistant_index(app)?.assistants,
            b["assistantId"].as_str(),
        )?)
    } else {
        None
    };
    let config = planner
        .as_ref()
        .map(|assistant| {
            let settings = app.state::<crate::state::AppState>();
            let (default_provider, default_model) = settings.settings_read().effective_chat_model();
            StudioConfig {
                agent_provider_id: if assistant.provider_id.is_empty() {
                    default_provider
                } else {
                    assistant.provider_id.clone()
                },
                agent_model: if assistant.model.is_empty() {
                    default_model
                } else {
                    assistant.model.clone()
                },
                ..StudioConfig::default()
            }
        })
        .unwrap_or_default();
    let root = source(app)?;
    let mut images: Vec<(String, String)> = b["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .enumerate()
        .map(|(index, path)| {
            let mode = b["inputMode"].as_str().unwrap_or("auto");
            let role = if (mode == "image" && index == 0)
                || (mode == "frames" && b["firstFrame"] == path)
            {
                "用户指定首帧"
            } else if mode == "frames" && b["lastFrame"] == path {
                "用户指定尾帧"
            } else {
                "用途以用户要求为准，不默认用作首帧"
            };
            (format!("参考图片 {}（{role}）", index + 1), path.into())
        })
        .collect();
    for (_, path) in &mut images {
        *path = image_url(path)?;
    }
    let mut analysis = Value::Null;
    let (instruction, data, field, result_action) = if action == "plan" {
        let revision = planning_revision(
            input["note"].as_str(),
            input["previousScript"]
                .as_str()
                .or_else(|| t["script"].as_str()),
        );
        let instruction =
            planning::instruction(planner.as_ref().ok_or("无视频助手")?, b, revision.is_some());
        let context = revision
            .map(|(note, previous)| revise_context(previous, note, b))
            .unwrap_or_else(|| b.clone());
        (instruction, context, "script", "plan_result")
    } else if action == "analyze" {
        analysis = mcp(
            app,
            "video-analyzer",
            "analyze_video",
            json!({"url":b["source"],"options":{"detail":"standard"}}),
            None,
        )
        .await?;
        if let Some(content) = analysis["content"].as_array() {
            for v in content {
                if v["type"] == "image" {
                    if let (Some(mime), Some(data)) = (v["mimeType"].as_str(), v["data"].as_str()) {
                        images.push((
                            "参考视频关键帧".into(),
                            format!("data:{mime};base64,{data}"),
                        ));
                    }
                }
            }
        }
        let mut evidence = analysis.clone();
        if let Some(content) = evidence["content"].as_array_mut() {
            content.retain(|v| v["type"] != "image");
        }
        analysis = evidence.clone();
        (format!("{}\nReturn {{\"script\":\"逐镜头拆解\"}} in the requested report_language (default Chinese). Follow the user request as the analysis focus. Clearly preserve warnings and missing evidence. Separate visible text, product logos, and audible dialogue. Never invent observations. Include reusable shot directions.",
            std::fs::read_to_string(root.join("skills/video-reference-analysis/SKILL.md")).map_err(|e|e.to_string())?), analysis_context(evidence, b), "script", "analysis_result")
    } else {
        return Err("不支持的视频规划操作".into());
    };
    let result = agent::run_specialized(
        app,
        id,
        &config,
        &instruction,
        data,
        images,
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .await?;
    if action == "plan" && planning_revision(input["note"].as_str(), Some("")).is_none() {
        if let Some(concepts) = result["concepts"].as_array() {
            if concepts.len() != 3
                || concepts
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
            {
                return Err("拍法需要包含三个完整选项，请重试".into());
            }
            let mut save = input;
            save["script"] = json!("");
            save["concepts"] = json!(concepts);
            return worker(app, "plan_result", save).await;
        }
    }
    let text = result[field]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("Agent 返回的剧本或提示词为空")?;
    let mut save = input;
    save[field] = json!(text);
    if action == "analyze" {
        save["analysis"] = analysis;
    }
    worker(app, result_action, save).await
}

async fn submit_comfy(app: &AppHandle, t: Value) -> Result<Value, String> {
    let id = t["id"].as_str().ok_or("无任务编号")?;
    let url = t["remote"]["base_url"].as_str();
    let mut submitting = false;
    let result: Result<Value, String> = async {
        let mut names = vec![];
        for path in t["brief"]["images"].as_array().into_iter().flatten() {
            let uploaded = mcp(
                app,
                "comfy-mcp",
                "upload_file",
                json!({"paths":[path]}),
                url,
            )
            .await?;
            let name = find_string(&uploaded, "cloud_name")
                .or_else(|| find_string(&uploaded, "name"))
                .ok_or("MCP 未返回上传文件名，未提交工作流")?;
            let subfolder = find_string(&uploaded, "subfolder").unwrap_or_default();
            names.push(if subfolder.is_empty() {
                name
            } else {
                format!("{subfolder}/{name}")
            });
        }
        let flow = worker(
            app,
            "comfy_workflow",
            json!({"id":id,"revision":t["revision"],"images":names}),
        )
        .await?;
        submitting = true;
        let result = mcp(
            app,
            "comfy-mcp",
            "run_workflow",
            json!({"workflow_path":flow["path"],"wait":false,"confirm_spend":true}),
            url,
        )
        .await?;
        let remote_id = find_string(&result, "prompt_id").ok_or("MCP 未返回 prompt_id")?;
        worker(
            app,
            "comfy_submitted",
            json!({"id":id,"revision":t["revision"],"remoteId":remote_id}),
        )
        .await
    }
    .await;
    match result {
        Ok(t) => Ok(t),
        Err(error) => {
            let settings = app.state::<AppState>().settings_read().clone();
            let mut detail = error;
            for provider in &settings.providers {
                for key in &provider.api_keys {
                    if !key.is_empty() {
                        detail = detail.replace(key, "[redacted]");
                    }
                }
            }
            let detail: String = detail.chars().take(600).collect();
            worker(
                app,
                if submitting {
                    "uncertain"
                } else {
                    "preflight_failed"
                },
                json!({"id":id,"revision":t["revision"],"detail":detail}),
            )
            .await
        }
    }
}

#[tauri::command]
pub async fn video_studio(app: AppHandle, action: String, input: Value) -> Result<Value, String> {
    plugin()?;
    match action.as_str() {
        "config" => {
            let result = worker(&app, "config", input).await?;
            for server in plugin()?.servers {
                app.state::<AppState>()
                    .mcp_disconnect_server(&server.id)
                    .await;
            }
            Ok(result)
        }
        "image_preview" => Ok(json!(image_url(
            input["path"].as_str().ok_or("无图片路径")?
        )?)),
        "open" | "preview" => {
            let reveal = input["mode"].as_str() == Some("reveal");
            let root = crate::app_data::app_data_dir()
                .ok_or("无数据目录")?
                .join("video-studio");
            let t = worker(&app, "get", input).await?;
            let output = PathBuf::from(t["output"].as_str().ok_or("任务还没有本地成片")?);
            let canonical = output.canonicalize().map_err(|e| e.to_string())?;
            let base = root.canonicalize().map_err(|e| e.to_string())?;
            if !canonical.starts_with(&base) {
                return Err("成片必须位于视频工作区内".into());
            }
            if action == "open" {
                let relative = canonical
                    .strip_prefix(&base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into();
                crate::dock::fs::dock_fs_open_path(
                    base.to_string_lossy().into(),
                    relative,
                    reveal.then(|| "reveal".into()),
                )
                .await?;
                return Ok(Value::Null);
            }
            if std::fs::metadata(&canonical)
                .map_err(|e| e.to_string())?
                .len()
                > 80 * 1024 * 1024
            {
                return Err("大文件请使用打开本地成片播放".into());
            }
            let mime = if canonical.extension().is_some_and(|e| e == "webm") {
                "video/webm"
            } else {
                "video/mp4"
            };
            Ok(json!(format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD
                    .encode(std::fs::read(canonical).map_err(|e| e.to_string())?)
            )))
        }
        "plan" | "prepare" | "analyze" => direct(&app, &action, input).await,
        "revise" => {
            let t = worker(&app, "get", input.clone()).await?;
            let status = t["status"].as_str().unwrap_or("");
            if matches!(status, "submitting" | "running" | "uncertain") {
                return Err("生成尚未结束，请等待完成后再改拍摄方案".into());
            }
            let note = input["note"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("请填写需要修改的地方")?
                .to_string();
            if t["script"].as_str().map(str::trim).unwrap_or("").is_empty() {
                return Err("没有可改写的拍摄方案".into());
            }
            let mut plan_input = input;
            plan_input["note"] = json!(note);
            plan_input["previousScript"] = t["script"].clone();
            plan_input["revision"] = t["revision"].clone();
            direct(&app, "plan", plan_input).await
        }
        "submit" => {
            let t = worker(&app, "submit", input).await?;
            if t["remote"]["route"] == "comfy" {
                submit_comfy(&app, t).await
            } else {
                Ok(t)
            }
        }
        "poll" => poll_task(app, input["id"].as_str().ok_or("需要任务 id")?).await,
        "wait" => crate::studio::wait::task(app, "video", &input).await,
        _ if WORKER_ACTIONS.contains(&action.as_str()) => worker(&app, &action, input).await,
        _ => Err(unknown_action(&action)),
    }
}

fn analysis_context(evidence: Value, brief: &Value) -> Value {
    json!({"evidence": evidence, "request": brief["request"], "report_language": brief["language"]})
}

fn planning_revision<'a>(
    note: Option<&'a str>,
    previous_script: Option<&'a str>,
) -> Option<(&'a str, &'a str)> {
    let note = note.map(str::trim).filter(|s| !s.is_empty())?;
    let previous = previous_script
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("");
    Some((note, previous))
}

fn revise_context(script: &str, note: &str, brief: &Value) -> Value {
    json!({"script": script, "note": note, "brief": brief})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_task_errors_are_data_not_transport_errors() {
        for (status, error) in [
            ("succeeded", ""),
            ("failed", "供应商拒绝请求"),
            ("uncertain", "提交结果未知"),
        ] {
            let record = json!({"id":"task", "status":status, "error":error});
            assert_eq!(
                worker_result(&serde_json::to_vec(&record).unwrap()).unwrap(),
                record
            );
        }
        assert_eq!(
            worker_result(br#"{"error":"worker failed"}"#).unwrap_err(),
            "worker failed"
        );
        assert!(worker_result(br#"{"error":""}"#).is_err());
        assert!(worker_result(b"invalid JSON").is_err());
    }
    #[test]
    fn documented_chat_task_actions_reach_the_public_video_service() {
        let guide = include_str!("../resources/plugins/dsvideo-plugin/STUDIO.md");
        let shared_tasks = guide.split("## Shared tasks").nth(1).unwrap();
        let supported = public_actions();
        for line in shared_tasks.lines().filter(|line| line.starts_with('`')) {
            let (heading, _) = line
                .split_once(':')
                .expect("documented action must have a description");
            for action in heading.split('`').skip(1).step_by(2) {
                assert!(
                    supported.contains(&action),
                    "documented action {action} is not exposed"
                );
            }
        }
        for action in ["plan_result", "analysis_result", "prompt_result"] {
            assert!(
                WORKER_ACTIONS.contains(&action),
                "chat-authored results must reach the worker"
            );
        }
        for internal in [
            "comfy_workflow",
            "comfy_submitted",
            "comfy_complete",
            "uncertain",
        ] {
            assert!(
                !supported.contains(&internal),
                "internal bookkeeping must stay host-owned"
            );
        }
    }

    #[test]
    fn unknown_action_error_lists_recovery_options_without_sending_agents_to_source_code() {
        let error = unknown_action("convert_prompt");
        assert!(error.contains("convert_prompt"));
        assert!(error.contains("prompt_result"));
        assert!(error.contains("prepare"));
        assert!(error.contains("不要搜索或修改安装目录"));
    }

    #[test]
    fn studio_tool_advertises_all_public_video_operations() {
        let tool = crate::mcp::types::native_studio_tool();
        for action in public_actions() {
            assert!(
                tool.description.contains(action),
                "tool does not advertise {action}"
            );
        }
    }

    #[test]
    fn reference_analysis_preserves_user_focus_and_report_language() {
        let context = analysis_context(
            json!({"warnings":["missing audio"]}),
            &json!({"request":"focus on opening", "language":"zh-CN"}),
        );
        assert_eq!(context["request"], "focus on opening");
        assert_eq!(context["report_language"], "zh-CN");
        assert_eq!(context["evidence"]["warnings"][0], "missing audio");
    }

    #[test]
    fn revision_plan_keeps_current_script_and_user_notes() {
        let brief = json!({"request": "宣传书包", "duration": 5});
        assert!(planning_revision(None, Some("0–5秒：特写脸")).is_none());
        let (note, script) =
            planning_revision(Some(" 书包要完整入画 "), Some("0–5秒：特写脸")).unwrap();
        assert_eq!(note, "书包要完整入画");
        assert_eq!(script, "0–5秒：特写脸");
        let data = revise_context(script, note, &brief);
        assert_eq!(data["note"], "书包要完整入画");
        assert_eq!(data["script"], "0–5秒：特写脸");
        assert_eq!(data["brief"]["request"], "宣传书包");
    }
    #[test]
    fn builtin_video_plugin_resolves_six_skills_and_two_mcps() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins/dsvideo-plugin");
        let package = packages::Package {
            id: PACKAGE_ID.into(),
            name: "dsvideo".into(),
            description: String::new(),
            version: None,
            format: String::new(),
            source: "builtin:dsvideo".into(),
            revision: None,
            enabled: true,
            components: Default::default(),
            diagnostics: vec![],
        };
        let resolved = packages::resolve(&root, package, &root.join("unused-test-data")).unwrap();
        assert!(
            resolved.package.diagnostics.is_empty(),
            "{:?}",
            resolved.package.diagnostics
        );
        assert_eq!(resolved.package.components.get("skills"), Some(&6));
        assert_eq!(resolved.servers.len(), 2);
        assert!(resolved
            .servers
            .iter()
            .all(|s| !s.args.join(" ").contains("${")));
        for server in &resolved.servers {
            assert!(std::path::Path::new(&server.command).is_absolute());
            assert!(server.command.contains("video-runtime"));
            assert!(!server.args.iter().any(|arg| arg == "-y"));
            assert!(server.env.contains_key("DSVIDEO_RUNTIME_ROOT"));
        }
    }

    #[test]
    fn upgrade_replaces_old_launch_commands_without_resetting_user_switches() {
        use crate::settings::{ChatMcpServer, Settings};
        let mut settings = Settings::default();
        settings.chat_tools.servers.push(ChatMcpServer {
            id: "builtin-analyzer".into(),
            command: "npx".into(),
            args: vec!["-y".into()],
            enabled: false,
            enabled_tools: vec!["get_metadata".into()],
            env: [
                ("CUSTOM_ENDPOINT".into(), "http://localhost:8188".into()),
                ("PATH".into(), "old runtime".into()),
            ]
            .into(),
            ..Default::default()
        });
        settings.chat_tools.servers.push(ChatMcpServer {
            id: "user-server".into(),
            command: "user-command".into(),
            ..Default::default()
        });
        let new = ChatMcpServer {
            id: "builtin-analyzer".into(),
            command: "/moved app/node".into(),
            args: vec!["/moved app/analyzer.js".into()],
            enabled: true,
            env: [("PATH".into(), "new runtime".into())].into(),
            ..Default::default()
        };
        sync_builtin_servers(&mut settings, vec![new.clone()]);
        sync_builtin_servers(&mut settings, vec![new]);
        let server = &settings.chat_tools.servers[0];
        assert_eq!(server.command, "/moved app/node");
        assert!(!server.enabled);
        assert_eq!(server.enabled_tools, vec!["get_metadata"]);
        assert_eq!(server.env["PATH"], "new runtime");
        assert_eq!(server.env["CUSTOM_ENDPOINT"], "http://localhost:8188");
        assert_eq!(settings.chat_tools.servers.len(), 2);
        assert_eq!(settings.chat_tools.servers[1].command, "user-command");
    }
    #[test]
    fn reads_actual_comfy_cli_upload_and_submission_envelopes() {
        let raw = json!({"content":[{"type":"text","text":json!({"data":{"uploads":[{"cloud_name":"product_2.png","subfolder":""}]}}).to_string()}]});
        assert_eq!(
            find_string(&raw, "cloud_name").as_deref(),
            Some("product_2.png")
        );
        let raw =
            json!({"content":[{"type":"text","text":"{\"data\":{\"prompt_id\":\"remote-123\"}}"}]});
        assert_eq!(
            find_string(&raw, "prompt_id").as_deref(),
            Some("remote-123")
        );
        assert!(find_string(&json!({"error":"no receipt"}), "prompt_id").is_none());
    }
}

pub(crate) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
type PollGate = tokio::sync::Mutex<Option<tokio::time::Instant>>;
fn poll_gate(id: &str) -> std::sync::Arc<PollGate> {
    use std::sync::{Mutex, OnceLock};
    type Entry = (std::sync::Arc<PollGate>, std::time::Instant);
    static GATES: OnceLock<Mutex<std::collections::HashMap<String, Entry>>> = OnceLock::new();
    let mut gates = GATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // Retain recent gates after the last caller exits so a page/chat caller
    // arriving just afterwards still observes the shared three-second backoff.
    gates.retain(|_, (gate, used)| {
        std::sync::Arc::strong_count(gate) > 1 || used.elapsed().as_secs() < 60
    });
    let (gate, used) = gates.entry(id.into()).or_insert_with(|| {
        (
            std::sync::Arc::new(PollGate::new(None)),
            std::time::Instant::now(),
        )
    });
    *used = std::time::Instant::now();
    gate.clone()
}

pub(crate) fn task_snapshot(id: &str) -> Result<Value, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    let path = crate::app_data::app_data_dir()
        .ok_or("无数据目录")?
        .join("video-studio/tasks")
        .join(format!("{id}.json"));
    let bytes = std::fs::read(path).map_err(|_| "视频任务不存在或无法读取")?;
    let task: Value = serde_json::from_slice(&bytes).map_err(|_| "视频任务记录无效")?;
    if task["id"] != id {
        return Err("视频任务编号不匹配".into());
    }
    Ok(task)
}

pub(crate) async fn poll_task(app: AppHandle, id: &str) -> Result<Value, String> {
    plugin()?;
    let gate = poll_gate(id);
    let mut last = gate.lock().await;
    let current = task_snapshot(id)?;
    if current["status"] != "running" {
        return Ok(current);
    }
    if last.is_some_and(|at| at.elapsed() < POLL_INTERVAL) {
        return Ok(current);
    }
    let result = poll_unlocked(app, json!({"id":id,"revision":current["revision"]})).await;
    *last = Some(tokio::time::Instant::now());
    result
}

async fn poll_unlocked(app: AppHandle, input: Value) -> Result<Value, String> {
    let t = worker(&app, "poll", input.clone()).await?;
    if t["status"] != "running" || t["remote"]["route"] != "comfy" {
        return Ok(t);
    }
    let id = t["id"].as_str().ok_or("无任务编号")?;
    let remote_id = t["remote"]["id"].as_str().ok_or("无远程任务编号")?;
    if remote_id.is_empty()
        || !remote_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("远程任务编号无效".into());
    }
    let dir = crate::app_data::app_data_dir()
        .ok_or("无数据目录")?
        .join("video-studio/outputs")
        .join(id)
        // Every remote job gets its own directory; a remake must never
        // pick a previous render while fetching the new one.
        .join(remote_id);
    let status = mcp(
        &app,
        "comfy-mcp",
        "job",
        json!({"prompt_id":t["remote"]["id"],"action":"status"}),
        t["remote"]["base_url"].as_str(),
    )
    .await?;
    let status = find_string(&status, "status").unwrap_or_default();
    if matches!(status.as_str(), "error" | "cancelled") {
        return worker(
            &app,
            "comfy_failed",
            json!({"id":id,"revision":t["revision"]}),
        )
        .await;
    }
    if status != "completed" {
        return Ok(t);
    }
    mcp(
        &app,
        "comfy-mcp",
        "fetch_outputs",
        json!({"prompt_id":t["remote"]["id"],"out_dir":dir}),
        t["remote"]["base_url"].as_str(),
    )
    .await?;
    let video = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "mp4" | "webm" | "mov"))
        })
        .ok_or("视频尚未完成，请稍后继续查询")?;
    worker(
        &app,
        "comfy_complete",
        json!({"id":id,"revision":t["revision"],"output":video}),
    )
    .await
}

#[cfg(test)]
mod poll_gate_tests {
    use super::*;
    #[tokio::test]
    async fn page_and_chat_share_poll_lock_and_recent_backoff() {
        let id = uuid::Uuid::new_v4().to_string();
        let first = poll_gate(&id);
        let mut guard = first.lock().await;
        *guard = Some(tokio::time::Instant::now());
        let second = poll_gate(&id);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(second.try_lock().is_err());
        drop(guard);
        drop(first);
        drop(second);
        let later = poll_gate(&id);
        assert!(later
            .lock()
            .await
            .is_some_and(|at| at.elapsed() < POLL_INTERVAL));
    }
}
