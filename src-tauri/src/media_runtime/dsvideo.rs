//! The bundled Dsvideo CLI uses the App's Node and media tools. No runtime npm install.
use super::runtime;
use std::{ffi::OsString, path::Path, process::{Command, ExitCode, Stdio}};

pub const SUBCOMMAND: &str = "dsvideo";
pub(crate) const PACKAGE_ID: &str = "2d8f8e6c-82d1-452e-9eed-fa00c8a00533";

/// Preloaded into every Node process of the bundled Dsvideo (NODE_OPTIONS is inherited by the
/// Runtime Worker and the HyperFrames Provider's browser preparation): Chrome for Testing
/// downloads try npmmirror first and fall back to the official source.
/// Source: `scripts/video-runtime/node-hooks/`.
const RENDER_BROWSER_MIRROR_HOOK: &str = "hooks/render-browser-mirror.mjs";

pub(super) fn command_at(resources: &Path, args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let root = resources.join("video-runtime");
    let tools = runtime::tools_at(&root)?;
    let node = tools.get("node").ok_or("内置 Node 缺失，请修复 Dsivio 安装")?;
    let entry = tools.get("dsvideo").ok_or("内置 dsvideo 缺失，请重新构建或修复 Dsivio 安装")?;
    let mut command = Command::new(node);
    command.arg(entry).args(args).envs(runtime::environment_at(&root)?);
    if let Some(options) = node_options_with_hook(&root, std::env::var_os("NODE_OPTIONS")) {
        command.env("NODE_OPTIONS", options);
    }
    command.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    Ok(command)
}

/// `NODE_OPTIONS` with `--import=<hook file URL>` appended (None without a bundled hook). A
/// file URL has no spaces, so NODE_OPTIONS needs no quoting even for a relocated App.
fn node_options_with_hook(root: &Path, existing: Option<OsString>) -> Option<OsString> {
    let hook = crate::utils::strip_windows_verbatim_prefix(root.join(RENDER_BROWSER_MIRROR_HOOK));
    if !hook.is_file() {
        return None;
    }
    let import = format!("--import={}", url::Url::from_file_path(&hook).ok()?);
    let existing = existing.unwrap_or_default();
    if existing.to_string_lossy().split_whitespace().any(|option| option == import) {
        return Some(existing);
    }
    let mut options = existing;
    if !options.is_empty() {
        options.push(" ");
    }
    options.push(import);
    Some(options)
}

pub fn run(args: impl Iterator<Item = OsString>) -> ExitCode {
    let mut args = args.peekable();
    if args.peek().is_some_and(|arg| arg == "projects") { args.next(); return super::projects::cli(args); }
    let result = runtime::tools_resource_directory()
        .and_then(|resources| command_at(&resources, args))
        .and_then(|mut command| command.status().map_err(|error| format!("无法运行内置 dsvideo：{error}")));
    match result {
        Ok(status) => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                return ExitCode::from(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1)).clamp(0, 255) as u8);
            }
            #[cfg(not(unix))]
            ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
        }
        Err(error) => { eprintln!("dsivio dsvideo: {error}"); ExitCode::from(1) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_cli_uses_bundled_node_and_preserves_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let resources = temp.path().join("relocated app with spaces/resources");
        let root = resources.join("video-runtime");
        let node = root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" });
        let entry = root.join("dsvideo/node_modules/@dsvideo/dsvideo/bin/dsvideo.mjs");
        for file in [&node, &entry] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "fixture").unwrap();
        }
        let args = ["check", "project with spaces/main.svml", "--workspace", "value%&!"];
        let command = command_at(&resources, args.iter().map(|value| OsString::from(*value))).unwrap();
        assert_eq!(command.get_program(), node.as_os_str());
        assert_eq!(command.get_args().collect::<Vec<_>>(), std::iter::once(entry.as_os_str()).chain(args.iter().map(std::ffi::OsStr::new)).collect::<Vec<_>>());
        assert!(command.get_current_dir().is_none(), "The user's workspace is inherited");
    }
    #[test]
    fn render_browser_hook_is_preloaded_once_and_keeps_user_node_options() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("relocated app with spaces/resources/video-runtime");
        assert_eq!(node_options_with_hook(&root, Some("--max-old-space-size=4096".into())), None, "no hook bundled");
        let hook = root.join(RENDER_BROWSER_MIRROR_HOOK);
        std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
        std::fs::write(&hook, "export {}").unwrap();
        let options = node_options_with_hook(&root, Some("--max-old-space-size=4096".into())).unwrap().into_string().unwrap();
        let (user, import) = options.split_once(' ').unwrap();
        assert_eq!(user, "--max-old-space-size=4096");
        let url = url::Url::parse(import.strip_prefix("--import=").unwrap()).unwrap();
        assert_eq!(url.to_file_path().unwrap(), hook);
        assert!(!import.contains(' '), "{import}");
        assert_eq!(node_options_with_hook(&root, Some(options.clone().into())).unwrap(), OsString::from(&options), "idempotent");
        assert_eq!(node_options_with_hook(&root, None).unwrap(), OsString::from(import));
        // The App-level command carries it.
        let entry = root.join("dsvideo/node_modules/@dsvideo/dsvideo/bin/dsvideo.mjs");
        let node = root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" });
        for file in [&node, &entry] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "fixture").unwrap();
        }
        let command = command_at(root.parent().unwrap(), std::iter::empty()).unwrap();
        let value = command.get_envs().find(|(key, _)| key.eq_ignore_ascii_case("NODE_OPTIONS")).and_then(|(_, value)| value).unwrap();
        assert!(value.to_string_lossy().ends_with(import), "{value:?}");
    }
    #[test]
    fn missing_bundled_cli_is_an_error_without_install_or_path_fallback() {
        let temp = tempfile::tempdir().unwrap();
        assert!(command_at(temp.path(), std::iter::empty()).is_err());
        assert!(!temp.path().join("video-runtime").exists());
    }
}
