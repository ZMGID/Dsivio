//! Local ffmpeg subtitle and edit tasks. No vendor call and no model-pool entry.
//!
//! The binary comes only from the bundled video runtime (`media_runtime::runtime::tools_at`).
//! A missing file is an error. Tests may use `~/.local/bin/ffmpeg` when the bundle has not been
//! built; production never consults PATH.
use super::{
    model_parameters::{self, argument, DataType, ModelDescription},
    transcribe_providers, MediaKind, MediaOutput, MediaRequest, MediaStatus, MediaSubmissionState,
    MediaTask,
};
use crate::settings::ModelProvider;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub(crate) const MODEL_SUBTITLE: &str = "ffmpeg-subtitle";
pub(crate) const MODEL_EDIT: &str = "ffmpeg-edit";
pub(crate) const PROTOCOL_SUBTITLE: &str = "ffmpeg_subtitle";
pub(crate) const PROTOCOL_EDIT: &str = "ffmpeg_edit";

pub(crate) fn is_local_edit_protocol(protocol: &str) -> bool {
    matches!(protocol, PROTOCOL_SUBTITLE | PROTOCOL_EDIT)
}

pub(crate) fn resolve_ffmpeg() -> Result<PathBuf, String> {
    resolve_tool("ffmpeg")
}

fn resolve_ffprobe() -> Result<PathBuf, String> {
    resolve_tool("ffprobe")
}

fn resolve_tool(name: &str) -> Result<PathBuf, String> {
    let root = crate::media_runtime::runtime::root()?;
    let mut tools = crate::media_runtime::runtime::tools_at(&root)?;
    tools.remove(name).ok_or_else(|| {
        let relative = if name == "ffprobe" {
            "bin/ffprobe"
        } else {
            "analyzer/node_modules/ffmpeg-static/ffmpeg"
        };
        format!(
            "未找到内置 {name}（{}）。请先构建视频运行时：src-tauri/resources/video-runtime（scripts/build-video-runtime.mjs）",
            root.join(relative).display()
        )
    })
}

