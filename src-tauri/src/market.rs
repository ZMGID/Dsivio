//! Curated market. Built-in integrations install their components directly;
//! their setup Skills check environments after installation.
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

const DEFAULT_CATALOG: &str =
    "https://raw.githubusercontent.com/ZMGID/Dsivio/main/packages/catalog.json";

#[derive(Clone, Deserialize)]
struct ProjectSpec {
    name: String,
    dir: String,
}

#[derive(Clone)]
struct BuiltIn {
    id: String,
    name: String,
    skill_id: String,
    summary: String,
    welcome: String,
    input_hint: String,
    start_prompt: String,
    setup: String,
    entry: Option<String>,
    /// Text of the shared Dsivio reference, present only when this plugin's setup reads it.
    dsivio_reference: Option<String>,
    /// Dsivio adapter shipped next to the setup Skill (`adapter/...`), present only when the setup uses it.
    adapter_files: Vec<(String, String)>,
    project: Option<ProjectSpec>,
    project_prompt: Option<String>,
    icon: String,
    icon_path: String,
    required_files: Vec<String>,
    repository: String,
    revision: String,
    subdir: Option<String>,
    skills: Vec<String>,
    category_ids: Vec<String>,
    unpack: String,
    skip: Vec<String>,
    preset_plugin_id: Option<String>,
}

struct Catalog {
    categories: Vec<(String, String)>,
    plugins: Vec<BuiltIn>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFile {
    categories: Vec<CatalogCategory>,
    plugins: Vec<CatalogPlugin>,
}

#[derive(Deserialize)]
struct CatalogCategory {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogPlugin {
    id: String,
    name: String,
    skill_id: String,
    summary: String,
    welcome: String,
    input_hint: String,
    start_prompt: String,
    setup: String,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    project: Option<ProjectSpec>,
    /// Text file added to the system prompt of conversations in this plugin's dedicated project.
    #[serde(default)]
    project_prompt: Option<String>,
    icon: String,
    icon_path: String,
    #[serde(default)]
    required_files: Vec<String>,
    repository: String,
    revision: String,
    #[serde(default)]
    subdir: Option<String>,
    #[serde(default)]
    skills: Vec<String>,
    category_ids: Vec<String>,
    #[serde(default = "default_unpack")]
    unpack: String,
    #[serde(default)]
    skip: Vec<String>,
    #[serde(default)]
    preset_plugin_id: Option<String>,
}

fn default_unpack() -> String { "skills".into() }

fn validate_subdir(subdir: &str) -> Result<(), String> {
    validate_archive_relative_path(subdir)
}

fn validate_archive_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("Skill 压缩包包含不安全路径".into());
    }
    Ok(())
}

fn catalog_text(dir: &Path, relative: &str) -> Result<String, String> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.split(['/', '\\']).any(|part| part.is_empty() || part == "." || part == "..")
    { return Err(format!("目录文件路径无效：{relative}")); }
    fs::read_to_string(dir.join(relative)).map_err(|e| format!("无法读取 {relative}：{e}"))
}

/// Shared reference about Dsivio. Only setups that ask to read it get a copy,
/// installed next to their SKILL.md.
const DSIVIO_REFERENCE_SOURCE: &str = "_shared/dsivio.md";
const DSIVIO_REFERENCE_PATH: &str = "references/dsivio.md";
/// A setup Skill that mentions this path ships the adapter found under `<setup dir>/adapter/` in the catalog.
const ADAPTER_PATH: &str = "adapter/";

fn adapter_files(dir: &Path, setup: &str, plugin_setup: &str) -> Result<Vec<(String, String)>, String> {
    if !setup.contains(ADAPTER_PATH) { return Ok(Vec::new()); }
    let root = Path::new(plugin_setup).parent().ok_or("适配器目录无效")?.join("adapter");
    let mut files = Vec::new();
    let mut pending = vec![dir.join(&root)];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).map_err(|e| format!("无法读取适配器目录：{e}"))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let relative = path.strip_prefix(dir.join(&root)).map_err(|e| e.to_string())?;
            // Tests live next to the source in the repository and are not installed.
            if relative.components().next().is_some_and(|c| c.as_os_str() == "test") { continue; }
            if path.is_dir() { pending.push(path); continue; }
            let relative = relative.to_string_lossy().replace('\\', "/");
            files.push((relative, fs::read_to_string(&path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?));
        }
    }
    if files.is_empty() { return Err("适配器目录没有文件".into()); }
    files.sort();
    Ok(files)
}

fn load_catalog_from(dir: &Path) -> Result<Catalog, String> {
    let file: CatalogFile = serde_json::from_str(&catalog_text(dir, "catalog.json")?).map_err(|e| format!("插件目录无效：{e}"))?;
    let reference = catalog_text(dir, DSIVIO_REFERENCE_SOURCE)?;
    let mut plugins = Vec::new();
    for plugin in file.plugins {
        if plugin.project.as_ref().is_some_and(|p| p.name.trim().is_empty() || !id_ok(&p.dir.to_ascii_lowercase())) {
            return Err(format!("插件目录条目无效：{}", plugin.id));
        }
        if !id_ok(&plugin.id) || plugin.unpack != "skills" && plugin.unpack != "root"
            || plugin.preset_plugin_id.as_deref().is_some_and(|id| id != plugin.id || crate::plugins::catalog_plugin(id).is_none()) {
            return Err(format!("插件目录条目无效：{}", plugin.id));
        }
        if let Some(subdir) = plugin.subdir.as_deref() {
            validate_subdir(subdir)?;
        }
        let setup = catalog_text(dir, &plugin.setup)?;
        let dsivio_reference = setup.contains(DSIVIO_REFERENCE_PATH).then(|| reference.clone());
        let adapter_files = adapter_files(dir, &setup, &plugin.setup)?;
        plugins.push(BuiltIn {
            setup,
            dsivio_reference,
            adapter_files,
            entry: plugin.entry.as_deref().map(|path| catalog_text(dir, path)).transpose()?,
            icon: catalog_text(dir, &plugin.icon)?,
            id: plugin.id,
            name: plugin.name,
            skill_id: plugin.skill_id,
            summary: plugin.summary,
            welcome: plugin.welcome,
            input_hint: plugin.input_hint,
            start_prompt: plugin.start_prompt,
            project_prompt: plugin.project_prompt.as_deref().map(|path| catalog_text(dir, path)).transpose()?
                .filter(|_| plugin.project.is_some()),
            project: plugin.project,
            icon_path: plugin.icon_path,
            required_files: plugin.required_files,
            repository: plugin.repository,
            revision: plugin.revision,
            subdir: plugin.subdir,
            skills: plugin.skills,
            category_ids: plugin.category_ids,
            unpack: plugin.unpack,
            skip: plugin.skip,
            preset_plugin_id: plugin.preset_plugin_id,
        });
    }
    Ok(Catalog { categories: file.categories.into_iter().map(|category| (category.id, category.name)).collect(), plugins })
}

