//! Video UI and bundled chat plugin share the same Python workspace service.
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
        .map_err(|e| format!("内置视频运行环境无法启动，请重新安装 Dsivio：{e}"))?;
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
    let result: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "视频服务未返回有效结果，请检查 Python 版本")?;
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return Err(error.into());
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
    let t = worker(app, "get", input.clone()).await?;
    let id = t["id"].as_str().ok_or("无任务编号")?;
    let b = &t["brief"];
    let root = source(app)?;
    let mut images: Vec<(String, String)> = b["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|p| ("商品参考".into(), p.into()))
        .collect();
    for (_, path) in &mut images {
        *path = image_url(path)?;
    }
    let mut analysis = Value::Null;
    let (instruction, data, field, result_action) = if action == "plan" {
        (format!("{}\nFor a broad request without selectedConcept or template, return {{\"concepts\":[\"一句话拍法1\",\"一句话拍法2\",\"一句话拍法3\"]}} and no script. These must be genuinely different approaches. Otherwise return {{\"script\":\"完整中文剧本\"}}. Honor selectedConcept. Include 3–6 contiguous shots covering the requested duration, action, camera, sound/dialogue, continuity and ending. Honor the selected template, do not add CTA unless requested. Respect speechMode: auto follows the user request or reference, without assuming narration; an explicitly requested dialogue language overrides the default language. dialogue preserves supplied dialogue verbatim in its language; ambient has no speech; silent has no audio. Follow music requirements. Reference video/audio paths are conditioning inputs, not observed evidence: never invent their contents.",
            std::fs::read_to_string(root.join("skills/video-director/SKILL.md")).map_err(|e|e.to_string())?), b.clone(), "script", "plan_result")
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
        if t["approved"] != true {
            return Err("请先确认当前剧本".into());
        }
        let guide = if b["route"] == "grok" {
            "Convert the approved script into one English Grok video prompt; no H3-only section tags. Preserve verbatim dialogue and on-screen text in their original language. Respect speechMode and music. In reference mode use <IMAGE_0>, <IMAGE_1> in upload order and <AUDIO_0>, <AUDIO_1> for the selected voiceIds in order.".into()
        } else {
            let name = if b["inputMode"] == "frames"
                || (images.is_empty()
                    && b["referenceVideos"].as_array().is_none_or(|v| v.is_empty()))
            {
                "base-en.txt"
            } else {
                "ref-en.txt"
            };
            std::fs::read_to_string(root.join("skills/h3-prompt-writing/references").join(name))
                .map_err(|e| e.to_string())?
        };
        (format!("{guide}\nReturn {{\"prompt\":\"complete prompt\"}}. Convert ONLY this approved script. Do not add or remove shots or facts."),json!({"script":t["script"],"brief":b}),"prompt","prompt_result")
    };
    let result = agent::run_specialized(
        app,
        id,
        &StudioConfig::default(),
        &instruction,
        data,
        images,
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .await?;
    if action == "plan" {
        if let Some(concepts) = result["concepts"].as_array() {
            if concepts.len() != 3 || concepts.iter().any(|v| v.as_str().is_none_or(|s| s.trim().is_empty())) {
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
        Err(_) => {
            worker(
                app,
                if submitting {
                    "uncertain"
                } else {
                    "preflight_failed"
                },
                json!({"id":id,"revision":t["revision"]}),
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
                crate::dock::fs::dock_fs_open_path(base.to_string_lossy().into(), relative, None)
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
        "submit" => {
            let t = worker(&app, "submit", input).await?;
            if t["remote"]["route"] == "comfy" {
                submit_comfy(&app, t).await
            } else {
                Ok(t)
            }
        }
        "poll" => {
            let t = worker(&app, "poll", input.clone()).await?;
            if t["remote"]["route"] != "comfy" {
                return Ok(t);
            }
            let id = t["id"].as_str().ok_or("无任务编号")?;
            let dir = crate::app_data::app_data_dir()
                .ok_or("无数据目录")?
                .join("video-studio/outputs")
                .join(id);
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
        "bootstrap" | "get" | "create" | "save" | "approve" | "quote" | "template_save"
        | "template_import" | "install_comfy" | "recover" => worker(&app, &action, input).await,
        _ => Err("未知视频操作".into()),
    }
}

fn analysis_context(evidence: Value, brief: &Value) -> Value {
    json!({"evidence": evidence, "request": brief["request"], "report_language": brief["language"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_analysis_preserves_user_focus_and_report_language() {
        let context = analysis_context(json!({"warnings":["missing audio"]}), &json!({"request":"focus on opening", "language":"zh-CN"}));
        assert_eq!(context["request"], "focus on opening");
        assert_eq!(context["report_language"], "zh-CN");
        assert_eq!(context["evidence"]["warnings"][0], "missing audio");
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
