//! Explicit product routes. The host owns the task; this module owns durable speech receipts.
use super::{artifacts, model_parameters::{self, argument, DataType, ModelDescription}, voices::{self, Journal, Step}, MediaKind, MediaOutput};
use crate::settings::ModelProvider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

const SAMPLE_LIMIT: usize = 20 * 1024 * 1024;
const AUDIO_LIMIT: usize = 512 * 1024 * 1024;
const OPENAI_MODELS: &[&str] = &["tts-1", "tts-1-hd", "gpt-4o-mini-tts", "gpt-4o-mini-tts-2025-12-15"];
const MINIMAX_MODELS: &[&str] = &["speech-2.8-hd", "speech-2.8-turbo", "speech-2.6-hd", "speech-2.6-turbo", "speech-02-hd", "speech-02-turbo", "speech-01-hd", "speech-01-turbo"];
const OPENAI_VOICES: &[&str] = &["alloy", "ash", "ballad", "coral", "echo", "fable", "onyx", "nova", "sage", "shimmer", "verse", "marin", "cedar"];
// A deliberately finite supported subset of the provider's published system voice IDs.
const MINIMAX_VOICES: &[&str] = &["English_expressive_narrator", "English_Graceful_Lady", "Chinese (Mandarin)_Reliable_Executive", "Chinese (Mandarin)_News_Anchor", "Chinese (Mandarin)_Lyrical_Voice", "Chinese (Mandarin)_HK_Flight_Attendant"];

