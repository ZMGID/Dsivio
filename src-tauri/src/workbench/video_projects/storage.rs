//! Existing project files remain readable; no provider configuration or requests live here.
use super::*;
use sha2::{Digest, Sha256};
use std::fs;
pub(super) fn root() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无应用数据目录")?
        .join("video-studio"))
}
fn path(id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    Ok(root()?.join("tasks").join(format!("{id}.json")))
}
pub(super) fn lock(id: &str) -> Result<fs::File, String> {
    let path = path(id)?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|_| "任务正在处理，请稍后重试")?;
    Ok(file)
}
pub(super) fn read(id: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(
        &fs::read(path(id)?)
            .map_err(|_| "VIDEO_TASK_NOT_FOUND: 视频任务不存在，请从保留的草稿重新创建")?,
    )
    .map_err(|e| e.to_string())?;
    if value["id"] != id {
        return Err("任务编号不匹配".into());
    }
    Ok(value)
}
pub(super) fn save(mut task: Value) -> Result<Value, String> {
    task["revision"] = json!(task["revision"].as_u64().unwrap_or(0) + 1);
    task["updatedAt"] = json!(chrono::Utc::now().timestamp_millis());
    let id = task["id"].as_str().ok_or("缺少任务编号")?;
    let path = path(id)?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(&path, &task.to_string(), "video project")?;
    crate::studio::wait::changed("video", id);
    Ok(task)
}
fn list() -> Result<Vec<Value>, String> {
    let dir = root()?.join("tasks");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut result = vec![];
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        if entry.path().extension().is_some_and(|v| v == "json") {
            if let Ok(bytes) = fs::read(entry.path()) {
                if let Ok(t) = serde_json::from_slice::<Value>(&bytes) {
                    result.push(t)
                }
            }
        }
    }
    result.sort_by_key(|v| std::cmp::Reverse(v["updatedAt"].as_i64().unwrap_or(0)));
    Ok(result)
}
fn import_file(source: &str, extensions: &[&str], limit: u64) -> Result<String, String> {
    let path = Path::new(source);
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !extensions.contains(&extension.as_str())
        || fs::metadata(path).map_err(|e| e.to_string())?.len() > limit * 1024 * 1024
    {
        return Err(format!("参考素材格式不支持或超过 {limit} MB"));
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let dir = root()?.join("assets");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("{:x}.{extension}", Sha256::digest(&bytes)));
    if !target.exists() {
        fs::write(&target, bytes).map_err(|e| e.to_string())?;
    }
    Ok(target.to_string_lossy().into_owned())
}
async fn import_brief(mut brief: Value) -> Result<Value, String> {
    let original = brief["images"].as_array().cloned().unwrap_or_default();
    if original.len() > 16 {
        return Err("最多使用 16 张参考图".into());
    }
    let images = original
        .iter()
        .map(|v| {
            import_file(
                v.as_str().ok_or("无效图片路径")?,
                &["png", "jpg", "jpeg", "webp"],
                30,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    for field in ["firstFrame", "lastFrame"] {
        if let Some(value) = brief[field].as_str().filter(|v| !v.is_empty()) {
            let index = original
                .iter()
                .position(|v| v == value)
                .ok_or("首尾帧必须从已添加的图片中选择")?;
            brief[field] = json!(images[index]);
        }
    }
    brief["images"] = json!(images);
    for (field, extensions, limit) in [
        ("referenceVideos", vec!["mp4", "mov"], 50),
        ("referenceAudios", vec!["mp3", "wav"], 15),
    ] {
        let values = brief[field].as_array().cloned().unwrap_or_default();
        if values.len() > 3 {
            return Err("参考视频和音频分别最多 3 个".into());
        }
        if brief["route"] == "minimax" {
            let mut total = 0.0;
            for source in &values {
                let binary = runtime::root()?.join("bin").join(if cfg!(windows) {
                    "ffprobe.exe"
                } else {
                    "ffprobe"
                });
                let output = tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    tokio::process::Command::new(binary)
                        .args([
                            "-v",
                            "error",
                            "-show_entries",
                            "format=duration",
                            "-of",
                            "default=noprint_wrappers=1:nokey=1",
                        ])
                        .arg(source.as_str().ok_or("无效素材路径")?)
                        .kill_on_drop(true)
                        .output(),
                )
                .await
                .map_err(|_| "参考素材时长读取超时")?
                .map_err(|e| e.to_string())?;
                let duration = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| "无法读取参考素材时长")?;
                validate_duration(duration)?;
                total += duration;
            }
            if total > 15.01 {
                return Err("参考视频和音频各自总时长不能超过 15 秒".into());
            }
        }
        brief[field] = json!(values
            .iter()
            .map(|v| import_file(v.as_str().ok_or("无效素材路径")?, &extensions, limit))
            .collect::<Result<Vec<_>, String>>()?);
    }
    Ok(brief)
}
fn validate_duration(duration: f64) -> Result<(), String> {
    if !duration.is_finite() || !(2.0..=15.01).contains(&duration) {
        return Err("每段参考视频或音频需要 2–15 秒".into());
    }
    Ok(())
}
fn revision(task: &Value, input: &Value) -> Result<(), String> {
    if task["revision"] != input["revision"] {
        Err("任务已在其他窗口更新，请重新打开任务".into())
    } else {
        Ok(())
    }
}
pub(super) async fn project(_app: &AppHandle, action: &str, input: Value) -> Result<Value, String> {
    if action == "bootstrap" {
        return Ok(
            json!({"tasks":list()?,"templates":crate::content_templates::video_templates_list()?.into_iter().map(crate::content_templates::video::for_studio).collect::<Vec<_>>(),"root":root()?,"config":{},"configPath":"","dependencies":{}}),
        );
    }
    if action == "create" {
        return save(
            json!({"id":uuid::Uuid::new_v4().to_string(),"brief":import_brief(input["brief"].clone()).await?,"script":"","prompt":"","approved":false,"status":"draft"}),
        );
    }
    let mut t = read(input["id"].as_str().ok_or("缺少任务编号")?)?;
    if action == "get" {
        return Ok(t);
    }
    revision(&t, &input)?;
    if matches!(
        t["status"].as_str(),
        Some("running" | "submitting" | "uncertain")
    ) {
        return Err("任务已提交，请先查看或恢复结果")?;
    }
    match action {
        "preflight" => {
            if input["operation"] == "analyze"
                && t["brief"]["source"]
                    .as_str()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                return Err("请添加参考视频".into());
            }
            return Ok(t);
        }
        "save" => {
            clear_attempt(&mut t);
            t["brief"] = import_brief(input["brief"].clone()).await?;
            t["script"] = input["script"].clone();
            t["status"] = json!("draft");
            t["concepts"] = json!([]);
            t["approved"] = json!(false);
            t["prompt"] = json!("");
        }
        "plan_result" | "analysis_result" => {
            t["script"] = input["script"].clone();
            t["concepts"] = input.get("concepts").cloned().unwrap_or(json!([]));
            t["analysis"] = input["analysis"].clone();
            t["approved"] = json!(false);
            t["prompt"] = json!("");
            t["status"] = json!("draft");
        }
        "approve" => {
            if t["script"].as_str().unwrap_or("").trim().is_empty() {
                return Err("请先完成剧本".into());
            }
            t["approved"] = json!(true);
            t["prompt"] = t["script"].clone();
            t["status"] = json!("approved");
        }
        "prompt_result" => {
            if t["approved"] != true {
                return Err("请先确认当前剧本".into());
            }
            t["prompt"] = input["prompt"].clone();
        }
        "quote" => {
            t["quote"] = json!({"pricingStatus":"unknown","note":"费用以所选模型服务的实际计费为准。","at":chrono::Utc::now().timestamp()});
        }
        "template_prepare" => {
            let analysis = t["brief"]["mode"] == "analysis";
            if !analysis && (t["status"] != "succeeded" || input["approvedOutput"] != true) {
                return Err("生成模板须在确认成片后保存".into());
            }
            if t["script"].as_str().unwrap_or("").trim().is_empty() {
                return Err("没有可保存的剧本".into());
            }
            return Ok(
                json!({"name":input["name"],"kind":if analysis{"reference"}else{"generation"},"script":t["script"],"spec":template_spec(&t)}),
            );
        }
        _ => return Err(format!("未知视频项目操作：{action}")),
    }
    save(t)
}

