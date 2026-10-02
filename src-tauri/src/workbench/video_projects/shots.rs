//! Multi-shot drama state. Creation and remake stay on the single project media task.
use serde_json::{json, Value};

pub(in crate::workbench::video_projects) fn ensure_approved(task: &Value) -> Result<(), String> {
    if task["approved"] != true {
        return Err("请先确认剧本；确认前不会提交付费生成".into());
    }
    Ok(())
}

fn heading_id(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("##")?.trim();
    let rest = rest.strip_prefix("镜头")?.trim();
    let id = rest.split_whitespace().next()?.trim();
    if id.is_empty() { None } else { Some(id.to_string()) }
}

pub(in crate::workbench::video_projects) fn parse_drama_shots(script: &str) -> Result<Vec<Value>, String> {
    let mut shots = Vec::new();
    let mut current: Option<String> = None;
    let mut body: Vec<&str> = Vec::new();
    let flush = |shots: &mut Vec<Value>, current: &mut Option<String>, body: &mut Vec<&str>| -> Result<(), String> {
        if let Some(id) = current.take() {
            let prompt = body.join("\n").trim().to_string();
            body.clear();
            if prompt.is_empty() {
                return Err(format!("镜头 {id} 缺少可生成的提示词"));
            }
            if shots.iter().any(|shot| shot["id"] == id) {
                return Err(format!("镜头编号重复：{id}"));
            }
            shots.push(json!({"id": id, "prompt": prompt, "status": "approved"}));
        }
        Ok(())
    };
    for line in script.lines() {
        if let Some(id) = heading_id(line) {
            flush(&mut shots, &mut current, &mut body)?;
            current = Some(id);
        } else if current.is_some() {
            body.push(line);
        }
    }
    flush(&mut shots, &mut current, &mut body)?;
    if shots.len() < 2 {
        return Err("短剧剧本需要至少两个以“## 镜头”开头的镜头".into());
    }
    Ok(shots)
}

fn frozen(shot: &Value) -> bool {
    shot["mediaTaskId"].as_str().is_some_and(|id| !id.is_empty())
        || shot["attempts"].as_array().is_some_and(|items| !items.is_empty())
}

pub(in crate::workbench::video_projects) fn has_frozen_shots(task: &Value) -> bool {
    task["shots"]
        .as_array()
        .is_some_and(|shots| shots.iter().any(frozen))
}

/// Script edits update only shots that have never been submitted.
pub(in crate::workbench::video_projects) fn apply_script_edit(task: &mut Value, script: &str) -> Result<(), String> {
    let parsed = parse_drama_shots(script)?;
    let existing = task["shots"].as_array().cloned().unwrap_or_default();
    let mut merged = Vec::new();
    let mut seen = Vec::new();
    for shot in parsed {
        let id = shot["id"].as_str().unwrap_or("").to_string();
        seen.push(id.clone());
        if let Some(previous) = existing.iter().find(|item| item["id"] == id) {
            if frozen(previous) {
                merged.push(previous.clone());
            } else {
                merged.push(shot);
            }
        } else {
            merged.push(shot);
        }
    }
    for previous in &existing {
        let id = previous["id"].as_str().unwrap_or("");
        if frozen(previous) && !seen.iter().any(|item| item == id) {
            merged.push(previous.clone());
        }
    }
    task["script"] = json!(script);
    task["shots"] = json!(merged);
    Ok(())
}

pub(in crate::workbench::video_projects) fn snapshot_shots_on_approve(task: &mut Value) -> Result<(), String> {
    let script = task["script"].as_str().unwrap_or("").to_string();
    apply_script_edit(task, &script)
}

pub(in crate::workbench::video_projects) fn drama_busy(task: &Value) -> bool {
    task["shots"].as_array().is_some_and(|shots| {
        shots.iter().any(|shot| {
            matches!(shot["status"].as_str(), Some("running" | "submitting" | "uncertain")) || shot["canResume"] == true
        })
    })
}

