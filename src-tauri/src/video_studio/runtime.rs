//! Application-owned, relocatable runtimes. No PATH lookup or runtime installer.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tauri::{AppHandle, Manager};

static ROOT: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn initialize(app: &AppHandle) -> Result<(), String> {
    let root = resource_directory(app)?.join("video-runtime");
    ROOT.set(root)
        .map_err(|_| "Video runtime already initialized".to_string())
}

pub(super) fn resource_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let bundled = app.path().resource_dir().map_err(|e| e.to_string())?;
    // Packaged debug apps must also work away from the developer checkout.
    if bundled.join("video-runtime/runtime.json").is_file() || !cfg!(debug_assertions) {
        return Ok(bundled);
    }
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))
}

pub(crate) fn root() -> Result<PathBuf, String> {
    if let Some(root) = ROOT.get() {
        return Ok(root.clone());
    }
    if cfg!(debug_assertions) {
        return Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/video-runtime"));
    }
    Err("内置视频运行环境尚未初始化".into())
}

pub(crate) fn environment() -> Result<BTreeMap<String, String>, String> {
    environment_at(&root()?)
}

pub(crate) fn bin_dirs() -> Vec<PathBuf> {
    let Ok(root) = root() else {
        return Vec::new();
    };
    vec![
        root.join("bin"),
        root.join(if cfg!(windows) {
            "python"
        } else {
            "python/bin"
        }),
        root.join(if cfg!(windows) { "node" } else { "node/bin" }),
        root.join("analyzer/node_modules/ffmpeg-static"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn environment_at(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let python = root.join(if cfg!(windows) {
        "python/python.exe"
    } else {
        "python/bin/python3"
    });
    let node = root.join(if cfg!(windows) {
        "node/node.exe"
    } else {
        "node/bin/node"
    });
    let mut paths = vec![
        root.join("bin"),
        python.parent().unwrap().into(),
        node.parent().unwrap().into(),
        root.join("analyzer/node_modules/ffmpeg-static"),
    ];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(paths).map_err(|e| e.to_string())?;
    Ok([
        ("DSVIDEO_RUNTIME_ROOT", root.display().to_string()),
        ("DSVIDEO_PYTHON", python.display().to_string()),
        ("DSVIDEO_NODE", node.display().to_string()),
        ("DSVIDEO_RUNTIME_PATH", path.to_string_lossy().into_owned()),
        ("PATH", path.to_string_lossy().into_owned()),
        (
            "PYTHONPATH",
            root.join("python-packages").display().to_string(),
        ),
        ("PYTHONNOUSERSITE", "1".into()),
        ("PYTHONHOME", String::new()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_paths_follow_the_installation_directory() {
        let temp = tempfile::tempdir().unwrap();
        let moved = temp.path().join("moved app with spaces/video-runtime");
        let env = environment_at(&moved).unwrap();
        assert!(Path::new(&env["DSVIDEO_PYTHON"]).starts_with(&moved));
        assert!(Path::new(&env["DSVIDEO_NODE"]).starts_with(&moved));
        assert_eq!(
            std::env::split_paths(&env["PATH"]).next().unwrap(),
            moved.join("bin")
        );
        assert_eq!(env["PYTHONNOUSERSITE"], "1");
    }
}
