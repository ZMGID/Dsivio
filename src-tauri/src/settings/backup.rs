//! Settings backup compatibility. Company imports retain machine-local paths and use the
//! current media pools; they never reinstall the retired image/video configuration files.
use super::{DefaultModelSelection, ModelProvider, Settings};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn export(settings: &Settings) -> Value {
    // Keep the full-settings v1 envelope compatible with existing backup consumers.
    // Media providers, credentials and pools now live inside Settings as well.
    json!({"app": "dsivio", "type": "settings-backup", "version": 1, "settings": settings})
}

pub(crate) fn parse(
    raw: &str,
    local: &Settings,
    complete_onboarding: bool,
) -> Result<Settings, String> {
    let packet: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|_| "文件不是有效的 JSON".to_string())?;
    if packet.get("type").and_then(Value::as_str) != Some("settings-backup") {
        return Err("这不是 Dsivio 设置配置文件".into());
    }
    let version = match packet.get("version") {
        None => 1,
        Some(value) => value.as_u64().ok_or("配置版本格式不正确")?,
    };
    if !(1..=2).contains(&version) {
        return Err("不支持的配置版本，请升级 Dsivio 后导入".into());
    }
    let value = packet
        .get("settings")
        .filter(|v| v.is_object())
        .ok_or("配置缺少 settings 对象")?;
    let mut imported: Settings =
        serde_json::from_value(value.clone()).map_err(|_| "应用设置格式不正确".to_string())?;
    // Old packets cannot erase a registry or media pool they did not know about.
    if value.get("capabilityConfigText").is_none() {
        imported.capability_config_text = local.capability_config_text.clone();
    }
    if value.get("workbenchMedia").is_none() {
        imported.workbench_media = local.workbench_media.clone();
    }
    if version == 2 {
        migrate_legacy_media(&packet, &mut imported)?;
    }
    if version == 2 || complete_onboarding {
        keep_local_paths(&mut imported, local);
    }
    if complete_onboarding {
        imported.onboarding_status = "completed".into();
    }
    Ok(imported)
}