#[derive(Debug, Clone)]
struct SubtitleJob {
    video: PathBuf,
    language: String,
    burn: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ValidatedEdit {
    clips: Vec<ValidatedClip>,
    aspect: Option<String>,
    fit: String,
    resolution: Option<String>,
    music: Option<ValidatedMusic>,
    subtitles: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ValidatedClip {
    source: PathBuf,
    start: Option<f64>,
    end: Option<f64>,
}

#[derive(Debug, Clone)]
struct ValidatedMusic {
    path: PathBuf,
    volume: f64,
    original_volume: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditPlan {
    clips: Vec<EditClip>,
    #[serde(default)]
    aspect: Option<String>,
    #[serde(default)]
    fit: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
    #[serde(default)]
    music: Option<EditMusic>,
    #[serde(default)]
    subtitles: Option<EditSubtitles>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditClip {
    source: String,
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    end: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditMusic {
    path: String,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    original_volume: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditSubtitles {
    path: String,
}

enum LocalJob {
    Subtitle(SubtitleJob),
    Edit(ValidatedEdit),
}

pub(crate) async fn start(app: &AppHandle, request: MediaRequest) -> Result<MediaTask, String> {
    if request.provider_id != "local" {
        return Err("本地处理只使用 local".into());
    }
    if !request.images.is_empty() {
        return Err("本地处理不接受参考图".into());
    }
    let job = match request.model.as_str() {
        MODEL_SUBTITLE => LocalJob::Subtitle(validate_subtitle(&request.options)?),
        MODEL_EDIT => LocalJob::Edit(validate_edit(&request.options)?),
        _ => return Err("未知的本地处理模型".into()),
    };
    if let Some(revision) = request.description_revision.as_deref() {
        let description = model_parameters::describe(&super::local_provider(), &request.model, &MediaKind::Edit);
        if revision != description.facts_revision {
            return Err(revision_mismatch(revision, &description.facts_revision));
        }
    }
    let protocol = match request.model.as_str() {
        MODEL_SUBTITLE => PROTOCOL_SUBTITLE,
        _ => PROTOCOL_EDIT,
    };
    let root = super::root()?;
    let id = uuid::Uuid::new_v4().to_string();
    let saved = super::StoredTask {
        task: MediaTask {
            id: id.clone(),
            provider_id: "local".into(),
            model: request.model.clone(),
            kind: MediaKind::Edit,
            status: MediaStatus::Running,
            created_at: chrono::Utc::now().to_rfc3339(),
            error: None,
            remote_id: None,
            outputs: vec![],
            can_resume: false,
            submission_state: None,
            origin: request.origin.clone(),
            prompt: request.prompt.clone(),
            result: None,
            request_hash: Some(super::request_hash(&request)?),
            cancellation: None,
        },
        base_url: String::new(),
        protocol: protocol.into(),
        download_url: None,
        download_requires_auth: false,
        accepted_at: None,
    };
    let guard = super::claim(&id).ok_or("任务已运行")?;
    super::save(&root, &saved)?;
    super::register_control(&id);
    let task = saved.task.clone();
    spawn_local(app.clone(), root, saved, job, guard);
    Ok(task)
}

fn revision_mismatch(actual: &str, expected: &str) -> String {
    serde_json::to_string(&json!({
        "code": "MODEL_DESCRIPTION_CHANGED",
        "argumentPath": "descriptionRevision",
        "ruleId": "revision",
        "actual": actual,
        "expected": expected,
        "message": "MODEL_DESCRIPTION_CHANGED: descriptionRevision violates revision",
    }))
    .unwrap_or_else(|_| "模型描述已变更".into())
}

fn spawn_local(
    app: AppHandle,
    root: PathBuf,
    mut saved: super::StoredTask,
    job: LocalJob,
    guard: super::Active,
) {
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let id = saved.task.id.clone();
        let control = super::CONTROLS
            .lock()
            .get(&id)
            .map(|control| (control.cancel.clone(), control.finished.clone()));
        let Some((cancel, finished)) = control else { return };
        let subtitle = saved.protocol == PROTOCOL_SUBTITLE;
        let result: Result<(), String> = async {
            if !super::begin_execution(&root, &id, super::ControlPhase::Local)? {
                return Ok(());
            }
            let work = async {
                let (outputs, result) = match job {
                    LocalJob::Subtitle(job) => run_subtitle(&app, &root, &id, &job).await?,
                    LocalJob::Edit(plan) => run_edit(&plan, &root, &id).await?,
                };
                saved.task.outputs = outputs;
                saved.task.result = Some(result);
                saved.task.status = MediaStatus::Succeeded;
                super::save(&root, &saved)
            };
            tokio::pin!(work);
            tokio::select! {
                biased;
                _ = cancel.notified() => {
                    if subtitle {
                        let _ = super::local_asr::cancel(&id).await;
                    }
                    Ok(())
                }
                result = &mut work => result,
            }
        }
        .await;
        if let Err(error) = result {
            if let Ok(mut current) = super::read(&root, &id) {
                if current.task.status == MediaStatus::Running {
                    current.task.status = MediaStatus::Failed;
                    current.task.can_resume = false;
                    current.task.submission_state = Some(MediaSubmissionState::Rejected);
                    current.task.error = Some(error);
                    let _ = super::save(&root, &current);
                }
            }
        }
        {
            let _lock = super::TASK_LOCK.lock();
            if let Ok(mut current) = super::read(&root, &id) {
                if current.task.status == MediaStatus::Running
                    && current
                        .task
                        .cancellation
                        .as_ref()
                        .is_some_and(|c| c.outcome == super::CancelOutcome::Requested)
                {
                    current.task.status = MediaStatus::Cancelled;
                    current.task.can_resume = false;
                    if let Some(cancellation) = &mut current.task.cancellation {
                        cancellation.outcome = super::CancelOutcome::Confirmed;
                        cancellation.confirmed_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                    let _ = super::save_locked(&root, &current);
                }
            }
        }
        finished.notify_waiters();
        super::CONTROLS.lock().remove(&id);
    });
}

fn validate_subtitle(options: &BTreeMap<String, Value>) -> Result<SubtitleJob, String> {
    for key in options.keys() {
        if !matches!(key.as_str(), "video" | "language" | "burn") {
            return Err(format!("不支持的参数 {key}"));
        }
    }
    let video = options
        .get("video")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("需要视频绝对路径 video")?;
    let video = PathBuf::from(video);
    if !video.is_absolute() {
        return Err("video 必须是绝对路径".into());
    }
    if !video.is_file() {
        return Err(format!("找不到视频：{}", video.display()));
    }
    let language = options
        .get("language")
        .and_then(Value::as_str)
        .ok_or("需要 language")?
        .to_owned();
    transcribe_providers::validate_language(&language)?;
    let burn = match options.get("burn") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err("burn 必须是布尔值".into()),
    };
    Ok(SubtitleJob { video, language, burn })
}

fn validate_edit(options: &BTreeMap<String, Value>) -> Result<ValidatedEdit, String> {
    for key in options.keys() {
        if key != "plan" {
            return Err(format!("不支持的参数 {key}"));
        }
    }
    let plan = options.get("plan").cloned().ok_or("需要 plan")?;
    let plan: EditPlan = serde_json::from_value(plan).map_err(|error| format!("plan 无效：{error}"))?;
    validate_plan(plan)
}

fn validate_plan(plan: EditPlan) -> Result<ValidatedEdit, String> {
    if plan.clips.is_empty() {
        return Err("plan.clips 至少需要一段素材".into());
    }
    let mut clips = Vec::with_capacity(plan.clips.len());
    for clip in plan.clips {
        if let Some(start) = clip.start {
            if !start.is_finite() || start < 0.0 {
                return Err("clip.start 必须是大于等于 0 的秒数".into());
            }
        }
        if let Some(end) = clip.end {
            if !end.is_finite() || end <= 0.0 {
                return Err("clip.end 必须是大于 0 的秒数".into());
            }
        }
        if let (Some(start), Some(end)) = (clip.start, clip.end) {
            if end <= start {
                return Err("clip.end 必须大于 start".into());
            }
        }
        let source = require_file(&clip.source, "素材")?;
        clips.push(ValidatedClip { source, start: clip.start, end: clip.end });
    }
    if let Some(aspect) = &plan.aspect {
        if !matches!(aspect.as_str(), "9:16" | "16:9" | "1:1" | "3:4" | "4:3") {
            return Err("aspect 只能是 9:16、16:9、1:1、3:4、4:3".into());
        }
    }
    let fit = plan.fit.unwrap_or_else(|| "pad".into());
    if !matches!(fit.as_str(), "pad" | "crop") {
        return Err("fit 只能是 pad 或 crop".into());
    }
    if let Some(resolution) = &plan.resolution {
        if !matches!(resolution.as_str(), "720p" | "1080p") {
            return Err("resolution 只能是 720p 或 1080p".into());
        }
    }
    let music = match plan.music {
        None => None,
        Some(music) => {
            let volume = music.volume.unwrap_or(1.0);
            let original = music.original_volume.unwrap_or(1.0);
            if !volume.is_finite() || !(0.0..=2.0).contains(&volume) || !original.is_finite() || !(0.0..=2.0).contains(&original) {
                return Err("音量必须在 0 到 2 之间".into());
            }
            Some(ValidatedMusic {
                path: require_file(&music.path, "音乐")?,
                volume,
                original_volume: original,
            })
        }
    };
    let subtitles = match plan.subtitles {
        None => None,
        Some(subtitles) => {
            let path = require_file(&subtitles.path, "字幕")?;
            if path.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case("srt")) {
                return Err("字幕必须是 .srt 文件".into());
            }
            Some(path)
        }
    };
    Ok(ValidatedEdit {
        clips,
        aspect: plan.aspect,
        fit,
        resolution: plan.resolution,
        music,
        subtitles,
    })
}

pub(crate) fn validate_edit_plan(plan: &Value) -> Result<(), String> {
    let mut options = BTreeMap::new();
    options.insert("plan".into(), plan.clone());
    validate_edit(&options).map(|_| ())
}

fn require_file(value: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{label}路径必须是绝对路径"));
    }
    if !path.is_file() {
        return Err(format!("找不到{label}：{}", path.display()));
    }
    Ok(path)
}

struct OwnedCue {
    start: f64,
    end: f64,
    text: String,
}

fn format_srt(cues: &[OwnedCue]) -> String {
    let mut out = String::new();
    let mut index = 1u32;
    for cue in cues {
        let text = cue.text.trim();
        if text.is_empty() || !cue.start.is_finite() || !cue.end.is_finite() || cue.end < cue.start {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!(
            "{index}\n{} --> {}\n{text}\n",
            srt_time(cue.start),
            srt_time(cue.end)
        ));
        index += 1;
    }
    out
}

fn srt_time(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let m = (total_s / 60) % 60;
    let h = total_s / 3600;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

fn cues_from_transcript(value: &Value) -> Vec<OwnedCue> {
    value["segments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|segment| {
            let text = segment["text"].as_str()?.to_owned();
            let start = segment["start"].as_f64()?;
            let end = segment["end"].as_f64()?;
            Some(OwnedCue { start, end, text })
        })
        .collect()
}

async fn run_subtitle(
    app: &AppHandle,
    root: &Path,
    id: &str,
    job: &SubtitleJob,
) -> Result<(Vec<MediaOutput>, Value), String> {
    let ffmpeg = resolve_ffmpeg()?;
    let dir = super::directory(root, id)?;
    let evidence = dir.join("evidence");
    std::fs::create_dir_all(&evidence).map_err(|error| error.to_string())?;
    let wav = evidence.join("audio.wav");
    run_command(
        &ffmpeg,
        &[
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            job.video.display().to_string(),
            "-vn".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "16000".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            wav.display().to_string(),
        ],
    )
    .await?;
    let transcript = transcribe_wav(app, id, &wav, &job.language).await?;
    let cues = cues_from_transcript(&transcript);
    let srt_path = dir.join("subtitles.srt");
    let cues: Vec<_> = cues
        .into_iter()
        .filter(|cue| !cue.text.trim().is_empty() && cue.end >= cue.start)
        .collect();
    crate::app_cli::write_private(&srt_path, format_srt(&cues).as_bytes())?;
    let mut outputs = vec![MediaOutput {
        path: srt_path.display().to_string(),
        mime: "application/x-subrip".into(),
    }];
    if job.burn {
        let burned = dir.join("subtitled.mp4");
        burn_subtitles(&ffmpeg, &job.video, &srt_path, &burned).await?;
        outputs.push(MediaOutput {
            path: burned.display().to_string(),
            mime: "video/mp4".into(),
        });
    }
    let segments: Vec<Value> = cues
        .iter()
        .map(|cue| json!({"start": cue.start, "end": cue.end, "text": cue.text.trim()}))
        .collect();
    Ok((outputs, json!({"segments": segments, "language": job.language})))
}

async fn transcribe_wav(app: &AppHandle, task_id: &str, wav: &Path, language: &str) -> Result<Value, String> {
    let frames = transcribe_providers::validate_wav_frames(wav, None)?;
    let state = app.state::<crate::state::AppState>();
    let (provider_id, model, config) = {
        let settings = state.settings_read();
        let config = settings.workbench_media.local_asr.clone();
        match super::cli::members(&settings, &MediaKind::Transcribe).into_iter().next() {
            Some(entry) if entry.provider_id != "local" => (Some(entry.provider_id), entry.model, config),
            Some(entry) => (None, entry.model, config),
            None => (None, "whisperx-small".into(), config),
        }
    };
    let provider = match provider_id {
        Some(provider_id) => super::provider(&state, &provider_id)?,
        None => super::local_provider(),
    };
    if provider.id == "local" {
        return super::local_asr::transcribe(task_id, wav, language, frames, "segment", config).await;
    }
    let info = provider.model_overrides.get(&model).ok_or("缺少转写配置")?;
    let base = info.transcribe_base_url.as_deref().ok_or("缺少转写产品地址")?;
    let key = provider.preferred_api_key().filter(|key| !key.trim().is_empty()).ok_or("缺少转写 API key")?;
    let input = transcribe_providers::TranscribeInput {
        audio_file: wav.to_path_buf(),
        language: language.to_owned(),
        sample_frames: frames,
        timestamps: "segment".into(),
    };
    let root = super::root()?;
    let id = task_id.to_owned();
    let before_send = move || {
        if super::begin_execution(&root, &id, super::ControlPhase::Local)? {
            Ok(())
        } else {
            Err("请求发送前已取消".into())
        }
    };
    transcribe_providers::transcribe_openai(
        super::artifacts::download_client(&provider),
        base,
        key,
        &input,
        &before_send,
    )
    .await
}

async fn run_edit(plan: &ValidatedEdit, root: &Path, id: &str) -> Result<(Vec<MediaOutput>, Value), String> {
    let ffmpeg = resolve_ffmpeg()?;
    let ffprobe = resolve_ffprobe()?;
    let output = super::directory(root, id)?.join("edited.mp4");
    run_edit_plan(&ffmpeg, &ffprobe, plan, &output).await?;
    Ok((
        vec![MediaOutput { path: output.display().to_string(), mime: "video/mp4".into() }],
        json!({"clips": plan.clips.len(), "fit": plan.fit}),
    ))
}

struct RenderFacts {
    width: u32,
    height: u32,
    clips: Vec<ClipMedia>,
}

struct ClipMedia {
    has_audio: bool,
    duration: f64,
}

async fn run_edit_plan(ffmpeg: &Path, ffprobe: &Path, plan: &ValidatedEdit, output: &Path) -> Result<(), String> {
    let facts = probe_facts(ffprobe, plan).await?;
    let args = edit_ffmpeg_args(plan, &facts, output);
    run_command(ffmpeg, &args).await
}

async fn probe_facts(ffprobe: &Path, plan: &ValidatedEdit) -> Result<RenderFacts, String> {
    let mut clips = Vec::with_capacity(plan.clips.len());
    for clip in &plan.clips {
        let has_audio = probe_has_audio(ffprobe, &clip.source).await?;
        let full = probe_duration(ffprobe, &clip.source).await?;
        let start = clip.start.unwrap_or(0.0);
        let duration = match clip.end {
            Some(end) => end - start,
            None => (full - start).max(0.001),
        };
        if duration <= 0.0 {
            return Err(format!("素材时长无效：{}", clip.source.display()));
        }
        clips.push(ClipMedia { has_audio, duration });
    }
    let (source_w, source_h) = probe_size(ffprobe, &plan.clips[0].source).await?;
    let (width, height) = target_frame(plan, source_w, source_h);
    Ok(RenderFacts { width, height, clips })
}

fn target_frame(plan: &ValidatedEdit, source_w: u32, source_h: u32) -> (u32, u32) {
    match (plan.aspect.as_deref(), plan.resolution.as_deref()) {
        (Some(aspect), Some(resolution)) => frame_size(aspect, resolution),
        (Some(aspect), None) => frame_size(aspect, "720p"),
        (None, Some(resolution)) => {
            let short = if resolution == "1080p" { 1080 } else { 720 };
            scale_short_side(source_w, source_h, short)
        }
        (None, None) => (even(source_w), even(source_h)),
    }
}

fn frame_size(aspect: &str, resolution: &str) -> (u32, u32) {
    let short = if resolution == "1080p" { 1080 } else { 720 };
    let (w_ratio, h_ratio) = match aspect {
        "9:16" => (9u32, 16u32),
        "1:1" => (1, 1),
        "3:4" => (3, 4),
        "4:3" => (4, 3),
        _ => (16, 9),
    };
    if w_ratio >= h_ratio {
        (even(short * w_ratio / h_ratio), short)
    } else {
        (short, even(short * h_ratio / w_ratio))
    }
}

fn scale_short_side(width: u32, height: u32, short: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (short, short);
    }
    if width <= height {
        (short, even(short * height / width))
    } else {
        (even(short * width / height), short)
    }
}

fn even(value: u32) -> u32 {
    (value.max(2) / 2) * 2
}

fn edit_ffmpeg_args(plan: &ValidatedEdit, facts: &RenderFacts, output: &Path) -> Vec<String> {
    let mut args = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];
    for clip in &plan.clips {
        args.push("-i".into());
        args.push(clip.source.display().to_string());
    }
    if let Some(music) = &plan.music {
        args.push("-i".into());
        args.push(music.path.display().to_string());
    }
    let original = plan.music.as_ref().map(|music| music.original_volume).unwrap_or(1.0);
    let graph = edit_filter_graph(plan, facts, original);
    let video_label = if plan.subtitles.is_some() { "[vout]" } else { "[v]" };
    let audio_label = if plan.music.is_some() { "[aout]" } else { "[a]" };
    args.extend([
        "-filter_complex".into(),
        graph,
        "-map".into(),
        video_label.into(),
        "-map".into(),
        audio_label.into(),
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.display().to_string(),
    ]);
    args
}