#[derive(Debug)]
pub struct SpeechError { pub message: String, pub uncertain: bool }
impl SpeechError {
    fn known(message: impl Into<String>) -> Self { Self { message: message.into(), uncertain: false } }
    fn uncertain(message: impl Into<String>) -> Self { Self { message: message.into(), uncertain: true } }
}
impl From<String> for SpeechError { fn from(message: String) -> Self { Self::known(message) } }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeechInput {
    pub mode: String,
    pub text: String,
    pub voice: Option<String>,
    pub instruction: Option<String>,
    pub output_format: String,
    pub speed: f64,
    #[serde(skip)]
    sample: Option<Arc<Vec<u8>>>,
    sample_extension: Option<String>,
    sample_sha256: Option<String>,
    consent_sha256: Option<String>,
    evidence: BTreeMap<String, String>,
}
impl SpeechInput {
    pub fn fingerprint(&self) -> BTreeMap<String, String> { self.evidence.clone() }
    /// Immutable evidence participates in the host's idempotency request identity.
    pub fn request_fingerprint(&self) -> String {
        voices::sha256(&serde_jcs::to_vec(&json!({"mode":self.mode,"text":self.text,"voice":self.voice,"instruction":self.instruction,"outputFormat":self.output_format,"speed":self.speed,"sampleSha256":self.sample_sha256,"consentSha256":self.consent_sha256})).expect("validated finite speech parameters"))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredInput { version: u32, input: SpeechInput, sha256: String }

fn input_checksum(input: &SpeechInput) -> Result<String, String> {
    Ok(voices::sha256(&serde_jcs::to_vec(input).map_err(|e| e.to_string())?))
}

/// Called before the host acknowledges a task. Only private evidence files contain sample bytes.
pub fn persist_input(input: &SpeechInput, task_dir: &Path) -> Result<(), String> {
    let checksum = input_checksum(input)?;
    let path = task_dir.join("speech-input.json");
    if path.exists() {
        if input_checksum(&load_input(task_dir)?)? != checksum { return Err("SPEECH_REQUEST_CHANGED: persisted input differs".into()); }
        return Ok(());
    }
    if input.mode == "clone" {
        let bytes = input.sample.as_ref().ok_or("Validated clone evidence missing")?;
        voices::atomic_bytes(&task_dir.join("speech-sample.evidence"), bytes)?;
    }
    #[derive(Serialize)]
    struct InputToStore<'a> { version: u32, input: &'a SpeechInput, sha256: String }
    let record = InputToStore { version: 1, input, sha256: checksum };
    voices::atomic_bytes(&path, &serde_jcs::to_vec(&record).map_err(|e| e.to_string())?)
}

pub fn load_input(task_dir: &Path) -> Result<SpeechInput, String> {
    let mut record: StoredInput = serde_json::from_slice(&std::fs::read(task_dir.join("speech-input.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if record.version != 1 || input_checksum(&record.input)? != record.sha256 { return Err("SPEECH_INPUT_CORRUPT: persisted input checksum differs".into()); }
    let pending_upload = match std::fs::read(task_dir.join("speech-state.json")) {
        Ok(bytes) => serde_json::from_slice::<Journal>(&bytes).map_err(|e| e.to_string())?.step == Step::PendingUpload,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(e.to_string()),
    };
    if record.input.mode == "clone" && pending_upload {
        let bytes = read_limited(&task_dir.join("speech-sample.evidence"), SAMPLE_LIMIT)?;
        if record.input.sample_sha256.as_deref() != Some(voices::sha256(&bytes).as_str()) { return Err("SPEECH_INPUT_CORRUPT: sample evidence checksum differs".into()); }
        record.input.sample = Some(Arc::new(bytes));
    }
    Ok(record.input)
}

/// A known receipt may advance only the next step; any uncertain/rejected POST remains terminal.
pub fn is_resumable(task_dir: &Path) -> bool {
    match std::fs::read(task_dir.join("speech-state.json")) {
        Ok(bytes) => if !serde_json::from_slice::<Journal>(&bytes).is_ok_and(|state| state.ensure_known().is_ok()) { return false; },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(_) => return false,
    }
    load_input(task_dir).is_ok()
}

struct Route<'a> { protocol: &'a str, base: reqwest::Url }
fn route<'a>(provider: &'a ModelProvider, model: &str) -> Result<Route<'a>, String> {
    let info = provider.model_overrides.get(model).ok_or("SPEECH_NOT_CONFIGURED: explicitly select a speech protocol and product base URL")?;
    let protocol = info.speech_protocol.as_deref().ok_or("SPEECH_NOT_CONFIGURED: speech protocol missing")?;
    let models = match protocol { "minimax_tts" => MINIMAX_MODELS, "openai_tts" => OPENAI_MODELS, _ => return Err("SPEECH_PROTOCOL_UNSUPPORTED".into()) };
    if !models.contains(&model) { return Err("SPEECH_MODEL_UNSUPPORTED: model is not implemented for this protocol".into()); }
    let base = info.speech_base_url.as_deref().filter(|s| !s.trim().is_empty()).ok_or("SPEECH_NOT_CONFIGURED: product base URL missing")?;
    let mut base = reqwest::Url::parse(base).map_err(|_| "SPEECH_BASE_URL_INVALID")?;
    if !matches!(base.scheme(), "https"|"http") || base.host_str().is_none() || !base.username().is_empty() || base.password().is_some() || base.query().is_some() || base.fragment().is_some() {
        return Err("SPEECH_BASE_URL_INVALID: HTTP(S) product base without credentials/query/fragment required".into());
    }
    if base.scheme() == "http" && !matches!(base.host_str(), Some("127.0.0.1"|"localhost"|"[::1]"|"::1")) { return Err("SPEECH_BASE_URL_INVALID: remote speech services require HTTPS".into()); }
    let path = base.path().trim_end_matches('/');
    let path = if path.ends_with("/v1") { format!("{path}/") } else if path.is_empty() { "/v1/".into() } else { format!("{path}/") };
    base.set_path(&path);
    Ok(Route { protocol, base })
}
pub fn configured(provider: &ModelProvider, model: &str) -> bool { route(provider, model).is_ok() && provider.preferred_api_key().is_some() }
fn systems(protocol: &str) -> &'static [&'static str] { if protocol == "minimax_tts" { MINIMAX_VOICES } else { OPENAI_VOICES } }
fn instructions_supported(model: &str) -> bool { model.starts_with("gpt-4o-mini-tts") }

