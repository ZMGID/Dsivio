//! Setup only: agents use the installed CLIs through the existing shell + skills.
use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::Mutex,
};

use crate::{proc::NoConsoleWindow, skills::SkillMeta};

static INSTALL_LOCK: Mutex<()> = Mutex::const_new(());

pub(crate) const CUA_MCP_SERVER_ID: &str = "computer-control-cua-driver";
pub(crate) const LEGACY_CUA_MCP_SERVER_ID: &str = "plugin-cua-driver";
pub(crate) const LEGACY_CUA_MCP_CONNECTOR_ID: &str = "plugin:cua-driver";

pub(crate) fn is_cua_mcp_server_id(id: &str) -> bool {
    id == CUA_MCP_SERVER_ID || id == LEGACY_CUA_MCP_SERVER_ID
}

pub(crate) fn mcp_server_ids_equivalent(left: &str, right: &str) -> bool {
    left == right || (is_cua_mcp_server_id(left) && is_cua_mcp_server_id(right))
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlTool {
    Cua,
    Playwright,
}

impl ControlTool {
    fn command(self) -> &'static str {
        match self {
            Self::Cua => "cua-driver",
            Self::Playwright => "playwright-cli",
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlToolStatus {
    pub current_version: String,
    pub installed: bool,
    pub issue: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
}

fn control_server(
    tool: ControlTool,
    state: &crate::state::AppState,
) -> crate::settings::ChatMcpServer {
    if matches!(tool, ControlTool::Cua) {
        if let Some(server) = state
            .settings_read()
            .chat_tools
            .servers
            .iter()
            .find(|server| {
                is_cua_mcp_server_id(&server.id)
                    || matches!(
                        server.connector_id.as_deref(),
                        Some("computer-control:cua" | "plugin:cua-driver")
                    )
            })
        {
            return server.clone();
        }
    }
    crate::settings::ChatMcpServer {
        id: CUA_MCP_SERVER_ID.into(),
        command: tool.command().into(),
        args: vec!["mcp".into()],
        ..Default::default()
    }
}

// Only a missing default executable permits installation. An existing but broken
// driver, or a missing custom path, requires fixing that environment instead.
fn needs_install(command: &str, default: &str) -> Result<bool, String> {
    match rmcp::transport::which_command(command) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && command == default => {
            Ok(true)
        }
        Err(error) => Err(format!("Cannot find configured driver {command}: {error}")),
    }
}

async fn run_driver(
    server: &crate::settings::ChatMcpServer,
    args: &[&str],
    seconds: u64,
) -> Result<String, String> {
    run_with_env(
        &server.command,
        args,
        server.cwd.as_deref().map(Path::new),
        &server.env,
        seconds,
    )
    .await
}

fn permission_issue(value: &serde_json::Value) -> Option<String> {
    let mut missing = Vec::new();
    if value.get("accessibility").and_then(|v| v.as_bool()) == Some(false) {
        missing.push("辅助功能");
    }
    if value.get("screen_recording").and_then(|v| v.as_bool()) == Some(false) {
        missing.push("屏幕录制");
    }
    (!missing.is_empty()).then(|| format!("请在系统设置中授权：{}", missing.join("、")))
}

#[derive(Deserialize)]
struct CuaUpdateStatus {
    current_version: Option<String>,
    latest_version: Option<String>,
    update_available: bool,
}

fn extract_version(output: &str) -> String {
    output
        .split_whitespace()
        .find_map(|part| {
            let candidate = part.trim_start_matches('v').trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
            });
            let mut numbers = candidate.split('.');
            let valid = numbers
                .by_ref()
                .take(3)
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
            (valid && candidate.matches('.').count() >= 1).then(|| candidate.to_string())
        })
        .unwrap_or_else(|| output.trim().to_string())
}

fn numeric_version(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    numeric_version(latest) > numeric_version(current)
}

