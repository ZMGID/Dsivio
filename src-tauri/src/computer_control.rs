//! Setup only: agents use the installed CLIs through the existing shell + skills.
use std::{path::Path, process::Stdio, time::Duration};

use serde::Deserialize;
use tauri::AppHandle;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::Mutex,
};

use crate::{proc::NoConsoleWindow, skills::SkillMeta};

static INSTALL_LOCK: Mutex<()> = Mutex::const_new(());

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
    let mut process = rmcp::transport::which_command(command)
        .map_err(|e| format!("Cannot find {command}: {e}"))?;
    process
        .args(args)
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

#[tauri::command]
pub async fn computer_control_check(tool: ControlTool) -> Result<String, String> {
    crate::path_env::refresh_path_now();
    run(tool.command(), &["--version"], None, 15).await
}

#[tauri::command]
pub async fn computer_control_install(
    app: AppHandle,
    tool: ControlTool,
) -> Result<SkillMeta, String> {
    let _guard = INSTALL_LOCK
        .try_lock()
        .map_err(|_| "Another computer-control installation is running")?;
    if computer_control_check(tool).await.is_err() {
        match tool {
            ControlTool::Playwright => {
                run(
                    "npm",
                    &["install", "-g", "@playwright/cli@latest"],
                    None,
                    300,
                )
                .await?;
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
    computer_control_check(tool).await?;
    let home = directories::BaseDirs::new()
        .ok_or("Home directory unavailable")?
        .home_dir()
        .to_path_buf();
    let source = match tool {
        ControlTool::Cua => {
            run("cua-driver", &["skills", "install"], None, 120).await?;
            home.join(".cua-driver/skills/cua-driver")
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
    let result = crate::skills::chat_skills_import(app, source.to_string_lossy().into_owned());
    result.skill.filter(|_| result.success).ok_or_else(|| {
        result
            .error
            .unwrap_or_else(|| "Skill installation failed".into())
    })
}
