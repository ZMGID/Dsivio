//! One distributable JSON for application, image and video model configuration.
use crate::{image_studio::types::StudioConfig, settings::Settings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const VERSION: u32 = 2;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct VideoConfig {
    version: u32,
    providers: BTreeMap<String, Value>,
}

impl VideoConfig {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("不支持的视频配置版本，请升级 dsivio 后导入".into());
        }
        for (name, provider) in &self.providers {
            if name.is_empty() || name.chars().any(char::is_whitespace) || !provider.is_object() {
                return Err("视频供应商配置格式不正确".into());
            }
            for field in ["base_url", "api_key", "model"] {
                if provider.get(field).is_some_and(|v| !v.is_string()) {
                    return Err(format!("视频供应商的 {field} 必须是字符串"));
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct Import {
    pub settings: Settings,
    pub image: Option<StudioConfig>,
    pub video: Option<VideoConfig>,
}

pub(crate) fn image_path() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("image-studio/config.json"))
}

fn read_optional<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| format!("配置文件格式不正确：{}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("无法读取 {}：{e}", path.display())),
    }
}

pub(crate) fn export(settings: &Settings) -> Result<Value, String> {
    let image = read_optional(&image_path()?)?.unwrap_or_else(|| StudioConfig {
        protocol: "openai".into(),
        ..Default::default()
    });
    let video = read_optional(&crate::video_studio::config::path()?)?.unwrap_or(VideoConfig {
        version: 1,
        providers: BTreeMap::new(),
    });
    make_backup(settings, image, video)
}

fn make_backup(
    settings: &Settings,
    mut image: StudioConfig,
    video: VideoConfig,
) -> Result<Value, String> {
    video.validate()?;
    let mut settings = settings.clone();
    settings.chat_tools.native_tools.working_directory.clear();
    settings.chat_tools.native_tools.workspace_roots.clear();
    settings.chat_tools.skill_scan_paths.clear();
    settings.image_archive_path.clear();
    settings.obsidian_vault_path.clear();
    for agent in settings.chat.external_cli_agents.values_mut() {
        agent.path.clear();
    }
    image.output_root.clear();
    Ok(
        json!({"app":"kivio", "type":"settings-backup", "version":VERSION,
        "settings":settings, "imageStudio":image, "videoProviders":video}),
    )
}

pub(crate) fn parse(
    raw: &str,
    local: &Settings,
    complete_onboarding: bool,
) -> Result<Import, String> {
    let value: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|_| "文件不是有效的 JSON".to_string())?;
    if value.get("type").and_then(Value::as_str) != Some("settings-backup") {
        return Err("这不是 dsivio 配置文件".into());
    }
    let version = match value.get("version") {
        None => 1,
        Some(v) => v.as_u64().ok_or("配置版本格式不正确")?,
    };
    if version == 0 || version > u64::from(VERSION) {
        return Err("不支持的配置版本，请升级 dsivio 后导入".into());
    }
    let mut settings: Settings = serde_json::from_value(
        value
            .get("settings")
            .filter(|v| v.is_object())
            .ok_or("配置缺少 settings 对象")?
            .clone(),
    )
    .map_err(|_| "应用设置格式不正确".to_string())?;
    if complete_onboarding {
        settings.onboarding_status = "completed".into();
    }
    // v1 retains the original full-settings restore behavior, with no studio writes.
    if version == 1 {
        return Ok(Import {
            settings,
            image: None,
            video: None,
        });
    }
    let image: StudioConfig = serde_json::from_value(
        value
            .get("imageStudio")
            .filter(|v| v.is_object())
            .ok_or("配置缺少 imageStudio 对象")?
            .clone(),
    )
    .map_err(|_| "生图配置格式不正确".to_string())?;
    if image.agent_provider_id.is_empty() != image.agent_model.is_empty() {
        return Err("生图规划供应商和模型需要一起配置".into());
    }
    let video: VideoConfig = serde_json::from_value(
        value
            .get("videoProviders")
            .ok_or("配置缺少 videoProviders 对象")?
            .clone(),
    )
    .map_err(|_| "视频配置格式不正确".to_string())?;
    video.validate()?;
    keep_local_paths(&mut settings, local);
    Ok(Import {
        settings,
        image: Some(image),
        video: Some(video),
    })
}

