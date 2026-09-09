//! Task organization is separate from generation revisions and output files.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Mutex};

static LIBRARY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Organization {
    pub archived: bool,
    pub pinned: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationPatch {
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
}

#[tauri::command]
pub fn studio_task_library(
    domain: String,
    ids: Option<Vec<String>>,
    patch: Option<OrganizationPatch>,
) -> Result<BTreeMap<String, Organization>, String> {
    let root = crate::app_data::app_data_dir().ok_or("无法定位任务目录")?;
    library_at(&root, &domain, ids, patch)
}

fn library_at(root: &Path, domain: &str, ids: Option<Vec<String>>, patch: Option<OrganizationPatch>) -> Result<BTreeMap<String, Organization>, String> {
    if !matches!(domain, "image" | "video") { return Err("未知任务类型".into()); }
    let _guard = LIBRARY_LOCK.lock().map_err(|_| "任务管理暂不可用")?;
    let directory = root.join(format!("{domain}-studio"));
    let path = directory.join("task-library.json");
    let mut entries: BTreeMap<String, Organization> = if path.exists() {
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
    } else { BTreeMap::new() };
    let Some(patch) = patch else { return Ok(entries); };
    let ids = ids.ok_or("请选择任务")?;
    if ids.is_empty() || ids.len() > 200 { return Err("每次请选择 1–200 个任务".into()); }
    // Validate the entire batch before writing anything.
    for id in &ids {
        uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
        let task: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("tasks").join(format!("{id}.json"))).map_err(|_| "任务不存在，请刷新列表")?
        ).map_err(|_| "任务记录无法读取")?;
        if patch.archived == Some(true) && matches!(task["status"].as_str(), Some("running" | "submitting" | "uncertain")) {
            return Err("进行中或等待核查的任务不能归档，请先查看任务状态".into());
        }
    }
    for id in ids {
        let entry = entries.entry(id).or_default();
        if let Some(value) = patch.archived { entry.archived = value; }
        if let Some(value) = patch.pinned { entry.pinned = value; }
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(&path, &serde_json::to_string(&entries).map_err(|e| e.to_string())?, "task library")?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(root: &Path, domain: &str, id: &str, status: &str) {
        let directory = root.join(format!("{domain}-studio/tasks"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{id}.json")), serde_json::json!({"id":id,"status":status,"revision":7}).to_string()).unwrap();
    }
    #[test]
    fn organization_survives_reload_without_changing_generation_and_isolates_domains() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        task(root.path(), "video", &id, "succeeded");
        library_at(root.path(), "video", Some(vec![id.clone()]), Some(OrganizationPatch { archived: Some(true), pinned: Some(true) })).unwrap();
        let reloaded = library_at(root.path(), "video", None, None).unwrap();
        assert!(reloaded[&id].archived && reloaded[&id].pinned);
        assert!(library_at(root.path(), "image", None, None).unwrap().is_empty());
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(root.path().join(format!("video-studio/tasks/{id}.json"))).unwrap()).unwrap();
        assert_eq!(saved["revision"], 7);
        assert_eq!(saved["status"], "succeeded");
        let restored = library_at(root.path(), "video", Some(vec![id.clone()]), Some(OrganizationPatch { archived: Some(false), pinned: None })).unwrap();
        assert!(!restored[&id].archived && restored[&id].pinned);
    }
    #[test]
    fn invalid_batch_does_not_partially_archive_or_escape_the_workspace() {
        let root = tempfile::tempdir().unwrap();
        let ready = uuid::Uuid::new_v4().to_string();
        let running = uuid::Uuid::new_v4().to_string();
        task(root.path(), "image", &ready, "ready");
        task(root.path(), "image", &running, "running");
        for ids in [vec![ready.clone(), running], vec![ready, "../outside".into()]] {
            assert!(library_at(root.path(), "image", Some(ids), Some(OrganizationPatch { archived: Some(true), pinned: None })).is_err());
            assert!(library_at(root.path(), "image", None, None).unwrap().is_empty());
        }
    }
}
