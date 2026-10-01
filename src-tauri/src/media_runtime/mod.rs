//! Bundled analyzer runtime and read-only legacy settings migration.
pub mod cli;
pub mod dsvideo;
pub mod projects;
pub(crate) mod runtime;
pub(crate) mod config;
pub(crate) mod migration;
use crate::plugins::packages;
use tauri::AppHandle;
pub fn initialize(app: &AppHandle) -> Result<(), String> {
    runtime::initialize(app)?;
    packages::remove_retired_builtin(migration::RETIRED_PACKAGE)?;
    packages::ensure_builtin(dsvideo::PACKAGE_ID, &runtime::resource_directory(app)?.join("plugins/dsvideo"))?;
    Ok(())
}