fn validate_self_update_result(
    result: Result<String, String>,
    previous_version: &str,
    observed_version: &str,
) -> Result<(), String> {
    match result {
        Ok(_) => Ok(()),
        Err(_) if is_newer_version(observed_version, previous_version) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn read_output(mut stream: impl AsyncRead + Unpin) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        // Drain pipes even after reaching the display limit, so installs cannot block.
        let keep = n.min((128 * 1024usize).saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn run(
    command: &str,
    args: &[&str],
    cwd: Option<&Path>,
    seconds: u64,
) -> Result<String, String> {
    run_with_env(command, args, cwd, &Default::default(), seconds).await
}

async fn run_with_env(
    command: &str,
    args: &[&str],
    cwd: Option<&Path>,
    env: &std::collections::HashMap<String, String>,
    seconds: u64,
) -> Result<String, String> {
    let mut process = rmcp::transport::which_command(command)
        .map_err(|e| format!("Cannot find {command}: {e}"))?;
    process.args(args).envs(crate::mcp::conn::clean_env(env));
    run_process(process, command, cwd, seconds).await
}

async fn run_process(
    mut process: tokio::process::Command,
    command: &str,
    cwd: Option<&Path>,
    seconds: u64,
) -> Result<String, String> {
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process.kill_on_drop(true).no_console_window();
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    let mut child = process
        .spawn()
        .map_err(|e| format!("Cannot start {command}: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let execution = async {
        tokio::try_join!(
            async { child.wait().await.map_err(|e| e.to_string()) },
            read_output(stdout),
            read_output(stderr),
        )
    };
    let (status, out, err) = tokio::time::timeout(Duration::from_secs(seconds), execution)
        .await
        .map_err(|_| {
            format!("{command} timed out. Check installation status before retrying.")
        })??;
    if !status.success() {
        return Err(format!("{command}: {status}\n{out}\n{err}"));
    }
    Ok(out.trim().to_string())
}

/// The App's bundled runtime root, when it ships Node and npm.
fn bundled_npm_root() -> Option<std::path::PathBuf> {
    let root = crate::media_runtime::runtime::root().ok()?;
    let tools = crate::media_runtime::runtime::tools_at(&root).ok()?;
    (tools.contains_key("node") && tools.contains_key("npm")).then_some(root)
}

/// npm through the App's bundled Node, with `-g` going to the App-private prefix
/// (~/.kivio/npm-global). Fetches try npmmirror first and the official registry once more.
async fn run_bundled_npm(root: &Path, args: &[&str], seconds: u64) -> Result<String, String> {
    use crate::media_runtime::launch;
    let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
    let prefix = launch::npm_prefix().ok_or("Home directory unavailable")?;
    let mut failures = Vec::new();
    for source in launch::npm_sources(&args, |key| std::env::var_os(key)) {
        let command = launch::npm_command(root, &args, source, Some(&prefix))?;
        match run_process(command.into(), "npm", None, seconds).await {
            Ok(output) => {
                // The private bin directory must be on PATH for the CLI that was just installed.
                launch::extend_process_path(root);
                return Ok(output);
            }
            Err(error) => failures.push(format!("[{}] {error}", source.label())),
        }
    }
    Err(failures.join("\n"))
}

const PLAYWRIGHT_PACKAGE: &str = "@playwright/cli@latest";

/// New installs use the bundled npm and the private prefix, so no system Node is required.
/// An existing system-wide `playwright-cli` keeps being updated by the system npm, otherwise
/// the private copy would be shadowed by the older one earlier on PATH. Without a bundled
/// runtime (broken installation) the previous `npm -g` path is the fallback.
async fn install_playwright_cli(update: bool) -> Result<String, String> {
    let private = crate::media_runtime::launch::npm_prefix()
        .map(|prefix| crate::media_runtime::launch::npm_bin_dir(&prefix));
    let system_install = update
        && !private
            .as_deref()
            .is_some_and(|dir| crate::media_runtime::launch::resolves_inside("playwright-cli", dir))
        && rmcp::transport::which_command("npm").is_ok();
    match bundled_npm_root() {
        Some(root) if !system_install => {
            run_bundled_npm(
                &root,
                &["install", "--global", PLAYWRIGHT_PACKAGE, "--no-audit", "--no-fund"],
                300,
            )
            .await
        }
        _ => run("npm", &["install", "-g", PLAYWRIGHT_PACKAGE], None, 300).await,
    }
}

async fn latest_playwright_cli_version() -> Result<String, String> {
    let args = ["view", "@playwright/cli", "version"];
    match bundled_npm_root() {
        Some(root) => run_bundled_npm(&root, &args, 30).await,
        None => run("npm", &args, None, 30).await,
    }
}

#[tauri::command]
pub async fn computer_control_check(
    state: State<'_, crate::state::AppState>,
    tool: ControlTool,
) -> Result<String, String> {
    crate::path_env::refresh_path_now();
    run_driver(&control_server(tool, &state), &["--version"], 15).await
}

#[tauri::command]
pub async fn computer_control_status(
    state: State<'_, crate::state::AppState>,
    tool: ControlTool,
    check_updates: Option<bool>,
) -> Result<ControlToolStatus, String> {
    crate::path_env::refresh_path_now();
    let server = control_server(tool, &state);
    let installed = !needs_install(&server.command, tool.command()).unwrap_or(false);
    let mut status = ControlToolStatus {
        installed,
        current_version: String::new(),
        latest_version: None,
        update_available: false,
        issue: None,
    };
    if !installed {
        return Ok(status);
    }
    match run_driver(&server, &["--version"], 15).await {
        Ok(output) => status.current_version = extract_version(&output),
        Err(error) => {
            status.issue = Some(error);
            return Ok(status);
        }
    }
    if matches!(tool, ControlTool::Cua) {
        // Use the application's existing MCP connection, including its configured
        // executable/environment. This read-only call also detects protocol mismatch.
        let probe = state.mcp_call_tool(None, &server, "check_permissions", serde_json::json!({}));
        status.issue = match tokio::time::timeout(Duration::from_secs(15), probe).await {
            Ok(Ok(result)) if !result.is_error => {
                let value = result
                    .structured_content
                    .or_else(|| serde_json::from_str(&result.content).ok());
                match value {
                    Some(value) => permission_issue(&value),
                    None => Some("CUA 权限检测未返回有效结果，请检查驱动连接".into()),
                }
            }
            Ok(Ok(result)) => Some(result.content),
            Ok(Err(error)) => Some(error),
            Err(_) => Some("CUA 连接检测超时，请检查已有驱动服务".into()),
        };
    }
    if !check_updates.unwrap_or(false) {
        return Ok(status);
    }
    match tool {
        ControlTool::Cua => {
            let output = run_driver(&server, &["check-update", "--json"], 30).await?;
            let update: CuaUpdateStatus =
                serde_json::from_str(&output).map_err(|e| e.to_string())?;
            status.latest_version = update.latest_version.or(update.current_version);
            status.update_available = update.update_available;
        }
        ControlTool::Playwright => {
            let output = latest_playwright_cli_version().await?;
            let latest = extract_version(&output);
            status.update_available = is_newer_version(&latest, &status.current_version);
            status.latest_version = Some(latest);
        }
    }
    Ok(status)
}

fn import_control_skill(app: AppHandle, source: &Path) -> Result<SkillMeta, String> {
    let result = crate::skills::chat_skills_import(app, source.to_string_lossy().into_owned());
    result.skill.filter(|_| result.success).ok_or_else(|| {
        result
            .error
            .unwrap_or_else(|| "Skill installation failed".into())
    })
}

#[tauri::command]
pub async fn computer_control_install(
    app: AppHandle,
    state: State<'_, crate::state::AppState>,
    tool: ControlTool,
) -> Result<SkillMeta, String> {
    let _guard = INSTALL_LOCK
        .try_lock()
        .map_err(|_| "Another computer-control installation is running")?;
    crate::path_env::refresh_path_now();
    let server = control_server(tool, &state);
    if needs_install(&server.command, tool.command())? {
        match tool {
            ControlTool::Playwright => {
                install_playwright_cli(false).await?;
            }
            ControlTool::Cua => {
                #[cfg(windows)]
                run("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; irm https://cua.ai/driver/install.ps1 | iex"], None, 300).await?;
                #[cfg(not(windows))]
                run(
                    "bash",
                    &[
                        "-c",
                        "set -o pipefail; curl -fsSL https://cua.ai/driver/install.sh | bash",
                    ],
                    None,
                    300,
                )
                .await?;
            }
        }
    }
    crate::path_env::refresh_path_now();
    run_driver(&server, &["--version"], 15).await?;
    if let Ok(registry) = crate::skills::build_registry_metadata(
        &app,
        &state.settings_read().chat_tools.skill_scan_paths,
    ) {
        if let Some(skill) = registry
            .metas()
            .into_iter()
            .find(|skill| skill.id == tool.command() || skill.name == tool.command())
        {
            return Ok(skill);
        }
    }
    let home = directories::BaseDirs::new()
        .ok_or("Home directory unavailable")?
        .home_dir()
        .to_path_buf();
    let source = match tool {
        ControlTool::Cua => {
            let source = home.join(".cua-driver/skills/cua-driver");
            if !source.join("SKILL.md").is_file() {
                run_driver(&server, &["skills", "install"], 120).await?;
            }
            source
        }
        ControlTool::Playwright => {
            let staging = home.join(".kivio/tool-setup/playwright");
            std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
            run(
                "playwright-cli",
                &["install", "--skills"],
                Some(&staging),
                120,
            )
            .await?;
            staging.join(".claude/skills/playwright-cli")
        }
    };
    // Reuse the normal importer: retain upstream references and expose the skill
    // in ~/.kivio/skills, where existing discovery and enable/disable already work.
    import_control_skill(app, &source)
}

#[tauri::command]
pub async fn computer_control_update(
    app: AppHandle,
    state: State<'_, crate::state::AppState>,
    tool: ControlTool,
) -> Result<SkillMeta, String> {
    let _guard = INSTALL_LOCK
        .try_lock()
        .map_err(|_| "Another computer-control installation is running")?;
    crate::path_env::refresh_path_now();
    let server = control_server(tool, &state);
    let previous_version = extract_version(&run_driver(&server, &["--version"], 15).await?);

    let home = directories::BaseDirs::new()
        .ok_or("Home directory unavailable")?
        .home_dir()
        .to_path_buf();
    let source = match tool {
        ControlTool::Cua => {
            // Cua's MCP server is part of the driver binary. Update the binary first,
            // then refresh its separately versioned official Skill pack.
            state.mcp_disconnect_server(CUA_MCP_SERVER_ID).await;
            state.mcp_disconnect_server(LEGACY_CUA_MCP_SERVER_ID).await;
            let update_result = run_driver(&server, &["update", "--apply", "--json"], 300).await;
            crate::path_env::refresh_path_now();
            let observed_version = extract_version(&run_driver(&server, &["--version"], 15).await?);
            validate_self_update_result(update_result, &previous_version, &observed_version)?;
            run_driver(&server, &["skills", "update"], 180).await?;
            home.join(".cua-driver/skills/cua-driver")
        }
        ControlTool::Playwright => {
            install_playwright_cli(true).await?;
            crate::path_env::refresh_path_now();
            let staging = home.join(".kivio/tool-setup/playwright");
            std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
            run(
                "playwright-cli",
                &["install", "--skills"],
                Some(&staging),
                120,
            )
            .await?;
            staging.join(".claude/skills/playwright-cli")
        }
    };

    crate::path_env::refresh_path_now();
    run_driver(&server, &["--version"], 15).await?;
    import_control_skill(app, &source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn existing_broken_driver_never_requests_installation() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cua-driver");
        std::fs::write(&file, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let command = file.to_str().unwrap();
        assert!(!needs_install(command, command).unwrap());
        assert!(run(command, &["--version"], None, 1).await.is_err());
        assert!(!needs_install(command, command).unwrap());
        assert!(needs_install("/missing/custom/cua-driver", "cua-driver").is_err());
        assert!(needs_install("dsivio-test-missing-driver", "dsivio-test-missing-driver").unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn configured_driver_uses_its_environment_and_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let server = crate::settings::ChatMcpServer {
            command: "/bin/sh".into(),
            cwd: Some(dir.path().to_string_lossy().into()),
            env: [("DSIVIO_CONTROL_TEST".into(), "existing-driver".into())].into(),
            ..Default::default()
        };
        let output = run_driver(
            &server,
            &["-c", "printf '%s\n' \"$DSIVIO_CONTROL_TEST\"; pwd -P"],
            1,
        )
        .await
        .unwrap();
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(output, format!("existing-driver\n{}", expected.display()));
    }

    #[test]
    fn reports_missing_permissions_without_reinstallation() {
        assert!(permission_issue(
            &serde_json::json!({"accessibility": true, "screen_recording": true})
        )
        .is_none());
        assert!(permission_issue(
            &serde_json::json!({"accessibility": false, "screen_recording": false})
        )
        .unwrap()
        .contains("辅助功能、屏幕录制"));
    }

    #[test]
    fn extracts_cli_versions() {
        assert_eq!(extract_version("cua-driver 0.28.2"), "0.28.2");
        assert_eq!(extract_version("0.1.20"), "0.1.20");
    }

    #[test]
    fn compares_numeric_versions() {
        assert!(is_newer_version("0.28.2", "0.28.1"));
        assert!(!is_newer_version("0.28.1", "0.28.2"));
        assert!(!is_newer_version("0.28.2", "0.28.2"));
    }

    #[test]
    fn accepts_self_update_when_the_binary_was_replaced_despite_process_error() {
        assert!(validate_self_update_result(
            Err("installer process exited unsuccessfully".to_string()),
            "0.28.1",
            "0.28.2",
        )
        .is_ok());
    }
}