pub fn model_description(provider: &ModelProvider, model: &str) -> ModelDescription {
    let root = crate::app_data::app_data_dir();
    description(provider, model, root.as_deref())
}
fn description(provider: &ModelProvider, model: &str, root: Option<&Path>) -> ModelDescription {
    let protocol = route(provider, model).map(|r| r.protocol).unwrap_or("");
    let minimax = protocol == "minimax_tts";
    let mut args = BTreeMap::new();
    args.insert("mode".into(), argument(DataType::String, Some("--mode"), json!({"allowed":if minimax {vec!["tts","clone"]} else {vec!["tts"]},"defaultValue":"tts"})));
    let mut text = argument(DataType::String, Some("--text-file"), json!({"minLength":1,"maxLength":if minimax {9999} else {4096},"lengthUnit":"unicodeCodePoint"}));
    text.required = true; text.transport["encoding"] = json!("utf8-file"); args.insert("text".into(), text);
    let mut voice_ids: Vec<String> = systems(protocol).iter().map(|s| (*s).into()).collect();
    if let Some(root) = root {
        if let Ok(references) = voices::list(root) {
            voice_ids.extend(references.into_iter().filter(|v| v.provider_id == provider.id && v.protocol == protocol && v.expires_at.as_ref().is_none_or(|e| chrono::DateTime::parse_from_rfc3339(e).is_ok_and(|t| t > chrono::Utc::now()))).map(|v| v.voice_id));
        }
    }
    args.insert("voice".into(), argument(DataType::String, Some("--voice"), json!({"allowed":voice_ids,"systemVoices":systems(protocol),"customVoiceOwnership":"providerLocalReference"})));
    args.insert("speed".into(), argument(DataType::Number, None, json!({"minimum":if minimax {0.5} else {0.25},"maximum":if minimax {2.0} else {4.0},"defaultValue":1})));
    args.insert("outputFormat".into(), argument(DataType::String, Some("--output-format"), json!({"allowed":if minimax {vec!["mp3","flac","wav","opus"]} else {vec!["mp3","opus","aac","flac","wav"]},"defaultValue":"wav"})));
    if protocol == "openai_tts" && instructions_supported(model) {
        let mut arg = argument(DataType::String, Some("--instruction-file"), json!({})); arg.transport["encoding"] = json!("utf8-file"); args.insert("instruction".into(), arg);
    }
    let mut constraints = vec![json!({"ruleId":"tts-needs-voice","when":{"equals":{"argument":"mode","value":"tts"}},"check":"require","arguments":["voice"]})];
    if minimax {
        args.insert("voiceReference".into(), argument(DataType::MediaList, Some("--voice-ref"), json!({"minCount":1,"maxCount":1,"maxBytes":SAMPLE_LIMIT,"mimePatterns":["audio/wav","audio/mpeg","audio/mp4"],"locations":["local"],"minDuration":10,"maxDuration":300})));
        let mut consent = argument(DataType::String, Some("--consent-attestation"), json!({"minLength":1,"resource":true,"locations":["local"]})); consent.transport["encoding"] = json!("utf8-file"); args.insert("consentAttestation".into(), consent);
        constraints.extend([
            json!({"ruleId":"clone-needs-evidence","when":{"equals":{"argument":"mode","value":"clone"}},"check":"require","arguments":["voiceReference","consentAttestation"]}),
            json!({"ruleId":"clone-excludes-voice","when":{"equals":{"argument":"mode","value":"clone"}},"check":"restrictAllowed","arguments":["voice"],"allowed":[]}),
            json!({"ruleId":"tts-excludes-reference","when":{"equals":{"argument":"mode","value":"tts"}},"check":"restrictAllowed","arguments":["voiceReference","consentAttestation"],"allowed":[]})]);
    }
    let mut result = model_parameters::finish(provider, model, MediaKind::Speech, args, constraints, false, Some(1), false);
    result.products = json!({"mediaKind":"audio","ordered":true,"countMeaning":"exact","minCount":1,"maxCount":1,"mimeTypes":if minimax {vec!["audio/mpeg","audio/flac","audio/wav","audio/ogg"]} else {vec!["audio/mpeg","audio/ogg","audio/aac","audio/flac","audio/wav"]},"hasAlpha":false});
    result.unknown_facts = vec!["text.lengthUnit".into()];
    model_parameters::rehash(&mut result);
    result
}

