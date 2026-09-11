//! Same location as dsvideo_config.py; independent of the bundled Python runtime.
use std::path::{Path, PathBuf};

pub(crate) fn path() -> Result<PathBuf, String> {
    let home = directories::BaseDirs::new().ok_or("无法定位用户目录")?;
    Ok(resolve(
        home.home_dir(),
        std::env::var_os("DSVIDEO_CONFIG_PATH").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    ))
}

fn resolve(
    home: &Path,
    override_path: Option<PathBuf>,
    appdata: Option<PathBuf>,
    xdg: Option<PathBuf>,
) -> PathBuf {
    let nonempty = |p: &PathBuf| !p.as_os_str().is_empty();
    let expand = |p: PathBuf| match p.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => p,
    };
    if let Some(path) = override_path.filter(nonempty) {
        return expand(path);
    }
    if let Some(path) = appdata.filter(nonempty) {
        return path.join("dsvideo/providers.json");
    }
    xdg.filter(nonempty)
        .map(expand)
        .unwrap_or_else(|| home.join(".config"))
        .join("dsvideo/providers.json")
}

/// Resolve file-backed provider settings before MCP session fingerprinting.
/// Task recovery explicitly pins its original endpoint; ordinary chat uses the latest settings.
pub(crate) fn current_comfy_server(server: &crate::settings::ChatMcpServer) -> Result<crate::settings::ChatMcpServer, String> {
    let mut resolved = server.clone();
    if server.connector_id.as_deref() != Some(&format!("plugin:package:{}", super::PACKAGE_ID))
        || !server.name.ends_with("comfy-mcp") {
        return Ok(resolved);
    }
    let config_path = path()?;
    let provider: serde_json::Value = match std::fs::read(&config_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "视频配置文件格式无效，请检查视频设置")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Value::Null,
        Err(_) => return Err("无法读取视频配置文件，请检查视频设置".into()),
    };
    let url = comfy_endpoint(&server.env, &provider);
    resolved.env.insert("COMFYUI_URL".into(), url);
    resolved.env.insert("DSVIDEO_CONFIG_PATH".into(), config_path.to_string_lossy().into_owned());
    Ok(resolved)
}

fn comfy_endpoint(env: &std::collections::HashMap<String, String>, provider: &serde_json::Value) -> String {
    env.get("DSVIDEO_COMFY_TASK_URL").map(String::as_str)
        .or_else(|| provider["providers"]["comfy"]["base_url"].as_str().filter(|url| !url.trim().is_empty()))
        .unwrap_or("http://192.168.1.171:8188").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_uses_latest_provider_but_recovery_keeps_original_endpoint() {
        let mut env = std::collections::HashMap::from([("COMFYUI_URL".into(), "http://127.0.0.1:8188".into())]);
        let provider = serde_json::json!({"providers":{"comfy":{"base_url":"http://192.168.1.171:8188"}}});
        assert_eq!(comfy_endpoint(&env, &provider), "http://192.168.1.171:8188");
        env.insert("DSVIDEO_COMFY_TASK_URL".into(), "http://original:8188".into());
        assert_eq!(comfy_endpoint(&env, &provider), "http://original:8188");
    }

    #[test]
    fn video_config_locations_match_python_contract() {
        let home = Path::new("/users/employee");
        assert_eq!(
            resolve(home, None, None, None),
            home.join(".config/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(home, None, Some("roaming".into()), None),
            PathBuf::from("roaming/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(home, None, None, Some("~/config".into())),
            home.join("config/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(
                home,
                Some("~/company.json".into()),
                Some("roaming".into()),
                None
            ),
            home.join("company.json")
        );
    }
}
