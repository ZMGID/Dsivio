//! Standard evidence validation and the explicitly configured OpenAI whisper-1 route.
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

pub struct WavEvidence { pub sample_frames: u64, pub bytes: Vec<u8> }
pub struct TranscribeInput {
    pub audio_file: PathBuf,
    pub language: String,
    pub sample_frames: u64,
    pub timestamps: String,
}
pub fn validate_language(language: &str) -> Result<(), String> {
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) || matches!(language, "auto" | "und") {
        return Err("ASR_LANGUAGE_INVALID: explicit lowercase 2–3 letter language required".into());
    }
    Ok(())
}
pub fn validate_input(args: &BTreeMap<String, Value>) -> Result<TranscribeInput, String> {
    for key in args.keys() {
        if !matches!(key.as_str(), "audioFile" | "language" | "sampleFrames" | "timestamps") {
            return Err(format!("MODEL_ARGUMENT_UNSUPPORTED: {key}"));
        }
    }
    let audio_file = PathBuf::from(args.get("audioFile").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("ASR_AUDIO_REQUIRED")?);
    if !audio_file.is_absolute() { return Err("ASR_AUDIO_INVALID: absolute path required".into()); }
    let language = args.get("language").and_then(Value::as_str).ok_or("ASR_LANGUAGE_REQUIRED")?.to_owned();
    validate_language(&language)?;
    let expected = args.get("sampleFrames").map(|value| value.as_u64().filter(|n| *n > 0 && *n <= 9_007_199_254_740_991).ok_or("ASR_SAMPLE_FRAMES_INVALID")).transpose()?;
    let timestamps = args.get("timestamps").map(|v| v.as_str().ok_or("ASR_TIMESTAMPS_INVALID")).transpose()?.unwrap_or("word");
    if !matches!(timestamps, "word" | "segment") { return Err("ASR_TIMESTAMPS_INVALID".into()); }
    let sample_frames = validate_wav_frames(&audio_file, expected)?;
    Ok(TranscribeInput {audio_file, language, sample_frames, timestamps:timestamps.into()})
}