fn edit_filter_graph(plan: &ValidatedEdit, facts: &RenderFacts, original_volume: f64) -> String {
    let mut parts = Vec::new();
    for (index, clip) in plan.clips.iter().enumerate() {
        let media = &facts.clips[index];
        parts.push(video_chain(index, clip.start, clip.end, facts.width, facts.height, &plan.fit));
        parts.push(audio_chain(index, clip.start, clip.end, original_volume, media));
    }
    let mut concat = String::new();
    for index in 0..plan.clips.len() {
        concat.push_str(&format!("[v{index}][a{index}]"));
    }
    concat.push_str(&format!("concat=n={}:v=1:a=1[v][a]", plan.clips.len()));
    parts.push(concat);
    if let Some(subtitles) = &plan.subtitles {
        let escaped = escape_filter_path(&subtitles.display().to_string());
        parts.push(format!("[v]subtitles=filename='{escaped}'[vout]"));
    }
    if let Some(music) = &plan.music {
        let index = plan.clips.len();
        parts.push(format!(
            "[{index}:a]volume={}[m];[a][m]amix=inputs=2:duration=first:dropout_transition=0[aout]",
            secs(music.volume)
        ));
    }
    parts.join(";")
}

fn video_chain(index: usize, start: Option<f64>, end: Option<f64>, width: u32, height: u32, fit: &str) -> String {
    let geometry = if fit == "crop" {
        format!("scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}")
    } else {
        format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:black")
    };
    format!(
        "[{index}:v]{}setpts=PTS-STARTPTS,{geometry},setsar=1[v{index}]",
        range_prefix("trim", start, end)
    )
}