fn load_market_catalog(app: &AppHandle) -> Result<Catalog, String> {
    let bundled = crate::media_runtime::runtime::resource_directory(app)?.join("plugins");
    if bundled.join("catalog.json").is_file() { return load_catalog_from(&bundled); }
    load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins"))
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltInState {
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    plugin_id: Option<String>,
    #[serde(default)]
    owned_skills: Vec<String>,
    #[serde(default = "loaded_by_default")]
    enabled: bool,
}

fn loaded_by_default() -> bool { true }

fn built_in_state_path(item: &BuiltIn) -> Result<PathBuf, String> {
    Ok(root()?.join(format!("{}-state.json", item.id)))
}

fn built_in_state(item: &BuiltIn) -> BuiltInState {
    built_in_state_path(item).ok().and_then(|p| read_json(&p).ok())
        .and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn save_built_in_state(item: &BuiltIn, state: &BuiltInState) -> Result<(), String> {
    write_json(&built_in_state_path(item)?, state)
}

fn built_in_ready(item: &BuiltIn, state: &BuiltInState) -> bool {
    let skills_ready = crate::skills::kivio_skills_dir()
        .is_some_and(|root| built_in_ready_at(item, state, &root));
    // Releases before this one installed a companion command package. Keep such
    // an install in repair state until the owned package has been removed.
    let command_ready = state.plugin_id.is_none();
    skills_ready && command_ready
}

fn built_in_ready_at(item: &BuiltIn, state: &BuiltInState, root: &Path) -> bool {
    state.revision.as_deref() == Some(item.revision.as_str())
        && built_in_setup_installed_at(item, root)
        && item.entry.as_deref().is_none_or(|entry| {
            let dir = root.join(&item.skill_id);
            market_skill_installed_at(&dir)
                && fs::read_to_string(dir.join("SKILL.md")).is_ok_and(|content| content == entry)
        })
        && item.skills.iter().all(|skill| root.join(skill).join("SKILL.md").is_file())
        && built_in_skill_installed_at(item, root)
}

fn built_in_setup_id(item: &BuiltIn) -> String { format!("{}-setup", item.id) }

const SETUP_DONE_MARKER: &str = " [setup completed once]";

/// The setup Skill records a successful first setup by ending its YAML
/// `description` line with the marker. Anything else counts as not done.
fn setup_description_done(text: &str) -> bool {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") { return false; }
    lines.take_while(|line| line.trim_end() != "---")
        .find_map(|line| line.strip_prefix("description:"))
        .is_some_and(|value| value.trim_end().ends_with(SETUP_DONE_MARKER.trim_start()) && value.trim_end().len() > SETUP_DONE_MARKER.len())
}

fn built_in_setup_done(item: &BuiltIn) -> bool {
    built_in_setup_dir(item).ok()
        .and_then(|dir| fs::read_to_string(dir.join("SKILL.md")).ok())
        .is_some_and(|text| setup_description_done(&text))
}

/// Windows keeps work out of the system drive: `<first non-C drive>:\dsiviowork\<dir>`.
/// Elsewhere (and without another drive) it is `Documents/Dsivio/<dir>`.
fn project_root_for(dir: &str, drive: Option<String>, documents: Option<PathBuf>) -> Option<PathBuf> {
    match drive {
        Some(drive) => Some(PathBuf::from(drive).join("dsiviowork").join(dir)),
        None => documents.map(|documents| documents.join("Dsivio").join(dir)),
    }
}

fn work_drive_root(exists: impl Fn(&str) -> bool) -> Option<String> {
    ('D'..='Z').map(|letter| format!("{letter}:\\")).find(|root| exists(root))
}

/// Extra system-prompt text for a conversation in the dedicated project of an installed and
/// enabled built-in plugin. Installation state is the only switch: uninstalling resets it and
/// disabling turns it off, so the text disappears with the plugin. Other projects and other
/// plugins are never affected.
pub fn project_prompt_for(app: &AppHandle, project_name: &str) -> Option<String> {
    let catalog = load_market_catalog(app).ok()?;
    project_prompt_in(&catalog, project_name, |item| {
        let state = built_in_state(item);
        state.revision.is_some() && state.enabled
    })
}

fn project_prompt_in(catalog: &Catalog, project_name: &str, installed: impl Fn(&BuiltIn) -> bool) -> Option<String> {
    catalog.plugins.iter()
        .find(|item| item.project.as_ref().is_some_and(|project| project.name == project_name) && item.project_prompt.is_some())
        .filter(|item| installed(item))
        .and_then(|item| item.project_prompt.clone())
}

/// The plugin's dedicated project: reuse it when it exists, otherwise create
/// it at the fixed root. A missing folder is recreated in the same place.
fn ensure_project(app: &AppHandle, item: &BuiltIn) -> Result<Value, String> {
    let spec = item.project.as_ref().ok_or("该插件没有专属项目")?;
    let drive = if cfg!(windows) { work_drive_root(|root| Path::new(root).is_dir()) } else { None };
    let documents = directories::UserDirs::new().map(|dirs| dirs.document_dir().map(Path::to_path_buf).unwrap_or_else(|| dirs.home_dir().to_path_buf()));
    let root = project_root_for(&spec.dir, drive, documents).ok_or("无法确定项目目录")?;
    let root_text = root.display().to_string();
    let existing = crate::chat::storage::get_projects(app)?.into_iter()
        .find(|project| project.root_path.as_deref() == Some(root_text.as_str()) || project.name == spec.name);
    let project = match existing {
        Some(project) => {
            if let Some(path) = project.root_path.as_deref() {
                fs::create_dir_all(path).map_err(|e| format!("无法创建项目目录 {path}：{e}"))?;
            }
            project
        }
        None => {
            fs::create_dir_all(&root).map_err(|e| format!("无法创建项目目录 {root_text}：{e}"))?;
            let now = chrono::Local::now().timestamp();
            crate::chat::storage::create_project_with_options(app, crate::chat::ChatProject {
                id: format!("proj_{}", uuid::Uuid::new_v4()),
                name: spec.name.clone(),
                description: Some(format!("{} 专属项目", item.name)),
                color: None,
                root_path: Some(root_text),
                created_at: now,
                updated_at: now,
            }, true)?
        }
    };
    Ok(json!({"id":project.id,"name":project.name,"rootPath":project.root_path}))
}

fn built_in_setup_dir(item: &BuiltIn) -> Result<PathBuf, String> {
    Ok(crate::skills::kivio_skills_dir().ok_or("用户目录不可用")?.join(built_in_setup_id(item)))
}

fn install_market_skill(dir: &Path, content: &str) -> Result<(), String> {
    let file = dir.join("SKILL.md");
    if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink())
        || fs::symlink_metadata(&file).is_ok_and(|meta| meta.file_type().is_symlink())
    { return Err(format!("{} 包含符号链接，请先检查该目录", dir.display())); }
    if file.is_file() {
        if !market_skill_installed_at(dir) { return Err(format!("{} 已存在其他 Skill，请先检查该目录", dir.display())); }
        return fs::write(file, content).map_err(|e| e.to_string());
    }
    if dir.exists() { return Err(format!("{} 已存在，请先检查该目录", dir.display())); }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(file, content).map_err(|e| e.to_string())
}

fn install_built_in_setup(item: &BuiltIn) -> Result<(), String> {
    sync_setup_files(item, &built_in_setup_dir(item)?)
}

/// Bring the installed setup Skill, its Dsivio reference and its adapter in line with this
/// release. They ship with the app, not with the upstream revision, so an app update must reach
/// installs that are otherwise ready. The done marker survives only when nothing changed: a changed
/// adapter or Skill has to be set up again (e.g. copied into the plugin's project).
fn sync_setup_files(item: &BuiltIn, dir: &Path) -> Result<(), String> {
    let skill_file = dir.join("SKILL.md");
    let current = fs::read_to_string(&skill_file).ok();
    let unchanged = current.as_deref().is_some_and(|text| without_done_marker(text) == item.setup)
        && item.dsivio_reference.as_deref().is_none_or(|reference| {
            fs::read_to_string(dir.join(DSIVIO_REFERENCE_PATH)).is_ok_and(|text| text == reference)
        })
        && adapter_matches(item, &dir.join(ADAPTER_PATH));
    if unchanged {
        return Ok(());
    }
    install_market_skill(dir, &item.setup)?;
    if let Some(reference) = &item.dsivio_reference {
        let file = dir.join(DSIVIO_REFERENCE_PATH);
        fs::create_dir_all(file.parent().ok_or("无效路径")?).map_err(|e| e.to_string())?;
        fs::write(file, reference).map_err(|e| e.to_string())?;
    }
    let adapter = dir.join(ADAPTER_PATH);
    if adapter.is_dir() && !fs::symlink_metadata(&adapter).is_ok_and(|meta| meta.file_type().is_symlink()) {
        fs::remove_dir_all(&adapter).map_err(|e| e.to_string())?;
    }
    for (relative, content) in &item.adapter_files {
        let file = adapter.join(relative);
        fs::create_dir_all(file.parent().ok_or("无效路径")?).map_err(|e| e.to_string())?;
        fs::write(file, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn without_done_marker(text: &str) -> String {
    let mut in_front_matter = false;
    text.split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                in_front_matter = line.trim_end() == "---";
            } else if in_front_matter && line.trim_end() == "---" {
                in_front_matter = false;
            } else if in_front_matter && line.starts_with("description:") {
                let (body, newline) = line.strip_suffix('\n').map_or((line, ""), |b| (b, "\n"));
                if let Some(original) = body.trim_end().strip_suffix(SETUP_DONE_MARKER.trim_start()) {
                    return format!("{}{newline}", original.trim_end());
                }
            }
            line.to_string()
        })
        .collect()
}

fn adapter_matches(item: &BuiltIn, adapter: &Path) -> bool {
    if item.adapter_files.is_empty() {
        return !adapter.exists();
    }
    let mut installed = Vec::new();
    let mut pending = vec![adapter.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = fs::read_dir(&current) else { return false };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(adapter) {
                installed.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    installed.sort();
    installed.len() == item.adapter_files.len()
        && item.adapter_files.iter().zip(&installed).all(|((relative, content), found)| {
            relative == found && fs::read_to_string(adapter.join(relative)).is_ok_and(|text| &text == content)
        })
}

fn built_in_entry_dir(item: &BuiltIn) -> Result<PathBuf, String> {
    Ok(crate::skills::kivio_skills_dir().ok_or("用户目录不可用")?.join(&item.skill_id))
}

fn install_built_in_entry(item: &BuiltIn) -> Result<(), String> {
    if let Some(content) = item.entry.as_deref() { install_market_skill(&built_in_entry_dir(item)?, content)?; }
    Ok(())
}

fn built_in_setup_installed(item: &BuiltIn) -> bool {
    crate::skills::kivio_skills_dir()
        .is_some_and(|root| built_in_setup_installed_at(item, &root))
}

fn built_in_setup_installed_at(item: &BuiltIn, root: &Path) -> bool {
    market_skill_installed_at(&root.join(built_in_setup_id(item)))
}

fn market_skill_installed_at(dir: &Path) -> bool {
    if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink())
        || fs::symlink_metadata(dir.join("SKILL.md")).is_ok_and(|meta| meta.file_type().is_symlink())
    { return false; }
    fs::read_to_string(dir.join("SKILL.md")).ok()
        .is_some_and(|text| text.contains("\nkivio-market-managed: true\n"))
}

fn remove_built_in_setup(item: &BuiltIn) -> Result<(), String> {
    remove_market_skill(&built_in_setup_dir(item)?)
}

fn remove_built_in_entry(item: &BuiltIn) -> Result<(), String> {
    if item.entry.is_some() { remove_market_skill(&built_in_entry_dir(item)?)?; }
    Ok(())
}

fn remove_market_skill(dir: &Path) -> Result<(), String> {
    if !market_skill_installed_at(dir) { return Ok(()); }
    let reference = dir.join(DSIVIO_REFERENCE_PATH);
    if reference.is_file() {
        fs::remove_file(&reference).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir(dir.join("references"));
    }
    let adapter = dir.join("adapter");
    if adapter.is_dir() && !fs::symlink_metadata(&adapter).is_ok_and(|meta| meta.file_type().is_symlink()) {
        fs::remove_dir_all(&adapter).map_err(|e| e.to_string())?;
    }
    fs::remove_file(dir.join("SKILL.md")).map_err(|e| e.to_string())?;
    if fs::read_dir(&dir).map_err(|e| e.to_string())?.next().is_none() {
        fs::remove_dir(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn built_in_skill_installed_at(item: &BuiltIn, root: &Path) -> bool {
    let skill = root.join(&item.skill_id);
    skill.join("SKILL.md").is_file()
        && item.required_files.iter().all(|file| skill.join(file).is_file())
}

fn built_in_manifest(item: &BuiltIn) -> Value {
    let mut skill_ids = item.skills.clone();
    if item.entry.is_some() { skill_ids.insert(0, item.skill_id.clone()); }
    json!({"schemaVersion":1,"id":item.id,"version":"1.0.0","name":item.name,
        "summary":item.summary,"categoryIds":item.category_ids,
        "icon":item.icon_path,
        "compatibility":{"minAppVersion":"1.0.1","platforms":["macos-arm64","macos-x64","windows-x64","linux-x64","linux-arm64"]},
        "notices":[],"welcome":item.welcome,"inputHint":item.input_hint,
        "startPrompt":item.start_prompt,"verification":[],
        "setupSkillId":built_in_setup_id(item),"mainSkillId":item.skill_id,
        "skillIds":skill_ids,
        "project":item.project.as_ref().map(|p| json!({"name":p.name}))})
}

fn built_in_local(item: &BuiltIn, state: &BuiltInState, installed: bool) -> Option<Value> {
    if !installed && state.plugin_id.is_none() && state.revision.is_none() { return None; }
    let preset_active = item.preset_plugin_id.as_deref().is_none_or(|id| {
        !crate::plugins::is_installed(id) || crate::plugins::is_enabled(id)
    });
    Some(json!({"id":item.id,"manifest":built_in_manifest(item),"source":{"kind":"built-in"},
        "status":if installed { "ready" } else { "failed" },"enabled":installed && state.enabled && preset_active,
        "pluginId":state.plugin_id,"skillId":if installed { Some(item.skill_id.clone()) } else { None },
        "conversationId":null,"error":if installed { None } else { Some("插件组件缺失或未启用") },
        "phase":if installed { Some("ready") } else { None }}))
}

fn built_in_snapshot(catalog: &Catalog) -> Value {
    json!({"categories":catalog.categories.iter().map(|(id, name)| json!({"id":id,"name":name})).collect::<Vec<_>>(),
        "entries":catalog.plugins.iter().map(|item| json!({"id":item.id,"version":"1.0.0","source":{"kind":"built-in"},"manifest":built_in_manifest(item)})).collect::<Vec<_>>(),
        "installed":catalog.plugins.iter().filter_map(|item| { let state = built_in_state(item); built_in_local(item, &state, built_in_ready(item, &state)) }).collect::<Vec<_>>(),
        "refreshedAt":chrono::Utc::now().timestamp_millis(),"error":null,"sourceUrl":"built-in"})
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Local {
    id: String,
    manifest: Value,
    source: Value,
    status: String,
    enabled: bool,
    plugin_id: Option<String>,
    skill_id: Option<String>,
    conversation_id: Option<String>,
    #[serde(default)]
    uninstall_conversation_id: Option<String>,
    error: Option<String>,
}
fn root() -> Result<PathBuf, String> {
    crate::app_data::app_data_dir()
        .map(|p| p.join("market"))
        .ok_or("应用数据目录不可用".into())
}
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.split('-').all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}
fn package_dir(id: &str) -> Result<PathBuf, String> {
    if !id_ok(id) {
        return Err("无效应用 ID".into());
    }
    Ok(root()?.join("installed").join(id))
}
fn read_json(p: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn write_json(p: &Path, v: &impl Serialize) -> Result<(), String> {
    fs::create_dir_all(p.parent().ok_or("Invalid path")?).map_err(|e| e.to_string())?;
    crate::chat::storage::atomic_write(
        p,
        &serde_json::to_string_pretty(v).map_err(|e| e.to_string())?,
        "market record",
    )
}
fn read_local(id: &str) -> Result<Local, String> {
    serde_json::from_value(read_json(&package_dir(id)?.join("record.json"))?)
        .map_err(|e| e.to_string())
}
fn save_local(local: &Local) -> Result<(), String> {
    write_json(&package_dir(&local.id)?.join("record.json"), local)
}
fn str_field<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("缺少字段 {k}"))
}
fn source_parts(source: &Value, id: &str) -> Result<(String, String, String), String> {
    let repo = str_field(source, "repository")?
        .strip_prefix("https://github.com/")
        .ok_or("只支持 GitHub HTTPS 仓库")?;
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|s| {
            s.is_empty()
                || *s == "."
                || *s == ".."
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("无效 GitHub 仓库".into());
    }
    let revision = str_field(source, "revision")?;
    if revision.len() != 40 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("应用必须固定到完整提交 SHA".into());
    }
    let directory = str_field(source, "directory")?;
    if !id_ok(id) || directory != format!("packages/{id}") {
        return Err("包路径必须与应用 ID 一致".into());
    }
    Ok((repo.into(), revision.into(), directory.into()))
}
fn version(v: &str) -> Option<[u64; 3]> {
    let p: Vec<_> = v.split('.').collect();
    if p.len() != 3 {
        return None;
    }
    Some([p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?])
}
fn validate_manifest(m: &Value, entry: &Value) -> Result<(), String> {
    if m["schemaVersion"] != 1 || m["id"] != entry["id"] || m["version"] != entry["version"] {
        return Err("清单版本或应用标识不匹配".into());
    }
    for (field, max) in [
        ("name", 24),
        ("summary", 80),
        ("welcome", 140),
        ("inputHint", 100),
    ] {
        if str_field(m, field)?.chars().count() > max {
            return Err(format!("字段 {field} 过长"));
        }
    }
    if version(str_field(m, "version")?).is_none() {
        return Err("版本格式无效".into());
    }
    if m["categoryIds"]
        .as_array()
        .is_none_or(|v| v.is_empty() || v.iter().any(|x| x.as_str().is_none_or(|s| !id_ok(s))))
    {
        return Err("应用分类无效".into());
    }
    if m["notices"].as_array().is_none_or(|v| {
        v.len() > 3
            || v.iter()
                .any(|x| x.as_str().is_none_or(|s| s.chars().count() > 80))
    }) {
        return Err("使用提示无效".into());
    }
    if m["compatibility"]["platforms"]
        .as_array()
        .is_none_or(|v| v.is_empty() || v.iter().any(|x| x.as_str().is_none()))
        || version(str_field(&m["compatibility"], "minAppVersion")?).is_none()
    {
        return Err("系统兼容信息无效".into());
    }
    if !is_local_draft(&entry["source"]) && m["verification"].as_array().is_none_or(|v| {
        v.is_empty()
            || v.iter().any(|x| {
                ["platform", "dsivioVersion", "verifiedAt", "record"]
                    .iter()
                    .any(|key| x[*key].as_str().is_none_or(|s| s.is_empty()))
            })
    }) {
        return Err("未验证的包不能上架".into());
    }
    Ok(())
}
fn check_platform(m: &Value, draft: bool) -> Result<(), String> {
    let platform = format!(
        "{}-{}",
        if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        },
        if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        }
    );
    if m["compatibility"]["platforms"]
        .as_array()
        .is_none_or(|a| !a.iter().any(|v| v.as_str() == Some(&platform)))
    {
        return Err(format!("这个版本暂不支持你的系统（{platform}）"));
    }
    if !draft && m["verification"]
        .as_array()
        .is_none_or(|a| !a.iter().any(|v| v["platform"].as_str() == Some(&platform)))
    {
        return Err("当前系统没有验证记录".into());
    }
    let minimum =
        version(str_field(&m["compatibility"], "minAppVersion")?).ok_or("最低软件版本无效")?;
    if version(env!("CARGO_PKG_VERSION")).ok_or("软件版本无效")? < minimum {
        return Err("请升级 dsivio 后安装此应用".into());
    }
    Ok(())
}
async fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    download_optional(url, limit, false)
        .await?
        .ok_or("下载内容不存在".into())
}
async fn download_optional(
    url: &str,
    limit: usize,
    allow_missing: bool,
) -> Result<Option<Vec<u8>>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .user_agent("dsivio-market/1")
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("无法连接插件：{e}"))?;
    if allow_missing && response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let mut response = response
        .error_for_status()
        .map_err(|e| format!("市场尚未发布或暂不可用：{e}"))?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("下载内容超过限制".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > limit {
            return Err("下载内容超过限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}
async fn fetch_json(url: &str) -> Result<Value, String> {
    serde_json::from_slice(&download(url, 2 * 1024 * 1024).await?).map_err(|e| e.to_string())
}
fn raw_base(source: &Value, id: &str) -> Result<String, String> {
    let (repo, rev, dir) = source_parts(source, id)?;
    Ok(format!(
        "https://raw.githubusercontent.com/{repo}/{rev}/{dir}/"
    ))
}
fn catalog_url() -> String {
    std::env::var("DSIVIO_MARKET_CATALOG_URL")
        .ok()
        .filter(|s| s.starts_with("https://raw.githubusercontent.com/"))
        .unwrap_or(DEFAULT_CATALOG.into())
}
fn is_local_draft(source: &Value) -> bool {
    cfg!(debug_assertions) && source["kind"] == "local-draft"
}

// Maintainer-only preview. Release builds never read local draft directories.
fn draft_root() -> Option<PathBuf> {
    if !cfg!(debug_assertions) { return None; }
    std::env::var_os("DSIVIO_MARKET_DRAFT_DIR").map(PathBuf::from)
}
fn draft_package(id: &str) -> Result<PathBuf, String> {
    let root = draft_root().ok_or("本地试用入口已关闭")?;
    if !id_ok(id) { return Err("无效包 ID".into()); }
    let path = if root.join("market.json").is_file() { root } else { root.join(id) };
    if draft_descriptor(&path)?["manifest"]["id"] != id { return Err("包 ID 不一致".into()); }
    Ok(path)
}
fn draft_catalogs_at(root: &Path) -> Result<Value, String> {
    if root.join("market.json").is_file() { return draft_catalog_at(root); }
    let mut paths = fs::read_dir(root).map_err(|e|e.to_string())?
        .filter_map(Result::ok).map(|e|e.path())
        .filter(|p| p.is_dir() && p.join("market.json").is_file()).collect::<Vec<_>>();
    paths.sort();
    let mut catalog = json!({"categories":[{"id":"videos","name":"视频"},{"id":"images","name":"图片"},{"id":"other","name":"其他"}],"entries":[],"refreshedAt":chrono::Utc::now().timestamp_millis()});
    for path in paths {
        let single = draft_catalog_at(&path)?;
        let entry = &single["entries"][0];
        if path.file_name().and_then(|s|s.to_str()) != entry["id"].as_str() { return Err("包目录与 ID 不一致".into()); }
        catalog["entries"].as_array_mut().unwrap().push(entry.clone());
    }
    Ok(catalog)
}
fn draft_files(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    fn visit(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>, total: &mut u64) -> Result<(), String> {
        for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let name = item.file_name();
            if matches!(name.to_str(), Some("data" | "runtime" | "node_modules" | ".git" | "__pycache__")) { continue; }
            let kind = item.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() { return Err("本地试用包不能包含符号链接".into()); }
            if kind.is_dir() { visit(base, &item.path(), out, total)?; }
            else if kind.is_file() {
                *total += item.metadata().map_err(|e| e.to_string())?.len();
                if *total > 100 * 1024 * 1024 || out.len() >= 10000 { return Err("本地试用包超过大小限制".into()); }
                out.push((item.path().strip_prefix(base).map_err(|e| e.to_string())?.to_path_buf(), fs::read(item.path()).map_err(|e| e.to_string())?));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files, &mut 0)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}
fn draft_descriptor(path: &Path) -> Result<Value, String> {
    read_json(&path.join("market.json"))
}
async fn descriptor(source: &Value, id: &str) -> Result<Value, String> {
    let (repo, rev, directory) = source_parts(source, id)?;
    fetch_json(&format!("https://raw.githubusercontent.com/{repo}/{rev}/{directory}/market.json")).await
}
fn draft_catalog_at(path: &Path) -> Result<Value, String> {
    use sha2::{Digest, Sha256};
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let info = draft_descriptor(&path)?;
    let manifest = info["manifest"].clone();
    let id = str_field(&manifest, "id")?;
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&info).map_err(|e|e.to_string())?);
    for (name, bytes) in draft_files(&path)? {
        digest.update(name.to_string_lossy().as_bytes()); digest.update([0]);
        digest.update((bytes.len() as u64).to_le_bytes()); digest.update(bytes);
    }
    let source = json!({"kind":"local-draft", "directory":path, "revision":format!("{:x}", digest.finalize())});
    let entry = json!({"id":id,"version":manifest["version"],"source":source,"manifest":manifest});
    validate_manifest(&manifest, &entry)?;
    validate_example(&info["example"])?;
    for name in ["INSTALL.md", "DSIVIO.md"] {
        if !path.join(name).is_file() { return Err(format!("本地试用包缺少 {name}")); }
    }
    Ok(json!({"categories":[{"id":"videos","name":"视频"},{"id":"images","name":"图片"},{"id":"other","name":"其他"}],"entries":[entry],"refreshedAt":chrono::Utc::now().timestamp_millis()}))
}
fn cached() -> Value {
    if let Some(path) = draft_root() {
        return draft_catalogs_at(&path).unwrap_or_else(|e| json!({"categories":[],"entries":[],"refreshedAt":null,"draftError":e}));
    }
    root()
        .ok()
        .and_then(|r| read_json(&r.join("catalog-cache.json")).ok())
        .unwrap_or(json!({"categories":[],"entries":[],"refreshedAt":null}))
}
fn all_locals() -> Vec<Local> {
    let mut result = Vec::new();
    if let Ok(entries) =
        root().and_then(|r| fs::read_dir(r.join("installed")).map_err(|e| e.to_string()))
    {
        for entry in entries.flatten() {
            if let Some(id) = entry.file_name().to_str() {
                if let Ok(mut local) = read_local(id) {
                    if local.status == "ready" {
                        local.enabled = local
                            .plugin_id
                            .as_deref()
                            .is_some_and(crate::plugins::packages::owner_enabled);
                        let valid = crate::plugins::packages::plugin_packages_list()
                            .unwrap_or_default()
                            .iter()
                            .any(|p| {
                                Some(&p.id) == local.plugin_id.as_ref() && p.diagnostics.is_empty()
                            });
                        if !valid {
                            local.status = "failed".into();
                            local.error = Some("应用组件缺失或无效，需要修复".into());
                        }
                    }
                    result.push(local);
                }
            }
        }
    }
    result
}
fn snapshot(error: Option<String>) -> Value {
    let cache = cached();
    json!({"categories":cache["categories"],"entries":cache["entries"],"refreshedAt":cache["refreshedAt"],"installed":all_locals(),"error":error.or_else(||cache["draftError"].as_str().map(String::from)),"sourceUrl":draft_root().map(|p|p.display().to_string()).unwrap_or_else(catalog_url)})
}
fn missing_catalog_snapshot() -> Value {
    // First publication has not happened yet. Never discard an existing catalog.
    snapshot(missing_catalog_error(&cached()))
}
fn missing_catalog_error(cache: &Value) -> Option<String> {
    if cache["refreshedAt"].is_null()
        && cache["entries"]
            .as_array()
            .is_none_or(|entries| entries.is_empty())
    {
        None
    } else {
        Some("市场目录暂时无法找到，已保留上次内容。".into())
    }
}
async fn refresh() -> Result<Value, String> {
    if draft_root().is_some() { return Ok(snapshot(None)); }
    let bytes = download_optional(&catalog_url(), 2 * 1024 * 1024, true).await?;
    let Some(bytes) = bytes else {
        return Ok(missing_catalog_snapshot());
    };
    let catalog: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if catalog["schemaVersion"] != 1 {
        return Err("暂不支持这个市场目录版本".into());
    }
    let categories = catalog["categories"].as_array().ok_or("市场分类格式无效")?;
    let entries = catalog["packages"].as_array().ok_or("市场列表格式无效")?;
    if entries.len() > 300 {
        return Err("市场条目过多".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut resolved = Vec::new();
    for entry in entries {
        let id = str_field(entry, "id")?;
        if !seen.insert(id) || !id_ok(id) {
            return Err("市场应用 ID 重复或无效".into());
        }
        let mut item = entry.clone();
        let load = async {
            let m = descriptor(&entry["source"], id).await?["manifest"].clone();
            validate_manifest(&m, entry)?;
            if m["categoryIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| !categories.iter().any(|c| &c["id"] == id))
            {
                return Err("应用引用了不存在的分类".into());
            }
            Ok::<_, String>(m)
        }
        .await;
        match load {
            Ok(m) => item["manifest"] = m,
            Err(e) => item["error"] = json!(e),
        }
        resolved.push(item);
    }
    if let Some(previous) = cached()["entries"].as_array() {
        for entry in previous {
            if entry["source"]["kind"] == "local-draft"
                && !resolved.iter().any(|item| item["id"] == entry["id"])
            {
                resolved.push(entry.clone());
            }
        }
    }
    write_json(
        &root()?.join("catalog-cache.json"),
        &json!({"categories":categories,"entries":resolved,"refreshedAt":chrono::Utc::now().timestamp_millis()}),
    )?;
    Ok(snapshot(None))
}
fn remember_catalog_entry(local: &Local) -> Result<(), String> {
    if local.source["kind"] != "local-draft" && local.source["repository"].as_str().is_none() {
        return Ok(());
    }
    let mut cache = cached();
    if !cache["entries"].is_array() {
        cache["categories"] = json!([{"id":"videos","name":"视频"},{"id":"images","name":"图片"},{"id":"other","name":"其他"}]);
        cache["entries"] = json!([]);
    }
    let entries = cache["entries"].as_array_mut().ok_or("市场目录格式无效")?;
    if !entries.iter().any(|entry| entry["id"] == local.id) {
        entries.push(json!({
            "id": local.id,
            "version": local.manifest["version"],
            "source": local.source,
            "manifest": local.manifest,
        }));
    }
    if cache["refreshedAt"].is_null() {
        cache["refreshedAt"] = json!(chrono::Utc::now().timestamp_millis());
    }
    write_json(&root()?.join("catalog-cache.json"), &cache)
}
fn entry(id: &str) -> Result<Value, String> {
    cached()["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e["id"].as_str() == Some(id)))
        .cloned()
        .ok_or("应用不在当前市场目录中，请刷新市场".into())
}
fn content(local: &Local) -> Result<PathBuf, String> {
    Ok(package_dir(&local.id)?
        .join("versions")
        .join(str_field(&local.source, "revision")?))
}
fn unzip_package(bytes: Vec<u8>, source: &Value, id: &str, dest: &Path) -> Result<(), String> {
    let (_, _, directory) = source_parts(source, id)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let mut count = 0;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some((_, relative)) = file.name().split_once('/') else {
            continue;
        };
        let Some(relative) = relative.strip_prefix(&format!("{directory}/")) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        if relative.starts_with('/')
            || relative.contains('\\')
            || relative.split('/').any(|s| s == ".." || s == ".")
            || relative.contains(':')
            || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("包包含不安全路径或符号链接".into());
        }
        let target = dest.join(relative);
        if !target.starts_with(dest) {
            return Err("包路径越界".into());
        }
        if file.is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
            continue;
        }
        total = total.checked_add(file.size()).ok_or("应用过大")?;
        count += 1;
        if total > 100 * 1024 * 1024 || count > 10000 {
            return Err("应用解压内容超过限制".into());
        }
        fs::create_dir_all(target.parent().ok_or("Invalid path")?).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        file.by_ref()
            .take(100 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 100 * 1024 * 1024 {
            return Err("应用文件超过限制".into());
        }
        fs::write(target, data).map_err(|e| e.to_string())?;
    }
    if count == 0 {
        return Err("仓库中没有找到应用文件".into());
    }
    Ok(())
}
fn draft_changed(local: &Local, item: &Value) -> bool {
    is_local_draft(&local.source) && is_local_draft(&item["source"]) && local.source != item["source"]
}
async fn prepare(id: &str) -> Result<Value, String> {
    let previous = all_locals().into_iter().find(|local| local.id == id);
    if let Some(mut local) = previous.clone().filter(|l| !entry(id).is_ok_and(|item| draft_changed(l, &item))) {
        if local.status == "ready" {
            return Err("应用已经安装；当前版本暂不支持原地升级，请保留旧版使用".into());
        }
        local.status = "configuring".into();
        local.enabled = false;
        save_local(&local)?;
        return Ok(json!({"brief":fs::read_to_string(content(&local)?.join("INSTALL.md")).map_err(|e| e.to_string())?,"local":local}));
    }
    let item = entry(id)?;
    let manifest = &item["manifest"];
    validate_manifest(manifest, &item)?;
    check_platform(manifest, is_local_draft(&item["source"]))?;
    let source = &item["source"];

    let local = Local {
        id: id.into(),
        manifest: manifest.clone(),
        source: source.clone(),
        status: "configuring".into(),
        enabled: false,
        plugin_id: previous.as_ref().and_then(|l| l.plugin_id.clone()),
        skill_id: None,
        conversation_id: None,
        uninstall_conversation_id: None,
        error: None,
    };
    let dest = content(&local)?;
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    if is_local_draft(source) {
        let path = draft_package(id)?;
        let fresh = draft_catalog_at(&path)?;
        if fresh["entries"][0]["source"] != *source { return Err("试用包已变化，请刷新后重试".into()); }
        for (relative, bytes) in draft_files(&path)? {
            let target = dest.join(relative);
            fs::create_dir_all(target.parent().ok_or("Invalid path")?).map_err(|e|e.to_string())?;
            fs::write(target, bytes).map_err(|e|e.to_string())?;
        }
    } else {
        let (repo, rev, _) = source_parts(source, id)?;
        let bytes = download(&format!("https://codeload.github.com/{repo}/zip/{rev}"), 100 * 1024 * 1024).await?;
        unzip_package(bytes, source, id, &dest)?;
    }
    let info = if is_local_draft(source) { draft_descriptor(&draft_package(id)?)? } else { descriptor(source, id).await? };
    let downloaded = info["manifest"].clone();
    if downloaded != *manifest {
        return Err("下载清单与目录不一致".into());
    }
    write_json(&package_dir(id)?.join("display.json"), &info)?;
    for name in ["INSTALL.md", "DSIVIO.md"] {
        if !dest.join(name).is_file() {
            return Err(format!("缺少 {name}"));
        }
    }
    let runtime = dest.join("runtime");
    fs::create_dir_all(runtime.join(".kivio-plugin")).map_err(|e| e.to_string())?;
    fs::create_dir_all(runtime.join("skills/market-entry")).map_err(|e| e.to_string())?;
    write_json(
        &runtime.join(".kivio-plugin/plugin.json"),
        &json!({"schemaVersion":1,"name":id,"version":manifest["version"],"description":manifest["summary"]}),
    )?;
    let adaptation = fs::read_to_string(dest.join("DSIVIO.md")).map_err(|e|e.to_string())?;
    fs::write(runtime.join("skills/market-entry/SKILL.md"), format!("---\nname: market-{id}\ndescription: {}\n---\n\n{adaptation}\n", serde_json::to_string(&manifest["summary"]).map_err(|e|e.to_string())?)).map_err(|e|e.to_string())?;
    save_local(&local)?;
    Ok(json!({"brief":fs::read_to_string(content(&local)?.join("INSTALL.md")).map_err(|e| e.to_string())?,"local":local}))
}
fn safe_asset(value: &str) -> bool {
    value.starts_with("assets/")
        && !value.contains('\\')
        && !value.contains(':')
        && !value.contains('?')
        && !value.contains('#')
        && !value.contains('%')
        && value
            .split('/')
            .all(|s| !s.is_empty() && s != ".." && s != ".")
}
fn validate_example(example: &Value) -> Result<(), String> {
    if example["schemaVersion"] != 1 {
        return Err("示例格式不支持".into());
    }
    let messages = example["messages"]
        .as_array()
        .filter(|a| (2..=6).contains(&a.len()))
        .ok_or("示例消息无效")?;
    for m in messages {
        if !matches!(m["role"].as_str(), Some("user" | "assistant"))
            || str_field(m, "text")?.chars().count() > 160
        {
            return Err("示例文字无效".into());
        }
        if let Some(a) = m.get("attachments") {
            let a = a
                .as_array()
                .filter(|a| a.len() <= 3)
                .ok_or("示例附件过多")?;
            for f in a {
                if !matches!(f["type"].as_str(), Some("image" | "video" | "file"))
                    || !safe_asset(str_field(f, "path")?)
                    || f.get("poster")
                        .is_some_and(|p| p.as_str().is_none_or(|p| !safe_asset(p)))
                {
                    return Err("示例附件路径无效".into());
                }
            }
        }
    }
    Ok(())
}
fn mutation_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(Default::default)
}
#[tauri::command]
pub async fn market_command(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Value,
) -> Result<Value, String> {
    let action = str_field(&request, "action")?;
    let catalog = load_market_catalog(&app)?;
    if action == "snapshot" || action == "refresh" {
        return Ok(built_in_snapshot(&catalog));
    }
    let id = str_field(&request, "id")?;
    if let Some(item) = catalog.plugins.iter().find(|item| item.id == id) {
        let _guard = mutation_lock().lock().await;
        return match action {
            "install" => install_plugin(&app, item).await,
            "uninstall" => uninstall_plugin(&app, item).await,
            "ensure_project" => ensure_project(&app, item),
            "set_enabled" => set_built_in_enabled(&app, &*state, item, request["enabled"].as_bool().ok_or("缺少加载状态")?).await,
            _ => built_in_command(item, &request),
        };
    }
    package_dir(id)?;
    if action == "icon" {
        use base64::Engine;
        let local = read_local(id).ok();
        let item = entry(id).ok();
        let (source, manifest) = if let Some(ref l) = local { (&l.source, &l.manifest) }
            else { let e = item.as_ref().ok_or("找不到应用")?; (&e["source"], &e["manifest"]) };
        let icon = str_field(manifest, "icon")?;
        if !safe_asset(icon) { return Err("无效图标路径".into()); }
        let mime = match Path::new(icon).extension().and_then(|s| s.to_str()) {
            Some("svg") => "image/svg+xml", Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg", Some("webp") => "image/webp",
            _ => return Err("不支持的图标格式".into()),
        };
        let base = if let Some(ref l) = local { Some(content(l)?) }
            else if is_local_draft(source) { Some(draft_package(id)?) } else { None };
        if let Some(base) = base {
            let base = base.canonicalize().map_err(|e|e.to_string())?;
            let path = base.join(icon).canonicalize().map_err(|e|e.to_string())?;
            if !path.starts_with(&base) { return Err("无效图标路径".into()); }
            if fs::metadata(&path).map_err(|e|e.to_string())?.len() > 1024 * 1024 { return Err("图标过大".into()); }
            let bytes = fs::read(path).map_err(|e|e.to_string())?;
            return Ok(json!(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))));
        }
        return Ok(json!(format!("{}{icon}", raw_base(source, id)?)));
    }
    if action == "detail" {
        let local = read_local(id).ok();
        let item = entry(id).ok();
        let source = if request["local"] == true || item.is_none() {
            local.as_ref().map(|l| &l.source)
        } else {
            item.as_ref().map(|e| &e["source"])
        }
        .ok_or("找不到应用")?;
        let draft = is_local_draft(source);
        let base = if draft { String::new() } else { raw_base(source, id)? };
        let local_path = local
            .as_ref()
            .filter(|l| &l.source == source)
            .and_then(|l| content(l).ok())
            .map(|_| package_dir(id).unwrap().join("display.json"));
        let example = if let Some(p) = local_path.filter(|p| p.is_file()) {
            read_json(&p)?["example"].clone()
        } else {
            if draft {
                let path = draft_package(id)?;
                let fresh = draft_catalog_at(&path)?;
                if fresh["entries"][0]["source"] != *source { return Err("试用包已变化，请刷新".into()); }
                draft_descriptor(&path)?["example"].clone()
            } else { descriptor(source, id).await?["example"].clone() }
        };
        validate_example(&example)?;
        return Ok(json!({"example":example,"assetBase":base}));
    }
    let _guard = mutation_lock().lock().await;
    if action == "prepare" && state.chat_runtime().has_any_active_reply()
    {
        return Err("当前仍有对话任务在运行，请结束后再更改应用加载状态。".into());
    }
    let value = match action {
        "prepare" => {
            if let (Ok(old), Ok(item)) = (read_local(id), entry(id)) {
                if draft_changed(&old, &item) {
                    if let Some(plugin) = old.plugin_id {
                        crate::plugins::packages::plugin_packages_set_enabled(app.clone(), state.clone(), plugin, false).await?;
                    }
                }
            }
            prepare(id).await?
        },
        "attach_conversation" => {
            let mut l = read_local(id)?;
            let cid = str_field(&request, "conversationId")?;
            crate::chat::storage::load_conversation(&app, cid)?;
            l.conversation_id = Some(cid.into());
            save_local(&l)?;
            json!(l)
        }
        "set_enabled" => {
            let mut l = read_local(id)?;
            if l.status != "ready" {
                return Err("应用尚未完成安装验收".into());
            }
            let enabled = request["enabled"].as_bool().ok_or("缺少加载状态")?;
            crate::plugins::packages::plugin_packages_set_enabled(
                app.clone(),
                state.clone(),
                l.plugin_id.clone().ok_or("应用组件不存在")?,
                enabled,
            )
            .await?;
            l.enabled = enabled;
            save_local(&l)?;
            json!(l)
        }
        "prepare_remove" => {
            let mut l = read_local(id)?;
            let cid = str_field(&request, "conversationId")?;
            crate::chat::storage::load_conversation(&app, cid)?;
            l.uninstall_conversation_id = Some(cid.into());
            save_local(&l)?;
            json!({"brief":""})
        }
        "discard" => {
            let local = read_local(id)?;
            if local.status == "ready" || local.plugin_id.is_some() {
                return Err("已完成安装的插件请从卸载对话移除".into());
            }
            remember_catalog_entry(&local)?;
            fs::remove_dir_all(package_dir(id)?).map_err(|e| e.to_string())?;
            json!({"removed": true})
        }
        _ => return Err("未知市场操作".into()),
    };
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(value)
}

fn built_in_command(item: &BuiltIn, request: &Value) -> Result<Value, String> {
    let action = str_field(request, "action")?;
    match action {
        "icon" => {
            use base64::Engine;
            Ok(json!(format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(item.icon.as_bytes()))))
        }
        "setup_state" => {
            // Read right before the plugin is used: refresh shipped setup files first, so an app
            // update that changed them sends this use through setup again.
            if built_in_ready(item, &built_in_state(item)) {
                install_built_in_setup(item)?;
            }
            Ok(json!({"setupDone":built_in_setup_done(item)}))
        }
        "detail" => Ok(json!({"example":{"schemaVersion":1,"messages":[
            {"role":"user","text":item.input_hint},
            {"role":"assistant","text":item.welcome}
        ]},"assetBase":""})),
        _ => Err("未知内置插件操作".into()),
    }
}

