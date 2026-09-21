//! Curated GitHub market. Content never comes from bundled resources.
//! Installers prepare an isolated plugin; only successful machine checks publish it.
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
        .map_err(|e| format!("无法连接应用市场：{e}"))?;
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
    write_json(
        &root()?.join("catalog-cache.json"),
        &json!({"categories":categories,"entries":resolved,"refreshedAt":chrono::Utc::now().timestamp_millis()}),
    )?;
    Ok(snapshot(None))
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
    if action == "snapshot" {
        return Ok(snapshot(None));
    }
    if action == "refresh" {
        return Ok(match refresh().await {
            Ok(s) => s,
            Err(e) => snapshot(Some(e)),
        });
    }
    let id = str_field(&request, "id")?;
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
        _ => return Err("未知市场操作".into()),
    };
    let _ = app.emit("kivio-configuration-changed", ());
    Ok(value)
}
pub async fn finish_remove(app: &AppHandle, state: &AppState, conversation_id: &str, id: &str) -> Result<Value, String> {
    let _guard = mutation_lock().lock().await;
    let local = read_local(id)?;
    if local.uninstall_conversation_id.as_deref() != Some(conversation_id) {
        return Err("请从应用市场发起卸载对话".into());
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
    #[test]
    fn local_catalog_lists_multiple_packages() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
        let catalog = draft_catalogs_at(&root).unwrap();
        let entries = catalog["entries"].as_array().unwrap();
        for id in ["hypit", "srt-whiteboard-animation"] {
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
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/hypit");
        let package = dir.path().join("hypit");
        fs::create_dir(&package).unwrap();
        fs::copy(source.join("market.json"), package.join("market.json")).unwrap();
        for name in ["INSTALL.md", "DSIVIO.md"] {
            fs::copy(source.join(name), package.join(name)).unwrap();
        }
        let before = draft_catalog_at(&package).unwrap();
        assert_eq!(before["entries"][0]["manifest"]["verification"], json!([]));
        let mut remote = before["entries"][0].clone();
        remote["source"] = json!({"repository":"https://github.com/example/repo","revision":"a".repeat(40),"directory":"packages/hypit"});
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
}
