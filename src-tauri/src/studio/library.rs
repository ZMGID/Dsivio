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

#[tauri::command]
pub async fn studio_task_file_action(domain: String, id: String, action: String) -> Result<(), String> {
    let root = crate::app_data::app_data_dir().ok_or("无法定位任务目录")?;
    let target = tauri::async_runtime::spawn_blocking(move || task_file_action_at(&root, &domain, &id, &action))
        .await.map_err(|e| e.to_string())??;
    if let Some(path) = target {
        let parent = path.parent().ok_or("无法定位所在文件夹")?;
        crate::dock::fs::dock_fs_open_path(
            parent.to_string_lossy().into_owned(),
            path.file_name().ok_or("无效文件名")?.to_string_lossy().into_owned(),
            Some("reveal".into()),
        ).await?;
    }
    Ok(())
}

fn task_file_action_at(root: &Path, domain: &str, id: &str, action: &str) -> Result<Option<std::path::PathBuf>, String> {
    if !matches!(domain, "image" | "video") { return Err("未知任务类型".into()); }
    if !matches!(action, "reveal" | "delete") { return Err("未知任务操作".into()); }
    uuid::Uuid::parse_str(id).map_err(|_| "无效任务编号")?;
    let _guard = LIBRARY_LOCK.lock().map_err(|_| "任务管理暂不可用")?;
    // Use the writers' locks before reading status, and hold them through cleanup.
    let _image_guard = crate::image_studio::lock()?;
    let _draft_guard = super::DRAFT_LOCK.lock().map_err(|_| "草稿管理暂不可用")?;
    let directory = root.join(format!("{domain}-studio"));
    let path = directory.join("tasks").join(format!("{id}.json"));
    let _video_guard = if domain == "video" && action == "delete" {
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(path.with_extension("lock")).map_err(|e| e.to_string())?;
        // std uses flock on Unix and LockFileEx on Windows, interoperating with
        // studio.py's flock / msvcrt byte-range lock. Never unlink the lock file.
        file.try_lock().map_err(|_| "任务正在处理，请稍后再删除")?;
        Some(file)
    } else { None };
    let task: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).map_err(|_| "任务不存在，请刷新列表")?
    ).map_err(|_| "任务记录无法读取")?;
    if action == "reveal" {
        // Completed videos reveal their output; drafts reveal their saved task record.
        if domain == "video" {
            if let Some(output) = task["output"].as_str() {
                if let (Ok(output), Ok(base)) = (std::path::PathBuf::from(output).canonicalize(), directory.canonicalize()) {
                    if output.starts_with(base) && output.is_file() { return Ok(Some(output)); }
                }
            }
        }
        return Ok(Some(path));
    }
    if matches!(task["status"].as_str(), Some("running" | "submitting" | "uncertain")) {
        return Err("进行中或等待核查的任务不能删除，请先处理任务".into());
    }
    delete_task_files(root, &directory, &path, id, &task)?;
    Ok(None)
}

// Resolve only app-managed files; source files outside the workspace are never owned.
fn managed_references(value: &serde_json::Value, directory: &Path, out: &mut std::collections::BTreeSet<std::path::PathBuf>) {
    match value {
        serde_json::Value::String(raw) => {
            let mut path = std::path::PathBuf::from(raw);
            // Image result/template paths use a virtual outputs/<task-id>/ prefix.
            if let Some(relative) = raw.strip_prefix("outputs/") {
                if let Some((id, suffix)) = relative.split_once('/') {
                    if uuid::Uuid::parse_str(id).is_ok() {
                        if let Ok(bytes) = std::fs::read(directory.join("tasks").join(format!("{id}.json"))) {
                            if let Ok(task) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                                if let Some(output) = task["outputDirectory"].as_str() { path = Path::new(output).join(suffix); }
                            }
                        }
                    }
                }
            }
            let path = if path.is_absolute() { path } else { directory.join(path) };
            if let Ok(canonical) = path.canonicalize() {
                out.insert(canonical);
            }
        }
        serde_json::Value::Array(values) => for value in values { managed_references(value, directory, out); },
        serde_json::Value::Object(values) => for value in values.values() { managed_references(value, directory, out); },
        _ => {}
    }
}