fn keep_local_paths(settings: &mut Settings, local: &Settings) {
    settings.chat_tools.native_tools.working_directory =
        local.chat_tools.native_tools.working_directory.clone();
    settings.chat_tools.native_tools.workspace_roots =
        local.chat_tools.native_tools.workspace_roots.clone();
    settings.chat_tools.skill_scan_paths = local.chat_tools.skill_scan_paths.clone();
    settings.image_archive_path = local.image_archive_path.clone();
    settings.obsidian_vault_path = local.obsidian_vault_path.clone();
    for (id, agent) in &mut settings.chat.external_cli_agents {
        agent.path = local
            .chat
            .external_cli_agents
            .get(id)
            .map(|a| a.path.clone())
            .unwrap_or_default();
    }
    // Bundled/plugin commands include installation-specific Python/Node paths.
    // Keep local runtime paths while still transferring configured credentials.
    for server in &mut settings.chat_tools.servers {
        if !server.id.starts_with("plugin-") {
            continue;
        }
        if let Some(installed) = local.chat_tools.servers.iter().find(|s| s.id == server.id) {
            server.command = installed.command.clone();
            server.args = installed.args.clone();
            server.cwd = installed.cwd.clone();
            for key in [
                "PATH",
                "PYTHONPATH",
                "NODE_PATH",
                "DSVIDEO_RUNTIME_ROOT",
                "DSVIDEO_CONFIG_PATH",
                "DSVIDEO_PYTHON",
                "DSVIDEO_NODE",
                "DSVIDEO_RUNTIME_PATH",
            ] {
                match installed.env.get(key) {
                    Some(value) => {
                        server.env.insert(key.into(), value.clone());
                    }
                    None => {
                        server.env.remove(key);
                    }
                }
            }
        }
    }
}

impl Import {
    pub(crate) fn install_studios(&mut self) -> Result<FileUpdates, String> {
        if self.image.is_none() && self.video.is_none() {
            return FileUpdates::apply(Vec::new());
        }
        self.install_studios_at(image_path()?, crate::video_studio::config::path()?)
    }

    fn install_studios_at(
        &mut self,
        image_path: PathBuf,
        video_path: PathBuf,
    ) -> Result<FileUpdates, String> {
        let mut writes = Vec::new();
        if let Some(image) = &mut self.image {
            let path = image_path;
            let current: Option<StudioConfig> = read_optional(&path)?;
            image.output_root = current.map(|c| c.output_root).unwrap_or_default();
            writes.push((
                path,
                serde_json::to_vec_pretty(image).map_err(|e| e.to_string())?,
            ));
        }
        if let Some(video) = &self.video {
            writes.push((
                video_path,
                serde_json::to_vec_pretty(video).map_err(|e| e.to_string())?,
            ));
        }
        FileUpdates::apply(writes)
    }
}

/// Replace files only after every backup section is validated. Roll back if any
/// file or the existing application-settings transaction fails.
pub(crate) struct FileUpdates {
    previous: Vec<(PathBuf, Option<Vec<u8>>)>,
    active: bool,
}

impl FileUpdates {
    fn apply(writes: Vec<(PathBuf, Vec<u8>)>) -> Result<Self, String> {
        let mut transaction = Self {
            previous: Vec::new(),
            active: true,
        };
        for (path, bytes) in writes {
            let result = (|| {
                let previous = match fs::read(&path) {
                    Ok(bytes) => Some(bytes),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                };
                write_atomic(&path, &bytes)?;
                transaction.previous.push((path, previous));
                Ok(())
            })();
            if let Err(error) = result {
                return Err(transaction.rollback_error(error));
            }
        }
        Ok(transaction)
    }

    pub(crate) fn commit(mut self) {
        self.active = false;
    }

