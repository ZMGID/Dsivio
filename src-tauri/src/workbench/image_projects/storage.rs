use super::types::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(crate) use crate::content_templates::STORE_LOCK;
pub fn root() -> Result<PathBuf, String> {
    let p = crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("image-studio");
    for dir in ["tasks", "assets", "results", "templates", "exports"] {
        fs::create_dir_all(p.join(dir)).map_err(|e| e.to_string())?;
    }
    Ok(p)
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

pub use crate::content_templates::files::{id, now, read, write, decode};
pub use crate::content_templates::image::{templates, save_template, validate_template};
#[cfg(test)]
pub(crate) use crate::content_templates::image::scan_templates;