fn shared_references(root: &Path, target: &Path) -> Result<std::collections::BTreeSet<std::path::PathBuf>, String> {
    let mut references = std::collections::BTreeSet::new();
    for domain in ["image", "video"] {
        let directory = root.join(format!("{domain}-studio"));
        for folder in ["tasks", "templates", "drafts"] {
            let folder = directory.join(folder);
            if !folder.exists() { continue; }
            let mut pending = vec![folder];
            while let Some(folder) = pending.pop() {
              for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                if kind.is_dir() { pending.push(path); continue; }
                if !kind.is_file() { continue; }
                if path == target || path.extension().is_none_or(|ext| ext != "json") { continue; }
                let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|_| "其他任务或模板无法读取，暂不删除以免误删共用文件")?;
                managed_references(&value, &directory, &mut references);
                // Image output references may use outputs/<task-id>/... rather than absolute paths.
                if let Some(output) = value["outputDirectory"].as_str() {
                    if let Ok(path) = Path::new(output).canonicalize() { references.insert(path); }
                }
              }
            }
        }
    }
    Ok(references)
}

fn remove_owned(path: &Path, shared: &std::collections::BTreeSet<std::path::PathBuf>) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    // Do not follow a link out of a task-owned directory.
    if metadata.file_type().is_symlink() { return Ok(()); }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if shared.iter().any(|reference| canonical == *reference || canonical.starts_with(reference)) { return Ok(()); }
    if metadata.is_dir() {
        for child in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            remove_owned(&child.map_err(|e| e.to_string())?.path(), shared)?;
        }
        if std::fs::read_dir(path).map_err(|e| e.to_string())?.next().is_none() {
            std::fs::remove_dir(path).map_err(|e| e.to_string())?;
        }
    } else {
        std::fs::remove_file(path).map_err(|e| format!("无法删除 {}：{e}", path.display()))?;
    }
    Ok(())
}

