use super::{builtins, files::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub fn root() -> Result<PathBuf, String> {
    let root = crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("image-studio");
    fs::create_dir_all(root.join("templates")).map_err(|e| e.to_string())?;
    Ok(root)
}
#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    #[ts(type = "Record<string, unknown>")]
    pub data: Value,
    pub directory: String,
    pub builtin: bool,
}

pub fn templates() -> Result<Vec<Template>, String> {
    list_at(&root()?)
}

pub fn list_at(base: &Path) -> Result<Vec<Template>, String> {
    fs::create_dir_all(base.join("templates")).map_err(|e| e.to_string())?;
    let mut all = builtins::templates(&base)?;
    scan_templates(&base, &base.join("templates"), 0, &mut all)?;
    Ok(all)
}

/// template.json is the shared source of truth; record.json only preserves UI identity.
pub(crate) fn scan_templates(
    base: &Path,
    folder: &Path,
    depth: usize,
    all: &mut Vec<Template>,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    if depth > 2 {
        return Ok(());
    }
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_symlink() || !path.is_dir() {
            continue;
        }
        let relative = path
            .strip_prefix(base)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if all.iter().any(|t| t.directory == relative) {
            continue;
        }
        let metadata = read::<Template>(&path.join("record.json")).ok();
        let standard = path.join("template.json");
        // Migrate UI templates created before the shared directory contract.
        if !standard.exists() {
            if let Some(ref record) = metadata {
                write(&standard, &record.data)?;
            }
        }
        if standard.is_file() {
            if let Ok(mut data) = read::<Value>(&standard) {
                if depth > 0 {
                    let Ok(requirements) = read::<Value>(&folder.join("要求.json")) else {
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !requirements["templates"]
                        .as_array()
                        .is_some_and(|names| names.iter().any(|n| n.as_str() == Some(&name)))
                    {
                        continue;
                    }
                    for field in ["language", "style", "brand"] {
                        if data[field].is_null()
                            || data[field] == ""
                            || data[field] == serde_json::json!({})
                        {
                            if !requirements[field].is_null() {
                                data[field] = requirements[field].clone();
                            }
                        }
                    }
                    if !data["output"].is_object() {
                        data["output"] = serde_json::json!({});
                    }
                    for field in ["resolution", "format", "quality", "deliver"] {
                        if data["output"][field].is_null() || data["output"][field] == "" {
                            if !requirements["generation"][field].is_null() {
                                data["output"][field] = requirements["generation"][field].clone();
                            }
                        }
                    }
                }
                if validate_template(&data).is_ok() {
                    let id = metadata.map(|t| t.id).unwrap_or_else(|| {
                        format!("skill-{:x}", Sha256::digest(relative.as_bytes()))
                    });
                    all.push(Template {
                        id,
                        directory: relative,
                        builtin: false,
                        data,
                    });
                }
            }
        } else {
            scan_templates(base, &path, depth + 1, all)?;
        }
    }
    Ok(())
}

pub fn save_template(template: &Template) -> Result<(), String> {
    save_record_at(&root()?, template)
}

fn save_record_at(base: &Path, template: &Template) -> Result<(), String> {
    let directory = base.join(&template.directory);
    write(&directory.join("template.json"), &template.data)?;
    write(&directory.join("record.json"), template)
}
pub fn validate_template(data: &Value) -> Result<(), String> {
    if !matches!(data["mode"].as_str(), Some("smart" | "replace")) {
        return Err("模板 mode 必须是 smart 或 replace".into());
    }
    if data["name"].as_str().unwrap_or("").trim().is_empty() {
        return Err("请填写模板名称".into());
    }
    let slots = data["slots"].as_array().ok_or("模板缺少 slots")?;
    if slots.is_empty() || slots.len() > 30 {
        return Err("模板应包含 1–30 个页面".into());
    }
    let mut ids = std::collections::HashSet::new();
    for s in slots {
        let id = s["id"].as_str().ok_or("页面缺少 id")?;
        if !ids.insert(id) || id.trim().is_empty() {
            return Err("页面 id 不能为空或重复".into());
        }
        if data["mode"] == "replace" && s["example"].as_str().unwrap_or("").is_empty() {
            return Err(format!("换货模板的 {id} 缺少 example 样图"));
        }
    }
    Ok(())
}
fn collect(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                if k == "example" {
                    if let Some(s) = v.as_str() {
                        out.insert(s.into());
                    }
                } else if k == "refs" || k == "refs_by_kind" {
                    collect_refs(v, out);
                } else {
                    collect(v, out);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                collect(v, out);
            }
        }
        _ => {}
    }
}
fn collect_refs(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::String(s) => {
            if !s.starts_with('@') {
                out.insert(s.into());
            }
        }
        Value::Array(a) => {
            for v in a {
                collect_refs(v, out);
            }
        }
        Value::Object(m) => {
            for v in m.values() {
                collect_refs(v, out);
            }
        }
        _ => {}
    }
}

