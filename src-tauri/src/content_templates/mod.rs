//! Content-management template IO; independent of studio execution and workers.
pub(crate) mod builtins;
pub(crate) mod files;
pub mod image;
pub mod video;
use image::Template;
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use video::VideoTemplate;
pub(crate) static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    STORE_LOCK
        .get_or_init(Default::default)
        .lock()
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn image_templates_list() -> Result<Vec<Template>, String> {
    let _guard = lock()?;
    image::templates()
}
#[tauri::command]
pub fn image_template_get(id: String) -> Result<Template, String> {
    image_templates_list()?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("图片模板不存在".into())
}
#[tauri::command]
pub fn image_template_save(template: Template) -> Result<Template, String> {
    let _guard = lock()?;
    image::save_at(&image::root()?, template)
}
#[tauri::command]
pub async fn image_template_import(path: String) -> Result<Template, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock()?;
        image::import_at(&image::root()?, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn image_template_export(id: String, destination: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock()?;
        image::export_at(&image::root()?, &id, &destination)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn image_template_preview(id: String, reference: String) -> Result<String, String> {
    use base64::Engine;
    let template = image_template_get(id)?;
    let base = image::root()?
        .join(template.directory)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = Path::new(&reference);
    if path
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("素材路径必须在模板内".into());
    }
    let path = base.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(base) {
        return Err("素材路径超出模板目录".into());
    }
    let image =
        files::decode(&std::fs::read(path).map_err(|e| e.to_string())?)?.thumbnail(720, 720);
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, ::image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}
#[tauri::command]
pub fn video_templates_list() -> Result<Vec<VideoTemplate>, String> {
    let _guard = lock()?;
    video::list_at(&video::root()?)
}
#[tauri::command]
pub fn video_template_get(id: String) -> Result<VideoTemplate, String> {
    let _guard = lock()?;
    video::get_at(&video::root()?, &id)
}
#[tauri::command]
pub fn video_template_save(template: VideoTemplate) -> Result<VideoTemplate, String> {
    let _guard = lock()?;
    video::save_at(&video::root()?, template)
}
#[tauri::command]
pub fn video_template_import(path: String) -> Result<VideoTemplate, String> {
    let _guard = lock()?;
    video::import_at(&video::root()?, Path::new(&path))
}
#[tauri::command]
pub fn video_template_export(id: String, destination: String) -> Result<String, String> {
    let _guard = lock()?;
    video::export_at(&video::root()?, &id, Path::new(&destination))
}
pub(crate) fn save_video_result(data: serde_json::Value) -> Result<serde_json::Value, String> {
    let _guard = lock()?;
    Ok(video::for_studio(video::insert_at(&video::root()?, data)?))
}
#[cfg(test)]
mod tests;