fn unpack_built_in_skills(item: &BuiltIn, bytes: Vec<u8>, stage: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let mut count = 0usize;
    let subdir_prefix = item.subdir.as_deref().map(|subdir| {
        validate_subdir(subdir)?;
        Ok::<_, String>(format!("{subdir}/"))
    }).transpose()?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
        // Validate names before filtering: an unrelated subtree cannot hide malicious entries.
        let original = file.name().strip_suffix('/').unwrap_or(file.name());
        validate_archive_relative_path(original)?;
        let Some((_, archive_path)) = original.split_once('/') else { continue; };
        let scoped_path = match subdir_prefix.as_deref() {
            None => archive_path,
            Some(prefix) => match archive_path.strip_prefix(prefix) {
                Some(rest) if !rest.is_empty() => rest,
                _ => continue,
            },
        };
        let selected = if item.unpack == "root" {
            if item.skip.iter().any(|prefix| scoped_path.starts_with(prefix)) { continue; }
            format!("{}/{scoped_path}", item.skill_id)
        } else {
            let Some(path) = scoped_path.strip_prefix("skills/") else { continue; };
            let Some(skill) = path.split('/').next() else { continue; };
            if !item.skills.iter().any(|name| name == skill) { continue; }
            path.to_string()
        };
        validate_archive_relative_path(&selected)?;
        // Symlinks are refused only where they would be written; repos ship links such as
        // .claude/skills/<name> -> skills/<name> outside the selected Skills.
        if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("Skill 压缩包包含不安全路径".into());
        }
        let target = stage.join(&selected);
        if file.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        total = total.checked_add(file.size()).ok_or("Skill 内容过大")?;
        count += 1;
        if total > 30 * 1024 * 1024 || count > 2000 { return Err("Skill 内容超过限制".into()); }
        fs::create_dir_all(target.parent().ok_or("无效 Skill 路径")?).map_err(|e| e.to_string())?;
        let mut output = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut output).map_err(|e| e.to_string())?;
    }
    for skill in &item.skills {
        let dir = stage.join(skill);
        if !dir.join("SKILL.md").is_file() { return Err(format!("官方包缺少 {skill}/SKILL.md")); }
    }
    if item.unpack == "root" {
        let dir = stage.join(&item.skill_id);
        for relative in &item.required_files {
            if !dir.join(relative).is_file() { return Err(format!("官方包缺少 {relative}")); }
        }
    }
    Ok(())
}

