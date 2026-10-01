//! Logical request identity includes the bytes of local media and consent evidence.
//! The task owner may supply captured hashes so accepted work never hashes a later file revision.
use super::MediaRequest;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

pub(crate) fn bytes_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn file_hash(path: &str) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("媒体证据不可读：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn media_sources(value: &Value, paths: &mut Vec<String>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(source) = item.as_str().or_else(|| item.get("source").and_then(Value::as_str)) {
                if Path::new(source).is_absolute() { paths.push(source.into()); }
            }
        }
    } else if let Some(source) = value.as_str().filter(|s| Path::new(s).is_absolute()) {
        paths.push(source.into());
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
struct LogicalRequest<'a> {
    provider_id: &'a str,
    model: &'a str,
    kind: &'a super::MediaKind,
    prompt: &'a str,
    images: &'a [String],
    options: &'a BTreeMap<String,Value>,
    evidence: BTreeMap<String,String>,
}
pub(crate) fn hash(request: &MediaRequest, snapshots: &BTreeMap<String, String>) -> Result<String, String> {
    let mut paths = request.images.iter().filter(|s| Path::new(s).is_absolute()).cloned().collect::<Vec<_>>();
    for (key, value) in &request.options {
        if matches!(key.as_str(), "voiceReference" | "consentAttestation" | "audioFile" | "firstFrame" | "lastFrame" | "referenceImages" | "referenceVideos" | "referenceAudios") {
            media_sources(value, &mut paths);
        }
    }
    paths.sort();
    paths.dedup();
    let mut evidence = BTreeMap::new();
    for path in paths {
        let hash = match snapshots.get(&path) { Some(hash) => hash.clone(), None => file_hash(&path)? };
        evidence.insert(path, hash);
    }
    let normalized;
    let options = if request.options.keys().any(|key|super::model_parameters::canonical_key(key)!=key) {
        normalized = super::model_parameters::normalize(request.options.clone())?;
        &normalized
    } else {&request.options};
    let content = LogicalRequest {provider_id:&request.provider_id,model:&request.model,kind:&request.kind,
        prompt:&request.prompt,images:&request.images,options,evidence};
    Ok(bytes_hash(&serde_jcs::to_vec(&content).map_err(|e| e.to_string())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_and_changed_wav_bytes_are_distinct_but_captured_evidence_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("audio.wav");
        std::fs::write(&audio, b"original audio").unwrap();
        let path = audio.to_string_lossy().into_owned();
        let mut request = MediaRequest { provider_id: "local".into(), model: "whisperx-small".into(), kind: super::super::MediaKind::Transcribe,
            prompt: String::new(), images: vec![], options: BTreeMap::from([("audioFile".into(),json!(path))]), origin: None, description_revision: None };
        let captured = BTreeMap::from([(path, bytes_hash(b"original audio"))]);
        let before = hash(&request, &BTreeMap::new()).unwrap();
        std::fs::write(&audio, b"changed audio").unwrap();
        assert_ne!(before, hash(&request, &BTreeMap::new()).unwrap());
        assert_eq!(before, hash(&request, &captured).unwrap());
        request.prompt = "different text".into();
        assert_ne!(before, hash(&request, &captured).unwrap());
    }
    #[test]
    fn equivalent_aliases_share_identity_but_zero_is_not_omission() {
        let mut request = MediaRequest {provider_id:"image-provider".into(),model:"image-model".into(),kind:super::super::MediaKind::Image,
            prompt:"one icon".into(),images:vec![],options:BTreeMap::from([("aspect_ratio".into(),json!("1:1")),("seed".into(),json!(0))]),origin:None,description_revision:None};
        let alias=hash(&request,&BTreeMap::new()).unwrap();
        request.options.remove("aspect_ratio");
        request.options.insert("aspectRatio".into(),json!("1:1"));
        assert_eq!(alias,hash(&request,&BTreeMap::new()).unwrap());
        request.options.remove("seed");
        assert_ne!(alias,hash(&request,&BTreeMap::new()).unwrap());
    }
}