    fn rollback(&mut self) -> Result<(), String> {
        self.active = false;
        let mut failures = Vec::new();
        for (path, bytes) in self.previous.iter().rev() {
            let result = match bytes {
                Some(bytes) => write_atomic(path, bytes),
                None => fs::remove_file(path).map_err(|e| e.to_string()),
            };
            if let Err(e) = result {
                failures.push(format!("{}：{e}", path.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("；"))
        }
    }

    pub(crate) fn rollback_error(mut self, error: String) -> String {
        match self.rollback() {
            Ok(()) => error,
            Err(rollback) => format!("{error}；恢复原配置失败：{rollback}"),
        }
    }
}

impl Drop for FileUpdates {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = self.rollback() {
                eprintln!("配置导入回滚失败：{error}");
            }
        }
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{sanitize_settings, ChatMcpServer};

    fn company_backup() -> Value {
        let mut settings: Settings = serde_json::from_value(json!({
            "providers":[{"id":"company", "name":"Company", "baseUrl":"https://gateway.example/v1",
                "apiKeys":["test-company-key"], "availableModels":["chat-test", "image-test"],
                "enabledModels":["chat-test", "image-test"], "apiFormat":"openai_chat"}],
            "defaultModels":{"chat":{"providerId":"company", "model":"chat-test"},
                "imageGeneration":{"providerId":"company", "model":"image-test"}},
            "chat":{"externalCliAgents":{"codex":{"path":"/developer/codex", "customModels":[{"id":"code-test", "name":"Code"}]}}}
        })).unwrap();
        settings.chat_tools.native_tools.working_directory = "/developer/work".into();
        settings.chat_tools.servers.push(ChatMcpServer {
            id: "plugin-video".into(),
            command: "/developer/python".into(),
            enabled: true,
            env: [
                ("PATH".into(), "/developer/runtime".into()),
                ("API_KEY".into(), "test-plugin-key".into()),
            ]
            .into(),
            ..Default::default()
        });
        let image = StudioConfig {
            provider_id: "company".into(),
            model: "image-test".into(),
            protocol: "openai".into(),
            agent_provider_id: "company".into(),
            agent_model: "chat-test".into(),
            output_root: "/developer/images".into(),
        };
        let video = serde_json::from_value(json!({"version":1,"providers":{
            "grok":{"base_url":"https://video.example", "api_key":"test-video-key", "model":"video-test"},
            "minimax":{"base_url":"https://mini.example", "api_key":"test-mini-key", "model":"mini-test"},
            "comfy":{"base_url":"http://localhost:8188"}
        }})).unwrap();
        make_backup(&settings, image, video).unwrap()
    }

    #[test]
    fn company_json_installs_all_models_and_keys_while_preserving_employee_paths() {
        let packet = company_backup();
        assert_eq!(
            packet["settings"]["chatTools"]["nativeTools"]["workingDirectory"],
            ""
        );
        assert_eq!(
            packet["settings"]["chat"]["externalCliAgents"]["codex"]["path"],
            ""
        );
        assert_eq!(packet["imageStudio"]["outputRoot"], "");
        let mut local = Settings::default();
        local.chat_tools.native_tools.working_directory = "/employee/work".into();
        local.chat_tools.servers.push(ChatMcpServer {
            id: "plugin-video".into(),
            command: "/employee/python".into(),
            env: [("PATH".into(), "/employee/runtime".into())].into(),
            ..Default::default()
        });
        let mut imported = parse(&packet.to_string(), &local, false).unwrap();
        let settings = sanitize_settings(imported.settings.clone());
        assert_eq!(settings.providers[0].api_keys, ["test-company-key"]);
        assert_eq!(settings.default_models.chat.model, "chat-test");
        assert_eq!(settings.default_models.image_generation.model, "image-test");
        assert_eq!(
            settings.chat.external_cli_agents["codex"].custom_models[0].id,
            "code-test"
        );
        assert_eq!(
            settings.chat_tools.native_tools.working_directory,
            "/employee/work"
        );
        assert_eq!(
            imported.settings.chat_tools.servers[0].command,
            "/employee/python"
        );
        assert!(imported.settings.chat_tools.servers[0].enabled);
        assert_eq!(
            imported.settings.chat_tools.servers[0].env["API_KEY"],
            "test-plugin-key"
        );
        assert_eq!(
            imported.settings.chat_tools.servers[0].env["PATH"],
            "/employee/runtime"
        );
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("image/config.json");
        let video_path = dir.path().join("video/providers.json");
        write_atomic(&image_path, br#"{"outputRoot":"/employee/images"}"#).unwrap();
        imported
            .install_studios_at(image_path.clone(), video_path.clone())
            .unwrap()
            .commit();
        let image: StudioConfig = read_optional(&image_path).unwrap().unwrap();
        assert_eq!(image.provider_id, "company");
        assert_eq!(image.model, "image-test");
        assert_eq!(image.agent_model, "chat-test");
        assert_eq!(image.output_root, "/employee/images");
        let video: VideoConfig = read_optional(&video_path).unwrap().unwrap();
        assert_eq!(video.providers["grok"]["api_key"], "test-video-key");
        assert_eq!(video.providers["grok"]["model"], "video-test");
        assert_eq!(video.providers["minimax"]["api_key"], "test-mini-key");
        assert_eq!(
            video.providers["comfy"]["base_url"],
            "http://localhost:8188"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&video_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn legacy_backup_does_not_touch_studio_configuration() {
        let old = json!({"type":"settings-backup", "version":1, "settings":{"theme":"dark"}});
        let mut imported = parse(&old.to_string(), &Settings::default(), false).unwrap();
        assert_eq!(imported.settings.theme, "dark");
        assert!(imported.image.is_none() && imported.video.is_none());
        assert!(imported.install_studios().unwrap().previous.is_empty());
    }

    #[test]
    fn incomplete_or_future_packages_are_rejected_before_writing() {
        let local = Settings::default();
        for field in ["settings", "imageStudio", "videoProviders"] {
            let mut packet = company_backup();
            packet.as_object_mut().unwrap().remove(field);
            assert!(parse(&packet.to_string(), &local, false).is_err());
        }
        let mut packet = company_backup();
        packet["version"] = json!(99);
        assert!(parse(&packet.to_string(), &local, false).is_err());
        packet = company_backup();
        packet["videoProviders"]["providers"]["grok"]["api_key"] = json!(123);
        assert!(parse(&packet.to_string(), &local, false).is_err());
    }

    #[test]
    fn onboarding_import_completes_setup_without_losing_imported_models() {
        let local = Settings::default();
        for version in [1, 2] {
            let mut packet = company_backup();
            packet["version"] = json!(version);
            packet["settings"]["onboardingStatus"] = json!("pending");
            let ordinary = parse(&packet.to_string(), &local, false).unwrap();
            assert_eq!(ordinary.settings.onboarding_status, "pending");
            let imported = parse(&packet.to_string(), &local, true).unwrap();
            let saved = sanitize_settings(imported.settings);
            assert_eq!(saved.onboarding_status, "completed");
            assert_eq!(saved.default_models.chat.model, "chat-test");
            assert_eq!(saved.providers[0].api_keys, ["test-company-key"]);
            assert_eq!(imported.video.is_some(), version == 2);
        }
    }

    #[test]
    fn failed_second_file_restores_first_file_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("image.json");
        let blocked = dir.path().join("blocked");
        fs::write(&image, b"original image config").unwrap();
        fs::write(&blocked, b"file cannot be a directory").unwrap();
        assert!(FileUpdates::apply(vec![
            (image.clone(), b"replacement".to_vec()),
            (blocked.join("providers.json"), b"video config".to_vec()),
        ])
        .is_err());
        assert_eq!(fs::read(&image).unwrap(), b"original image config");
        assert_eq!(fs::read(&blocked).unwrap(), b"file cannot be a directory");
    }

    #[test]
    fn failed_application_settings_restores_existing_and_removes_new_studio_files() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("image.json");
        let video = dir.path().join("video.json");
        fs::write(&image, b"original").unwrap();
        let files = FileUpdates::apply(vec![
            (image.clone(), b"new image".to_vec()),
            (video.clone(), b"new video".to_vec()),
        ])
        .unwrap();
        assert_eq!(
            files.rollback_error("settings failed".into()),
            "settings failed"
        );
        assert_eq!(fs::read(image).unwrap(), b"original");
        assert!(!video.exists());
    }
}