fn market_companion_package(item: &BuiltIn, id: &str) -> Result<Option<crate::plugins::packages::Package>, String> {
    let package = crate::plugins::packages::plugin_packages_list()?
        .into_iter().find(|package| package.id == id);
    if let Some(package) = &package {
        let suffix = format!("/market-companions/{}", item.id);
        if package.format != "kivio" || !package.source.replace('\\', "/").ends_with(&suffix) {
            return Err("市场记录指向了其他插件，未执行操作".into());
        }
    }
    Ok(package)
}

fn market_owned_skill(item: &BuiltIn, state: &BuiltInState, root: &Path, skill: &str) -> bool {
    if !state.owned_skills.iter().any(|owned| owned == skill) { return false; }
    fs::read(root.join(skill).join(".kivio-market-owner.json")).ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|value| value["id"] == item.id)
}

fn built_in_skill_plan(item: &BuiltIn, state: &BuiltInState, root: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let mut missing = Vec::new();
    let mut replace = Vec::new();
    for skill in &item.skills {
        let dir = root.join(skill);
        let metadata = match fs::symlink_metadata(&dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(skill.clone());
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!("{} 已存在其他文件，请先检查", dir.display()));
        }
        let complete = dir.join("SKILL.md").is_file()
            && (skill != &item.skill_id || item.required_files.iter().all(|file| dir.join(file).is_file()));
        if complete { continue; }
        if !market_owned_skill(item, state, root, skill) {
            return Err(format!("{} 已存在不完整的用户 Skill，请先检查", dir.display()));
        }
        missing.push(skill.clone());
        replace.push(skill.clone());
    }
    Ok((missing, replace))
}

