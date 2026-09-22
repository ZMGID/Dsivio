//! One-way import of legacy settings. User files and original receipts are never deleted.
use crate::settings::{ModelProvider, Settings};
use serde_json::{json, Value};
use std::path::Path;
use tauri::AppHandle;
pub(crate) const RETIRED_PACKAGE: &str = "42df724b-34e1-47b8-aa2c-6c738b09d280";

pub(crate) fn retire_servers(settings: &mut Settings) {
    let owner = format!("plugin:package:{RETIRED_PACKAGE}");
    settings
        .chat_tools
        .servers
        .retain(|s| s.connector_id.as_deref() != Some(&owner));
}
pub(crate) fn migrate(app: &AppHandle, settings: &mut Settings) -> Result<(), String> {
    retire_servers(settings);
    let root = crate::app_data::app_data_dir().ok_or("无法定位迁移目录")?;
    let marker = root.join("media-settings-migrated-v1");
    if marker.exists() {
        return crate::media_generation::import_legacy_at(&root, settings);
    }
    let home = directories::BaseDirs::new().ok_or("无法定位旧视频设置")?;
    let path = std::env::var_os("DSVIDEO_CONFIG_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("APPDATA")
                .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| home.home_dir().join(".config"));
            base.join("dsvideo/providers.json")
        });
    if path.exists() {
        let data: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        import_providers(settings, &data)?;
    }
    crate::settings::persist_settings(app, settings)?;
    crate::media_generation::import_legacy_at(&root, settings)?;
    crate::chat::storage::atomic_write(&marker, "1", "media migration")
}
fn import_providers(settings: &mut Settings, data: &Value) -> Result<(), String> {
    for (route, model, protocol) in [
        ("grok", "grok-imagine-video", "xai_video"),
        ("minimax", "MiniMax-H3", "minimax_h3"),
        ("comfy", "", ""),
    ] {
        let old = &data["providers"][route];
        let Some(base) = old["base_url"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let id = format!("legacy-dsvideo-{route}");
        if settings
            .providers
            .iter()
            .any(|p| p.id == id || p.base_url.trim_end_matches('/') == base.trim_end_matches('/'))
        {
            continue;
        }
        let model = old["model"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(model);
        let mut value = json!({"id":id,"name":format!("{}（已迁移）",if route=="comfy"{"ComfyUI"}else{route}),"baseUrl":base,"enabled":true,"apiKeys":[],"enabledModels":[],"availableModels":[]});
        if route == "comfy" {
            value["request"] = json!({"comfy":{"workflows":[]},"useSystemProxy":false});
        } else {
            value["enabledModels"] = json!([model]);
            value["availableModels"] = json!([model]);
            if let Some(key) = old["api_key"].as_str().filter(|s| !s.is_empty()) {
                value["apiKeys"] = json!([key]);
            }
            value["modelOverrides"] =
                json!({model:{"videoProtocol":protocol,"capabilities":{"videoGeneration":true}}});
        }
        let provider: ModelProvider = serde_json::from_value(value).map_err(|e| e.to_string())?;
        settings.providers.push(provider);
    }
    Ok(())
}

/// Historical tasks remain on disk. This reader exposes saved movies even after plugin removal.
#[tauri::command]
pub fn legacy_video_outputs() -> Result<Vec<crate::media_generation::MediaOutput>, String> {
    let root = crate::app_data::app_data_dir()
        .ok_or("无法定位历史作品")?
        .join("video-studio");
    Ok(legacy_outputs_at(&root))
}
fn legacy_outputs_at(root: &Path) -> Vec<crate::media_generation::MediaOutput> {
    let mut out = Vec::new();
    let Ok(base) = root.canonicalize() else {
        return out;
    };
    if let Ok(files) = std::fs::read_dir(root.join("tasks")) {
        for file in files.flatten() {
            let Ok(bytes) = std::fs::read(file.path()) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let Some(output) = value["output"].as_str() else {
                continue;
            };
            let Ok(path) = Path::new(output).canonicalize() else {
                continue;
            };
            if path.starts_with(&base) && path.is_file() {
                out.push(crate::media_generation::MediaOutput {
                    path: path.to_string_lossy().into_owned(),
                    mime: "video/mp4".into(),
                });
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_config_once_without_replacing_existing_choices() {
        let mut settings = Settings::default();
        let data = json!({"providers":{"grok":{"base_url":"https://example.test","api_key":"test-key","model":"grok-imagine-video"},"comfy":{"base_url":"http://127.0.0.1:8188"}}});
        import_providers(&mut settings, &data).unwrap();
        import_providers(&mut settings, &data).unwrap();
        assert_eq!(settings.providers.len(), 2);
        assert_eq!(settings.providers[0].api_keys, vec!["test-key"]);
        assert!(settings
            .default_models
            .video_generation
            .provider_id
            .is_empty());
        assert!(settings.providers[1].request.comfy.is_some());
    }
}