fn check_references(base: &Path, data: &Value) -> Result<(), String> {
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let mut refs = HashSet::new();
    collect(data, &mut refs);
    for reference in refs {
        let path = Path::new(&reference);
        if reference.is_empty()
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("模板资产只能使用模板目录内的相对路径".into());
        }
        let resolved = base
            .join(path)
            .canonicalize()
            .map_err(|_| format!("模板素材不存在：{reference}"))?;
        if !resolved.starts_with(&base) {
            return Err("模板素材超出目录".into());
        }
        decode(&fs::read(resolved).map_err(|e| e.to_string())?)?;
    }
    Ok(())
}
pub fn import_at(root: &Path, path: &str) -> Result<Template, String> {
    let path = Path::new(&path);
    let base = if path.is_dir() {
        path
    } else {
        path.parent().ok_or("无效模板路径")?
    };
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let data: Value = read(&base.join("template.json"))?;
    validate_template(&data)?;
    let template_id = id();
    let directory = format!("templates/{template_id}");
    let dest = root.join(&directory);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut refs = HashSet::new();
    collect(&data, &mut refs);
    for r in refs {
        let rel = Path::new(&r);
        if rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("模板资产只能使用模板目录内的相对路径".into());
        }
        let source = base
            .join(rel)
            .canonicalize()
            .map_err(|_| format!("模板素材不存在：{r}"))?;
        if !source.starts_with(&base) {
            return Err("模板素材超出目录".into());
        }
        if fs::metadata(&source).map_err(|e| e.to_string())?.len() > 50 * 1024 * 1024 {
            return Err("模板图片超过 50 MB".into());
        }
        let bytes = fs::read(source).map_err(|e| e.to_string())?;
        decode(&bytes)?;
        let target = dest.join(rel);
        if let Some(p) = target.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        fs::write(target, bytes).map_err(|e| e.to_string())?;
    }
    let t = Template {
        id: template_id,
        data,
        directory,
        builtin: false,
    };
    save_record_at(root, &t)?;
    Ok(t)
}

pub fn save_at(root: &Path, mut template: Template) -> Result<Template, String> {
    validate_template(&template.data)?;
    if template.id.is_empty() {
        template.builtin = false;
    } else {
        let old = list_at(root)?
            .into_iter()
            .find(|t| t.id == template.id)
            .ok_or("模板不存在")?;
        template.directory = old.directory;
        template.builtin = old.builtin;
    }
    if template.id.is_empty() || template.builtin {
        let source_builtin = template.builtin.then(|| template.id.clone());
        template.id = id();
        template.directory = format!("templates/{}", template.id);
        template.builtin = false;
        fs::create_dir_all(root.join(&template.directory)).map_err(|e| e.to_string())?;
        if let Some(source_id) = source_builtin {
            builtins::install_assets(&source_id, &root.join(&template.directory))?;
        }
    }
    check_references(&root.join(&template.directory), &template.data)?;
    save_record_at(root, &template)?;
    Ok(template)
}

pub fn export_at(root: &Path, id: &str, destination: &str) -> Result<String, String> {
    let tpl = list_at(root)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("模板不存在")?;
    let base = Path::new(&destination)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let dest = base.join(format!("dsivio-template-{}", super::files::id()));
    fs::create_dir(&dest).map_err(|e| e.to_string())?;
    if !tpl.directory.is_empty() {
        let source = root
            .join(&tpl.directory)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        fn copy(source: &Path, dest: &Path) -> Result<(), String> {
            for e in fs::read_dir(source).map_err(|e| e.to_string())?.flatten() {
                let p = e.path();
                if p.is_symlink() {
                    return Err("模板素材不能是符号链接".into());
                }
                if p.file_name().is_some_and(|s| s == "record.json") {
                    continue;
                }
                let target = dest.join(e.file_name());
                if p.is_dir() {
                    fs::create_dir(&target).map_err(|e| e.to_string())?;
                    copy(&p, &target)?;
                } else {
                    fs::copy(&p, &target).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        copy(&source, &dest)?;
    }
    write(&dest.join("template.json"), &tpl.data)?;
    Ok(dest.to_string_lossy().into())
}
