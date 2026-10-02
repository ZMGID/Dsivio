//! System speech stays inside the App media task lifecycle; no network or voice downloads.
use super::{artifacts, model_parameters::{self, argument, DataType, ModelDescription}, speech_providers::SpeechInput, MediaKind, MediaOutput};
use crate::settings::ModelProvider;
use serde_json::json;
use std::{collections::BTreeMap, path::Path, process::{Command, Stdio}, time::{Duration, Instant}};
pub const MODEL: &str = "system-tts";

pub fn available() -> bool { !voices().is_empty() }
pub fn voices() -> Vec<String> {
    let mut command: Command;
    #[cfg(target_os="macos")] { command = Command::new("/usr/bin/say"); command.args(["-v", "?"]); }
    #[cfg(target_os="windows")] {
        command = windows_command();
        command.args(["-Command", "$ErrorActionPreference = 'Stop'; [Console]::OutputEncoding = [Text.UTF8Encoding]::new(); Add-Type -AssemblyName System.Speech; $s = [System.Speech.Synthesis.SpeechSynthesizer]::new(); try { $s.GetInstalledVoices() | Where-Object Enabled | ForEach-Object { $_.VoiceInfo.Name } } finally { $s.Dispose() }"]);
    }
    #[cfg(not(any(target_os="macos",target_os="windows")))] { return vec![]; }
    #[cfg(any(target_os="macos",target_os="windows"))] {
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        let Ok(mut child) = command.spawn() else { return vec![]; };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() { return vec![]; }
                    let Ok(output) = child.wait_with_output() else { return vec![]; };
                    return parse_voices(&String::from_utf8_lossy(&output.stdout));
                },
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => { let _ = child.kill(); let _ = child.wait(); return vec![]; }
            }
        }
    }
}
fn parse_voices(text: &str) -> Vec<String> {
    text.lines().filter_map(|line| {
        #[cfg(target_os="macos")] let name = line.split(" #").next().unwrap_or(line).split_whitespace().collect::<Vec<_>>();
        #[cfg(target_os="macos")] let name = name.get(..name.len().saturating_sub(1)).unwrap_or(&[]).join(" ");
        #[cfg(not(target_os="macos"))] let name = line.trim().to_string();
        (!name.is_empty()).then_some(name)
    }).collect()
}
#[cfg(target_os="windows")]
fn windows_command() -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("powershell.exe");
    command.creation_flags(0x08000000).args(["-NoProfile", "-NonInteractive"]);
    command
}
pub fn description(provider: &ModelProvider) -> ModelDescription {
    let mut args = BTreeMap::new();
    let mut text = argument(DataType::String, Some("--text-file"), json!({"minLength":1,"maxLength":10000,"lengthUnit":"unicodeCodePoint"}));
    text.required = true; text.transport["encoding"] = json!("utf8-file"); args.insert("text".into(),text);
    args.insert("voice".into(),argument(DataType::String,Some("--voice"),json!({"allowed":voices(),"defaultMeaning":"systemDefault"})));
    args.insert("mode".into(),argument(DataType::String,Some("--mode"),json!({"allowed":["tts"],"defaultValue":"tts"})));
    args.insert("speed".into(),argument(DataType::Number,None,json!({"minimum":0.5,"maximum":2.0,"defaultValue":1})));
    args.insert("outputFormat".into(),argument(DataType::String,Some("--output-format"),json!({"allowed":["wav"],"defaultValue":"wav"})));
    let mut result = model_parameters::finish(provider,MODEL,MediaKind::Speech,args,vec![],true,Some(1),false);
    result.products["mimeTypes"] = json!(["audio/wav"]);
    result.billing_info = Some(json!({"kind":"local","cost":0}));
    result.lifecycle["backend"] = json!("systemLocal");
    model_parameters::rehash(&mut result); result
}
pub async fn execute(input: &SpeechInput, dir: &Path) -> Result<Vec<MediaOutput>,String> {
    let text = dir.join("system-tts.txt");
    super::voices::atomic_bytes(&text,input.text.as_bytes())?;
    let output = dir.join("system-tts.wav");
    let mut command: tokio::process::Command;
    #[cfg(target_os="macos")] {
        command = tokio::process::Command::new("/usr/bin/say");
        command.arg("-f").arg(&text).arg("-o").arg(&output).args(["--file-format=WAVE","--data-format=LEI16@22050"]);
        if let Some(voice) = &input.voice {command.arg("-v").arg(voice);}
        command.arg("-r").arg((175.0 * input.speed).round().to_string());
    }
    #[cfg(target_os="windows")] {
        command = tokio::process::Command::from(windows_command());
        command.env("DSIVIO_TTS_TEXT",&text).env("DSIVIO_TTS_OUTPUT",&output).env("DSIVIO_TTS_VOICE",input.voice.as_deref().unwrap_or("")).env("DSIVIO_TTS_RATE",((input.speed.log2()*10.0).round() as i32).clamp(-10,10).to_string());
        command.args(["-Command", "$ErrorActionPreference = 'Stop'; Add-Type -AssemblyName System.Speech; $s = [System.Speech.Synthesis.SpeechSynthesizer]::new(); try { if ($env:DSIVIO_TTS_VOICE) { $s.SelectVoice($env:DSIVIO_TTS_VOICE) }; $s.Rate = [int]$env:DSIVIO_TTS_RATE; $s.SetOutputToWaveFile($env:DSIVIO_TTS_OUTPUT); $s.Speak([IO.File]::ReadAllText($env:DSIVIO_TTS_TEXT,[Text.Encoding]::UTF8)) } finally { $s.Dispose() }"]);
    }
    #[cfg(not(any(target_os="macos",target_os="windows")))] { return Err("System TTS supports macOS and Windows".into()); }
    #[cfg(any(target_os="macos",target_os="windows"))] {
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
        let result = tokio::time::timeout(Duration::from_secs(120),command.output()).await.map_err(|_|"System TTS timed out")?.map_err(|e|e.to_string())?;
        if !result.status.success() { return Err(format!("System TTS failed: {}",String::from_utf8_lossy(&result.stderr))); }
        let bytes = std::fs::read(&output).map_err(|e|e.to_string())?;
        Ok(vec![artifacts::save_audio(&bytes,&dir.join("audio.wav"),"wav").await?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_parameters_refuse_cloud_features_and_invalid_voice() {
        let provider = super::super::local_provider();
        let root = tempfile::tempdir().unwrap();
        for options in [json!({"text":"hello","mode":"clone"}),json!({"text":"hello","outputFormat":"mp3"}),json!({"text":"hello","voice":"not-an-installed-voice"}),json!({"text":"   "})] {
            let args = serde_json::from_value(options).unwrap();
            assert!(super::super::speech_providers::validate_input(&provider,MODEL,&args,root.path()).is_err());
        }
    }
    #[tokio::test]
    #[ignore = "requires installed system voices and the bundled ffprobe"]
    async fn system_synthesis_produces_decodable_wav() {
        let root = tempfile::tempdir().unwrap();
        let args = serde_json::from_value(json!({"text":"你好，这是本地语音测试。 Hello from Dsivio.","speed":1})).unwrap();
        let input = super::super::speech_providers::validate_input(&super::super::local_provider(),MODEL,&args,root.path()).unwrap();
        let outputs = execute(&input,root.path()).await.unwrap();
        assert_eq!(outputs.len(),1);
        assert!(root.path().join("audio.wav").metadata().unwrap().len()>44);
    }
}