pub fn validate_input(provider: &ModelProvider, model: &str, args: &BTreeMap<String, Value>, root: &Path) -> Result<SpeechInput, String> {
    let route = route(provider, model)?;
    provider.preferred_api_key().ok_or("SPEECH_KEY_MISSING: configure an authorized product key on this provider")?;
    let args = model_parameters::resolve(&description(provider, model, Some(root)), args.clone(), None)?;
    let mode = args.get("mode").and_then(Value::as_str).unwrap_or("tts");
    let text = args.get("text").and_then(Value::as_str).ok_or("Speech text required")?;
    if text.trim().is_empty() { return Err("Speech text must contain non-whitespace text".into()); }
    // Conservative UTF-16 guard: vendor character counting is not documented precisely.
    if text.encode_utf16().count() > if route.protocol == "minimax_tts" {9999} else {4096} { return Err("Speech text exceeds the conservative character limit".into()); }
    let voice = args.get("voice").and_then(Value::as_str).map(str::to_owned);
    let mut input = SpeechInput { mode: mode.into(), text: text.into(), voice, instruction: args.get("instruction").and_then(Value::as_str).map(str::to_owned), output_format: args.get("outputFormat").and_then(Value::as_str).unwrap_or("wav").into(), speed: args.get("speed").and_then(Value::as_f64).unwrap_or(1.0), sample: None, sample_extension: None, sample_sha256: None, consent_sha256: None, evidence: BTreeMap::new() };
    if mode == "tts" {
        let voice = input.voice.as_deref().ok_or("TTS requires voice")?;
        if !systems(route.protocol).contains(&voice) { voices::require_owned(root, &provider.id, voice, route.protocol)?; }
        return Ok(input);
    }
    let consent_path = args.get("consentAttestation").and_then(Value::as_str).ok_or("Clone requires the user's consent attestation file")?;
    let consent_path = Path::new(consent_path);
    if !consent_path.is_absolute() { return Err("Consent attestation must be an absolute local file".into()); }
    let consent = read_limited(consent_path, 1024 * 1024)?;
    if std::str::from_utf8(&consent).map_err(|_| "Consent attestation must be UTF-8")?.trim().is_empty() { return Err("Consent attestation must contain the user's nonblank declaration; none is generated automatically".into()); }
    let source = args.get("voiceReference").and_then(Value::as_array).and_then(|items| items.first()).and_then(|item| item.as_str().or_else(|| item.get("source").and_then(Value::as_str))).ok_or("Clone requires one local audio sample")?;
    let source = Path::new(source);
    let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(extension.as_str(), "wav"|"mp3"|"m4a") { return Err("Clone sample must be WAV, MP3 or M4A".into()); }
    let sample = read_limited(source, SAMPLE_LIMIT)?;
    validate_sample(root, &sample, &extension)?;
    let sample_sha256 = voices::sha256(&sample);
    let consent_sha256 = voices::sha256(&consent);
    input.evidence.insert(source.to_string_lossy().into_owned(), sample_sha256.clone());
    input.evidence.insert(consent_path.to_string_lossy().into_owned(), consent_sha256.clone());
    input.sample = Some(Arc::new(sample)); input.sample_extension = Some(extension); input.sample_sha256 = Some(sample_sha256); input.consent_sha256 = Some(consent_sha256);
    Ok(input)
}
fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Audio/consent source must be a regular local file".into()); }
    let mut bytes = vec![]; file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() > limit { return Err(format!("Input file must contain 1..={limit} bytes")); }
    Ok(bytes)
}
fn validate_sample(root: &Path, sample: &[u8], extension: &str) -> Result<(), String> {
    let tools = crate::media_runtime::runtime::tools_at(&crate::media_runtime::runtime::root()?)?;
    let probe = tools.get("ffprobe").ok_or("Bundled ffprobe required to validate clone sample duration")?;
    let path = root.join("media-voice-validation").join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
    voices::atomic_bytes(&path, sample)?;
    let result = std::process::Command::new(probe).args(["-v","error","-show_entries","format=duration,format_name:stream=codec_type,codec_name","-of","json"]).arg(&path).output().map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    let output = result?;
    if !output.status.success() { return Err("Clone sample is not decodable audio".into()); }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Invalid audio probe result")?;
    validate_probe(&value, extension)
}
fn validate_probe(value: &Value, extension: &str) -> Result<(), String> {
    let duration = value.pointer("/format/duration").and_then(Value::as_str).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).ok_or("Clone sample duration unavailable")?;
    let format = value.pointer("/format/format_name").and_then(Value::as_str).unwrap_or("");
    let format_matches = match extension { "wav" => format == "wav", "mp3" => format == "mp3", "m4a" => format.split(',').any(|f| f == "m4a" || f == "mov"), _ => false };
    let streams = value.get("streams").and_then(Value::as_array).ok_or("Clone sample audio stream unavailable")?;
    if !format_matches || !streams.iter().any(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio")) || streams.iter().any(|s| s.get("codec_type").and_then(Value::as_str) == Some("video")) { return Err("Clone sample must be genuine WAV/MP3/M4A audio without video".into()); }
    if !(10.0..=300.0).contains(&duration) { return Err("Clone sample duration must be 10 seconds to 5 minutes".into()); }
    Ok(())
}