fn audio_chain(index: usize, start: Option<f64>, end: Option<f64>, volume: f64, media: &ClipMedia) -> String {
    if media.has_audio {
        format!(
            "[{index}:a]{}asetpts=PTS-STARTPTS,volume={}[a{index}]",
            range_prefix("atrim", start, end),
            secs(volume)
        )
    } else {
        format!(
            "anullsrc=channel_layout=stereo:sample_rate=44100,atrim=end={},asetpts=PTS-STARTPTS,volume={}[a{index}]",
            secs(media.duration),
            secs(volume)
        )
    }
}

fn range_prefix(filter: &str, start: Option<f64>, end: Option<f64>) -> String {
    let mut options = Vec::new();
    if let Some(start) = start {
        options.push(format!("start={}", secs(start)));
    }
    if let Some(end) = end {
        options.push(format!("end={}", secs(end)));
    }
    if options.is_empty() {
        String::new()
    } else {
        format!("{filter}={},{}", options.join(":"), "")
    }
}

fn secs(value: f64) -> String {
    format!("{value:.3}")
}

fn escape_filter_path(path: &str) -> String {
    let mut out = String::new();
    for ch in path.chars() {
        if matches!(ch, '\\' | '\'' | ':' | ',' | ';' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

async fn burn_subtitles(ffmpeg: &Path, video: &Path, srt: &Path, output: &Path) -> Result<(), String> {
    ensure_subtitles_filter(ffmpeg).await?;
    let filter = format!("subtitles=filename='{}'", escape_filter_path(&srt.display().to_string()));
    run_command(
        ffmpeg,
        &[
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            video.display().to_string(),
            "-vf".into(),
            filter,
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-movflags".into(),
            "+faststart".into(),
            output.display().to_string(),
        ],
    )
    .await
}

async fn ensure_subtitles_filter(ffmpeg: &Path) -> Result<(), String> {
    let text = command_stdout(ffmpeg, &["-hide_banner".into(), "-filters".into()]).await?;
    if text.split_whitespace().any(|token| token == "subtitles") {
        return Ok(());
    }
    Err("内置 ffmpeg 没有 libass 的 subtitles 滤镜，无法烧录字幕".into())
}

async fn probe_duration(ffprobe: &Path, path: &Path) -> Result<f64, String> {
    let text = command_stdout(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-show_entries".into(),
            "format=duration".into(),
            "-of".into(),
            "default=nw=1:nk=1".into(),
            path.display().to_string(),
        ],
    )
    .await?;
    text.trim().parse::<f64>().map_err(|_| format!("无法读取时长：{}", path.display()))
}

async fn probe_has_audio(ffprobe: &Path, path: &Path) -> Result<bool, String> {
    let text = command_stdout(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "a".into(),
            "-show_entries".into(),
            "stream=index".into(),
            "-of".into(),
            "csv=p=0".into(),
            path.display().to_string(),
        ],
    )
    .await?;
    Ok(!text.trim().is_empty())
}

async fn probe_size(ffprobe: &Path, path: &Path) -> Result<(u32, u32), String> {
    let text = command_stdout(
        ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            "v:0".into(),
            "-show_entries".into(),
            "stream=width,height".into(),
            "-of".into(),
            "csv=s=x:p=0".into(),
            path.display().to_string(),
        ],
    )
    .await?;
    let (width, height) = text.trim().split_once('x').ok_or("无法读取画面尺寸")?;
    Ok((
        width.parse().map_err(|_| "画面宽度无效")?,
        height.parse().map_err(|_| "画面高度无效")?,
    ))
}