/// Clears only failed shots so they can be submitted again. Prompts stay as submitted.
pub(in crate::workbench::video_projects) fn prepare_shot_retry(task: &mut Value) -> Result<Vec<usize>, String> {
    ensure_approved(task)?;
    if drama_busy(task) {
        return Err("有镜头仍在生成或结果未确认，请先查询，不要重复提交".into());
    }
    let shots = task["shots"].as_array_mut().ok_or("没有可重试的镜头")?;
    let mut indexes = Vec::new();
    for (index, shot) in shots.iter_mut().enumerate() {
        if shot["status"] == "failed" && shot["canResume"] != true {
            let mut attempt = shot.clone();
            attempt.as_object_mut().unwrap().remove("attempts");
            let mut attempts = shot["attempts"].as_array().cloned().unwrap_or_default();
            attempts.push(attempt);
            for key in ["mediaTaskId", "remote", "output", "error", "canResume"] {
                shot.as_object_mut().unwrap().remove(key);
            }
            shot["attempts"] = json!(attempts);
            shot["status"] = json!("approved");
            indexes.push(index);
        }
    }
    if indexes.is_empty() {
        return Err("没有失败的镜头可重试".into());
    }
    task["status"] = json!("approved");
    task.as_object_mut().unwrap().remove("error");
    Ok(indexes)
}

pub(in crate::workbench::video_projects) fn prompts_for_paid_shots(task: &Value) -> Result<Vec<(usize, String)>, String> {
    ensure_approved(task)?;
    let shots = task["shots"].as_array().ok_or("请先确认分镜剧本")?;
    let mut jobs = Vec::new();
    for (index, shot) in shots.iter().enumerate() {
        if shot["mediaTaskId"].as_str().is_some_and(|id| !id.is_empty()) {
            continue;
        }
        if matches!(shot["status"].as_str(), Some("running" | "submitting" | "succeeded" | "uncertain" | "cancelled")) {
            continue;
        }
        let prompt = shot["prompt"].as_str().map(str::trim).filter(|prompt| !prompt.is_empty()).ok_or("镜头提示词为空")?;
        jobs.push((index, prompt.to_string()));
    }
    if jobs.is_empty() {
        return Err("没有待提交的镜头".into());
    }
    Ok(jobs)
}

pub(in crate::workbench::video_projects) fn derive_project_status(task: &mut Value) {
    let Some(shots) = task["shots"].as_array().cloned() else { return };
    if shots.is_empty() { return }
    let has = |name: &str| shots.iter().any(|shot| shot["status"] == name);
    let status = if has("submitting") || has("running") {
        "running"
    } else if has("uncertain") || shots.iter().any(|shot| shot["canResume"] == true) {
        "uncertain"
    } else if has("failed") {
        "failed"
    } else if shots.iter().all(|shot| shot["status"] == "succeeded") {
        "succeeded"
    } else if task["approved"] == true {
        "approved"
    } else {
        "draft"
    };
    task["status"] = json!(status);
    if status == "failed" {
        let errors = shots.iter().filter(|shot| shot["status"] == "failed").filter_map(|shot| shot["error"].as_str()).collect::<Vec<_>>().join("；");
        if !errors.is_empty() { task["error"] = json!(errors); }
    } else if status == "succeeded" {
        task.as_object_mut().unwrap().remove("error");
    }
}

