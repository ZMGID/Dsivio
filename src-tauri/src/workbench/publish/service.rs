//! Tauri commands registered from this module (`workbench::publish::service`).
use super::{
    PublishAccount, PublishAppConfig, PublishBeginResult, PublishRecord, PublishRecordFilter, PublishRequest,
    PublishSubmitResult, VideoStats,
};

fn text(error: super::PublishError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn publish_begin(config: PublishAppConfig) -> Result<PublishBeginResult, String> {
    super::publish_begin(config).map_err(text)
}

#[tauri::command]
pub async fn publish_complete(request_id: String, callback_url: String) -> Result<PublishAccount, String> {
    super::publish_complete(request_id, callback_url).await.map_err(text)
}

#[tauri::command]
pub async fn publish_list_accounts() -> Result<Vec<PublishAccount>, String> {
    super::publish_list_accounts().await.map_err(text)
}

#[tauri::command]
pub async fn publish_refresh_account(id: String) -> Result<PublishAccount, String> {
    super::publish_refresh_account(id).await.map_err(text)
}

#[tauri::command]
pub async fn publish_unbind(id: String) -> Result<(), String> {
    super::publish_unbind(id).await.map_err(text)
}

#[tauri::command]
pub async fn publish_submit(request: PublishRequest) -> Result<PublishSubmitResult, String> {
    super::publish_submit(request).await.map_err(text)
}

#[tauri::command]
pub async fn publish_retry(id: String) -> Result<PublishRecord, String> {
    super::publish_retry(id).await.map_err(text)
}

#[tauri::command]
pub async fn publish_list_records(filter: Option<PublishRecordFilter>) -> Result<Vec<PublishRecord>, String> {
    super::publish_list_records(filter).await.map_err(text)
}

#[tauri::command]
pub async fn publish_refresh_record(id: String) -> Result<PublishRecord, String> {
    super::publish_refresh_record(id).await.map_err(text)
}

#[tauri::command]
pub async fn publish_stats(id: String) -> Result<VideoStats, String> {
    super::publish_stats(id).await.map_err(text)
}