fn delete_task_files(root: &Path, directory: &Path, record: &Path, id: &str, task: &serde_json::Value) -> Result<(), String> {
    let shared = shared_references(root, record)?;
    let mut candidates = std::collections::BTreeSet::new();
    managed_references(task, directory, &mut candidates);
    let base = directory.canonicalize().map_err(|e| e.to_string())?;
    // Shared assets use content hashes, whereas intermediate files live under a task ID.
    candidates.retain(|path| {
        path.is_file() && ["assets", "results", "outputs", "exports"].iter().any(|folder| path.starts_with(base.join(folder)))
    });
    for folder in ["outputs", "results", "exports"] {
        let owned = directory.join(folder).join(id);
        if owned.is_dir() && !std::fs::symlink_metadata(&owned).map_err(|e| e.to_string())?.file_type().is_symlink() {
            let canonical = owned.canonicalize().map_err(|e| e.to_string())?;
            if canonical.starts_with(&base) { candidates.insert(canonical); }
        }
    }
    if let Some(output) = task["outputDirectory"].as_str() {
        let output = Path::new(output);
        // ImageStudio creates immutable directories ending in the task UUID.
        if output.exists() {
            if !output.file_name().is_some_and(|name| name.to_string_lossy().ends_with(&format!("__{id}")))
                || std::fs::symlink_metadata(output).map_err(|e| e.to_string())?.file_type().is_symlink() {
                return Err("图片输出目录无法确认归属，未删除任务".into());
            }
            candidates.insert(output.canonicalize().map_err(|e| e.to_string())?);
        }
    }
    // Read metadata before touching files, so a corrupt index cannot cause partial cleanup.
    let organization_path = directory.join("task-library.json");
    let mut organization: BTreeMap<String, Organization> = if organization_path.exists() {
        serde_json::from_slice(&std::fs::read(&organization_path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?
    } else { BTreeMap::new() };
    for path in candidates { remove_owned(&path, &shared)?; }
    organization.remove(id);
    if organization_path.exists() {
        crate::chat::storage::atomic_write(&organization_path, &serde_json::to_string(&organization).map_err(|e| e.to_string())?, "task library")?;
    }
    // Delete the record last: failed cleanup remains visible and can be retried.
    std::fs::remove_file(record).map_err(|e| e.to_string())?;
    Ok(())
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
    fn deletion_preserves_assets_referenced_by_shared_drafts_and_nested_templates() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        task(root.path(), "image", &id, "ready");
        let base = root.path().join("image-studio");
        std::fs::create_dir_all(base.join("assets")).unwrap();
        for name in ["draft.png", "template.png", "unused.png"] {
            std::fs::write(base.join("assets").join(name), b"image").unwrap();
        }
        let record = base.join(format!("tasks/{id}.json"));
        std::fs::write(&record, serde_json::json!({"status":"ready", "images":["assets/draft.png", "assets/template.png", "assets/unused.png"]}).to_string()).unwrap();
        std::fs::create_dir_all(base.join("drafts")).unwrap();
        std::fs::write(base.join("drafts/main.json"), r#"{"revision":1,"value":{"brief":{"images":["assets/draft.png"]}}}"#).unwrap();
        std::fs::create_dir_all(base.join("templates/set")).unwrap();
        std::fs::write(base.join("templates/set/template.json"), r#"{"image":"assets/template.png"}"#).unwrap();
        task_file_action_at(root.path(), "image", &id, "delete").unwrap();
        assert!(!record.exists());
        assert!(base.join("assets/draft.png").exists());
        assert!(base.join("assets/template.png").exists());
        assert!(!base.join("assets/unused.png").exists());
    }

    #[test]
    fn deletion_rejects_a_video_task_locked_by_a_worker() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        task(root.path(), "video", &id, "approved");
        let record = root.path().join(format!("video-studio/tasks/{id}.json"));
        let lock = std::fs::File::create(record.with_extension("lock")).unwrap();
        lock.lock().unwrap();
        assert!(task_file_action_at(root.path(), "video", &id, "delete").unwrap_err().contains("任务正在处理"));
        assert!(record.exists());
        drop(lock);
        task_file_action_at(root.path(), "video", &id, "delete").unwrap();
        assert!(!record.exists());
    }
    #[test]
    fn file_actions_reveal_drafts_and_delete_only_idle_records() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        task(root.path(), "video", &id, "running");
        let path = root.path().join(format!("video-studio/tasks/{id}.json"));
        assert_eq!(task_file_action_at(root.path(), "video", &id, "reveal").unwrap(), Some(path.clone()));
        assert!(task_file_action_at(root.path(), "video", &id, "delete").is_err());
        assert!(path.exists());
        assert!(task_file_action_at(root.path(), "video", "../outside", "delete").is_err());
        task(root.path(), "video", &id, "draft");
        let output = root.path().join("video-studio/output.mp4");
        std::fs::write(&output, b"keep").unwrap();
        task_file_action_at(root.path(), "video", &id, "delete").unwrap();
        assert!(!path.exists());
        assert!(output.exists());
        assert!(!root.path().join("video-studio/deleted-tasks").exists());
    }
    #[test]
    fn deletes_outputs_and_exclusive_assets_but_keeps_shared_and_original_files() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let base = root.path().join("video-studio");
        let output = base.join("outputs").join(&id);
        std::fs::create_dir_all(&output).unwrap();
        std::fs::create_dir_all(base.join("assets")).unwrap();
        let own = base.join("assets/own.png");
        let shared = base.join("assets/shared.png");
        let original = root.path().join("original.png");
        for path in [&own, &shared, &original, &output.join("video.mp4"), &output.join("workflow.json")] {
            std::fs::write(path, b"test").unwrap();
        }
        task(root.path(), "video", &id, "succeeded");
        task(root.path(), "video", &other, "draft");
        let record = base.join(format!("tasks/{id}.json"));
        std::fs::write(&record, serde_json::json!({"id":id,"status":"succeeded","output":output.join("video.mp4"),"brief":{"images":[own,shared,original]}}).to_string()).unwrap();
        std::fs::write(base.join(format!("tasks/{other}.json")), serde_json::json!({"id":other,"status":"draft","brief":{"images":[shared]}}).to_string()).unwrap();
        task_file_action_at(root.path(), "video", &id, "delete").unwrap();
        assert!(!record.exists());
        assert!(!output.exists());
        assert!(!own.exists());
        assert!(shared.exists());
        assert!(original.exists());
        task_file_action_at(root.path(), "video", &other, "delete").unwrap();
        assert!(!shared.exists());
    }
    #[test]
    fn deletes_custom_image_output_folder_and_preserves_template_assets() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let base = root.path().join("image-studio");
        let output = root.path().join(format!("Pictures/shoes__{id}"));
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("generated.png"), b"test").unwrap();
        std::fs::create_dir_all(base.join("assets")).unwrap();
        std::fs::write(base.join("assets/shared.png"), b"test").unwrap();
        std::fs::create_dir_all(base.join("templates")).unwrap();
        std::fs::write(base.join("templates/test.json"), r#"{"image":"assets/shared.png"}"#).unwrap();
        task(root.path(), "image", &id, "ready");
        std::fs::write(base.join(format!("tasks/{id}.json")), serde_json::json!({"id":id,"status":"ready","outputDirectory":output,"image":"assets/shared.png"}).to_string()).unwrap();
        task_file_action_at(root.path(), "image", &id, "delete").unwrap();
        assert!(!output.exists());
        assert!(base.join("assets/shared.png").exists());
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
