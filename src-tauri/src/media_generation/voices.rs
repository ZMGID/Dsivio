//! Local references and durable receipts for speech. Deletion never calls the vendor.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}, sync::Mutex};
use ts_rs::TS;

static REFERENCES: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VoiceReference {
    pub id: String,
    pub provider_id: String,
    pub voice_id: String,
    pub protocol: String,
    pub consent_sha256: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub used: bool,
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub preview: Option<super::MediaOutput>,
}

pub(crate) fn sha256(bytes: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(bytes)) }
fn reference_path(root: &Path, id: &str) -> PathBuf {
    root.join("media-voices").join(format!("{:x}.json", Sha256::digest(id.as_bytes())))
}

/// Private, fsynced atomic replacement. The sibling files contain receipts, never samples/keys.
pub(crate) fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing storage directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let temporary = parent.join(format!(".{}.part", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        #[cfg(unix)] fs::File::open(parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}
fn save_reference(root: &Path, reference: &VoiceReference) -> Result<(), String> {
    atomic_bytes(&reference_path(root, &reference.id), &serde_json::to_vec(reference).map_err(|e| e.to_string())?)
}

pub fn list(root: &Path) -> Result<Vec<VoiceReference>, String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let directory = root.join("media-voices");
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut result = vec![];
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        result.push(serde_json::from_slice::<VoiceReference>(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?);
    }
    result.sort_by(|a,b| a.id.cmp(&b.id));
    Ok(result)
}

pub fn delete(root: &Path, id: &str) -> Result<(), String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    match fs::remove_file(reference_path(root, id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn require_owned(root: &Path, provider: &str, voice: &str, protocol: &str) -> Result<VoiceReference, String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let reference: VoiceReference = serde_json::from_slice(&fs::read(reference_path(root, &format!("{provider}/{voice}"))).map_err(|_| "VOICE_NOT_OWNED: register an existing authorized voice for this provider first")?).map_err(|e| e.to_string())?;
    if reference.provider_id != provider || reference.voice_id != voice || reference.protocol != protocol {
        return Err("VOICE_NOT_OWNED: voice belongs to another provider/protocol".into());
    }
    if reference.expires_at.as_ref().is_some_and(|expiry| chrono::DateTime::parse_from_rfc3339(expiry).map_or(true, |time| time <= chrono::Utc::now())) {
        return Err("VOICE_EXPIRED: the unused temporary voice has expired".into());
    }
    Ok(reference)
}

/// Register an already-created lawful custom voice. Caller supplies the user's attestation hash;
/// this does not upload a sample, create a voice, or manufacture consent.
pub fn register_custom(root: &Path, provider: &str, voice: &str, consent_sha256: &str) -> Result<VoiceReference, String> {
    if provider.trim().is_empty() || voice.trim().is_empty() || !voice.starts_with("voice_") || voice.contains('/') ||
        !consent_sha256.strip_prefix("sha256:").is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("Invalid provider/custom voice/consent SHA256".into());
    }
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let reference = VoiceReference { id: format!("{provider}/{voice}"), provider_id: provider.into(), voice_id: voice.into(), protocol: "openai_tts".into(), consent_sha256: consent_sha256.into(), created_at: chrono::Utc::now().to_rfc3339(), expires_at: None, used: false, last_used_at: None, preview: None };
    save_reference(root, &reference)?;
    Ok(reference)
}

pub(crate) fn cloned(root: &Path, provider: &str, voice: &str, consent: &str, created_at: &str) -> Result<(), String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let path = reference_path(root, &format!("{provider}/{voice}"));
    if path.exists() { return Ok(()); } // Recovery never resets a used voice's lifetime.
    let created = chrono::DateTime::parse_from_rfc3339(created_at).map_err(|e| e.to_string())?;
    save_reference(root, &VoiceReference { id: format!("{provider}/{voice}"), provider_id: provider.into(), voice_id: voice.into(), protocol: "minimax_tts".into(), consent_sha256: consent.into(), created_at: created_at.into(), expires_at: Some((created + chrono::Duration::days(7)).to_rfc3339()), used: false, last_used_at: None, preview: None })
}
pub(crate) fn mark_used(root: &Path, provider: &str, voice: &str) -> Result<(), String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let path = reference_path(root, &format!("{provider}/{voice}"));
    let mut reference: VoiceReference = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    reference.used = true;
    reference.last_used_at = Some(chrono::Utc::now().to_rfc3339());
    // MiniMax's seven-day deletion applies only to voices not used for synthesis.
    reference.expires_at = None;
    save_reference(root, &reference)
}

