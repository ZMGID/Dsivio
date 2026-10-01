//! Dsvideo project registration and first-use preparation; UI and CLI share this owner.
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::{Path, PathBuf}, process::Stdio};
use tauri::AppHandle;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub version: u32,
    pub current: Option<String>,
    pub projects: Vec<Project>,
    #[serde(default)]
    pub document: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub initialized_at: i64,
    pub last_used_at: i64,
    #[serde(default)]
    pub available: bool,
}
fn root() -> Result<PathBuf, String> {
    let root = crate::plugins::packages::package_data_dir(super::dsvideo::PACKAGE_ID)?;
    let legacy = crate::app_data::app_data_dir().ok_or("无法定位应用数据目录")?.join("dsvideo");
    migrate(&legacy, &root)?;
    Ok(root)
}
fn migrate(legacy: &Path, root: &Path) -> Result<(), String> {
    if !legacy.exists() { return Ok(()); }
    let lock = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(legacy.join("projects.lock")).map_err(|e| e.to_string())?;
    lock.try_lock().map_err(|_| "Dsvideo 项目登记正在使用，请稍后重试".to_string())?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let target_lock = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join("projects.lock")).map_err(|e| e.to_string())?;
    target_lock.try_lock().map_err(|_| "Dsvideo 项目登记正在使用，请稍后重试".to_string())?;
    for name in ["projects.json", "PROJECTS.md"] {
        if legacy.join(name).exists() && root.join(name).exists() {
            return Err(format!("Dsvideo 新旧登记文件同时存在，请检查：{} 与 {}", legacy.display(), root.display()));
        }
    }
    for name in ["projects.json", "PROJECTS.md"] {
        let source = legacy.join(name);
        if !source.exists() { continue; }
        let target = root.join(name);
        if target.exists() { return Err(format!("Dsvideo 新旧登记文件同时存在，请检查：{} 与 {}", source.display(), target.display())); }
        fs::rename(source, target).map_err(|e| e.to_string())?;
    }
    drop(lock);
    fs::remove_file(legacy.join("projects.lock")).map_err(|e| e.to_string())?;
    // Remove only the empty former state directory; preserve unrelated files.
    let _ = fs::remove_dir(legacy);
    Ok(())
}
fn write(path: &Path, text: &str) -> Result<(), String> {
    crate::chat::storage::atomic_write(path, text, "Dsvideo project state")
}
fn read(root: &Path) -> Result<Registry, String> {
    let file = root.join("projects.json");
    let mut registry: Registry = if file.exists() {
        serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?).map_err(|e| format!("Dsvideo 项目登记损坏：{e}"))?
    } else { Registry { version: 1, ..Default::default() } };
    if registry.version != 1 { return Err("不支持的 Dsvideo 项目登记版本".into()); }
    registry.document = root.join("PROJECTS.md").display().to_string();
    for project in &mut registry.projects { project.available = ready(Path::new(&project.path)); }
    Ok(registry)
}
fn ready(path: &Path) -> bool {
    let Ok(selection) = fs::read_to_string(path.join(".dsvideo/runtime")) else { return false; };
    let profile = path.join(selection.trim());
    fs::read(profile).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value["format"] == "dsvideo.runtime-local@1")
}
fn save(root: &Path, registry: &Registry) -> Result<(), String> {
    write(&root.join("projects.json"), &serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?)?;
    let mut doc = "# Dsvideo 项目登记\n\n由 Dsivio 维护。通过 `dsivio dsvideo projects list/init/use` 登记和切换；不要直接编辑本文件。\n项目内的 `DSVIDEO_STATE.md` 由创作过程持续维护。\n\n".to_string();
    for project in &registry.projects {
        let current = registry.current.as_deref() == Some(&project.id);
        doc.push_str(&format!("## {}{}\n\n- ID：{}\n- 目录：{}\n- 已初始化：{}\n- 最近使用：{}\n\n", project.name.replace(['\n', '\r'], " "), if current { "（当前）" } else { "" }, project.id, project.path.replace(['\n','\r'], " "), project.initialized_at, project.last_used_at));
    }
    write(&root.join("PROJECTS.md"), &doc)
}
fn prepare(path: &Path) -> Result<(), String> {
    let already_ready = ready(path);
    if !already_ready && path.join(".dsvideo/runtime").exists() { return Err("项目原 Runtime 不可用，请修复或明确选择已有 Profile；不会覆盖它".into()); }
    if !path.join("package.json").exists() {
        write(&path.join("package.json"), "{\"name\":\"dsvideo-project\",\"private\":true,\"type\":\"module\"}\n")?;
    }
    if already_ready { return Ok(()); }
    let resources = super::runtime::tools_resource_directory()?;
    let profile = path.join("dsvideo.runtime.json");
    let args = if profile.exists() {
        vec!["runtime".into(), "use".into(), profile.into_os_string(), "--workspace".into(), path.as_os_str().to_owned()]
    } else {
        vec!["runtime".into(), "init".into(), "--workspace".into(), path.as_os_str().to_owned()]
    };
    let mut command = super::dsvideo::command_at(&resources, args.into_iter())?;
    let mut child = command.current_dir(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status; }
        if std::time::Instant::now() >= deadline { let _ = child.kill(); let _ = child.wait(); return Err("项目初始化超时；未登记完成，请检查后重试".into()); }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    if !status.success() {
        let mut message = String::new();
        if let Some(stderr) = child.stderr { let _ = stderr.take(8192).read_to_string(&mut message); }
        return Err(format!("项目初始化失败：{message}"));
    }
    if !ready(path) { return Err("项目 Runtime 尚未准备完成".into()); }
    Ok(())
}
fn operate(root: &Path, action: &str, path: Option<&str>, name: Option<&str>, prepare: impl FnOnce(&Path) -> Result<(), String>) -> Result<Registry, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let lock = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join("projects.lock")).map_err(|e| e.to_string())?;
    lock.try_lock().map_err(|_| "Dsvideo 项目登记正在使用，请稍后重试".to_string())?;
    let mut registry = read(root)?;
    if action == "list" { save(root, &registry)?; return Ok(registry); }
    if action != "init" && action != "use" { return Err("支持 projects list/init/use".into()); }
    let path = PathBuf::from(path.filter(|s| !s.trim().is_empty()).ok_or("请指定项目目录")?);
    if !path.is_absolute() { return Err("请使用项目目录的绝对路径".into()); }
    if action == "init" { fs::create_dir_all(&path).map_err(|e| e.to_string())?; }
    let path = fs::canonicalize(path).map_err(|e| format!("项目目录不可用：{e}"))?;
    let path = crate::utils::strip_windows_verbatim_prefix(path);
    let index = registry.projects.iter().position(|p| Path::new(&p.path) == path);
    if action == "use" && index.is_none() { return Err("此目录尚未登记，请先 projects init".into()); }
    prepare(&path)?;
    // Never overwrite creative notes on repeated setup or switching.
    let state = path.join("DSVIDEO_STATE.md");
    if !state.exists() {
        write(&state, "# Dsvideo 项目状态\n\n## 创作目标\n待确认。\n\n## 当前进度\n项目已初始化，尚未开始制作。\n\n## 素材与成果\n记录输入素材、剧本、编排文件及成片路径。构建结果默认保存在 `.dsvideo/results/`。\n\n## 待办与已确认决定\n每次制作或交接后更新；不要在这里保存密钥。\n")?;
    }
    let now = chrono::Utc::now().timestamp();
    let index = match index {
        Some(index) => index,
        None => {
            let name = name.filter(|s| !s.trim().is_empty()).map(str::trim).map(str::to_owned)
                .unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().into_owned());
            registry.projects.push(Project { id: uuid::Uuid::new_v4().to_string(), name, path: path.display().to_string(), initialized_at: now, last_used_at: now, available: true });
            registry.projects.len() - 1
        }
    };
    registry.projects[index].last_used_at = now;
    registry.projects[index].available = true;
    registry.current = Some(registry.projects[index].id.clone());
    save(root, &registry)?;
    Ok(registry)
}
pub fn cli(args: impl Iterator<Item = std::ffi::OsString>) -> std::process::ExitCode {
    let args: Vec<_> = args.collect();
    let result = (|| {
        let action = args.first().and_then(|s| s.to_str()).unwrap_or("list");
        let mut path = None; let mut name = None; let mut i = 1;
        while i < args.len() {
            let key = args[i].to_str().ok_or("参数需为 UTF-8")?;
            if key == "--json" { i += 1; continue; }
            let value = args.get(i + 1).and_then(|s| s.to_str()).ok_or("参数缺少值")?;
            match key { "--path" => path = Some(value), "--name" => name = Some(value), _ => return Err("用法：dsivio dsvideo projects list | init/use --path <绝对目录> [--name <名称>]".into()) }
            i += 2;
        }
        operate(&root()?, action, path, name, prepare)
    })();
    match result {
        Ok(registry) => { println!("{}", serde_json::to_string_pretty(&registry).unwrap()); std::process::ExitCode::SUCCESS }
        Err(error) => { eprintln!("{error}"); std::process::ExitCode::from(1) }
    }
}
#[tauri::command]
pub async fn dsvideo_projects(action: String, path: Option<String>, name: Option<String>) -> Result<serde_json::Value, String> {
    let registry = tauri::async_runtime::spawn_blocking(move || operate(&root()?, &action, path.as_deref(), name.as_deref(), prepare)).await.map_err(|e| e.to_string())??;
    Ok(serde_json::to_value(registry).map_err(|e| e.to_string())?)
}
#[tauri::command]
pub async fn dsvideo_project_bind(app: AppHandle, path: String) -> Result<serde_json::Value, String> {
    let registry = tauri::async_runtime::spawn_blocking(move || operate(&root()?, "use", Some(&path), None, prepare)).await.map_err(|e| e.to_string())??;
    let record = registry.projects.iter().find(|p| Some(&p.id) == registry.current.as_ref()).ok_or("没有当前项目")?;
    let existing = crate::chat::storage::get_projects(&app)?.into_iter().find(|p| p.root_path.as_deref().is_some_and(|root| fs::canonicalize(root).ok().map(crate::utils::strip_windows_verbatim_prefix).as_deref() == Some(Path::new(&record.path))));
    let project = if let Some(project) = existing { project } else {
        let now = chrono::Local::now().timestamp();
        crate::chat::storage::create_project_with_options(&app, crate::chat::ChatProject {
            id: format!("proj_{}", uuid::Uuid::new_v4()), name: format!("Dsvideo · {} · {}", record.name, &record.id[..8]),
            description: Some("Dsvideo 视频创作项目".into()), color: None, root_path: Some(record.path.clone()), created_at: now, updated_at: now,
        }, true)?
    };
    Ok(serde_json::json!({"id":project.id,"name":project.name,"rootPath":project.root_path}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(path: &Path) -> Result<(), String> {
        fs::create_dir_all(path.join(".dsvideo")).unwrap();
        fs::write(path.join("dsvideo.runtime.json"), r#"{"format":"dsvideo.runtime-local@1","credentials":{},"endpoints":{}}"#).unwrap();
        fs::write(path.join(".dsvideo/runtime"), "dsvideo.runtime.json\n").unwrap();
        Ok(())
    }
    #[test]
    fn multiple_projects_persist_and_switch_without_overwriting_creative_state() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("registry");
        let a = temp.path().join("project A"); let b = temp.path().join("project B");
        operate(&root, "init", a.to_str(), Some("A"), fixture).unwrap();
        fs::write(a.join("DSVIDEO_STATE.md"), "User-approved script and progress").unwrap();
        let second = operate(&root, "init", b.to_str(), Some("B"), fixture).unwrap();
        assert_eq!(second.projects.len(), 2);
        let selected = operate(&root, "use", a.to_str(), None, |_| Ok(())).unwrap();
        assert_eq!(selected.current, Some(selected.projects[0].id.clone()));
        assert_eq!(fs::read_to_string(a.join("DSVIDEO_STATE.md")).unwrap(), "User-approved script and progress");
        let again = operate(&root, "init", a.to_str(), None, |_| Ok(())).unwrap();
        assert_eq!(again.projects.len(), 2);
        let doc = fs::read_to_string(root.join("PROJECTS.md")).unwrap();
        assert!(doc.contains(a.to_str().unwrap()) && doc.contains(b.to_str().unwrap()));
        fs::remove_dir_all(b).unwrap();
        assert!(!operate(&root, "list", None, None, |_| Ok(())).unwrap().projects[1].available);
    }
    #[test]
    fn failed_setup_is_not_registered_and_cannot_be_selected() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("registry"); let project = temp.path().join("broken");
        assert!(operate(&root, "init", project.to_str(), None, |_| Err("setup failed".into())).is_err());
        assert!(read(&root).unwrap().projects.is_empty());
        assert!(operate(&root, "use", project.to_str(), None, |_| Ok(())).is_err());
        assert!(!project.join("DSVIDEO_STATE.md").exists());
    }
    #[test]
    fn lock_contention_does_not_change_registration() {
        let temp = tempfile::tempdir().unwrap();
        let lock = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(temp.path().join("projects.lock")).unwrap();
        lock.lock().unwrap();
        assert!(operate(temp.path(), "list", None, None, |_| Ok(())).is_err());
    }
    #[test]
    fn migration_preserves_projects_and_refreshes_document_location() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("dsvideo");
        let root = temp.path().join("plugin/data");
        let project = temp.path().join("creative project");
        let original = operate(&legacy, "init", project.to_str(), Some("创作"), fixture).unwrap();
        migrate(&legacy, &root).unwrap();
        let migrated = operate(&root, "list", None, None, |_| Ok(())).unwrap();
        assert_eq!(migrated.current, original.current);
        assert_eq!(migrated.projects[0].id, original.projects[0].id);
        assert_eq!(migrated.document, root.join("PROJECTS.md").display().to_string());
        assert!(!legacy.exists());
        migrate(&legacy, &root).unwrap();
    }
}

#[cfg(test)]
mod bundled_smoke {
    use super::*;
    #[test]
    #[ignore = "requires the prepared bundled Dsvideo runtime"]
    fn real_bundled_cli_initializes_and_preserves_existing_project() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("registry");
        let project = temp.path().join("real project with spaces");
        let first = operate(&root, "init", project.to_str(), Some("真实项目"), prepare).unwrap();
        assert!(first.projects[0].available);
        let profile = fs::read_to_string(project.join("dsvideo.runtime.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&profile).unwrap();
        assert_eq!(value["credentials"], serde_json::json!({}));
        assert_eq!(value["endpoints"]["dsivio.media"]["use"], "@dsvideo/provider-dsivio");
        fs::write(project.join("DSVIDEO_STATE.md"), "已有剧本与进度").unwrap();
        fs::remove_file(project.join(".dsvideo/runtime")).unwrap();
        operate(&root, "init", project.to_str(), None, prepare).unwrap();
        assert_eq!(fs::read_to_string(project.join("dsvideo.runtime.json")).unwrap(), profile);
        assert_eq!(fs::read_to_string(project.join("DSVIDEO_STATE.md")).unwrap(), "已有剧本与进度");
        assert_eq!(read(&root).unwrap().projects.len(), 1);
    }
}
