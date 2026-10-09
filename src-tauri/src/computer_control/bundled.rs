//! One owner for the pinned computer-control resources, offline installation and first-use defaults.
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tauri::{AppHandle, Manager};
static ROOT: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn root() -> Option<&'static PathBuf> {
    ROOT.get()
}
pub(crate) fn command(name: &str) -> Option<PathBuf> {
    let file = root()?.join("bin").join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    });
    // Keep the owned path even if damaged: never fall through to an online installer.
    Some(file)
}
pub(crate) fn skill_dir(id: &str) -> Option<PathBuf> {
    let path = root()?.join("skills").join(id);
    path.join("SKILL.md").is_file().then_some(path)
}
pub(crate) fn version(name: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../scripts/computer-control/versions.json"
    ))
    .ok()?;
    value.get(name)?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_install_registers_ready_entries_without_replacing_custom_drivers() {
        let mut settings = crate::settings::Settings::default();
        configure(
            &mut settings,
            true,
            Path::new("/bundle/computer-control/bin/cua-driver"),
        );
        let server = settings
            .chat_tools
            .servers
            .iter()
            .find(|s| super::super::is_cua_mcp_server_id(&s.id))
            .unwrap();
        assert!(server.enabled);
        assert_eq!(server.args, vec!["mcp"]);
        assert!(
            settings.chat_tools.native_tools.skill_runtime
                && settings.chat_tools.native_tools.run_command
        );
        let mut custom = server.clone();
        custom.command = "/custom/driver".into();
        custom.enabled = false;
        settings.chat_tools.servers = vec![custom];
        configure(
            &mut settings,
            true,
            Path::new("/new/bundle/computer-control/bin/cua-driver"),
        );
        assert_eq!(settings.chat_tools.servers.len(), 1);
        assert_eq!(settings.chat_tools.servers[0].command, "/custom/driver");
        assert!(!settings.chat_tools.servers[0].enabled);
    }
    #[test]
    fn relocation_preserves_disabled_preferences() {
        let mut settings = crate::settings::Settings::default();
        configure(
            &mut settings,
            true,
            Path::new("/old/computer-control/bin/cua-driver"),
        );
        let server = settings
            .chat_tools
            .servers
            .iter_mut()
            .find(|s| super::super::is_cua_mcp_server_id(&s.id))
            .unwrap();
        server.enabled = false;
        server.env.insert("CUSTOM".into(), "kept".into());
        settings.chat_tools.enabled = false;
        settings
            .chat_tools
            .disabled_skill_ids
            .push("playwright-cli".into());
        configure(
            &mut settings,
            false,
            Path::new("/moved/computer-control/bin/cua-driver"),
        );
        let server = settings
            .chat_tools
            .servers
            .iter()
            .find(|s| super::super::is_cua_mcp_server_id(&s.id))
            .unwrap();
        assert_eq!(server.command, "/moved/computer-control/bin/cua-driver");
        assert!(!server.enabled && !settings.chat_tools.enabled);
        assert_eq!(server.env["CUSTOM"], "kept");
        assert!(settings
            .chat_tools
            .disabled_skill_ids
            .contains(&"playwright-cli".into()));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn local_preparation_timeout_terminates_child() {
        let start = std::time::Instant::now();
        assert!(run_local(std::process::Command::new("/bin/sleep").arg("10"), 0).is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
    #[test]
    fn custom_driver_does_not_require_default_installation() {
        assert!(!owned_cua_command("/custom/cua-driver"));
        assert!(owned_cua_command("/bundle/computer-control/bin/cua-driver"));
        assert!(owned_cua_command("cua-driver"));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn offline_cua_install_keeps_signature_and_reuses_matching_install() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/computer-control");
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("Applications/CuaDriver.app");
        std::fs::create_dir_all(app.parent().unwrap()).unwrap();
        prepare_cua_at(&root, &temp.path().join("data"), &app).unwrap();
        // Second launch works even with no archive: it validates/reuses the installed signed app.
        let missing_archive = temp.path().join("bundle");
        std::fs::create_dir(&missing_archive).unwrap();
        std::fs::copy(
            root.join("manifest.json"),
            missing_archive.join("manifest.json"),
        )
        .unwrap();
        prepare_cua_at(&missing_archive, &temp.path().join("data"), &app).unwrap();
        let binary = app.join("Contents/MacOS/cua-driver");
        std::fs::write(&binary, b"damaged").unwrap();
        assert!(prepare_cua_at(&root, &temp.path().join("data"), &app).is_err());
        assert_eq!(std::fs::read(binary).unwrap(), b"damaged");
    }
}
pub(crate) fn initialize(app: &AppHandle) -> Result<(), String> {
    let path = crate::media_runtime::runtime::resource_directory(app)?.join("computer-control");
    // Record ownership before validation, so damaged packages cannot select external installers.
    ROOT.set(path)
        .map_err(|_| "Computer control already initialized".to_string())?;
    let key = if cfg!(windows) { "Path" } else { "PATH" };
    let current = std::env::var_os(key).unwrap_or_default();
    let bin = root().unwrap().join("bin");
    let joined = std::env::join_paths(
        std::iter::once(bin.clone()).chain(std::env::split_paths(&current).filter(|p| p != &bin)),
    )
    .map_err(|e| e.to_string())?;
    std::env::set_var(key, joined);
    if !root().unwrap().join("manifest.json").is_file() {
        return Err("内置电脑控制资源缺失，请修复 Dsivio 安装".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_local(
    command: &mut std::process::Command,
    seconds: u64,
) -> Result<std::process::Output, String> {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().map_err(|e| e.to_string()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(e) => e.to_string(),
                    _ => "本地电脑控制准备超时".into(),
                });
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn prepare_cua_at(root: &Path, data: &Path, destination: &Path) -> Result<(), String> {
    use std::process::Command;
    let expected = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(root.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?["cua"]
        .as_str()
        .ok_or("Missing pinned CUA version")?
        .to_string();
    let verify = |path: &Path| {
        run_local(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(path),
            15,
        )
        .is_ok_and(|o| o.status.success())
    };
    let version = verify(destination)
        .then(|| {
            run_local(
                Command::new(destination.join("Contents/MacOS/cua-driver")).arg("--version"),
                15,
            )
            .ok()
            .filter(|o| o.status.success())
            .map(|o| super::extract_version(&String::from_utf8_lossy(&o.stdout)))
        })
        .flatten();
    if version.as_deref() == Some(&expected) {
        return Ok(());
    }
    // Never overwrite an independently installed driver. The user can explicitly remove it
    // or choose its custom path; pinning must not silently replace another app's environment.
    if destination.exists() {
        return Err(format!(
            "现有 CuaDriver 与内置固定版本 {expected} 不一致或签名无效，请先修复该安装"
        ));
    }
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    let stage = data.join(format!("cua-stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let staged_app =
        destination.with_file_name(format!(".Dsivio-CuaDriver-{}.app", uuid::Uuid::new_v4()));
    let result = (|| {
        let output = run_local(
            Command::new("/usr/bin/ditto")
                .args(["-x", "-k"])
                .arg(root.join("CuaDriver.zip"))
                .arg(&stage),
            60,
        )?;
        let app = stage.join("CuaDriver.app");
        if !output.status.success() || !verify(&app) {
            return Err("内置 CUA 解包或签名校验失败".into());
        }
        let output = run_local(
            Command::new("/usr/bin/ditto").arg(&app).arg(&staged_app),
            60,
        )?;
        if !output.status.success() || !verify(&staged_app) {
            return Err("无法离线安装 CuaDriver 到 Applications，请检查目录权限".into());
        }
        if destination.exists() {
            return Err("CUA 安装过程中目标已发生变化".into());
        }
        std::fs::rename(&staged_app, destination).map_err(|e| e.to_string())?;
        if destination == Path::new("/Applications/CuaDriver.app") {
            let _=run_local(Command::new("/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister").args(["-f"]).arg(destination), 10);
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(staged_app);
    let _ = std::fs::remove_dir_all(stage);
    result
}

pub(crate) async fn prepare(app: &AppHandle, tool: super::ControlTool) -> Result<(), String> {
    if matches!(tool, super::ControlTool::Playwright) {
        return Ok(());
    }
    let root = root().ok_or("内置电脑控制尚未初始化")?.clone();
    #[cfg(target_os = "macos")]
    {
        let data = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("computer-control");
        tokio::task::spawn_blocking(move || {
            prepare_cua_at(&root, &data, Path::new("/Applications/CuaDriver.app"))
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, root);
    Ok(())
}

pub(crate) async fn bootstrap(app: AppHandle) -> Result<(), String> {
    let _guard = super::INSTALL_LOCK.lock().await;
    let state = app.state::<crate::state::AppState>();
    let prepare_default = !state
        .settings_read()
        .chat_tools
        .servers
        .iter()
        .any(|s| super::is_cua_mcp_server_id(&s.id) && !owned_cua_command(&s.command));
    // A custom driver owns its installation; a default CUA failure cannot block other tools.
    let cua_error = if prepare_default {
        prepare(&app, super::ControlTool::Cua).await.err()
    } else {
        None
    };
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("computer-control");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let marker = data.join("defaults-initialized");
    // Refresh owned paths only when changed; keep custom paths and disabled states.
    let cua = command("cua-driver").ok_or("内置 CUA 缺失")?;
    let first_install = !marker.is_file();
    let relocated = state.settings_read().chat_tools.servers.iter().any(|s| {
        super::is_cua_mcp_server_id(&s.id)
            && owned_cua_command(&s.command)
            && s.command != cua.to_string_lossy()
    });
    if first_install || relocated {
        crate::settings::update_settings(&app, &state, |settings| {
            configure(settings, first_install, &cua);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    }
    crate::plugins::refresh_bundled_office_path(&app, &state)?;
    if !marker.is_file() {
        // Absence means first install; existing disabled OfficeCLI stays disabled.
        if !crate::plugins::has_saved_state("officecli") {
            crate::plugins::set_plugin_enabled(&app, &state, "officecli", true).await?;
        }
        std::fs::write(marker, b"1").map_err(|e| e.to_string())?;
    }
    cua_error.map_or(Ok(()), Err)
}

fn configure(settings: &mut crate::settings::Settings, first_install: bool, cua: &Path) {
    for server in &mut settings.chat_tools.servers {
        if super::is_cua_mcp_server_id(&server.id) && owned_cua_command(&server.command) {
            server.command = cua.to_string_lossy().into_owned();
        }
    }
    if first_install {
        settings.chat_tools.enabled = true;
        settings.chat_tools.native_tools.skill_runtime = true;
        settings.chat_tools.native_tools.run_command = true;
        settings.chat_tools.native_tools.read_file = true;
        if !settings
            .chat_tools
            .servers
            .iter()
            .any(|s| super::is_cua_mcp_server_id(&s.id))
        {
            settings
                .chat_tools
                .servers
                .push(crate::settings::ChatMcpServer {
                    id: super::CUA_MCP_SERVER_ID.into(),
                    name: "Cua Driver".into(),
                    command: cua.to_string_lossy().into_owned(),
                    args: vec!["mcp".into()],
                    enabled: true,
                    ..Default::default()
                });
        }
    }
}

fn owned_cua_command(command: &str) -> bool {
    command == "cua-driver"
        || command.contains("computer-control/bin/cua-driver")
        || command.contains("computer-control\\bin\\cua-driver")
}