async fn command_stdout(program: &Path, args: &[String]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("无法启动 {}：{error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} 失败（{}）：{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn run_command(program: &Path, args: &[String]) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动 {}：{error}", program.display()))?;
    let mut stderr = child.stderr.take().ok_or("无法读取进程输出")?;
    let drain = async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let Ok(n) = stderr.read(&mut chunk).await else { break };
            if n == 0 {
                break;
            }
            if buf.len() < 8000 {
                let room = 8000 - buf.len();
                buf.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
        buf
    };
    let (status, err_buf) = tokio::join!(child.wait(), drain);
    let status = status.map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg 失败（{status}）：{}", String::from_utf8_lossy(&err_buf)))
    }
}

pub(crate) fn description(provider: &ModelProvider, model: &str) -> ModelDescription {
    let mut args = BTreeMap::new();
    if model == MODEL_SUBTITLE {
        let mut video = argument(DataType::String, None, json!({"minLength": 1, "resource": true, "locations": ["local"]}));
        video.required = true;
        args.insert("video".into(), video);
        let mut language = argument(DataType::String, Some("--language"), json!({"minLength": 2, "maxLength": 3, "lengthUnit": "unicodeCodePoint"}));
        language.required = true;
        args.insert("language".into(), language);
        args.insert("burn".into(), argument(DataType::Boolean, Some("--burn"), json!({"defaultValue": false})));
    } else if model == MODEL_EDIT {
        let mut plan = argument(DataType::String, Some("--plan-file"), json!({
            "encoding": "json",
            "schema": {
                "clips": [{"source": "absolute path", "start": "seconds?", "end": "seconds?"}],
                "aspect": ["9:16", "16:9", "1:1", "3:4", "4:3"],
                "fit": ["pad", "crop"],
                "resolution": ["720p", "1080p"],
                "music": {"path": "absolute path", "volume": "0..2", "originalVolume": "0..2"},
                "subtitles": {"path": "absolute .srt"}
            }
        }));
        plan.required = true;
        plan.transport["encoding"] = json!("json-file");
        args.insert("plan".into(), plan);
    }
    let mut description = model_parameters::finish(provider, model, MediaKind::Edit, args, vec![], model == MODEL_SUBTITLE || model == MODEL_EDIT, Some(if model == MODEL_SUBTITLE { 2 } else { 1 }), false);
    description.products["mimeTypes"] = if model == MODEL_SUBTITLE {
        json!(["application/x-subrip", "video/mp4"])
    } else {
        json!(["video/mp4"])
    };
    description.billing_info = Some(json!({"kind": "local", "cost": 0}));
    description.lifecycle["backend"] = json!("ffmpeg");
    description.lifecycle["localCancel"] = json!("kill-child");
    model_parameters::rehash(&mut description);
    description
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: f64, end: f64, text: &str) -> OwnedCue {
        OwnedCue { start, end, text: text.into() }
    }

    #[test]
    fn workbench_local_srt_from_segments() {
        let text = format_srt(&[
            cue(1.0, 2.5, "Hello"),
            cue(3.0, 3.4, " 世界 "),
            cue(4.0, 3.0, "skip reversed"),
            cue(5.0, 6.0, "   "),
        ]);
        assert_eq!(
            text,
            "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:03,400\n世界\n"
        );
    }

    #[test]
    fn workbench_local_edit_plan_and_ffmpeg_args() {
        let missing = validate_plan(EditPlan {
            clips: vec![EditClip { source: "/no/such/clip.mp4".into(), start: Some(1.0), end: Some(0.5) }],
            aspect: None,
            fit: None,
            resolution: None,
            music: None,
            subtitles: None,
        });
        assert!(missing.unwrap_err().contains("end"));
        let root = tempfile::tempdir().unwrap();
        let clip = root.path().join("clip.mp4");
        let music = root.path().join("music.m4a");
        let srt = root.path().join("cues.srt");
        std::fs::write(&clip, b"v").unwrap();
        std::fs::write(&music, b"a").unwrap();
        std::fs::write(&srt, b"s").unwrap();
        let bad_volume = validate_plan(EditPlan {
            clips: vec![EditClip { source: clip.display().to_string(), start: None, end: None }],
            aspect: Some("9:16".into()),
            fit: Some("pad".into()),
            resolution: Some("720p".into()),
            music: Some(EditMusic { path: music.display().to_string(), volume: Some(2.5), original_volume: None }),
            subtitles: None,
        });
        assert!(bad_volume.unwrap_err().contains("音量"));
        assert!(validate_plan(EditPlan {
            clips: vec![],
            aspect: None,
            fit: None,
            resolution: None,
            music: None,
            subtitles: None,
        })
        .unwrap_err()
        .contains("clips"));
        let plan = validate_plan(EditPlan {
            clips: vec![
                EditClip { source: clip.display().to_string(), start: Some(0.0), end: Some(1.0) },
                EditClip { source: clip.display().to_string(), start: Some(1.0), end: Some(2.0) },
            ],
            aspect: Some("9:16".into()),
            fit: Some("pad".into()),
            resolution: Some("720p".into()),
            music: Some(EditMusic {
                path: music.display().to_string(),
                volume: Some(0.5),
                original_volume: Some(1.0),
            }),
            subtitles: Some(EditSubtitles { path: srt.display().to_string() }),
        })
        .unwrap();
        assert_eq!(frame_size("9:16", "720p"), (720, 1280));
        assert_eq!(frame_size("16:9", "1080p"), (1920, 1080));
        let facts = RenderFacts {
            width: 720,
            height: 1280,
            clips: vec![ClipMedia { has_audio: true, duration: 1.0 }, ClipMedia { has_audio: true, duration: 1.0 }],
        };
        let output = root.path().join("edited.mp4");
        let args = edit_ffmpeg_args(&plan, &facts, &output);
        let graph = args.iter().find(|arg| arg.contains("concat=")).cloned().unwrap();
        assert!(graph.contains("trim=start=0.000:end=1.000"), "{graph}");
        assert!(graph.contains("trim=start=1.000:end=2.000"), "{graph}");
        assert!(graph.contains("concat=n=2:v=1:a=1"), "{graph}");
        assert!(graph.contains("scale=720:1280"), "{graph}");
        assert!(graph.contains("pad=720:1280"), "{graph}");
        assert!(graph.contains("amix=inputs=2"), "{graph}");
        assert!(graph.contains("volume=0.500"), "{graph}");
        assert!(graph.contains("subtitles=filename="), "{graph}");
        assert!(!graph.contains("drawtext"), "{graph}");
        assert!(args.iter().any(|arg| arg == music.to_str().unwrap()));
        assert_eq!(args.last().unwrap(), &output.display().to_string());
        assert_eq!(escape_filter_path("C:/a,b[c].srt"), "C\\:/a\\,b\\[c\\].srt");
    }

    /// Bundled tools win. If the video runtime has not been built, tests may use the
    /// developer ffmpeg at ~/.local/bin. Production `resolve_ffmpeg` never does this.
    fn test_tool(name: &str) -> PathBuf {
        if let Ok(path) = resolve_tool(name) {
            return path;
        }
        let fallback = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/bin").join(name);
        assert!(fallback.is_file(), "missing bundled {name}; tests also look at {fallback:?}");
        fallback
    }

    #[test]
    fn workbench_local_ffmpeg_comes_from_the_bundle_when_present() {
        match resolve_ffmpeg() {
            Ok(path) => {
                let text = path.display().to_string();
                assert!(text.contains("ffmpeg-static"), "{text}");
            }
            Err(error) => assert!(error.contains("ffmpeg"), "{error}"),
        }
    }

    #[tokio::test]
    async fn workbench_local_ffmpeg_edit_produces_playable_mp4() {
        let ffmpeg = test_tool("ffmpeg");
        let ffprobe = test_tool("ffprobe");
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mp4");
        let music = root.path().join("music.m4a");
        run_command(
            &ffmpeg,
            &[
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-f".into(), "lavfi".into(), "-i".into(), "testsrc=size=320x240:rate=25:duration=3".into(),
                "-f".into(), "lavfi".into(), "-i".into(), "sine=frequency=440:sample_rate=44100:duration=3".into(),
                "-shortest".into(), "-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
                "-c:a".into(), "aac".into(), source.display().to_string(),
            ],
        )
        .await
        .unwrap();
        run_command(
            &ffmpeg,
            &[
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-f".into(), "lavfi".into(), "-i".into(), "sine=frequency=880:sample_rate=44100:duration=4".into(),
                "-c:a".into(), "aac".into(), music.display().to_string(),
            ],
        )
        .await
        .unwrap();
        let plan = validate_plan(EditPlan {
            clips: vec![
                EditClip { source: source.display().to_string(), start: Some(0.0), end: Some(1.0) },
                EditClip { source: source.display().to_string(), start: Some(1.0), end: Some(2.0) },
            ],
            aspect: Some("9:16".into()),
            fit: Some("pad".into()),
            resolution: Some("720p".into()),
            music: Some(EditMusic { path: music.display().to_string(), volume: Some(0.4), original_volume: Some(1.0) }),
            subtitles: None,
        })
        .unwrap();
        let output = root.path().join("edited.mp4");
        run_edit_plan(&ffmpeg, &ffprobe, &plan, &output).await.unwrap();
        let duration = probe_duration(&ffprobe, &output).await.unwrap();
        assert!((1.8..2.3).contains(&duration), "duration {duration}");
        let (width, height) = probe_size(&ffprobe, &output).await.unwrap();
        assert_eq!((width, height), (720, 1280));
        assert!(probe_has_audio(&ffprobe, &output).await.unwrap());
    }

    #[tokio::test]
    async fn workbench_local_subtitle_burn_is_playable() {
        let ffmpeg = test_tool("ffmpeg");
        let ffprobe = test_tool("ffprobe");
        let filters = command_stdout(&ffmpeg, &["-hide_banner".into(), "-filters".into()]).await.unwrap();
        assert!(
            filters.split_whitespace().any(|token| token == "subtitles"),
            "bundled ffmpeg lacks libass subtitles; filters sample: {}",
            filters.lines().find(|line| line.contains("subtitle") || line.contains("drawtext")).unwrap_or("none")
        );
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mp4");
        let srt = root.path().join("cues.srt");
        let output = root.path().join("subtitled.mp4");
        run_command(
            &ffmpeg,
            &[
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
                "-f".into(), "lavfi".into(), "-i".into(), "testsrc=size=320x240:rate=25:duration=1".into(),
                "-f".into(), "lavfi".into(), "-i".into(), "sine=frequency=440:sample_rate=44100:duration=1".into(),
                "-shortest".into(), "-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
                "-c:a".into(), "aac".into(), source.display().to_string(),
            ],
        )
        .await
        .unwrap();
        std::fs::write(
            &srt,
            format_srt(&[cue(0.2, 0.8, "Hello burn")]),
        )
        .unwrap();
        burn_subtitles(&ffmpeg, &source, &srt, &output).await.unwrap();
        let duration = probe_duration(&ffprobe, &output).await.unwrap();
        assert!((0.8..1.4).contains(&duration), "duration {duration}");
    }
}
