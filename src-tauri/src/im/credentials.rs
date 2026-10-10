use super::types::{CredentialInput, ImPlatform};
use parking_lot::Mutex;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

/// Local JSON storage or an isolated test store. The owning runtime shares its write gate.
pub(crate) trait SecretVault: Send + Sync {
    fn set_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
        input: CredentialInput,
    ) -> Result<(), String>;
    fn get_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
    ) -> Result<Option<CredentialInput>, String>;
    fn delete_secret(&self, platform: ImPlatform, identity: &str) -> Result<(), String>;
}

type Records = BTreeMap<String, BTreeMap<String, CredentialInput>>;

struct JsonVault {
    path: PathBuf,
}

impl JsonVault {
    fn read(&self) -> Result<Records, String> {
        match std::fs::read(&self.path) {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|_| "IM 凭证文件损坏，请检查 credentials.json".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Records::new()),
            Err(_) => Err("无法读取 IM 凭证文件".into()),
        }
    }

    fn write(&self, records: &Records) -> Result<(), String> {
        let data = serde_json::to_string_pretty(records).map_err(|_| "无法编码 IM 凭证")?;
        crate::chat::storage::atomic_write(&self.path, &data, "IM credentials")
    }
}

impl SecretVault for JsonVault {
    fn set_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
        input: CredentialInput,
    ) -> Result<(), String> {
        let mut records = self.read()?;
        records
            .entry(platform.key().to_owned())
            .or_default()
            .insert(identity.to_owned(), input);
        self.write(&records)
    }

    fn get_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
    ) -> Result<Option<CredentialInput>, String> {
        Ok(self
            .read()?
            .get_mut(platform.key())
            .and_then(|bots| bots.remove(identity)))
    }

    fn delete_secret(&self, platform: ImPlatform, identity: &str) -> Result<(), String> {
        let mut records = self.read()?;
        if let Some(bots) = records.get_mut(platform.key()) {
            if bots.remove(identity).is_some() {
                return self.write(&records);
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct CredentialStore {
    vault: Arc<dyn SecretVault>,
    gate: Arc<Mutex<()>>,
}

impl CredentialStore {
    pub(crate) fn file(path: PathBuf) -> Self {
        Self::from_vault(Arc::new(JsonVault { path }))
    }
    pub(crate) fn from_vault(vault: Arc<dyn SecretVault>) -> Self {
        Self {
            vault,
            gate: Arc::new(Mutex::new(())),
        }
    }
    #[cfg(test)]
    pub(crate) fn memory() -> (Self, Arc<MemoryVault>) {
        let vault = Arc::new(MemoryVault::default());
        (Self::from_vault(vault.clone()), vault)
    }

    #[cfg(test)]
    pub(crate) fn save(
        &self,
        platform: ImPlatform,
        identity: &str,
        input: CredentialInput,
    ) -> Result<(), String> {
        let _guard = self.gate.lock();
        self.write_locked(platform, identity, input)
    }

    /// Manual save. Identity is read after the credential gate is acquired, so a settings
    /// change while this call waited cannot write the wrong slot.
    pub(crate) fn save_if_current(
        &self,
        platform: ImPlatform,
        expected: &str,
        input: CredentialInput,
        canonical: impl FnOnce() -> String,
    ) -> Result<(), String> {
        let _guard = self.gate.lock();
        let canonical = canonical();
        if expected.is_empty() || canonical != expected {
            return Err("机器人 ID 已改变，未保存密钥".into());
        }
        self.write_locked(platform, expected, input)
    }

    pub(crate) fn load(
        &self,
        platform: ImPlatform,
        identity: &str,
    ) -> Result<CredentialInput, String> {
        if identity.trim().is_empty() {
            return Err("请先保存机器人 ID".into());
        }
        let _guard = self.gate.lock();
        let Some(input) = self.vault.get_secret(platform, identity)? else {
            return Err("机器人密钥未配置，请重新扫码连接".into());
        };
        validate(platform, &input)?;
        Ok(input)
    }

    pub(crate) fn clear_if_current(
        &self,
        platform: ImPlatform,
        expected: &str,
        canonical: impl FnOnce() -> String,
    ) -> Result<(), String> {
        let _guard = self.gate.lock();
        let canonical = canonical();
        if expected.is_empty() || canonical != expected {
            return Err("机器人 ID 已改变，未删除密钥".into());
        }
        self.vault.delete_secret(platform, expected)
    }

    /// Last check before the external write. `still_ok` is the setup cancel flag.
    pub(crate) fn commit_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
        expected: &str,
        input: CredentialInput,
        canonical: impl FnOnce() -> String,
        still_ok: &dyn Fn() -> bool,
    ) -> Result<bool, String> {
        let _guard = self.gate.lock();
        if !still_ok() {
            return Ok(false);
        }
        let canonical_now = canonical();
        if expected.is_empty() || canonical_now != expected || expected != identity {
            return Err("机器人身份与当前配置不一致，未保存密钥".into());
        }
        self.write_locked(platform, identity, input)?;
        Ok(still_ok())
    }

    fn write_locked(
        &self,
        platform: ImPlatform,
        identity: &str,
        input: CredentialInput,
    ) -> Result<(), String> {
        if identity.trim().is_empty() {
            return Err("请先保存机器人 ID".into());
        }
        validate(platform, &input)?;
        self.vault.set_secret(platform, identity, input)
    }
}

/// File I/O stays on the blocking pool so Tokio and UI cancellation can still run.
pub(crate) async fn off_runtime<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(_) => Err("IM 凭证操作失败".into()),
    }
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

#[cfg(test)]
#[derive(Default)]
pub(crate) struct MemoryVault {
    rows: Mutex<Records>,
    sets: std::sync::atomic::AtomicUsize,
    fail_next: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MemoryVault {
    pub(crate) fn set_count(&self) -> usize {
        self.sets.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(crate) fn fail_next_set(&self) {
        self.fail_next
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
impl SecretVault for MemoryVault {
    fn set_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
        input: CredentialInput,
    ) -> Result<(), String> {
        if self
            .fail_next
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err("无法保存 IM 凭证".into());
        }
        self.rows
            .lock()
            .entry(platform.key().to_owned())
            .or_default()
            .insert(identity.to_owned(), input);
        self.sets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn get_secret(
        &self,
        platform: ImPlatform,
        identity: &str,
    ) -> Result<Option<CredentialInput>, String> {
        Ok(self
            .rows
            .lock()
            .get(platform.key())
            .and_then(|bots| bots.get(identity))
            .cloned())
    }
    fn delete_secret(&self, platform: ImPlatform, identity: &str) -> Result<(), String> {
        if let Some(bots) = self.rows.lock().get_mut(platform.key()) {
            bots.remove(identity);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(value: &str) -> CredentialInput {
        CredentialInput {
            secret: value.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn identity_drift_and_failed_write_leave_both_slots() {
        let (store, vault) = CredentialStore::memory();
        store
            .save(ImPlatform::Feishu, "app-a", secret("kept-a"))
            .unwrap();
        store
            .save(ImPlatform::Wecom, "bot-a", secret("kept-wecom"))
            .unwrap();
        let err = store
            .save_if_current(
                ImPlatform::Feishu,
                "app-b",
                secret("should-not-land"),
                || "app-a".into(),
            )
            .unwrap_err();
        assert!(!err.contains("should-not-land"));
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "kept-a"
        );

        vault.fail_next_set();
        let err = store
            .save_if_current(ImPlatform::Feishu, "app-a", secret("replacement"), || {
                "app-a".into()
            })
            .unwrap_err();
        assert!(!err.contains("replacement"));
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "kept-a"
        );
        assert_eq!(
            store.load(ImPlatform::Wecom, "bot-a").unwrap().secret,
            "kept-wecom"
        );

        let err = store
            .clear_if_current(ImPlatform::Feishu, "app-a", || "app-b".into())
            .unwrap_err();
        assert!(!err.contains("kept-a"));
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "kept-a"
        );
    }

    #[test]
    fn clear_removes_only_the_selected_identity() {
        let (store, _) = CredentialStore::memory();
        store
            .save(ImPlatform::Feishu, "app-a", secret("kept-a"))
            .unwrap();
        store
            .save(ImPlatform::Feishu, "app-b", secret("kept-b"))
            .unwrap();
        store
            .clear_if_current(ImPlatform::Feishu, "app-b", || "app-b".into())
            .unwrap();
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "kept-a"
        );
    }

    #[test]
    fn json_survives_restart_and_scopes_credentials_by_platform_and_bot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("im/credentials.json");
        let store = CredentialStore::file(path.clone());
        store
            .save(ImPlatform::Feishu, "bot-a", secret("feishu-a"))
            .unwrap();
        store
            .save(ImPlatform::Feishu, "bot-b", secret("feishu-b"))
            .unwrap();
        store
            .save(ImPlatform::Wecom, "bot-a", secret("wecom-a"))
            .unwrap();
        let mut callback = secret("callback-secret");
        callback.token = "callback-token".into();
        callback.encoding_aes_key = "k".repeat(43);
        store
            .save(ImPlatform::WecomCallback, "corp-a", callback)
            .unwrap();
        let restarted = CredentialStore::file(path);
        assert_eq!(
            restarted.load(ImPlatform::Feishu, "bot-a").unwrap().secret,
            "feishu-a"
        );
        assert_eq!(
            restarted.load(ImPlatform::Feishu, "bot-b").unwrap().secret,
            "feishu-b"
        );
        assert_eq!(
            restarted.load(ImPlatform::Wecom, "bot-a").unwrap().secret,
            "wecom-a"
        );
        let callback = restarted.load(ImPlatform::WecomCallback, "corp-a").unwrap();
        assert_eq!(callback.secret, "callback-secret");
        assert_eq!(callback.token, "callback-token");
        assert_eq!(callback.encoding_aes_key, "k".repeat(43));
        restarted
            .clear_if_current(ImPlatform::Feishu, "bot-a", || "bot-a".into())
            .unwrap();
        assert!(restarted.load(ImPlatform::Feishu, "bot-a").is_err());
        assert_eq!(
            restarted.load(ImPlatform::Feishu, "bot-b").unwrap().secret,
            "feishu-b"
        );
        assert_eq!(
            restarted.load(ImPlatform::Wecom, "bot-a").unwrap().secret,
            "wecom-a"
        );
        assert!(restarted.load(ImPlatform::Feishu, "not-saved").is_err());
    }

    #[test]
    fn invalid_json_is_not_overwritten_and_does_not_leak_secret_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let invalid = r#"{"secret":"private-marker", broken"#;
        std::fs::write(&path, invalid).unwrap();
        let store = CredentialStore::file(path.clone());
        let error = store.load(ImPlatform::Feishu, "app-a").err().unwrap();
        assert!(!error.contains("private-marker"));
        assert!(store
            .save(ImPlatform::Feishu, "app-a", secret("replacement"))
            .is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), invalid);
    }
}
