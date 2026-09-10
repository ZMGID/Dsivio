use super::types::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub fn root() -> Result<PathBuf, String> {
    let p = crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("image-studio");
    for dir in ["tasks", "assets", "results", "templates", "exports"] {
        fs::create_dir_all(p.join(dir)).map_err(|e| e.to_string())?;
    }
    Ok(p)
}

pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn safe_id(s: &str) -> Result<&str, String> {
    if s.is_empty() || s.len() > 100 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("无效的记录 ID".into());
    }
    Ok(s)
}
pub fn resolve(relative: &str) -> Result<PathBuf, String> {
    let p = Path::new(relative);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) || relative.is_empty() {
        return Err("素材路径必须在图片工作台中".into());
    }
    let normalized = relative.replace('\\', "/");
    if let Some(path) = normalized.strip_prefix("outputs/") {
        return super::output::resolve(&path.split('/').collect::<Vec<_>>());
    }
    let base = root()?.canonicalize().map_err(|e| e.to_string())?;
    let full = base.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !full.starts_with(base) {
        return Err("素材路径超出工作台目录".into());
    }
    Ok(full)
}
pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn write<T: Serialize>(p: &Path, value: &T) -> Result<(), String> {
    let tmp = p.with_extension(format!("{}.tmp", id()));
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, p).map_err(|e| e.to_string())
}
pub fn load_task(task_id: &str) -> Result<Task, String> {
    read(
        &root()?
            .join("tasks")
            .join(format!("{}.json", safe_id(task_id)?)),
    )
}
pub fn save_task(task: &mut Task) -> Result<(), String> {
    task.updated_at = now();
    write(
        &root()?
            .join("tasks")
            .join(format!("{}.json", safe_id(&task.id)?)),
        task,
    )?;
    crate::studio::wait::changed("image", &task.id);
    Ok(())
}
pub fn config() -> Result<StudioConfig, String> {
    let p = root()?.join("config.json");
    if p.exists() {
        read(&p)
    } else {
        Ok(StudioConfig {
            protocol: "openai".into(),
            ..Default::default()
        })
    }
}
pub fn templates() -> Result<Vec<Template>, String> {
    let base = root()?;
    let mut all = super::builtins::templates(&base)?;
    scan_templates(&base, &base.join("templates"), 0, &mut all)?;
    Ok(all)
}

/// template.json is the shared source of truth; record.json only preserves UI identity.
pub(super) fn scan_templates(
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
    let directory = root()?.join(&template.directory);
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
pub fn preview(relative: &str, original: bool) -> Result<String, String> {
    let p = resolve(relative)?;
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    let decoded = decode(&bytes)?;
    let img = if original {
        decoded.thumbnail(1800, 1800)
    } else {
        decoded.thumbnail(420, 420)
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(out.into_inner())
    ))
}
pub fn decode(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("单张图片不能超过 50 MB".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("无法读取图片（支持 PNG、JPEG、WebP）：{e}"))
}
pub fn import_asset(path: &Path) -> Result<Asset, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 50 * 1024 * 1024 {
        return Err("单张图片不能超过 50 MB".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    decode(&bytes)?;
    let fmt = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    let extension = match fmt {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        _ => return Err("仅支持 PNG、JPEG、WebP".into()),
    };
    let asset_id = id();
    let relative = format!("assets/{asset_id}.{extension}");
    fs::write(root()?.join(&relative), bytes).map_err(|e| e.to_string())?;
    Ok(Asset {
        id: asset_id,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        path: relative,
    })
}
