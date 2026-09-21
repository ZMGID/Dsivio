//! Resolve page resources and use the host Python environment.
use std::{collections::BTreeMap, path::PathBuf};
use tauri::{AppHandle, Manager};
pub(super) fn initialize(_: &AppHandle) -> Result<(), String> { Ok(()) }
pub(super) fn resource_directory(app: &AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) { return Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")); }
    app.path().resource_dir().map_err(|e| e.to_string())
}
pub(crate) fn environment() -> Result<BTreeMap<String, String>, String> {
    Ok([
        ("DSVIDEO_CONFIG_PATH".into(), super::config::path()?.display().to_string()),
        ("DSVIDEO_PYTHON".into(), std::env::var("DSVIDEO_PYTHON").unwrap_or_else(|_| if cfg!(windows) { "python".into() } else { "python3".into() })),
        ("PYTHONUTF8".into(), "1".into()),
        ("PYTHONIOENCODING".into(), "utf-8".into()),
    ].into_iter().collect())
}