pub(crate) fn set_preview(root: &Path, provider: &str, voice: &str, output: &super::MediaOutput) -> Result<(), String> {
    let _lock = REFERENCES.lock().map_err(|_| "Voice references unavailable")?;
    let path = reference_path(root, &format!("{provider}/{voice}"));
    let mut reference: VoiceReference = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    reference.preview = Some(output.clone());
    save_reference(root, &reference)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Step { PendingUpload, Uploaded, Cloned, Synthesized, Downloaded }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Journal {
    pub step: Step,
    pub request_hash: String,
    pub voice_id: String,
    pub file_id: Option<u64>,
    pub cloned_at: Option<String>,
    pub in_flight: Option<String>,
    pub rejected: Option<String>,
    pub output: Option<super::MediaOutput>,
}
impl Journal {
    pub fn load_or_new(dir: &Path, task: &str, hash: &str, voice: &str, clone: bool) -> Result<Self, String> {
        let path = dir.join("speech-state.json");
        if path.exists() {
            let saved: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if saved.request_hash != hash { return Err("SPEECH_REQUEST_CHANGED: receipt input differs".into()); }
            return Ok(saved);
        }
        let uuid = uuid::Uuid::parse_str(task).map_err(|_| "Invalid speech task ID")?;
        let saved = Self { step: if clone { Step::PendingUpload } else { Step::Cloned }, request_hash: hash.into(), voice_id: if clone { format!("Dv{}", uuid.simple()) } else { voice.into() }, file_id: None, cloned_at: None, in_flight: None, rejected: None, output: None };
        saved.save(dir)?;
        Ok(saved)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        atomic_bytes(&dir.join("speech-state.json"), &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
    pub fn begin(&mut self, dir: &Path, operation: &str) -> Result<(), String> {
        self.ensure_known()?;
        self.in_flight = Some(operation.into());
        self.save(dir)
    }
    pub fn ensure_known(&self) -> Result<(), String> {
        if let Some(step) = &self.in_flight { return Err(format!("SPEECH_SUBMISSION_UNCERTAIN: {step} may have completed; never resubmit this task")); }
        if let Some(error) = &self.rejected { return Err(error.clone()); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf { std::env::temp_dir().join(format!("speech-voices-{}", uuid::Uuid::new_v4())) }
    #[test]
    fn interrupted_upload_never_restarts_and_known_upload_preserves_id() {
        let root = root(); let task = uuid::Uuid::new_v4().to_string();
        let mut state = Journal::load_or_new(&root, &task, "hash", "", true).unwrap();
        state.begin(&root, "upload").unwrap();
        let mut recovered = Journal::load_or_new(&root, &task, "hash", "", true).unwrap();
        assert!(recovered.begin(&root, "upload").unwrap_err().contains("UNCERTAIN"));
        recovered.in_flight = None; recovered.step = Step::Uploaded; recovered.file_id = Some(42); recovered.save(&root).unwrap();
        let resumed = Journal::load_or_new(&root, &task, "hash", "", true).unwrap();
        assert_eq!(resumed.file_id, Some(42)); assert_eq!(resumed.voice_id, state.voice_id); assert_eq!(resumed.step, Step::Uploaded);
        assert!(Journal::load_or_new(&root, &task, "other", "", true).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ownership_expiry_use_and_local_deletion() {
        let root = root(); let consent = sha256(b"user supplied statement");
        let expired = (chrono::Utc::now() - chrono::Duration::days(8)).to_rfc3339();
        cloned(&root, "p", "Dvabc", &consent, &expired).unwrap();
        assert!(require_owned(&root, "p", "Dvabc", "minimax_tts").unwrap_err().contains("EXPIRED"));
        mark_used(&root, "p", "Dvabc").unwrap();
        let owned = require_owned(&root, "p", "Dvabc", "minimax_tts").unwrap();
        assert!(owned.used); assert_eq!(owned.consent_sha256, consent); assert!(owned.expires_at.is_none());
        assert!(require_owned(&root, "other", "Dvabc", "minimax_tts").is_err());
        delete(&root, &owned.id).unwrap(); assert!(list(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
