//! The bundled Dsvideo CLI uses the App's Node and media tools. No runtime npm install.
use super::runtime;
use std::{ffi::OsString, path::Path, process::{Command, ExitCode, Stdio}};

pub const SUBCOMMAND: &str = "dsvideo";
pub(crate) const PACKAGE_ID: &str = "2d8f8e6c-82d1-452e-9eed-fa00c8a00533";

pub(super) fn command_at(resources: &Path, args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let root = resources.join("video-runtime");
    let tools = runtime::tools_at(&root)?;
    let node = tools.get("node").ok_or("内置 Node 缺失，请修复 Dsivio 安装")?;
    let entry = tools.get("dsvideo").ok_or("内置 dsvideo 缺失，请重新构建或修复 Dsivio 安装")?;
    let mut command = Command::new(node);
    command.arg(entry).args(args).envs(runtime::environment_at(&root)?);
    command.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    Ok(command)
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
    fn missing_bundled_cli_is_an_error_without_install_or_path_fallback() {
        let temp = tempfile::tempdir().unwrap();
        assert!(command_at(temp.path(), std::iter::empty()).is_err());
        assert!(!temp.path().join("video-runtime").exists());
    }
}
