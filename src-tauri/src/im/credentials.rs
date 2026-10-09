use super::types::{CredentialInput, ImPlatform};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

const SERVICE: &str = "Dsivio.IM";

/// OS keyring or a test double. Account bytes are the only stored secret material.
pub(crate) trait SecretVault: Send + Sync {
    fn set_secret(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String>;
    fn get_secret(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String>;
    fn delete_secret(&self, service: &str, account: &str) -> Result<(), String>;
}

struct OsVault;

impl SecretVault for OsVault {
    fn set_secret(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        entry(service, account)?
            .set_secret(secret)
            .map_err(|_| "无法保存到系统凭证库".to_string())
    }
    fn get_secret(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        match entry(service, account)?.get_secret() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("机器人密钥未配置或系统凭证库不可用".into()),
        }
    }
    fn delete_secret(&self, service: &str, account: &str) -> Result<(), String> {
        match entry(service, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("无法删除系统 IM 凭证".into()),
        }
    }
}

fn entry(service: &str, account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, account).map_err(|_| "系统凭证库不可用".to_string())
}

#[derive(Clone)]
pub(crate) struct CredentialStore {
    vault: Arc<dyn SecretVault>,
    gate: Arc<Mutex<()>>,
}

impl CredentialStore {
    pub(crate) fn system() -> Self {
        static STORE: OnceLock<CredentialStore> = OnceLock::new();
        STORE
            .get_or_init(|| Self::from_vault(Arc::new(OsVault)))
            .clone()
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

    /// Manual save. Identity is read after the keyring gate is acquired, so a settings
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
        self.migrate_legacy_locked(platform)?;
        let Some(data) = self
            .vault
            .get_secret(SERVICE, &scoped_account(platform, identity))?
        else {
            return Err("机器人密钥未配置或系统凭证库不可用".into());
        };
        decode(platform, identity, &data)
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
        self.migrate_legacy_locked(platform)?;
        self.vault
            .delete_secret(SERVICE, &scoped_account(platform, expected))
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
        self.migrate_legacy_locked(platform)?;
        self.vault
            .set_secret(SERVICE, &scoped_account(platform, identity), &data)
    }

    /// Move a platform-only keyring item into its identity slot, then remove the old slot.
    /// An identity slot that already exists is kept; unreadable legacy data is left in place.
    fn migrate_legacy_locked(&self, platform: ImPlatform) -> Result<(), String> {
        let legacy = platform.key();
        let Some(data) = self.vault.get_secret(SERVICE, legacy)? else {
            return Ok(());
        };
        let stored: StoredCredential =
            serde_json::from_slice(&data).map_err(|_| "IM 凭证损坏，请重新配置")?;
        if stored.identity.trim().is_empty() {
            return Err("IM 凭证损坏，请重新配置".into());
        }
        let account = scoped_account(platform, &stored.identity);
        if let Some(existing) = self.vault.get_secret(SERVICE, &account)? {
            if serde_json::from_slice::<StoredCredential>(&existing).is_err() {
                return Err("IM 凭证损坏，请重新配置".into());
            }
        } else {
            self.vault.set_secret(SERVICE, &account, &data)?;
        }
        self.vault.delete_secret(SERVICE, legacy)
    }
}

pub(crate) fn load(platform: ImPlatform, identity: &str) -> Result<CredentialInput, String> {
    CredentialStore::system().load(platform, identity)
}

/// Keyring I/O stays on the blocking pool so Tokio and UI cancellation can still run.
pub(crate) async fn off_runtime<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(_) => Err("系统凭证库操作失败".into()),
    }
}

#[derive(Serialize, Deserialize)]
struct StoredCredential {
    identity: String,
    value: CredentialInput,
}