/// No preview synthesis, retry loop or credential rotation: an uncertain POST is never repeated.
pub async fn execute(client: &reqwest::Client, provider: &ModelProvider, model: &str, input: &SpeechInput, task_id: &str, task_dir: &Path, root: &Path, before_send: &(dyn Fn() -> Result<(), String> + Send + Sync)) -> Result<Vec<MediaOutput>, SpeechError> {
    let route = route(provider, model)?;
    let key = provider.preferred_api_key().ok_or_else(|| SpeechError::known("SPEECH_KEY_MISSING"))?;
    let fingerprint = voices::sha256(&serde_jcs::to_vec(&json!({"providerId":provider.id,"model":model,"protocol":route.protocol,"base":route.base.as_str(),"input":input.request_fingerprint()})).expect("speech request JSON"));
    let mut journal = Journal::load_or_new(task_dir, task_id, &fingerprint, input.voice.as_deref().unwrap_or(""), input.mode == "clone")?;
    if let Some(error) = &journal.rejected { return Err(SpeechError::known(error.clone())); }
    journal.ensure_known().map_err(SpeechError::uncertain)?;
    if journal.step == Step::Downloaded { return journal.output.map(|o| vec![o]).ok_or_else(|| SpeechError::known("Downloaded speech receipt lacks output")); }
    if journal.step == Step::PendingUpload {
        let bytes = input.sample.as_ref().ok_or_else(|| SpeechError::known("Validated clone evidence missing"))?;
        let extension = input.sample_extension.as_deref().unwrap_or("wav");
        let part = reqwest::multipart::Part::bytes(bytes.as_ref().clone()).file_name(format!("sample.{extension}")).mime_str(match extension { "mp3" => "audio/mpeg", "m4a" => "audio/mp4", _ => "audio/wav" }).map_err(|e| SpeechError::known(e.to_string()))?;
        let form = reqwest::multipart::Form::new().text("purpose", "voice_clone").part("file", part);
        let request = client.post(endpoint(&route,"files/upload")?).bearer_auth(key).multipart(form).timeout(Duration::from_secs(600));
        journal.begin(task_dir, "upload")?;
        remember_result(task_dir, &mut journal, before_send().map_err(SpeechError::known))?;
        let response = request.send().await;
        let value = remember_result(task_dir, &mut journal, json_response(response, provider, true).await)?;
        let id = value.pointer("/file/file_id").and_then(Value::as_u64).filter(|v| *v > 0).ok_or_else(|| SpeechError::uncertain("Upload response lacks file_id; never repeat upload"))?;
        journal.file_id = Some(id); journal.step = Step::Uploaded; journal.in_flight = None; journal.save(task_dir).map_err(SpeechError::uncertain)?;
    }
    if journal.step != Step::PendingUpload {
        match std::fs::remove_file(task_dir.join("speech-sample.evidence")) {
            Ok(()) => {},
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => return Err(SpeechError::known(e.to_string())),
        }
    }
    if journal.step == Step::Uploaded {
        let file_id = journal.file_id.ok_or_else(|| SpeechError::known("Upload receipt lacks file ID"))?;
        let request = client.post(endpoint(&route,"voice_clone")?).bearer_auth(key).json(&json!({"file_id":file_id,"voice_id":journal.voice_id})).timeout(Duration::from_secs(600));
        journal.begin(task_dir, "clone")?;
        remember_result(task_dir, &mut journal, before_send().map_err(SpeechError::known))?;
        let response = request.send().await;
        let value = remember_result(task_dir, &mut journal, json_response(response, provider, true).await)?;
        if value.pointer("/input_sensitive/type").and_then(Value::as_u64).is_some_and(|n| n != 0) {
            journal.in_flight = None; journal.rejected = Some("Clone rejected by provider content safety".into()); journal.save(task_dir)?;
            return Err(SpeechError::known("Clone rejected by provider content safety"));
        }
        journal.cloned_at = Some(chrono::Utc::now().to_rfc3339()); journal.step = Step::Cloned; journal.in_flight = None; journal.save(task_dir).map_err(SpeechError::uncertain)?;
    }
    if input.mode == "clone" {
        voices::cloned(root, &provider.id, &journal.voice_id, input.consent_sha256.as_deref().ok_or_else(|| SpeechError::known("Consent hash missing"))?, journal.cloned_at.as_deref().ok_or_else(|| SpeechError::known("Clone receipt timestamp missing"))?)?;
    }
    if journal.step == Step::Cloned {
        if input.mode == "clone" || !systems(route.protocol).contains(&journal.voice_id.as_str()) { voices::require_owned(root, &provider.id, &journal.voice_id, route.protocol)?; }
        let (path, body) = if route.protocol == "minimax_tts" {
            ("t2a_v2", json!({"model":model,"text":input.text,"stream":false,"output_format":"hex","voice_setting":{"voice_id":journal.voice_id,"speed":input.speed},"audio_setting":{"format":input.output_format}}))
        } else {
            let voice = if systems(route.protocol).contains(&journal.voice_id.as_str()) { json!(journal.voice_id) } else { json!({"id":journal.voice_id}) };
            let mut body = json!({"model":model,"input":input.text,"voice":voice,"response_format":input.output_format,"speed":input.speed,"stream_format":"audio"});
            if let Some(instruction) = &input.instruction { body["instructions"] = json!(instruction); }
            ("audio/speech", body)
        };
        let request = client.post(endpoint(&route,path)?).bearer_auth(key).json(&body).timeout(Duration::from_secs(600));
        journal.begin(task_dir, "tts")?;
        remember_result(task_dir, &mut journal, before_send().map_err(SpeechError::known))?;
        let response = request.send().await;
        let bytes = if route.protocol == "minimax_tts" {
            let value = remember_result(task_dir, &mut journal, json_response(response, provider, true).await)?;
            if value.pointer("/data/status").and_then(Value::as_u64) != Some(2) { return Err(SpeechError::uncertain("TTS response is not a completed audio receipt")); }
            if value.pointer("/extra_info/audio_format").and_then(Value::as_str).is_some_and(|f| f != input.output_format) { return Err(SpeechError::uncertain("Provider returned a different audio format")); }
            decode_hex(value.pointer("/data/audio").and_then(Value::as_str).ok_or_else(|| SpeechError::uncertain("TTS receipt lacks audio; never resubmit"))?).map_err(SpeechError::uncertain)?
        } else { remember_result(task_dir, &mut journal, audio_response(response, provider).await)? };
        // Preserve the only synthesis receipt before artifact validation. Restart may finish local
        // conversion/commit but must never request another paid synthesis.
        voices::atomic_bytes(&task_dir.join("speech-audio.receipt"), &bytes).map_err(SpeechError::uncertain)?;
        journal.step = Step::Synthesized; journal.in_flight = None; journal.save(task_dir).map_err(SpeechError::uncertain)?;
    }
    if journal.step == Step::Synthesized {
        if input.mode == "clone" || !systems(route.protocol).contains(&journal.voice_id.as_str()) { voices::mark_used(root, &provider.id, &journal.voice_id)?; }
        let bytes = std::fs::read(task_dir.join("speech-audio.receipt")).map_err(|e| SpeechError::known(e.to_string()))?;
        let output = artifacts::save_audio(&bytes, &task_dir.join(format!("audio.{}", input.output_format)), &input.output_format).await?;
        if input.mode == "clone" || !systems(route.protocol).contains(&journal.voice_id.as_str()) { voices::set_preview(root, &provider.id, &journal.voice_id, &output)?; }
        journal.output = Some(output.clone()); journal.step = Step::Downloaded; journal.save(task_dir)?;
        return Ok(vec![output]);
    }
    Err(SpeechError::known("Invalid speech receipt state"))
}
fn endpoint(route: &Route<'_>, path: &str) -> Result<reqwest::Url, String> { route.base.join(path).map_err(|_| "Invalid speech endpoint".into()) }
fn remember_result<T>(dir: &Path, journal: &mut Journal, result: Result<T, SpeechError>) -> Result<T, SpeechError> {
    if let Err(error) = &result {
        if !error.uncertain { journal.in_flight = None; journal.rejected = Some(error.message.clone()); journal.save(dir).map_err(SpeechError::uncertain)?; }
    }
    result
}
fn reject_http(status: reqwest::StatusCode) -> SpeechError {
    let message = format!("Speech provider HTTP {}", status.as_u16());
    if status.is_client_error() && status.as_u16() != 408 && status.as_u16() != 429 { SpeechError::known(message) } else { SpeechError::uncertain(message) }
}
fn redact(provider: &ModelProvider, message: &str) -> String {
    let mut result = message.to_owned(); for key in provider.api_keys.iter().filter(|k| !k.is_empty()) { result = result.replace(key, "[redacted]"); } result
}
fn business(value: &Value, provider: &ModelProvider) -> Result<(), SpeechError> {
    let code = value.pointer("/base_resp/status_code").and_then(Value::as_i64).ok_or_else(|| SpeechError::uncertain("Speech provider response lacks business status"))?;
    if code != 0 { return Err(SpeechError::known(format!("Speech provider business error {code}: {}", redact(provider, value.pointer("/base_resp/status_msg").and_then(Value::as_str).unwrap_or("request rejected"))))); }
    Ok(())
}
async fn json_response(response: Result<reqwest::Response, reqwest::Error>, provider: &ModelProvider, minimax: bool) -> Result<Value, SpeechError> {
    let response = response.map_err(|_| SpeechError::uncertain("Speech request interrupted; provider result uncertain"))?;
    if !response.status().is_success() { return Err(reject_http(response.status())); }
    // Hex audio can be twice the bounded binary product size.
    let bytes = artifacts::read_bounded(response, AUDIO_LIMIT * 2 + 1024 * 1024).await.map_err(SpeechError::uncertain)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| SpeechError::uncertain("Speech provider returned invalid JSON; never resubmit"))?;
    if minimax { business(&value, provider)?; }
    Ok(value)
}
async fn audio_response(response: Result<reqwest::Response, reqwest::Error>, provider: &ModelProvider) -> Result<Vec<u8>, SpeechError> {
    let response = response.map_err(|_| SpeechError::uncertain("Speech request interrupted; provider result uncertain"))?;
    if !response.status().is_success() { return Err(reject_http(response.status())); }
    let bytes = artifacts::read_bounded(response, AUDIO_LIMIT).await.map_err(SpeechError::uncertain)?;
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
        if let Some(error) = value.get("error") { return Err(SpeechError::known(format!("OpenAI speech business error: {}", redact(provider, error.get("message").and_then(Value::as_str).unwrap_or("request rejected"))))); }
        return Err(SpeechError::uncertain("OpenAI speech returned JSON instead of audio"));
    }
    Ok(bytes)
}
fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.is_empty() || hex.len() % 2 != 0 || hex.len() / 2 > AUDIO_LIMIT { return Err("Invalid/oversize audio hex receipt".into()); }
    fn digit(b: u8) -> Option<u8> { match b { b'0'..=b'9' => Some(b-b'0'), b'a'..=b'f' => Some(b-b'a'+10), b'A'..=b'F' => Some(b-b'A'+10), _ => None } }
    hex.as_bytes().chunks_exact(2).map(|p| Ok(digit(p[0]).ok_or("Invalid audio hex")? * 16 + digit(p[1]).ok_or("Invalid audio hex")?)).collect()
}

