use super::types::{CredentialInput, ImPlatform};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

static GATE: Mutex<()> = Mutex::new(());
#[derive(Serialize, Deserialize)]
struct StoredCredential {
    identity: String,
    value: CredentialInput,
}
fn entry(platform: ImPlatform) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Dsivio.IM", platform.key()).map_err(|_| "系统凭证库不可用".into())
}
fn validate(platform: ImPlatform, input: &CredentialInput) -> Result<(), String> {
    if input.secret.trim().is_empty() {
        return Err("机器人密钥不能为空".into());
    }
    if platform == ImPlatform::WecomCallback
        && (input.token.trim().is_empty() || input.encoding_aes_key.trim().len() != 43)
    {
        return Err("自建应用需要 Token 和 43 字符 EncodingAESKey".into());
    }
    Ok(())
}
pub fn save(platform: ImPlatform, identity: &str, input: CredentialInput) -> Result<(), String> {
    if identity.trim().is_empty() {
        return Err("请先保存机器人 ID".into());
    }
    validate(platform, &input)?;
    let data = serde_json::to_vec(&StoredCredential {
        identity: identity.to_owned(),
        value: input,
    })
    .map_err(|_| "无法编码 IM 凭证")?;
    // Native Windows credentials have a small byte limit. IM credentials are short;
    // reject oversized input rather than silently falling back to a plaintext file.
    if data.len() > 2000 {
        return Err("IM 凭证超过系统凭证库大小限制".into());
    }
    let _guard = GATE.lock();
    entry(platform)?
        .set_secret(&data)
        .map_err(|_| "无法保存到系统凭证库".into())
}
pub fn load(platform: ImPlatform, identity: &str) -> Result<CredentialInput, String> {
    let _guard = GATE.lock();
    let data = entry(platform)?
        .get_secret()
        .map_err(|_| "机器人密钥未配置或系统凭证库不可用".to_string())?;
    let stored: StoredCredential =
        serde_json::from_slice(&data).map_err(|_| "IM 凭证损坏，请重新配置")?;
    if stored.identity != identity {
        return Err("机器人 ID 已改变，请为新机器人保存密钥".into());
    }
    validate(platform, &stored.value)?;
    Ok(stored.value)
}
pub fn clear(platform: ImPlatform) -> Result<(), String> {
    let _guard = GATE.lock();
    match entry(platform)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法删除系统 IM 凭证".into()),
    }
}