fn built_in_skill_ids(item: &BuiltIn) -> Vec<String> {
    let mut ids = vec![built_in_setup_id(item)];
    if item.entry.is_some() { ids.push(item.skill_id.clone()); }
    ids.extend(item.skills.iter().cloned());
    ids
}

async fn set_built_in_enabled(app: &AppHandle, state: &AppState, item: &BuiltIn, enabled: bool) -> Result<Value, String> {
    let mut saved = built_in_state(item);
    if !built_in_ready(item, &saved) { return Err(format!("{} 尚未安装", item.name)); }
    if let Some(id) = item.preset_plugin_id.as_deref() {
        if !enabled || crate::plugins::is_installed(id) {
            crate::plugins::set_plugin_enabled(app, state, id, enabled).await?;
        }
    }
    let ids = built_in_skill_ids(item);
    crate::settings::update_settings(app, state, |next| {
        for id in &ids {
            next.chat_tools.disabled_skill_ids.retain(|skill| skill != id);
            if !enabled { next.chat_tools.disabled_skill_ids.push(id.clone()); }
        }
        Ok(())
    }).map_err(|e| e.to_string())?;
    if let Some(id) = saved.plugin_id.clone() {
        if market_companion_package(item, &id)?.is_some() {
            crate::plugins::packages::plugin_packages_set_enabled(app.clone(), app.state::<AppState>(), id, enabled).await?;
        }
    }
    saved.enabled = enabled;
    save_built_in_state(item, &saved)?;
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(built_in_local(item, &saved, true).unwrap())
}

