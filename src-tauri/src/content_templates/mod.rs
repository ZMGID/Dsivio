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
fn list_image_templates() -> Result<Vec<Template>, String> {
    let _guard = lock()?;
    image::templates()
}
#[tauri::command]
pub async fn image_templates_list() -> Result<Vec<Template>, String> {
    tauri::async_runtime::spawn_blocking(list_image_templates)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn image_template_get(id: String) -> Result<Template, String> {
    list_image_templates()?
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
pub async fn image_template_preview(
    id: String,
    reference: String,
    size: Option<u32>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock()?;
        image::preview_at(&image::root()?, &id, &reference, size.unwrap_or(720))
    })
    .await
    .map_err(|e| e.to_string())?
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
mod image_set_schema;
#[cfg(test)]
mod tests;