fn keep_local_paths(imported: &mut Settings, local: &Settings) {
    imported.chat_tools.native_tools.working_directory =
        local.chat_tools.native_tools.working_directory.clone();
    imported.chat_tools.native_tools.workspace_roots =
        local.chat_tools.native_tools.workspace_roots.clone();
    imported.chat_tools.skill_scan_paths = local.chat_tools.skill_scan_paths.clone();
    imported.image_archive_path = local.image_archive_path.clone();
    imported.obsidian_vault_path = local.obsidian_vault_path.clone();
    for provider in &mut imported.providers {
        if let Some(auth) = &mut provider.request.oauth {
            // Credential IDs point into this computer's credential store, not the sender's.
            auth.credential_id = local
                .providers
                .iter()
                .find(|p| {
                    p.id == provider.id
                        && p.base_url == provider.base_url
                        && p.api_format == provider.api_format
                })
                .and_then(|p| p.request.oauth.as_ref())
                .filter(|saved| saved.provider == auth.provider)
                .and_then(|saved| saved.credential_id.clone());
        }
    }
    for server in &mut imported.chat_tools.servers {
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
        } else if server.transport == "stdio" {
            // A sender's bundled executable is not installed on this machine.
            server.enabled = false;
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyImage {
    provider_id: String,
    model: String,
}

#[derive(Deserialize)]
struct LegacyVideo {
    version: u32,
    providers: BTreeMap<String, Value>,
}

fn add_to_pool(pool: &mut Vec<DefaultModelSelection>, provider_id: &str, model: &str) {
    if !pool
        .iter()
        .any(|entry| entry.provider_id == provider_id && entry.model == model)
    {
        pool.push(DefaultModelSelection {
            provider_id: provider_id.into(),
            model: model.into(),
        });
    }
}

fn migrate_legacy_media(packet: &Value, imported: &mut Settings) -> Result<(), String> {
    let image: LegacyImage = serde_json::from_value(
        packet
            .get("imageStudio")
            .filter(|v| v.is_object())
            .ok_or("旧配置缺少 imageStudio 对象")?
            .clone(),
    )
    .map_err(|_| "生图配置格式不正确")?;
    let id = image.provider_id.trim();
    let model = image.model.trim();
    if !id.is_empty() || !model.is_empty() {
        let provider = imported
            .providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("旧生图配置引用了不存在的服务商")?;
        if model.is_empty() {
            return Err("旧生图配置缺少模型".into());
        }
        if !provider.enabled_models.iter().any(|m| m == model) {
            provider.enabled_models.push(model.into());
        }
        if !crate::chat::model_metadata::model_can_generate_images_directly(provider, model) {
            return Err(
                "旧生图模型无法迁移到统一媒体协议，请在新版「媒体创作」中配置后重新导出".into(),
            );
        }
        add_to_pool(&mut imported.workbench_media.image_models, id, model);
    }
    let video: LegacyVideo = serde_json::from_value(
        packet
            .get("videoProviders")
            .ok_or("旧配置缺少 videoProviders 对象")?
            .clone(),
    )
    .map_err(|_| "视频配置格式不正确")?;
    if video.version != 1 {
        return Err("不支持的旧视频配置版本".into());
    }
    for (name, value) in video.providers {
        if name.trim().is_empty() || name.chars().any(char::is_whitespace) || !value.is_object() {
            return Err("视频供应商配置格式不正确".into());
        }
        // MiniMax H3 is the one legacy product with an explicit native protocol.
        // Generic openai-video endpoints and Comfy servers without a workflow cannot be
        // translated into a native product route by guessing from their names.
        if value.get("type").and_then(Value::as_str) != Some("minimax-h3") {
            return Err(format!(
                "旧视频服务商「{name}」需要在新版「媒体创作」中指定协议或工作流后重新导出"
            ));
        }
        let key = value
            .get("api_key")
            .and_then(Value::as_str)
            .ok_or("旧视频配置缺少 API Key")?;
        if value
            .get("model")
            .is_some_and(|model| model.as_str() != Some("MiniMax-H3"))
        {
            return Err("旧 MiniMax H3 配置的模型不匹配，请在新版媒体创作中重新配置".into());
        }
        let catalog: Value =
            serde_json::from_str(include_str!("../../../src/data/videoModelCatalog.json"))
                .expect("validated video catalog");
        let base = match value.get("base_url") {
            None => catalog["protocols"]["minimax_h3"]["baseUrl"]
                .as_str()
                .ok_or("缺少视频协议地址")?,
            Some(value) => value
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or("旧视频地址格式不正确")?,
        };
        let id = format!("legacy-video-{name}");
        if imported.providers.iter().any(|p| p.id == id) {
            return Err("旧视频服务商 ID 与现有配置冲突".into());
        }
        let provider: ModelProvider = serde_json::from_value(json!({
            "id": id, "name": name, "baseUrl": base, "apiKeys": [key],
            "availableModels": ["MiniMax-H3"], "enabledModels": ["MiniMax-H3"],
            "modelOverrides": {"MiniMax-H3": {"videoProtocol": "minimax_h3"}}
        }))
        .map_err(|_| "旧视频服务商无法迁移")?;
        add_to_pool(
            &mut imported.workbench_media.video_models,
            &id,
            "MiniMax-H3",
        );
        imported.providers.push(provider);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{sanitize_settings, ChatMcpServer};

    fn company() -> Settings {
        serde_json::from_value(json!({
            "onboardingStatus": "pending",
            "providers": [{"id": "company", "name": "Company", "baseUrl": "https://gateway.test/v1",
                "apiKeys": ["test-key"], "enabledModels": ["chat-test", "gpt-image-1", "MiniMax-H3"]}],
            "defaultModels": {"chat": {"providerId": "company", "model": "chat-test"}},
            "workbenchMedia": {"imageModels": [{"providerId": "company", "model": "gpt-image-1"}],
                "videoModels": [{"providerId": "company", "model": "MiniMax-H3"}]},
            "chatTools": {"nativeTools": {"workingDirectory": "/sender/work"}}
        })).unwrap()
    }

    #[test]
    fn company_import_restores_unified_media_and_completes_without_copying_machine_paths() {
        let mut local = Settings::default();
        local.chat_tools.native_tools.working_directory = "/receiver/work".into();
        local.chat_tools.skill_scan_paths = vec!["/receiver/skills".into()];
        local.image_archive_path = "/receiver/images".into();
        let imported =
            sanitize_settings(parse(&export(&company()).to_string(), &local, true).unwrap());
        assert_eq!(imported.onboarding_status, "completed");
        assert_eq!(
            imported.chat_tools.native_tools.working_directory,
            "/receiver/work"
        );
        assert_eq!(imported.chat_tools.skill_scan_paths, ["/receiver/skills"]);
        assert_eq!(imported.image_archive_path, "/receiver/images");
        assert_eq!(imported.providers[0].api_keys, ["test-key"]);
        assert_eq!(imported.default_models.chat.model, "chat-test");
        assert_eq!(
            imported.workbench_media.image_models[0].model,
            "gpt-image-1"
        );
        assert_eq!(imported.workbench_media.video_models[0].model, "MiniMax-H3");
    }

    #[test]
    fn ordinary_v1_backup_still_restores_paths_and_onboarding_status() {
        let imported = parse(&export(&company()).to_string(), &Settings::default(), false).unwrap();
        assert_eq!(
            imported.chat_tools.native_tools.working_directory,
            "/sender/work"
        );
        assert_eq!(imported.onboarding_status, "pending");
    }

    #[test]
    fn company_import_keeps_installed_plugin_runtime_but_transfers_business_credentials() {
        let mut source = company();
        source.chat_tools.servers.push(ChatMcpServer {
            id: "plugin-test".into(),
            command: "/sender/python".into(),
            env: [
                ("PATH".into(), "/sender/runtime".into()),
                ("API_KEY".into(), "test-plugin-key".into()),
            ]
            .into(),
            enabled: true,
            ..Default::default()
        });
        let mut local = Settings::default();
        local.chat_tools.servers.push(ChatMcpServer {
            id: "plugin-test".into(),
            command: "/receiver/python".into(),
            env: [("PATH".into(), "/receiver/runtime".into())].into(),
            ..Default::default()
        });
        let imported = parse(&export(&source).to_string(), &local, true).unwrap();
        let server = &imported.chat_tools.servers[0];
        assert_eq!(server.command, "/receiver/python");
        assert_eq!(server.env["PATH"], "/receiver/runtime");
        assert_eq!(server.env["API_KEY"], "test-plugin-key");
        assert!(server.enabled);
    }

    #[test]
    fn company_oauth_needs_local_authorization() {
        let mut packet = export(&company());
        packet["settings"]["providers"][0]["request"] =
            json!({"oauth": {"provider": "codex", "credentialId": "sender-only"}});
        let imported = parse(&packet.to_string(), &Settings::default(), true).unwrap();
        assert!(imported.providers[0]
            .request
            .oauth
            .as_ref()
            .unwrap()
            .credential_id
            .is_none());
    }

    fn legacy() -> Value {
        let mut packet = export(&company());
        packet["version"] = json!(2);
        packet["settings"]
            .as_object_mut()
            .unwrap()
            .remove("workbenchMedia");
        packet["imageStudio"] = json!({"providerId": "company", "model": "gpt-image-1", "outputRoot": "/sender/images"});
        packet["videoProviders"] = json!({"version": 1, "providers": {"minimax": {"type": "minimax-h3", "api_key": "test-video-key"}}});
        packet
    }

    #[test]
    fn v2_media_moves_into_unified_settings_and_survives_export_reimport() {
        let imported =
            sanitize_settings(parse(&legacy().to_string(), &Settings::default(), true).unwrap());
        let again = parse(&export(&imported).to_string(), &Settings::default(), true).unwrap();
        assert_eq!(again.workbench_media.image_models[0].model, "gpt-image-1");
        assert_eq!(
            again.workbench_media.video_models[0].provider_id,
            "legacy-video-minimax"
        );
        let video = again
            .providers
            .iter()
            .find(|p| p.id == "legacy-video-minimax")
            .unwrap();
        assert_eq!(video.api_keys, ["test-video-key"]);
        assert_eq!(
            video.model_overrides["MiniMax-H3"]
                .video_protocol
                .as_deref(),
            Some("minimax_h3")
        );
    }

    #[test]
    fn unsupported_legacy_protocol_fails_instead_of_silently_discarding_media() {
        let mut packet = legacy();
        packet["videoProviders"]["providers"]["custom"] =
            json!({"type": "openai-video", "api_key": "test-key", "model": "unknown-video"});
        assert!(parse(&packet.to_string(), &Settings::default(), true)
            .unwrap_err()
            .contains("指定协议"));
    }

    #[test]
    fn invalid_packets_and_future_versions_are_rejected_before_writes() {
        for raw in [
            "bad",
            "null",
            r#"{"type":"other","settings":{}}"#,
            r#"{"type":"settings-backup","version":99,"settings":{}}"#,
            r#"{"type":"settings-backup","version":0,"settings":{}}"#,
            r#"{"type":"settings-backup","version":2,"settings":{}}"#,
            r#"{"type":"settings-backup","settings":null}"#,
        ] {
            assert!(parse(raw, &Settings::default(), true).is_err(), "{raw}");
        }
        assert!(parse(
            &format!("\u{feff}{}", export(&company())),
            &Settings::default(),
            true
        )
        .is_ok());
    }
}