async fn install_plugin(app: &AppHandle, item: &BuiltIn) -> Result<Value, String> {
    let previous = built_in_state(item);
    if built_in_ready(item, &previous) { return Ok(built_in_local(item, &previous, true).unwrap()); }
    let skills_root = crate::skills::user_skills_dir(app)?;
    let (missing, replace) = built_in_skill_plan(item, &previous, &skills_root)?;
    let stage = skills_root.parent().ok_or("无效技能目录")?
        .join("market-staging").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let mut moved = Vec::<String>::new();
    let mut backed_up = Vec::<String>::new();
    let had_setup = built_in_setup_installed(item);
    let had_entry = item.entry.is_some() && crate::skills::kivio_skills_dir()
        .is_some_and(|root| market_skill_installed_at(&root.join(&item.skill_id)));
    let mut result: Result<Value, String> = async {
        if !missing.is_empty() {
            let url = format!("https://codeload.github.com/{}/zip/{}", item.repository, item.revision);
            let bytes = download(&url, 30 * 1024 * 1024).await?;
            unpack_built_in_skills(item, bytes, &stage)?;
            for skill in &missing {
                let from = stage.join(skill);
                fs::write(from.join(".kivio-market-owner.json"), serde_json::to_vec(&json!({"id":item.id,"revision":item.revision})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let target = skills_root.join(skill);
                if replace.contains(skill) {
                    if fs::symlink_metadata(&target).is_err()
                        || !market_owned_skill(item, &previous, &skills_root, skill)
                    { return Err(format!("{} 安装期间已发生变化", target.display())); }
                    let backup = stage.join("backup").join(skill);
                    fs::create_dir_all(backup.parent().ok_or("无效备份路径")?).map_err(|e| e.to_string())?;
                    fs::rename(&target, backup).map_err(|e| e.to_string())?;
                    backed_up.push(skill.clone());
                } else if fs::symlink_metadata(&target).is_ok() {
                    return Err(format!("{} 安装期间已出现同名 Skill", target.display()));
                }
                fs::rename(&from, target).map_err(|e| e.to_string())?;
                moved.push(skill.clone());
            }
        }
        install_built_in_setup(item)?;
        install_built_in_entry(item)?;
        if let Some(id) = previous.plugin_id.as_deref() {
            if market_companion_package(item, id)?.is_some() {
                crate::plugins::packages::plugin_packages_remove(app.clone(), app.state::<AppState>(), id.to_string()).await?;
            }
        }
        let mut owned_skills = previous.owned_skills.clone();
        for skill in &moved { if !owned_skills.contains(skill) { owned_skills.push(skill.clone()); } }
        let state = BuiltInState { revision: Some(item.revision.clone()), plugin_id: None, owned_skills, enabled: true };
        if !built_in_ready(item, &state) { return Err(format!("{} 的组件没有完成注册", item.name)); }
        if let Some(id) = item.preset_plugin_id.as_deref() {
            if crate::plugins::is_installed(id) {
                crate::plugins::set_plugin_enabled(app, &app.state::<AppState>(), id, true).await?;
            }
        }
        save_built_in_state(item, &state)?;
        if item.project.is_some() { let _ = ensure_project(app, item); }
        let _ = app.emit("kivio-configuration-changed", ());
        Ok(built_in_local(item, &state, true).unwrap())
    }.await;
    let mut keep_stage = false;
    if result.is_err() {
        for skill in moved {
            if let Err(error) = fs::remove_dir_all(skills_root.join(&skill)) {
                keep_stage = true;
                result = Err(format!("修复失败，移除新 Skill {skill} 时失败：{error}"));
            }
        }
        for skill in backed_up {
            if let Err(error) = fs::rename(stage.join("backup").join(&skill), skills_root.join(&skill)) {
                keep_stage = true;
                result = Err(format!("修复失败，原 Skill 备份保留在 {}：{error}", stage.display()));
            }
        }
        if !had_setup { let _ = remove_built_in_setup(item); }
        if !had_entry { let _ = remove_built_in_entry(item); }
    }
    if !keep_stage { let _ = fs::remove_dir_all(stage); }
    result
}

async fn uninstall_plugin(app: &AppHandle, item: &BuiltIn) -> Result<Value, String> {
    let state = built_in_state(item);
    if state.revision.is_none() && state.plugin_id.is_none() { return Err(format!("{} 尚未安装", item.name)); }
    if let Some(id) = item.preset_plugin_id.as_deref() {
        crate::plugins::set_plugin_enabled(app, &app.state::<AppState>(), id, false).await?;
    }
    let skills_root = crate::skills::user_skills_dir(app)?;
    if let Some(plugin_id) = state.plugin_id.as_ref() {
        if market_companion_package(item, plugin_id)?.is_some() {
            crate::plugins::packages::plugin_packages_remove(app.clone(), app.state::<AppState>(), plugin_id.clone()).await?;
        }
    }
    for skill in &state.owned_skills {
        if !item.skills.iter().any(|name| name == skill) { continue; }
        let dir = skills_root.join(skill);
        if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) { continue; }
        let owned = fs::read(dir.join(".kivio-market-owner.json")).ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| value["id"] == item.id);
        if owned { fs::remove_dir_all(dir).map_err(|e| e.to_string())?; }
    }
    remove_built_in_setup(item)?;
    remove_built_in_entry(item)?;
    save_built_in_state(item, &BuiltInState::default())?;
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(json!({"removed":true}))
}
pub async fn finish_remove(app: &AppHandle, state: &AppState, conversation_id: &str, id: &str) -> Result<Value, String> {
    let _guard = mutation_lock().lock().await;
    let local = read_local(id)?;
    if local.uninstall_conversation_id.as_deref() != Some(conversation_id) {
        return Err("请从插件发起卸载对话".into());
    }
    if let Some(plugin) = local.plugin_id {
        if crate::plugins::packages::plugin_packages_list()?.iter().any(|p| p.id == plugin) {
            crate::plugins::packages::plugin_packages_remove(app.clone(), app.state::<AppState>(), plugin).await?;
        }
    }
    let _ = state;
    let archive = root()?.join("removed");
    fs::create_dir_all(&archive).map_err(|e|e.to_string())?;
    fs::rename(package_dir(id)?.join("record.json"), archive.join(format!("{id}-{}",uuid::Uuid::new_v4()))).map_err(|e|e.to_string())?;
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(json!({"removed":true}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    id: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    contains: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checks {
    checks: Vec<Check>,
}
pub async fn finalize(
    app: &AppHandle,
    state: &AppState,
    conversation_id: &str,
    id: &str,
    config: &Path,
) -> Result<Value, String> {
    let _guard = mutation_lock().lock().await;
    let mut local = read_local(id)?;
    if local.conversation_id.as_deref() != Some(conversation_id) {
        return Err("只能在该应用的安装对话中完成验收".into());
    }
    if local.status == "ready" {
        return Ok(json!(local));
    }
    let base = content(&local)?;
    let canonical = fs::canonicalize(&base).map_err(|e| e.to_string())?;
    let config = fs::canonicalize(config).map_err(|e| e.to_string())?;
    if !config.starts_with(&canonical) {
        return Err("验收文件必须位于包内".into());
    }
    let checks: Checks = serde_json::from_value(read_json(&config)?).map_err(|e| e.to_string())?;
    if checks.checks.is_empty() || checks.checks.len() > 16 {
        return Err("需提供 1–16 项真实验收检查".into());
    }
    for required in ["environment", "components", "configuration"] {
        if !checks.checks.iter().any(|check| check.id == required) {
            return Err(format!("安装检查缺少 {required}，请按照 INSTALL.md 完成环境、组件和服务配置"));
        }
    }
    let mut receipts = Vec::new();
    for check in checks.checks {
        let mut command = tokio::process::Command::new(&check.program);
        command
            .args(&check.args)
            .current_dir(&base)
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            use crate::proc::NoConsoleWindow;
            command.no_console_window();
        }
        let result = tokio::time::timeout(Duration::from_secs(60), command.output())
            .await
            .map_err(|_| format!("验收 {} 超时", check.id))?
            .map_err(|e| format!("验收 {} 无法运行：{e}", check.id))?;
        if !result.status.success()
            || check
                .contains
                .as_ref()
                .is_some_and(|needle| !String::from_utf8_lossy(&result.stdout).contains(needle))
        {
            local.error = Some(format!("验收 {} 未通过，请修复后继续", check.id));
            save_local(&local)?;
            return Err(local.error.unwrap());
        }
        receipts
            .push(json!({"id":check.id,"passed":true,"checkedAt":chrono::Utc::now().to_rfc3339()}));
    }
    write_json(&base.join("verification-results.json"), &receipts)?;
    // A retry must import the repaired staging files, never reuse a stale copy.
    if let Some(existing) = &local.plugin_id {
        crate::plugins::packages::plugin_packages_set_enabled(
            app.clone(),
            app.state::<AppState>(),
            existing.clone(),
            false,
        )
        .await?;
    }
    let package = crate::plugins::packages::plugin_packages_import(
        base.join("runtime").to_string_lossy().into_owned(),
        None,
    )
    .await?;
    let plugin_id = package.id;
    local.plugin_id = Some(plugin_id.clone());
    save_local(&local)?;
    crate::plugins::packages::plugin_packages_set_enabled(
        app.clone(),
        app.state::<AppState>(),
        plugin_id.clone(),
        true,
    )
    .await?;
    let settings = state.settings_read().clone();
    let registry = crate::skills::build_registry_metadata_in(
        app,
        &settings.chat_tools.skill_scan_paths,
        None,
    )?;
    let marker = format!("/packages/{}/content/", plugin_id);
    let skill = registry
        .metas()
        .iter()
        .find(|m| {
            m.path.as_deref().is_some_and(|p| {
                p.replace('\\', "/").contains(&marker)
                    && p.replace('\\', "/")
                        .ends_with("/skills/market-entry/SKILL.md")
            })
        })
        .map(|m| m.id.clone());
    let Some(skill) = skill else {
        crate::plugins::packages::plugin_packages_set_enabled(
            app.clone(),
            app.state::<AppState>(),
            plugin_id,
            false,
        )
        .await?;
        return Err("已导入插件但没有找到应用入口 Skill，请保留并修复".into());
    };
    local.skill_id = Some(skill);
    local.status = "ready".into();
    local.enabled = true;
    local.error = None;
    save_local(&local)?;
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(json!(local))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plugin(id: &str) -> BuiltIn {
        load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins"))
            .unwrap()
            .plugins
            .into_iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }
    #[test]
    fn built_in_skills_have_setup_skills_and_market_entries() {
        let hypit = plugin("hypit");
        let remotion = plugin("remotion-agent-skills");
        let whiteboard = plugin("srt-whiteboard-animation");
        let feishu = plugin("feishu-cli");
        let resolve = plugin("davinci-resolve");
        assert!(hypit.setup.contains("github.com/hypit-ai/hypit"));
        assert!(hypit.setup.contains("~/.kivio/skills/hypit"));
        assert!(remotion.setup.contains("github.com/remotion-dev/skills"));
        assert!(remotion.setup.contains("remotion-best-practices"));
        assert!(whiteboard.setup.contains("github.com/geeklee/srt-whiteboard-animation"));
        assert!(whiteboard.setup.contains("python scripts/prepare_env.py --check"));
        assert!(feishu.setup.contains("lark-cli auth status --json --verify"));
        assert!(feishu.setup.contains("lark-cli auth check --scope"));
        assert!(feishu.setup.contains("最小只读请求"));
        assert!(feishu.entry.unwrap().contains("lark-im"));
        assert!(feishu.skills.is_empty());
        assert!(resolve.setup.contains("File > Setup AI Assistants"));
        assert!(resolve.setup.contains("mcp_upsert"));
        assert!(resolve.entry.unwrap().contains("official MCP"));
        assert!(resolve.skills.is_empty());
        let snapshot = built_in_snapshot(&load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins")).unwrap());
        assert_eq!(snapshot["entries"][0]["source"]["kind"], "built-in");
        assert_eq!(snapshot["entries"][0]["id"], "hypit");
        assert_eq!(snapshot["entries"][0]["manifest"]["icon"], "assets/hypit-logo.svg");
        assert_eq!(snapshot["entries"][1]["id"], "remotion-agent-skills");
        assert_eq!(snapshot["entries"][1]["manifest"]["icon"], "assets/remotion-logo.svg");
        assert_eq!(snapshot["entries"][2]["id"], "srt-whiteboard-animation");
        assert_eq!(snapshot["entries"][2]["manifest"]["icon"], "assets/srt-whiteboard-logo.svg");
        assert_eq!(snapshot["entries"][3]["id"], "feishu-cli");
        assert_eq!(snapshot["entries"][3]["manifest"]["startPrompt"], "使用飞书 CLI，告诉我可以做什么。");
        assert_eq!(snapshot["entries"][3]["manifest"]["categoryIds"][0], "productivity");
        assert_eq!(snapshot["entries"][3]["manifest"]["skillIds"].as_array().unwrap().len(), 1);
        assert!(hypit.icon.contains("viewBox=\"54.9 170.6 252.6 252.6\""));
        assert!(remotion.icon.contains("viewBox=\"0 0 410 425\""));
        assert!(whiteboard.icon.contains("viewBox=\"0 0 64 64\""));
        assert!(feishu.icon.contains("viewBox=\"0 0 24 24\""));
        assert_eq!(snapshot["entries"][4]["id"], "davinci-resolve");
        assert_eq!(snapshot["entries"][4]["manifest"]["categoryIds"][0], "videos");
        assert_eq!(snapshot["entries"][4]["manifest"]["icon"], "assets/davinci-resolve-logo.svg");
        assert!(resolve.icon.contains("viewBox=\"0 0 24 24\""));
        let fanpai = plugin("daihuo-fanpai");
        assert!(fanpai.setup.contains("github.com/wangcanyu/daihuo-fanpai"));
        assert!(fanpai.setup.contains("python3 doctor.py"));
        assert_eq!(snapshot["entries"][5]["id"], "daihuo-fanpai");
        assert_eq!(snapshot["entries"][5]["manifest"]["categoryIds"][0], "videos");
        assert_eq!(snapshot["entries"][5]["manifest"]["icon"], "assets/daihuo-fanpai-logo.svg");
        assert!(fanpai.icon.contains("viewBox=\"0 0 48 48\""));
        let jianying = plugin("jianying-editor");
        assert!(jianying.setup.contains("github.com/luoluoluo22/jianying-editor-skill"));
        assert!(jianying.setup.contains("scripts/jy_wrapper.py"));
        assert_eq!(snapshot["entries"][6]["id"], "jianying-editor");
        assert_eq!(snapshot["entries"][6]["manifest"]["categoryIds"][0], "videos");
        assert_eq!(snapshot["entries"][6]["manifest"]["icon"], "assets/jianying-editor-logo.svg");
        assert!(jianying.icon.contains("viewBox=\"0 0 32 32\""));
        let wecom = plugin("wecom-cli");
        assert!(wecom.setup.contains("npx skills add WeComTeam/wecom-cli -y -g"));
        assert!(wecom.setup.contains("wecom-cli auth show --status"));
        assert!(wecom.entry.as_deref().unwrap().contains("npx skills add WeComTeam/wecom-cli -y -g"));
        assert!(wecom.skills.is_empty());
        assert_eq!(snapshot["entries"][7]["id"], "wecom-cli");
        assert_eq!(snapshot["entries"][7]["manifest"]["categoryIds"][0], "productivity");
        assert_eq!(snapshot["entries"][7]["manifest"]["icon"], "assets/wecom-logo.svg");
        assert_eq!(snapshot["entries"][7]["manifest"]["skillIds"].as_array().unwrap().len(), 1);
        assert!(wecom.icon.contains("viewBox=\"0 0 24 24\""));
        let ziniao = plugin("ziniao-cli");
        assert_eq!(ziniao.preset_plugin_id.as_deref(), Some("ziniao-cli"));
        assert!(ziniao.setup.contains("ziniao-cli doctor"));
        assert!(ziniao.entry.as_deref().unwrap().contains("ziniao-shared"));
        assert!(ziniao.skills.is_empty());
        let shopkeeper = plugin("1688-shopkeeper");
        assert_eq!(shopkeeper.repository, "next-1688/1688-shopkeeper");
        assert!(shopkeeper.setup.contains("ALI_1688_AK"));
        assert!(shopkeeper.setup.contains("--dry-run"));
        assert_eq!(snapshot["entries"][8]["id"], "1688-shopkeeper");
        assert_eq!(snapshot["entries"][8]["manifest"]["categoryIds"][0], "commerce");
        assert_eq!(snapshot["entries"][8]["manifest"]["icon"], "assets/1688-shopkeeper-logo.svg");
        assert_eq!(snapshot["entries"][9]["id"], "ziniao-cli");
        assert_eq!(snapshot["entries"][9]["manifest"]["categoryIds"][0], "productivity");
        let shopify = plugin("shopify-ai-toolkit");
        assert_eq!(shopify.repository, "Shopify/Shopify-AI-Toolkit");
        assert_eq!(shopify.skill_id, "shopify-use-shopify-cli");
        assert_eq!(shopify.skills, ["shopify-use-shopify-cli", "shopify-admin", "shopify-shopifyql"]);
        assert!(shopify.setup.contains("shopify store auth list"));
        assert!(shopify.icon.contains("viewBox=\"0 0 109.5 124.5\""));
        assert_eq!(snapshot["entries"][10]["id"], "shopify-ai-toolkit");
        assert_eq!(snapshot["entries"][10]["manifest"]["categoryIds"][0], "commerce");
        assert_eq!(snapshot["entries"][10]["manifest"]["icon"], "assets/shopify-ai-toolkit-logo.svg");
    }
    #[test]
    fn built_in_status_requires_all_skill_files_and_setup() {
        let item = plugin("remotion-agent-skills");
        let dir = tempfile::tempdir().unwrap();
        let mut state = BuiltInState::default();
        for skill in &item.skills {
            let target = dir.path().join(skill);
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("SKILL.md"), "# Remotion").unwrap();
        }
        assert!(!built_in_ready_at(&item, &state, dir.path()));
        let setup = dir.path().join(built_in_setup_id(&item));
        fs::create_dir_all(&setup).unwrap();
        fs::write(setup.join("SKILL.md"), &item.setup).unwrap();
        assert!(!built_in_ready_at(&item, &state, dir.path()));
        state.revision = Some(item.revision.clone());
        assert!(built_in_ready_at(&item, &state, dir.path()));
        fs::remove_file(dir.path().join("remotion-render/SKILL.md")).unwrap();
        assert!(!built_in_ready_at(&item, &state, dir.path()));
    }
    #[test]
    fn hypit_adapter_ships_with_its_setup_and_is_removed_with_it() {
        let hypit = plugin("hypit");
        let names: Vec<&str> = hypit.adapter_files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(names, ["package.json", "src/activation.js", "src/models.js", "src/provider.js"], "tests stay in the repository");
        // Every other setup ships no adapter: the Skill decides by naming the path.
        for item in load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins")).unwrap().plugins {
            assert_eq!(item.adapter_files.is_empty(), !item.setup.contains(ADAPTER_PATH), "{}", item.id);
        }
        // The adapter is a bridge only: no service address, no key, no vendor protocol.
        for (path, text) in &hypit.adapter_files {
            for forbidden in ["fetch(", "https://", "Authorization", "apiKey:", "settings.json", "bearer"] {
                assert!(!text.to_ascii_lowercase().contains(&forbidden.to_ascii_lowercase()), "{path} 含有 {forbidden}");
            }
        }
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hypit-setup");
        install_market_skill(&dir, &hypit.setup).unwrap();
        for (relative, content) in &hypit.adapter_files {
            let file = dir.join(ADAPTER_PATH).join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }
        assert!(dir.join("adapter/src/provider.js").is_file());
        assert!(!dir.join("adapter/test").exists());
        remove_market_skill(&dir).unwrap();
        assert!(!dir.exists(), "adapter files are removed with the setup Skill");
    }
    #[test]
    fn shipped_setup_files_replace_stale_installs_and_rerun_setup_only_when_they_changed() {
        let hypit = plugin("hypit");
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hypit-setup");
        // An install from an older release: older Skill text marked done, an older adapter.
        let old = hypit.setup.replacen(
            "description: Check Dsivio integration",
            "description: Old text",
            1,
        );
        let old = old.replacen("\nkivio-market-managed", " [setup completed once]\nkivio-market-managed", 1);
        install_market_skill(&dir, &old).unwrap();
        assert!(setup_description_done(&old));
        fs::create_dir_all(dir.join("adapter/src")).unwrap();
        fs::write(dir.join("adapter/src/provider.js"), "// stale").unwrap();
        fs::write(dir.join("adapter/src/removed.js"), "// gone upstream").unwrap();

        sync_setup_files(&hypit, &dir).unwrap();
        let skill = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(skill.contains("description: Check Dsivio integration"), "the Skill text follows the catalog");
        assert!(!setup_description_done(&skill), "changed shipped files must be set up again (copied into the project)");
        let (_, provider) = hypit.adapter_files.iter().find(|(path, _)| path == "src/provider.js").unwrap();
        assert_eq!(&fs::read_to_string(dir.join("adapter/src/provider.js")).unwrap(), provider);
        assert!(!dir.join("adapter/src/removed.js").exists());

        // Setup completes against these files; a later sync with nothing changed keeps the marker.
        let done = skill.replacen("\nkivio-market-managed", " [setup completed once]\nkivio-market-managed", 1);
        fs::write(dir.join("SKILL.md"), &done).unwrap();
        sync_setup_files(&hypit, &dir).unwrap();
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), done);
    }
    #[test]
    fn hypit_project_prompt_follows_the_plugin_installation() {
        let catalog = load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins")).unwrap();
        let installed = |_: &BuiltIn| true;
        assert!(project_prompt_in(&catalog, "Hypit", installed).is_some());
        // Uninstalled: nothing is added. Other projects and other plugins never receive it.
        assert!(project_prompt_in(&catalog, "Hypit", |_| false).is_none());
        assert!(project_prompt_in(&catalog, "My own project", installed).is_none());
    }
    #[test]
    fn dsivio_reference_is_installed_only_for_setups_that_read_it() {
        let catalog = load_catalog_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/plugins")).unwrap();
        for item in &catalog.plugins {
            assert_eq!(item.dsivio_reference.is_some(), item.setup.contains(DSIVIO_REFERENCE_PATH), "{}", item.id);
        }
        let hypit = catalog.plugins.iter().find(|item| item.id == "hypit").unwrap();
        let text = hypit.dsivio_reference.as_deref().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hypit-setup");
        install_market_skill(&target, &hypit.setup).unwrap();
        fs::create_dir_all(target.join("references")).unwrap();
        fs::write(target.join(DSIVIO_REFERENCE_PATH), text).unwrap();
        remove_market_skill(&target).unwrap();
        assert!(!target.exists());
    }
    #[test]
    fn project_root_avoids_the_system_drive() {
        let docs = Some(PathBuf::from("/Users/a/Documents"));
        assert_eq!(project_root_for("Hypit", None, docs.clone()), Some(PathBuf::from("/Users/a/Documents/Dsivio/Hypit")));
        assert_eq!(project_root_for("Hypit", Some("D:\\".into()), docs), Some(PathBuf::from("D:\\").join("dsiviowork").join("Hypit")));
        assert_eq!(project_root_for("Hypit", None, None), None);
        assert_eq!(work_drive_root(|root| root == "E:\\" || root == "F:\\"), Some("E:\\".into()));
        assert_eq!(work_drive_root(|_| false), None);
    }
    #[test]
    fn setup_done_only_when_description_line_ends_with_marker() {
        let head = "---\nname: x-setup\n";
        let tail = "kivio-market-managed: true\n---\n# body [setup completed once]\n";
        assert!(!setup_description_done(&format!("{head}description: Check env.\n{tail}")));
        assert!(setup_description_done(&format!("{head}description: Check env. [setup completed once]\n{tail}")));
        assert!(setup_description_done(&format!("{head}description: Check env. [setup completed once]  \n{tail}")));
        assert!(!setup_description_done(&format!("{head}description: [setup completed once] Check env.\n{tail}")));
        assert!(!setup_description_done(&format!("{head}description: [setup completed once]\n{tail}")));
        assert!(!setup_description_done("description: Check. [setup completed once]\n"));
        assert!(!setup_description_done(""));
    }
    #[test]
    fn feishu_status_requires_entry_and_setup() {
        let item = plugin("feishu-cli");
        let dir = tempfile::tempdir().unwrap();
        let state = BuiltInState { revision: Some(item.revision.clone()), ..Default::default() };
        let setup = dir.path().join(built_in_setup_id(&item));
        fs::create_dir_all(&setup).unwrap();
        fs::write(setup.join("SKILL.md"), &item.setup).unwrap();
        assert!(!built_in_ready_at(&item, &state, dir.path()));
        let entry = dir.path().join(&item.skill_id);
        fs::create_dir_all(&entry).unwrap();
        fs::write(entry.join("SKILL.md"), item.entry.as_deref().unwrap()).unwrap();
        assert!(built_in_ready_at(&item, &state, dir.path()));
        fs::write(setup.join("SKILL.md"), "---\nname: feishu-cli-setup\ndescription: setup completed once\nkivio-market-managed: true\n---\n").unwrap();
        assert!(built_in_ready_at(&item, &state, dir.path()));
        fs::write(entry.join("SKILL.md"), "---\nkivio-market-managed: true\n---\n").unwrap();
        assert!(!built_in_ready_at(&item, &state, dir.path()));
        fs::write(entry.join("SKILL.md"), item.entry.as_deref().unwrap()).unwrap();
        fs::remove_file(setup.join("SKILL.md")).unwrap();
        assert!(!built_in_ready_at(&item, &state, dir.path()));
        assert!(built_in_local(&item, &state, false).is_some());
        assert!(built_in_local(&item, &BuiltInState::default(), false).is_none());
    }
    #[test]
    fn repair_replaces_only_incomplete_market_owned_skills() {
        let item = plugin("hypit");
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("hypit");
        fs::create_dir_all(&skill).unwrap();
        let mut state = BuiltInState::default();
        state.owned_skills.push("hypit".into());
        assert!(built_in_skill_plan(&item, &state, dir.path()).is_err());
        fs::write(skill.join(".kivio-market-owner.json"), r#"{"id":"hypit"}"#).unwrap();
        let (missing, replace) = built_in_skill_plan(&item, &state, dir.path()).unwrap();
        assert_eq!(missing, vec!["hypit"]);
        assert_eq!(replace, vec!["hypit"]);
        fs::write(skill.join("SKILL.md"), "# Hypit").unwrap();
        assert_eq!(built_in_skill_plan(&item, &state, dir.path()).unwrap(), (vec![], vec![]));
    }
    #[test]
    fn built_in_archive_extracts_only_the_official_skill_subtree() {
        use std::io::Write;
        let item = plugin("hypit");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("hypit-revision/skills/hypit/SKILL.md", options).unwrap();
        zip.write_all(b"# Hypit").unwrap();
        zip.start_file("hypit-revision/skills/hypit/references/check.md", options).unwrap();
        zip.write_all(b"check").unwrap();
        zip.start_file("hypit-revision/packages/unrelated.txt", options).unwrap();
        zip.write_all(b"skip").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, bytes, dir.path()).unwrap();
        assert!(dir.path().join("hypit/SKILL.md").is_file());
        assert!(dir.path().join("hypit/references/check.md").is_file());
        assert!(!dir.path().join("packages").exists());
    }
    #[test]
    fn daihuo_fanpai_archive_keeps_root_scripts_and_skips_showcase() {
        use std::io::Write;
        let item = plugin("daihuo-fanpai");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("daihuo-fanpai-revision/SKILL.md", options).unwrap();
        zip.write_all(b"# fanpai").unwrap();
        zip.start_file("daihuo-fanpai-revision/doctor.py", options).unwrap();
        zip.write_all(b"print('ok')").unwrap();
        zip.start_file("daihuo-fanpai-revision/route.py", options).unwrap();
        zip.write_all(b"print('route')").unwrap();
        zip.start_file("daihuo-fanpai-revision/deliver.py", options).unwrap();
        zip.write_all(b"print('deliver')").unwrap();
        zip.start_file("daihuo-fanpai-revision/showcase/demo.mp4", options).unwrap();
        zip.write_all(b"video").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, bytes, dir.path()).unwrap();
        assert!(dir.path().join("daihuo-fanpai/SKILL.md").is_file());
        assert!(dir.path().join("daihuo-fanpai/doctor.py").is_file());
        assert!(!dir.path().join("daihuo-fanpai/showcase").exists());
    }
    #[test]
    fn jianying_editor_archive_maps_root_and_skips_github() {
        use std::io::Write;
        let item = plugin("jianying-editor");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("jianying-editor-skill-revision/SKILL.md", options).unwrap();
        zip.write_all(b"# jianying").unwrap();
        zip.start_file("jianying-editor-skill-revision/requirements.txt", options).unwrap();
        zip.write_all(b"pyjianying\n").unwrap();
        zip.start_file("jianying-editor-skill-revision/scripts/jy_wrapper.py", options).unwrap();
        zip.write_all(b"print('ok')").unwrap();
        zip.start_file("jianying-editor-skill-revision/.github/workflows/ci.yml", options).unwrap();
        zip.write_all(b"ci").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, bytes, dir.path()).unwrap();
        assert!(dir.path().join("jianying-editor/SKILL.md").is_file());
        assert!(dir.path().join("jianying-editor/scripts/jy_wrapper.py").is_file());
        assert!(!dir.path().join("jianying-editor/.github").exists());
    }
    #[test]
    fn whiteboard_status_requires_the_complete_skill() {
        let item = plugin("srt-whiteboard-animation");
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join(&item.skill_id);
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Whiteboard").unwrap();
        assert!(!built_in_skill_installed_at(&item, dir.path()));
        for file in &item.required_files {
            let path = skill.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "ready").unwrap();
        }
        assert!(built_in_skill_installed_at(&item, dir.path()));
        assert_eq!(built_in_local(&item, &BuiltInState::default(), true).unwrap()["status"], "ready");
        fs::remove_file(skill.join("scripts/render_stream_whiteboard.py")).unwrap();
        assert!(!built_in_skill_installed_at(&item, dir.path()));
    }
    #[test]
    fn local_catalog_lists_multiple_packages() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
        let catalog = draft_catalogs_at(&root).unwrap();
        let entries = catalog["entries"].as_array().unwrap();
        for id in ["srt-whiteboard-animation"] {
            let item = entries.iter().find(|e|e["id"] == id).unwrap();
            let package = Path::new(item["source"]["directory"].as_str().unwrap());
            assert_eq!(draft_descriptor(package).unwrap()["manifest"]["id"], id);
            assert!(package.join("INSTALL.md").is_file());
            assert!(package.join("DSIVIO.md").is_file());
        }
    }
    #[test]
    fn local_trial_has_snapshot_identity_without_claiming_verification() {
        let dir = tempfile::tempdir().unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/srt-whiteboard-animation");
        let package = dir.path().join("srt-whiteboard-animation");
        fs::create_dir(&package).unwrap();
        fs::copy(source.join("market.json"), package.join("market.json")).unwrap();
        for name in ["INSTALL.md", "DSIVIO.md"] {
            fs::copy(source.join(name), package.join(name)).unwrap();
        }
        let before = draft_catalog_at(&package).unwrap();
        assert_eq!(before["entries"][0]["manifest"]["verification"], json!([]));
        let mut remote = before["entries"][0].clone();
        remote["source"] = json!({"repository":"https://github.com/example/repo","revision":"a".repeat(40),"directory":"packages/srt-whiteboard-animation"});
        assert!(validate_manifest(&remote["manifest"], &remote).is_err());
        fs::write(package.join("INSTALL.md"), "updated").unwrap();
        let after = draft_catalog_at(&package).unwrap();
        assert_ne!(before["entries"][0]["source"]["revision"], after["entries"][0]["source"]["revision"]);
        fs::create_dir(package.join("data")).unwrap();
        fs::write(package.join("data/key.txt"), "not distributed").unwrap();
        assert_eq!(after["entries"][0]["source"]["revision"], draft_catalog_at(&package).unwrap()["entries"][0]["source"]["revision"]);
    }
    #[cfg(unix)]
    #[test]
    fn local_trial_rejects_symlink_escape() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.path().join("outside")).unwrap();
        assert!(draft_files(dir.path()).is_err());
    }
    #[test]
    fn source_is_pinned_and_contained() {
        let source = json!({"repository":"https://github.com/example/repo","revision":"a".repeat(40),"directory":"packages/image-app"});
        assert!(source_parts(&source, "image-app").is_ok());
        let mut bad = source.clone();
        bad["revision"] = json!("main");
        assert!(source_parts(&bad, "image-app").is_err());
        bad = source;
        bad["directory"] = json!("packages/../secrets");
        assert!(source_parts(&bad, "image-app").is_err());
    }
    #[test]
    fn example_rejects_active_content_and_escape() {
        let mut e = json!({"schemaVersion":1,"messages":[{"role":"user","text":"需求"},{"role":"assistant","text":"结果","attachments":[{"type":"image","path":"assets/result.png","label":"结果"}]}]});
        assert!(validate_example(&e).is_ok());
        e["messages"][1]["attachments"][0]["path"] = json!("assets/../../secret");
        assert!(validate_example(&e).is_err());
        e["messages"][1]["role"] = json!("system");
        assert!(validate_example(&e).is_err());
    }
    #[test]
    fn missing_catalog_is_empty_only_before_first_publication() {
        assert!(missing_catalog_error(&json!({"entries":[],"refreshedAt":null})).is_none());
        assert!(missing_catalog_error(&json!({"entries":[],"refreshedAt":123})).is_some());
        assert!(missing_catalog_error(&json!({"entries":[{"id":"test"}]})).is_some());
    }
    #[test]
    fn version_comparison_is_numeric() {
        assert!(version("1.10.0") > version("1.9.0"));
        assert_eq!(version("1.0"), None);
        assert!(!id_ok("../escape"));
    }

    fn skill_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, content) in entries {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn subdir_plugin() -> BuiltIn {
        let mut item = plugin("hypit");
        item.id = "test-video".into();
        item.skill_id = "test-video".into();
        item.skills = vec!["test-video".into()];
        item.subdir = Some("plugins/test-video".into());
        item
    }

    #[test]
    fn subdir_extracts_only_selected_skills_and_preserves_hypit() {
        let item = subdir_plugin();
        let bytes = skill_archive(&[
            ("repo/plugins/test-video/skills/test-video/SKILL.md", b"video"),
            ("repo/plugins/test-video/skills/test-video/references/guide.md", b"guide"),
            ("repo/plugins/test-video/package.json", b"not a Skill"),
            ("repo/plugins/test-video-evil/skills/test-video/evil.txt", b"skip"),
            ("repo/skills/hypit/SKILL.md", b"hypit"),
        ]);
        let stage = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&plugin("hypit"), bytes.clone(), stage.path()).unwrap();
        unpack_built_in_skills(&item, bytes, stage.path()).unwrap();
        assert_eq!(fs::read(stage.path().join("hypit/SKILL.md")).unwrap(), b"hypit");
        assert_eq!(fs::read(stage.path().join("test-video/SKILL.md")).unwrap(), b"video");
        assert_eq!(fs::read(stage.path().join("test-video/references/guide.md")).unwrap(), b"guide");
        assert!(!stage.path().join("test-video/evil.txt").exists());
        assert!(!stage.path().join("test-video/package.json").exists());
        assert!(!stage.path().join("plugins").exists());
    }

    #[test]
    fn subdir_root_mapping_and_required_files() {
        let mut item = subdir_plugin();
        item.unpack = "root".into();
        item.required_files = vec!["bin/entry.js".into()];
        item.skip = vec!["showcase/".into()];
        let bytes = skill_archive(&[
            ("repo/plugins/test-video/SKILL.md", b"video"),
            ("repo/plugins/test-video/bin/entry.js", b"entry"),
            ("repo/plugins/test-video/showcase/demo.mp4", b"skip"),
            ("repo/plugins/other/bin/entry.js", b"skip"),
        ]);
        let stage = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, bytes, stage.path()).unwrap();
        assert_eq!(fs::read(stage.path().join("test-video/bin/entry.js")).unwrap(), b"entry");
        assert!(!stage.path().join("test-video/showcase").exists());
        let missing = skill_archive(&[("repo/plugins/test-video/SKILL.md", b"video")]);
        assert!(unpack_built_in_skills(&item, missing, tempfile::tempdir().unwrap().path()).is_err());
    }

    #[test]
    fn subdir_requires_a_real_skill_not_an_empty_or_sibling_directory() {
        let item = subdir_plugin();
        for path in [
            "repo/plugins/test-video-evil/skills/test-video/SKILL.md",
            "repo/plugins/test-video/skills/test-video/README.md",
            "repo/skills/test-video/SKILL.md",
        ] {
            assert!(unpack_built_in_skills(&item, skill_archive(&[(path, b"x")]), tempfile::tempdir().unwrap().path()).is_err());
        }
    }

    #[test]
    fn subdir_rejects_unsafe_components_and_unselected_archive_paths() {
        for path in ["", "/plugins/video", "plugins/video/", "plugins//video", ".", "..",
            "plugins/./video", "plugins/../video", "C:/video", "plugins\\video", "plugins/\0video"] {
            assert!(validate_subdir(path).is_err(), "{path:?}");
        }
        let item = subdir_plugin();
        for path in ["repo/other/../escape", "repo/other/C:/escape", "repo/other\\escape", "/repo/escape", "repo//escape"] {
            let archive = skill_archive(&[
                ("repo/plugins/test-video/skills/test-video/SKILL.md", b"valid"),
                (path, b"bad"),
            ]);
            assert!(unpack_built_in_skills(&item, archive, tempfile::tempdir().unwrap().path()).is_err(), "{path}");
        }
    }

    #[test]
    fn unselected_symlinks_are_skipped_but_selected_symlinks_are_rejected() {
        // hypit 仓库自带 .claude/skills/hypit → skills/hypit；不解压的链接不能挡住安装。
        let options = zip::write::SimpleFileOptions::default();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.add_symlink("repo/.claude/skills/hypit", "../../skills/hypit", options).unwrap();
        zip.start_file("repo/skills/hypit/SKILL.md", options).unwrap();
        std::io::Write::write_all(&mut zip, b"hypit").unwrap();
        let stage = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&plugin("hypit"), zip.finish().unwrap().into_inner(), stage.path()).unwrap();
        assert_eq!(fs::read_to_string(stage.path().join("hypit/SKILL.md")).unwrap(), "hypit");
        assert!(!stage.path().join(".claude").exists());

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("repo/skills/hypit/SKILL.md", options).unwrap();
        zip.add_symlink("repo/skills/hypit/escape", "/outside", options).unwrap();
        let stage = tempfile::tempdir().unwrap();
        assert!(unpack_built_in_skills(&plugin("hypit"), zip.finish().unwrap().into_inner(), stage.path()).is_err());
        assert!(fs::symlink_metadata(stage.path().join("hypit/escape")).is_err());
    }

    #[test]
    fn subdir_enforces_size_and_file_count_boundaries() {
        let item = subdir_plugin();
        let limit = vec![0; 30 * 1024 * 1024];
        let stage = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, skill_archive(&[("repo/plugins/test-video/skills/test-video/SKILL.md", &limit)]), stage.path()).unwrap();
        assert_eq!(fs::metadata(stage.path().join("test-video/SKILL.md")).unwrap().len(), limit.len() as u64);
        let oversized = vec![0; limit.len() + 1];
        let stage = tempfile::tempdir().unwrap();
        let bytes = skill_archive(&[("repo/plugins/test-video/skills/test-video/SKILL.md", &oversized)]);
        assert_eq!(unpack_built_in_skills(&item, bytes, stage.path()).unwrap_err(), "Skill 内容超过限制");
        assert!(!stage.path().join("test-video/SKILL.md").exists());
        let mut paths = vec!["repo/plugins/test-video/skills/test-video/SKILL.md".to_string()];
        paths.extend((1..2001).map(|i| format!("repo/plugins/test-video/skills/test-video/{i}")));
        let entries: Vec<_> = paths.iter().map(|path| (path.as_str(), b"x".as_slice())).collect();
        let stage = tempfile::tempdir().unwrap();
        unpack_built_in_skills(&item, skill_archive(&entries[..2000]), stage.path()).unwrap();
        assert!(stage.path().join("test-video/1999").is_file());
        assert_eq!(unpack_built_in_skills(&item, skill_archive(&entries), tempfile::tempdir().unwrap().path()).unwrap_err(), "Skill 内容超过限制");
    }

}