// Keep prior receipts available without carrying them into the next paid attempt.
pub(super) fn clear_attempt(t: &mut Value) {
    if t.get("mediaTaskId").is_some() || t.get("remote").is_some() {
        let mut attempt = t.clone();
        attempt.as_object_mut().unwrap().remove("attempts");
        let mut attempts = t["attempts"].as_array().cloned().unwrap_or_default();
        attempts.push(attempt);
        t["attempts"] = json!(attempts);
    }
    for key in ["remote", "mediaTaskId", "output", "error", "submission", "quote", "canResume"] {
        t.as_object_mut().unwrap().remove(key);
    }
}

pub(super) fn retry(t: &mut Value) -> Result<(), String> {
    if t["status"] != "failed" || t["canResume"] == true {
        return Err("仅明确失败的任务可重新生成；运行中或结果未确认的任务请查询原结果".into());
    }
    clear_attempt(t);
    t["status"] = json!("approved");
    Ok(())
}

fn template_spec(t: &Value) -> Value {
    if t["brief"]["mode"] != "analysis" {
        return json!({"duration_seconds":t["brief"]["duration"],"aspect_ratio":t["brief"]["ratio"]});
    }
    let analysis = &t["analysis"];
    let mut candidates = Vec::new();
    if let Some(metadata) = analysis.get("sourceMetadata") { candidates.push(metadata.clone()); }
    if let Some(content) = analysis["content"].as_array() {
        candidates.extend(content.iter().filter(|item| item["type"] == "text").filter_map(|item| {
            let value: Value = serde_json::from_str(item["text"].as_str()?).ok()?;
            value.get("metadata").cloned()
        }));
    }
    let dimension = |value: &Value| value.as_u64().or_else(|| value.as_str()?.parse::<u64>().ok()).filter(|v| *v > 0);
    let metadata = candidates.into_iter().find(|m| dimension(&m["width"]).is_some() && dimension(&m["height"]).is_some());
    let Some(metadata) = metadata else { return json!({}) };
    let mut spec = json!({});
    if let (Some(width), Some(height)) = (dimension(&metadata["width"]), dimension(&metadata["height"])) {
        if width > 0 && height > 0 {
            let (mut a, mut b) = (width, height);
            while b != 0 { (a, b) = (b, a % b); }
            spec["aspect_ratio"] = json!(format!("{}:{}", width / a, height / a));
        }
    }
    if let Some(duration) = metadata["duration"].as_f64().filter(|d| d.is_finite() && *d > 0.0) {
        spec["source_duration_seconds"] = json!(duration);
    }
    if let Some(audio) = metadata["hasAudio"].as_bool() { spec["source_has_audio"] = json!(audio); }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_templates_preserve_source_metadata_instead_of_generation_defaults() {
        let metadata = json!({"width":1920,"height":1080,"duration":22.5,"hasAudio":true});
        let expected = json!({"aspect_ratio":"16:9","source_duration_seconds":22.5,"source_has_audio":true});
        for analysis in [json!({"sourceMetadata":metadata}), json!({"content":[{"type":"text","text":json!({"metadata":metadata}).to_string()}]})] {
            assert_eq!(template_spec(&json!({"brief":{"mode":"analysis","duration":10,"ratio":"9:16"},"analysis":analysis})), expected);
        }
        for analysis in [json!({}), json!({"sourceMetadata":null}), json!({"content":[{"text":"bad json"}]}), json!({"sourceMetadata":{"width":0,"height":0,"duration":0}})] {
            assert_eq!(template_spec(&json!({"brief":{"mode":"analysis","duration":10,"ratio":"9:16"},"analysis":analysis})), json!({}));
        }
        assert_eq!(template_spec(&json!({"brief":{"mode":"create","duration":6,"ratio":"9:16"}})), json!({"duration_seconds":6,"aspect_ratio":"9:16"}));
    }

    #[test]
    fn retry_preserves_failed_attempt_but_never_replays_active_or_uncertain_work() {
        let task = json!({"id":"project","status":"failed","mediaTaskId":"old","remote":{"id":"receipt"},"canResume":false,"approved":true,"prompt":"approved shot","error":"rejected","attempts":[{"mediaTaskId":"older"}]});
        let mut retried = task.clone();
        retry(&mut retried).unwrap();
        assert_eq!(retried["status"], "approved");
        assert_eq!(retried["prompt"], "approved shot");
        assert_eq!(retried["attempts"].as_array().unwrap().len(), 2);
        assert_eq!(retried["attempts"][1]["remote"]["id"], "receipt");
        assert!(retried["attempts"][1].get("attempts").is_none());
        for key in ["mediaTaskId", "remote", "error", "canResume"] { assert!(retried.get(key).is_none()); }
        for status in ["running", "submitting", "uncertain", "succeeded"] {
            let mut blocked = task.clone(); blocked["status"] = json!(status);
            let before = blocked.clone();
            assert!(retry(&mut blocked).is_err()); assert_eq!(blocked, before);
        }
        let mut resumable = task; resumable["canResume"] = json!(true);
        assert!(retry(&mut resumable).is_err());
    }

    #[test]
    fn stale_edits_and_invalid_durations_are_rejected() {
        assert!(revision(&json!({"revision":3}), &json!({"revision":2})).is_err());
        assert!(revision(&json!({"revision":3}), &json!({"revision":3})).is_ok());
        for duration in [0.0, 1.9, 16.0, f64::NAN, f64::INFINITY] {
            assert!(validate_duration(duration).is_err());
        }
        assert!(validate_duration(2.0).is_ok());
        assert!(validate_duration(15.0).is_ok());
    }
}