fn scoped_account(platform: ImPlatform, identity: &str) -> String {
    format!("bot\u{1f}{}\u{1f}{identity}", platform.key())
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

fn decode(platform: ImPlatform, identity: &str, data: &[u8]) -> Result<CredentialInput, String> {
    let stored: StoredCredential =
        serde_json::from_slice(data).map_err(|_| "IM 凭证损坏，请重新配置")?;
    if stored.identity != identity {
        return Err("机器人 ID 已改变，请为新机器人保存密钥".into());
    }
    validate(platform, &stored.value)?;
    Ok(stored.value)
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct MemoryVault {
    rows: Mutex<std::collections::BTreeMap<(String, String), Vec<u8>>>,
    sets: std::sync::atomic::AtomicUsize,
    fail_next: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MemoryVault {
    pub(crate) fn raw(&self, service: &str, account: &str) -> Option<Vec<u8>> {
        self.rows
            .lock()
            .get(&(service.to_owned(), account.to_owned()))
            .cloned()
    }
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
    fn set_secret(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
        if self
            .fail_next
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err("无法保存到系统凭证库".into());
        }
        self.rows
            .lock()
            .insert((service.to_owned(), account.to_owned()), secret.to_vec());
        self.sets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn get_secret(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.raw(service, account))
    }
    fn delete_secret(&self, service: &str, account: &str) -> Result<(), String> {
        self.rows
            .lock()
            .remove(&(service.to_owned(), account.to_owned()));
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

    fn legacy(vault: &MemoryVault, platform: ImPlatform, identity: &str, input: CredentialInput) {
        let data = serde_json::to_vec(&StoredCredential {
            identity: identity.to_owned(),
            value: input,
        })
        .unwrap();
        vault.set_secret(SERVICE, platform.key(), &data).unwrap();
    }

    #[test]
    fn legacy_slot_moves_once_and_keeps_callback_fields() {
        let (store, vault) = CredentialStore::memory();
        let mut callback = secret("callback-secret");
        callback.token = "callback-token".into();
        callback.encoding_aes_key = "k".repeat(43);
        legacy(
            &vault,
            ImPlatform::WecomCallback,
            "corp-a",
            callback.clone(),
        );
        legacy(&vault, ImPlatform::Feishu, "app-a", secret("feishu-secret"));

        let loaded = store.load(ImPlatform::WecomCallback, "corp-a").unwrap();
        assert_eq!(loaded.secret, "callback-secret");
        assert_eq!(loaded.token, "callback-token");
        assert_eq!(loaded.encoding_aes_key.len(), 43);
        assert!(vault
            .raw(SERVICE, ImPlatform::WecomCallback.key())
            .is_none());
        assert!(vault
            .raw(
                SERVICE,
                &scoped_account(ImPlatform::WecomCallback, "corp-a")
            )
            .is_some());
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "feishu-secret"
        );
        assert!(vault.raw(SERVICE, ImPlatform::Feishu.key()).is_none());
        let sets = vault.set_count();
        store.load(ImPlatform::WecomCallback, "corp-a").unwrap();
        assert_eq!(vault.set_count(), sets);
    }

    #[test]
    fn migration_does_not_replace_an_existing_identity_slot() {
        let (store, vault) = CredentialStore::memory();
        store
            .save(ImPlatform::Feishu, "app-a", secret("new-slot"))
            .unwrap();
        legacy(&vault, ImPlatform::Feishu, "app-a", secret("old-slot"));
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "new-slot"
        );
        assert!(vault.raw(SERVICE, ImPlatform::Feishu.key()).is_none());
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
    fn clear_removes_only_the_selected_identity_and_legacy_alias() {
        let (store, vault) = CredentialStore::memory();
        legacy(&vault, ImPlatform::Feishu, "app-a", secret("legacy-a"));
        store
            .save(ImPlatform::Feishu, "app-b", secret("kept-b"))
            .unwrap();
        store
            .clear_if_current(ImPlatform::Feishu, "app-b", || "app-b".into())
            .unwrap();
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            "legacy-a"
        );
        assert!(vault.raw(SERVICE, ImPlatform::Feishu.key()).is_none());
    }
}