/// Validate the actual RIFF container, not its extension or MIME claim. No conversion occurs.
pub fn validate_wav(path: &Path, expected_frames: Option<u64>) -> Result<WavEvidence, String> {
    let file = fs::File::open(path).map_err(|e| format!("ASR_AUDIO_INVALID: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 { return Err("ASR_AUDIO_INVALID: regular WAV ≤512MiB required".into()); }
    use std::io::Read;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(512 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    validate_wav_bytes(&bytes, expected_frames).map(|sample_frames| WavEvidence {sample_frames, bytes})
}
/// Metadata-only preflight: skip PCM data without allocating or copying it.
pub fn validate_wav_frames(path: &Path, expected: Option<u64>) -> Result<u64, String> {
    let mut file=fs::File::open(path).map_err(|e|format!("ASR_AUDIO_INVALID: {e}"))?;
    if !file.metadata().map_err(|e|e.to_string())?.is_file(){return Err("ASR_AUDIO_INVALID: regular WAV required".into());}
    validate_wav_reader(&mut file,expected)
}
fn validate_wav_bytes(bytes: &[u8], expected: Option<u64>) -> Result<u64, String> {
    validate_wav_reader(&mut std::io::Cursor::new(bytes),expected)
}
fn validate_wav_reader(reader: &mut (impl std::io::Read + std::io::Seek), expected: Option<u64>) -> Result<u64, String> {
    use std::io::SeekFrom;
    let bad = || "ASR_AUDIO_INVALID: expected exact WAV16kHz/mono/PCM-s16 evidence".to_string();
    let size=reader.seek(SeekFrom::End(0)).map_err(|_|bad())?;
    if !(44..=512*1024*1024).contains(&size){return Err(bad());}
    reader.seek(SeekFrom::Start(0)).map_err(|_|bad())?;
    let mut header=[0u8;12];reader.read_exact(&mut header).map_err(|_|bad())?;
    if &header[..4]!=b"RIFF" || &header[8..]!=b"WAVE" || u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64+8!=size{return Err(bad());}
    let mut offset=12u64;let mut fmt=false;let mut frames=None;
    while offset<size {
        if size-offset<8 {return Err(bad());}
        let mut header=[0u8;8];reader.read_exact(&mut header).map_err(|_|bad())?;
        let id=&header[..4];let len=u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
        let end=(offset+8).checked_add(len).filter(|end|*end<=size).ok_or_else(bad)?;
        if id==b"fmt " {
            if fmt || len!=16{return Err(bad());}
            let mut data=[0u8;16];reader.read_exact(&mut data).map_err(|_|bad())?;
            let u16_at=|n:usize|u16::from_le_bytes([data[n],data[n+1]]);
            let u32_at=|n:usize|u32::from_le_bytes(data[n..n+4].try_into().unwrap());
            if u16_at(0)!=1 || u16_at(2)!=1 || u32_at(4)!=16000 || u32_at(8)!=32000 || u16_at(12)!=2 || u16_at(14)!=16{return Err(bad());}
            fmt=true;
        }else if id==b"data" {
            if !fmt || frames.is_some() || len==0 || len%2!=0{return Err(bad());}
            frames=Some(len/2);
        }
        offset=end.checked_add(len%2).filter(|end|*end<=size).ok_or_else(bad)?;
        reader.seek(SeekFrom::Start(offset)).map_err(|_|bad())?;
    }
    let frames=frames.ok_or_else(bad)?;
    if expected.is_some_and(|n|n==0 || n>9_007_199_254_740_991 || n!=frames){return Err("ASR_SAMPLE_FRAMES_MISMATCH".into());}
    Ok(frames)
}

pub async fn transcribe_openai(client: &reqwest::Client, base_url: &str, key: &str, input: &TranscribeInput, before_send: &(dyn Fn() -> Result<(), String> + Send + Sync)) -> Result<Value, String> {
    if key.trim().is_empty() { return Err("ASR_CREDENTIAL_REQUIRED".into()); }
    let endpoint = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let url = reqwest::Url::parse(&endpoint).map_err(|_| "ASR_ENDPOINT_INVALID")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() { return Err("ASR_ENDPOINT_INVALID: HTTPS without credentials/query/fragment required".into()); }
    if fs::metadata(&input.audio_file).map_err(|e|format!("ASR_AUDIO_INVALID: {e}"))?.len()>25_000_000 { return Err("ASR_UPLOAD_TOO_LARGE: whisper-1 allows 25MB".into()); }
    let wav = validate_wav(&input.audio_file, Some(input.sample_frames))?;
    if wav.bytes.len() > 25_000_000 { return Err("ASR_UPLOAD_TOO_LARGE: whisper-1 allows 25MB".into()); }
    let file = reqwest::multipart::Part::bytes(wav.bytes).file_name("evidence.wav").mime_str("audio/wav").map_err(|e| e.to_string())?;
    let mut form = reqwest::multipart::Form::new().part("file",file).text("model","whisper-1").text("language",input.language.clone()).text("response_format","verbose_json").text("timestamp_granularities[]","segment");
    if input.timestamps == "word" { form = form.text("timestamp_granularities[]","word"); }
    // One upload only: transport uncertainty must not cause a second paid submission.
    let request = client.post(url).bearer_auth(key).multipart(form).timeout(std::time::Duration::from_secs(600));
    before_send()?;
    let response = request.send().await.map_err(|_| "ASR_SUBMISSION_UNCERTAIN: transcription upload transport failed")?;
    let status = response.status();
    if !status.is_success() { return Err(format!("ASR_HTTP_ERROR: OpenAI returned {status}")); }
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "ASR_SUBMISSION_UNCERTAIN: transcription reply interrupted")? {
        if bytes.len()+chunk.len() > 32 * 1024 * 1024 { return Err("ASR_REPLY_TOO_LARGE".into()); }
        bytes.extend_from_slice(&chunk);
    }
    let reply = serde_json::from_slice(&bytes).map_err(|_| "ASR_REPLY_INVALID")?;
    decode_openai(reply, input)
}
fn normalized_word(value: &Value) -> Result<Value,String> {
    let text = value.get("word").or_else(||value.get("text")).and_then(Value::as_str).ok_or("ASR_REPLY_INVALID: word text missing")?;
    let mut word = json!({"text":text});
    for key in ["start","end","score"] {
        if let Some(value) = value.get(key).filter(|v| !v.is_null()) {
            let n = value.as_f64().filter(|n|n.is_finite() && *n >= 0.0).ok_or("ASR_REPLY_INVALID: word timing/score")?;
            word[key] = json!(n);
        }
    }
    if word.get("start").and_then(Value::as_f64).zip(word.get("end").and_then(Value::as_f64)).is_some_and(|(s,e)|e<s) {return Err("ASR_REPLY_INVALID: reversed word".into());}
    Ok(word)
}
fn decode_openai(reply: Value, input:&TranscribeInput) -> Result<Value,String> {
    let raw_segments = reply.get("segments").and_then(Value::as_array).ok_or("ASR_REPLY_INVALID: segments missing")?;
    let mut segments = Vec::with_capacity(raw_segments.len());
    for raw in raw_segments {
        let text = raw.get("text").and_then(Value::as_str).ok_or("ASR_REPLY_INVALID: segment text missing")?;
        let start = raw.get("start").and_then(Value::as_f64).filter(|n|n.is_finite() && *n >= 0.0).ok_or("ASR_REPLY_INVALID: segment start")?;
        let end = raw.get("end").and_then(Value::as_f64).filter(|n|n.is_finite() && *n >= start).ok_or("ASR_REPLY_INVALID: segment end")?;
        let words = raw.get("words").and_then(Value::as_array).map(|a|a.iter().map(normalized_word).collect::<Result<Vec<_>,_>>()).transpose()?.unwrap_or_default();
        segments.push(json!({"text":text,"start":start,"end":end,"words":words}));
    }
    if let Some(words) = reply.get("words").and_then(Value::as_array) {
        if raw_segments.iter().any(|s| s.get("words").is_some()) { return Err("ASR_REPLY_INVALID: ambiguous duplicate word evidence".into()); }
        let mut previous = 0usize;
        for raw in words {
            let word = normalized_word(raw)?;
            let index = word.get("start").and_then(Value::as_f64).and_then(|start| segments.iter().position(|s|start >= s["start"].as_f64().unwrap() && start < s["end"].as_f64().unwrap())).unwrap_or(previous);
            if segments.is_empty() {return Err("ASR_REPLY_INVALID: words without segments".into());}
            segments[index]["words"].as_array_mut().unwrap().push(word);
            previous = index;
        }
    }
    if input.timestamps=="segment" {for segment in &mut segments {segment.as_object_mut().unwrap().remove("words");}}
    Ok(json!({"schema":"dsivio.media.transcript/1","language":input.language,"sampleRate":16000,"sampleFrames":input.sample_frames,"engine":{"backend":"cloud","model":"whisper-1","protocol":"openai.audio.transcriptions/1"},"segments":segments}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec(); bytes.extend(40u32.to_le_bytes());bytes.extend(b"WAVEfmt ");bytes.extend(16u32.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(1u16.to_le_bytes());bytes.extend(16000u32.to_le_bytes());bytes.extend(32000u32.to_le_bytes());bytes.extend(2u16.to_le_bytes());bytes.extend(16u16.to_le_bytes());bytes.extend(b"data");bytes.extend(4u32.to_le_bytes());bytes.extend([0;4]);bytes
    }
    #[test] fn checks_container_and_exact_samples() {
        let bytes=wav(); assert_eq!(validate_wav_bytes(&bytes,Some(2)).unwrap(),2);
        assert!(validate_wav_bytes(&bytes,Some(3)).unwrap_err().contains("MISMATCH"));
        let mut truncated=bytes.clone();truncated.pop();assert!(validate_wav_bytes(&truncated,None).is_err());
        let mut stereo=bytes;stereo[22]=2;assert!(validate_wav_bytes(&stereo,None).is_err());
    }
    #[test] fn preserves_unaligned_words_without_interpolation() {
        let input=TranscribeInput{audio_file:PathBuf::new(),language:"en".into(),sample_frames:32000,timestamps:"word".into()};
        let result=decode_openai(json!({"segments":[{"text":"hello world","start":0.0,"end":2.0}],"words":[{"word":"hello","start":0.0,"end":1.0},{"word":"world"}]}),&input).unwrap();
        assert_eq!(result["segments"][0]["words"][1],json!({"text":"world"}));
        assert!(validate_language("auto").is_err()); assert!(validate_language("ZH").is_err());
    }
}