/// Checks authentication using free metadata, never paid preview synthesis.
pub async fn connection_check(provider: &ModelProvider, model: &str) -> Result<Value, String> {
    let route = route(provider, model)?;
    let key = provider.preferred_api_key().ok_or("SPEECH_KEY_MISSING")?;
    let client = artifacts::download_client(provider);
    let response = if route.protocol == "minimax_tts" {
        client.post(endpoint(&route,"get_voice")?).bearer_auth(key).json(&json!({"voice_type":"system"})).timeout(Duration::from_secs(30)).send().await
    } else {
        client.get(endpoint(&route,&format!("models/{model}"))?).bearer_auth(key).timeout(Duration::from_secs(30)).send().await
    };
    let value = json_response(response, provider, route.protocol == "minimax_tts").await.map_err(|e| e.message)?;
    if route.protocol == "openai_tts" && value.get("id").and_then(Value::as_str) != Some(model) { return Err("Model metadata did not confirm the selected model".into()); }
    Ok(json!({"configured":true,"authenticated":true,"speechAuthorization":"notVerified","message":"Metadata access confirmed; paid speech/clone entitlement is not verified and no audio was generated."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn provider(protocol: &str, model: &str) -> ModelProvider { serde_json::from_value(json!({"id":"p","name":"P","baseUrl":"https://chat.invalid","apiKeys":["private-key"],"modelOverrides":{model:{"speechProtocol":protocol,"speechBaseUrl":"https://speech.invalid/v1"}}})).unwrap() }
    #[test]
    fn options_consent_and_protocol_rejected_before_submission() {
        let root = std::env::temp_dir().join(format!("speech-input-{}",uuid::Uuid::new_v4()));
        let p = provider("openai_tts","tts-1");
        let base: BTreeMap<String,Value> = serde_json::from_value(json!({"text":"  Hello  ","voice":"alloy"})).unwrap();
        assert_eq!(validate_input(&p,"tts-1",&base,&root).unwrap().text,"  Hello  ");
        for extra in [json!({"instruction":"whisper"}),json!({"mode":"clone"}),json!({"speed":0}),json!({"outputFormat":"pcm"}),json!({"apiKey":"oops"})] {
            let mut args = base.clone(); args.extend(serde_json::from_value::<BTreeMap<String,Value>>(extra).unwrap()); assert!(validate_input(&p,"tts-1",&args,&root).is_err());
        }
        let p = provider("minimax_tts","speech-2.8-hd");
        let args = serde_json::from_value(json!({"mode":"clone","text":"Hello"})).unwrap();
        assert!(validate_input(&p,"speech-2.8-hd",&args,&root).is_err());
        std::fs::create_dir_all(&root).unwrap();
        let sample = root.join("sample.wav"); std::fs::write(&sample, b"not audio").unwrap();
        let consent = root.join("consent.txt"); std::fs::write(&consent, b" \n\t ").unwrap();
        let args = serde_json::from_value(json!({"mode":"clone","text":"Hello","voiceReference":[{"source":sample}],"consentAttestation":consent})).unwrap();
        assert!(validate_input(&p,"speech-2.8-hd",&args,&root).err().unwrap().contains("nonblank"));
        std::fs::remove_dir_all(&root).unwrap();
        let mut p = p; p.model_overrides.clear(); assert!(!configured(&p,"speech-2.8-hd"));
    }
    #[test]
    fn duration_and_business_errors_are_real_rejections() {
        let mut probe = json!({"format":{"duration":"10","format_name":"wav"},"streams":[{"codec_type":"audio"}]});
        assert!(validate_probe(&probe,"wav").is_ok());
        for duration in ["9.999","300.001","NaN"] { probe["format"]["duration"] = json!(duration); assert!(validate_probe(&probe,"wav").is_err()); }
        let p = provider("minimax_tts","speech-2.8-hd");
        let error = business(&json!({"base_resp":{"status_code":1004,"status_msg":"bad private-key"}}),&p).unwrap_err();
        assert!(!error.uncertain); assert!(!error.message.contains("private-key"));
        assert!(business(&json!({}),&p).unwrap_err().uncertain);
    }

    #[tokio::test]
    async fn cancelled_send_gate_prevents_upload_clone_and_tts_requests() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("speech-send-gate-{}", uuid::Uuid::new_v4()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut provider = provider("minimax_tts", "speech-2.8-hd");
        provider.model_overrides.get_mut("speech-2.8-hd").unwrap().speech_base_url = Some(format!("http://{}/v1", listener.local_addr().unwrap()));
        let input = SpeechInput { mode: "clone".into(), text: "Speech".into(), voice: None, instruction: None, output_format: "wav".into(), speed: 1.0,
            sample: Some(Arc::new(vec![0; 44])), sample_extension: Some("wav".into()), sample_sha256: Some(voices::sha256(&[0; 44])),
            consent_sha256: Some(voices::sha256(b"Explicit test-fixture attestation")), evidence: BTreeMap::new() };
        let route = route(&provider, "speech-2.8-hd").unwrap();
        let hash = voices::sha256(&serde_jcs::to_vec(&json!({"providerId":provider.id,"model":"speech-2.8-hd","protocol":route.protocol,"base":route.base.as_str(),"input":input.request_fingerprint()})).unwrap());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        for step in [Step::PendingUpload, Step::Uploaded, Step::Cloned] {
            let id = uuid::Uuid::new_v4().to_string();
            let directory = root.join(&id);
            let mut journal = Journal::load_or_new(&directory, &id, &hash, "", true).unwrap();
            journal.step = step; journal.file_id = Some(42); journal.cloned_at = Some(chrono::Utc::now().to_rfc3339()); journal.save(&directory).unwrap();
            let calls = AtomicUsize::new(0);
            let gate = || { calls.fetch_add(1, Ordering::SeqCst); Err("MEDIA_CANCELLED".into()) };
            let error = tokio::time::timeout(Duration::from_secs(2), execute(&client, &provider, "speech-2.8-hd", &input, &id, &directory, &root, &gate)).await.unwrap().unwrap_err();
            assert_eq!(error.message, "MEDIA_CANCELLED"); assert!(!error.uncertain);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(listener.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
            let recovered = Journal::load_or_new(&directory, &id, &hash, "", true).unwrap();
            assert!(recovered.in_flight.is_none()); assert_eq!(recovered.rejected.as_deref(), Some("MEDIA_CANCELLED"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persisted_clone_recovery_uses_snapshot_and_rejects_corruption_or_uncertainty() {
        let directory = std::env::temp_dir().join(format!("speech-input-recovery-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = b"private sample fixture";
        let input = SpeechInput { mode: "clone".into(), text: " Exact speech ".into(), voice: None, instruction: None, output_format: "wav".into(), speed: 1.0,
            sample: Some(Arc::new(bytes.to_vec())), sample_extension: Some("wav".into()), sample_sha256: Some(voices::sha256(bytes)),
            consent_sha256: Some(voices::sha256(b"Explicit test-fixture attestation")), evidence: [("/deleted/original.wav".into(), voices::sha256(bytes))].into() };
        persist_input(&input, &directory).unwrap();
        assert!(is_resumable(&directory));
        let loaded = load_input(&directory).unwrap();
        assert_eq!(loaded.text, " Exact speech "); assert_eq!(loaded.fingerprint(), input.fingerprint());
        assert_eq!(loaded.sample.as_deref().unwrap().as_slice(), bytes);
        assert!(!std::fs::read_to_string(directory.join("speech-input.json")).unwrap().contains("private sample fixture"));
        std::fs::write(directory.join("speech-sample.evidence"), b"corrupt").unwrap();
        assert!(!is_resumable(&directory));
        let mut journal = Journal::load_or_new(&directory, &id, "hash", "", true).unwrap();
        journal.step = Step::Uploaded; journal.file_id = Some(42); journal.save(&directory).unwrap();
        std::fs::remove_file(directory.join("speech-sample.evidence")).unwrap();
        assert!(is_resumable(&directory)); assert!(load_input(&directory).unwrap().sample.is_none());
        journal.begin(&directory, "clone").unwrap();
        assert!(!is_resumable(&directory));
        let mut stored: Value = serde_json::from_slice(&std::fs::read(directory.join("speech-input.json")).unwrap()).unwrap();
        stored["input"]["text"] = json!("changed");
        std::fs::write(directory.join("speech-input.json"), serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(load_input(&directory).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