pub(in crate::workbench::video_projects) fn drama_script_from_result(result: &Value) -> Result<String, String> {
    if let Some(error) = result["error"].as_str().filter(|error| !error.trim().is_empty()) {
        return Err(format!("无法编写短剧分镜：{}", error.trim()));
    }
    if let Some(script) = result["script"].as_str() {
        if parse_drama_shots(script).is_ok() {
            return Ok(script.trim().to_string());
        }
    }
    let shots = result["shots"].as_array().ok_or("短剧方案需要至少两个镜头")?;
    if shots.len() < 2 {
        return Err("短剧方案需要至少两个镜头".into());
    }
    let mut script = String::new();
    for (index, shot) in shots.iter().enumerate() {
        let id = shot["id"].as_str().map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned).unwrap_or_else(|| (index + 1).to_string());
        let prompt = shot["prompt"].as_str().map(str::trim).filter(|prompt| !prompt.is_empty()).ok_or("镜头提示词为空")?;
        script.push_str(&format!("## 镜头 {id}\n{prompt}\n\n"));
    }
    parse_drama_shots(&script)?;
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn drama() -> Value {
        json!({
            "id": "project",
            "approved": true,
            "status": "failed",
            "script": "## 镜头 1\n原提示一\n\n## 镜头 2\n原提示二\n",
            "prompt": "## 镜头 1\n原提示一\n\n## 镜头 2\n原提示二\n",
            "brief": {"mode": "drama"},
            "shots": [
                {"id": "1", "prompt": "原提示一", "status": "succeeded", "mediaTaskId": "media-1", "output": "/tmp/one.mp4"},
                {"id": "2", "prompt": "原提示二", "status": "failed", "mediaTaskId": "media-2", "error": "rejected"}
            ]
        })
    }

    #[test]
    fn approval_gate_rejects_paid_shot_requests() {
        let mut task = drama();
        task["approved"] = json!(false);
        assert!(prompts_for_paid_shots(&task).unwrap_err().contains("确认前不会提交"));
        assert!(prepare_shot_retry(&mut task).is_err());
        assert_eq!(task["shots"][1]["mediaTaskId"], "media-2");
    }

    #[test]
    fn script_edits_do_not_change_submitted_shots_and_retry_is_only_the_failed_one() {
        let mut task = drama();
        apply_script_edit(&mut task, "## 镜头 1\n改写一\n\n## 镜头 2\n改写二\n\n## 镜头 3\n全新镜头\n").unwrap();
        assert_eq!(task["shots"][0]["prompt"], "原提示一");
        assert_eq!(task["shots"][1]["prompt"], "原提示二");
        assert_eq!(task["shots"][0]["mediaTaskId"], "media-1");
        assert_eq!(task["shots"][2]["prompt"], "全新镜头");
        assert!(task["shots"][2].get("mediaTaskId").is_none());

        let indexes = prepare_shot_retry(&mut task).unwrap();
        assert_eq!(indexes, vec![1]);
        assert_eq!(task["shots"][0]["mediaTaskId"], "media-1");
        assert_eq!(task["shots"][0]["status"], "succeeded");
        assert!(task["shots"][1].get("mediaTaskId").is_none());
        assert_eq!(task["shots"][1]["prompt"], "原提示二");
        assert_eq!(task["shots"][1]["attempts"][0]["mediaTaskId"], "media-2");

        apply_script_edit(&mut task, "## 镜头 1\n再改一\n\n## 镜头 2\n再改二\n").unwrap();
        assert_eq!(task["shots"][0]["prompt"], "原提示一");
        assert_eq!(task["shots"][1]["prompt"], "原提示二");

        let jobs = prompts_for_paid_shots(&task).unwrap();
        let prompts = jobs.into_iter().map(|(_, prompt)| prompt).collect::<Vec<_>>();
        assert!(prompts.contains(&"原提示二".to_string()));
        assert!(!prompts.iter().any(|prompt| prompt.contains("再改") || prompt.contains("原提示一")));
    }

    #[test]
    fn removed_heading_keeps_a_submitted_shot() {
        let mut task = drama();
        apply_script_edit(&mut task, "## 镜头 2\n改写二\n\n## 镜头 3\n新镜头\n").unwrap();
        assert!(task["shots"].as_array().unwrap().iter().any(|shot| shot["id"] == "1" && shot["prompt"] == "原提示一"));
        assert!(task["shots"].as_array().unwrap().iter().any(|shot| shot["id"] == "2" && shot["prompt"] == "原提示二"));
    }

    #[test]
    fn drama_result_accepts_headings_or_a_shot_array_and_rejects_errors() {
        let script = drama_script_from_result(&json!({"script": "## 镜头 1\n开场\n\n## 镜头 2\n收尾\n"})).unwrap();
        assert!(script.contains("开场"));
        let built = drama_script_from_result(&json!({"shots": [{"id": "1", "prompt": "一"}, {"id": "2", "prompt": "二"}]})).unwrap();
        assert!(parse_drama_shots(&built).is_ok());
        assert!(drama_script_from_result(&json!({"error": "主体不清", "shots": [{"id":"1","prompt":"一"},{"id":"2","prompt":"二"}]})).is_err());
        assert!(parse_drama_shots("只有一段故事").is_err());
    }
}
