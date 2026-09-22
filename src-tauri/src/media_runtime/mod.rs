//! Bundled analyzer runtime and read-only legacy settings migration.
pub(crate) mod runtime;
pub(crate) mod config;
pub(crate) mod migration;
pub(crate) const PACKAGE_ID: &str = "42df724b-34e1-47b8-aa2c-6c738b09d280";
use crate::plugins::packages;
use tauri::AppHandle;
use std::path::PathBuf;
fn source(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(runtime::resource_directory(app)?.join("plugins/dsvideo-plugin"))
}
pub fn initialize(app: &AppHandle) -> Result<(), String> {
    runtime::initialize(app)?;
    packages::ensure_builtin(PACKAGE_ID, &source(app)?)?;
    Ok(())
}

pub(crate) fn plugin() -> Result<packages::Resolved, String> {
    // The restored page uses its original bundled adapters privately. Do not re-enable
    // the retired chat plugin or alter the native generation tools.
    packages::load(PACKAGE_ID)
}
