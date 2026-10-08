//! `dsivio media ...`: a thin client of the running App (ADR 0009).
//!
//! The client never reads provider settings or keys and never talks to a vendor. It sends one
//! JSON line to the App over loopback and prints the App's `MediaTask`. The App side resolves the
//! model from the 媒体创作 pool and calls the same `start_media_generation` / `get_media_task` as
//! every other caller, so there is still exactly one generation implementation.
use super::{MediaKind, MediaRequest, MediaStatus, MediaSubmissionState, MediaTask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub const SUBCOMMAND: &str = "media";

/// Exit codes are the contract callers branch on; see `resources/plugins/_shared/dsivio.md`.
mod exit {
    pub const OK: u8 = 0;
    pub const INTERNAL: u8 = 1;
    /// Bad arguments or model not enabled. Nothing was submitted.
    pub const INVALID: u8 = 2;
    /// The service refused the request. Nothing was charged; retrying is allowed.
    pub const REJECTED: u8 = 3;
    /// Submitted, then failed.
    pub const FAILED: u8 = 4;
    /// Submission outcome unknown. Never resubmit; query with `status`.
    pub const UNCERTAIN: u8 = 5;
    /// Dsivio is not running, or this client cannot reach it.
    #[allow(dead_code)]
    pub const UNAVAILABLE: u8 = 6;
    /// The task was confirmed cancelled; never treat it as a successful artifact.
    pub const CANCELLED: u8 = 7;
    /// Still running when the wait ended; continue with `wait <id>`.
    pub const TIMEOUT: u8 = 124;
}

const POLL: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Wire format

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Op {
    Models {
        #[serde(default)]
        kind: Option<String>,
    },
    Submit(Submit),
    Status {
        id: String,
        #[serde(default)]
        resume: bool,
    },
    Lookup { source: String, idempotency_key: String },
    Cancel { id: String },
    Asrstatus,
    Asrinstall { model: Option<String>, languages: Vec<String> },
    Asrstop,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Submit {
    kind: Option<MediaKind>,
    #[serde(default)]
    model: Option<String>,
    prompt: String,
    #[serde(default)]
    images: Vec<String>,
    #[serde(default)]
    options: BTreeMap<String, Value>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    description_revision: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ErrorCode {
    Invalid,
    Unauthorized,
    Internal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Reply {
    Ok {
        ok: bool,
        result: Value,
    },
    Err {
        ok: bool,
        code: ErrorCode,
        error: String,
    },
}

impl Reply {
    fn ok(result: Value) -> Self {
        Reply::Ok { ok: true, result }
    }
    fn err(code: ErrorCode, error: impl Into<String>) -> Self {
        Reply::Err {
            ok: false,
            code,
            error: error.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// App side

/// One loopback listener, owned by `app_cli`. Media ops are handled by `handle`.
pub fn serve(app: tauri::AppHandle) {
    crate::app_cli::serve(app);
}

pub(crate) async fn handle(app: &tauri::AppHandle, op: Value) -> Result<Value, String> {
    let op: Op = serde_json::from_value(op).map_err(|error| format!("请求格式无效：{error}"))?;
    match dispatch(app, op).await {
        Reply::Ok { result, .. } => Ok(result),
        Reply::Err { error, .. } => Err(error),
    }
}

async fn dispatch(app: &tauri::AppHandle, op: Op) -> Reply {
    use tauri::Manager;
    match op {
        Op::Models { kind } => {
            let kind = match kind.as_deref() {
                None => None,
                Some("matting") => return Reply::ok(json!([])),
                Some("image") => Some(MediaKind::Image),
                Some("video") => Some(MediaKind::Video),
                Some("speech") => Some(MediaKind::Speech),
                Some("transcribe") => Some(MediaKind::Transcribe),
                Some("edit") => Some(MediaKind::Edit),
                Some(_) => return Reply::err(ErrorCode::Invalid, "未知媒体类型"),
            };
            let settings = app
                .state::<crate::state::AppState>()
                .settings_read()
                .clone();
            Reply::ok(json!(list_models(&settings, kind.as_ref())))
        }
        Op::Cancel { id } => {
            if uuid::Uuid::parse_str(&id).is_err() {
                return Reply::err(ErrorCode::Invalid, format!("任务 ID 格式无效：{id}"));
            }
            match super::cancel_media_task(app.clone(), id).await {
                Ok(result) => Reply::ok(json!(result)),
                Err(error) => Reply::err(ErrorCode::Invalid, error),
            }
        }
        Op::Asrstatus => match super::local_asr::status().await {
            Ok(result) => Reply::ok(asr_status_reply(json!(result), &configured_local_asr(app))),
            Err(error) => Reply::err(ErrorCode::Invalid, error),
        },
        Op::Asrinstall { model, languages } => {
            let config = match asr_install_config(&configured_local_asr(app), model, languages) {
                Ok(config) => config,
                Err(error) => return Reply::err(ErrorCode::Invalid, error),
            };
            match super::local_asr::install(config).await {
                Ok(result) => Reply::ok(json!(result)),
                Err(error) => Reply::err(ErrorCode::Invalid, error),
            }
        }
        Op::Asrstop => match super::local_asr::stop().await {
            Ok(result) => Reply::ok(json!(result)),
            Err(error) => Reply::err(ErrorCode::Invalid, error),
        },
        Op::Status { id, resume } => {
            if uuid::Uuid::parse_str(&id).is_err() {
                return Reply::err(ErrorCode::Invalid, format!("任务 ID 格式无效：{id}"));
            }
            match super::get_media_task(app.clone(), id.clone(), Some(resume)) {
                Ok(task) => Reply::ok(json!(task)),
                Err(error) => Reply::err(ErrorCode::Invalid, format!("找不到任务 {id}（{error}）")),
            }
        }
        Op::Lookup { source, idempotency_key } => {
            let origin = match origin_for(Some(&source), Some(&idempotency_key)) {
                Ok(origin) => origin,
                Err(error) => return Reply::err(ErrorCode::Invalid, error),
            };
            // Wait for an in-flight keyed submission to persist its record; never submit here.
            let _serial = IDEMPOTENT.lock().await;
            match super::list_media_tasks(app.clone(), super::MediaTaskFilter {
                origin: Some(origin), ..Default::default()
            }) {
                Ok(tasks) => match lookup_task(tasks) {
                    Some(task) => Reply::ok(json!(task)),
                    None => Reply::err(ErrorCode::Invalid, "MEDIA_TASK_NOT_FOUND: 尚无此来源和 key 的任务"),
                },
                Err(error) => Reply::err(ErrorCode::Invalid, error),
            }
        }
        Op::Submit(submit) => match submit_request(app, submit).await {
            Ok(task) => Reply::ok(json!(task)),
            Err(error) => Reply::err(ErrorCode::Invalid, error),
        },
    }
}

fn configured_local_asr(app: &tauri::AppHandle) -> crate::settings::LocalAsrConfig {
    use tauri::Manager;
    app.state::<crate::state::AppState>().settings_read().workbench_media.local_asr.clone()
}

fn same_languages(a: &[String], b: &[String]) -> bool {
    let set = |items: &[String]| items.iter().cloned().collect::<std::collections::BTreeSet<_>>();
    set(a) == set(b)
}

/// Transcription always checks the Settings configuration, so installing anything else would
/// download several GB that the first transcription then replaces. Flags may only restate it.
fn asr_install_config(
    configured: &crate::settings::LocalAsrConfig,
    model: Option<String>,
    languages: Vec<String>,
) -> Result<crate::settings::LocalAsrConfig, String> {
    let model_differs = model.as_deref().is_some_and(|model| model != configured.model);
    if model_differs || !languages.is_empty() && !same_languages(&languages, &configured.languages) {
        return Err(format!(
            "转写按「设置 > 媒体创作 > 转写」安装（模型 {}，语言 {}）；要改语言请先在设置里修改，再不带参数运行 asr install",
            configured.model,
            configured.languages.join("、")
        ));
    }
    Ok(configured.clone())
}

/// Measured on macOS: ~1.5 GB shared environment plus 0.4–1.3 GB per language; reruns resume.
const ASR_DOWNLOAD_ESTIMATE: &str = "约 2–5 GB，视语言而定，中断后重试会续传";

/// Adds what the App will actually do, so callers do not mistake "not installed" for "unavailable".
fn asr_status_reply(mut status: Value, configured: &crate::settings::LocalAsrConfig) -> Value {
    let state = status["state"].as_str().unwrap_or_default().to_owned();
    let installed: Vec<String> = serde_json::from_value(status["languages"].clone()).unwrap_or_default();
    let note = match state.as_str() {
        "installing" => None,
        "ready" if same_languages(&installed, &configured.languages) => None,
        "ready" if configured.auto_install => Some(format!(
            "设置中的语言已改为 {}，下次 transcribe 会按设置重新安装（下载{ASR_DOWNLOAD_ESTIMATE}）",
            configured.languages.join("、")
        )),
        "ready" => Some("设置中的语言已变更，需先运行 dsivio media asr install 重新安装".to_owned()),
        _ if configured.auto_install => Some(format!(
            "可直接使用：首次 dsivio media transcribe 会自动安装（下载{ASR_DOWNLOAD_ESTIMATE}，耗时较长），之后离线运行"
        )),
        _ => Some(format!(
            "未开启自动安装：先运行 dsivio media asr install（下载{ASR_DOWNLOAD_ESTIMATE}）"
        )),
    };
    status["settings"] = json!(configured);
    if let Some(note) = note {
        status["note"] = json!(note);
    }
    status
}

/// Idempotent submissions are serialized so two calls with one key cannot both reach a vendor.
static IDEMPOTENT: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

async fn submit_request(app: &tauri::AppHandle, submit: Submit) -> Result<MediaTask, String> {
    use tauri::Manager;
    let kind = submit.kind.ok_or("缺少媒体类型")?;
    let (provider_id, model) = {
        let state = app.state::<crate::state::AppState>();
        let settings = state.settings_read();
        resolve_model(&settings, &kind, submit.model.as_deref())?
    };
    let origin = origin_for(submit.source.as_deref(), submit.idempotency_key.as_deref())?;
    let request = MediaRequest {
        provider_id,
        model,
        kind,
        prompt: submit.prompt,
        images: submit.images,
        options: submit.options,
        origin: Some(origin.clone()),
        description_revision: submit.description_revision,
    };
    if submit.idempotency_key.is_none() {
        return super::start_media_generation(app.clone(), request).await;
    }
    let _serial = IDEMPOTENT.lock().await;
    // Same reconciliation as generation workflows: a unique origin names the one paid submission.
    let existing = super::list_media_tasks(
        app.clone(),
        super::MediaTaskFilter {
            origin: Some(origin),
            ..Default::default()
        },
    )?;
    let hash = super::request_hash(&request)?;
    validate_existing_hashes(&existing, &hash)?;
    if let Some(task) = submission_for_key(existing) {
        return Ok(task);
    }
    super::start_configured(app, request).await
}

// Prefer the paid/uncertain receipt over earlier rejected attempts. A rejected-only record is
// still evidence: lookup must not turn it into a new submission.
fn lookup_task(tasks: Vec<MediaTask>) -> Option<MediaTask> {
    submission_for_key(tasks.clone()).or_else(|| tasks.into_iter().next())
}

fn validate_existing_hashes(existing: &[MediaTask], hash: &str) -> Result<(), String> {
    if existing.iter().any(|task| task.request_hash.as_deref() != Some(hash)) {
        return Err("IDEMPOTENCY_CONFLICT: 同一 key 的请求内容不同或旧任务缺少请求散列；不会重新提交".into());
    }
    Ok(())
}

/// The task an idempotency key already names, if a new submission must not be made. Only a
/// request the provider definitively refused (nothing charged) may be submitted again under the
/// same key, so a caller that fixed the cause can retry the same operation.
fn submission_for_key(existing: Vec<MediaTask>) -> Option<MediaTask> {
    let refused = |task: &MediaTask| {
        task.status == MediaStatus::Failed
            && task.submission_state == Some(MediaSubmissionState::Rejected)
            && !task.can_resume
    };
    existing.into_iter().find(|task| !refused(task))
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ModelEntry {
    kind: MediaKind,
    /// `provider/model`; pass back as `--model`.
    id: String,
    provider_id: String,
    provider_name: String,
    model: String,
    default: bool,
    /// False when Dsivio has no facts about this model; then `capabilities` is null and a caller
    /// must not assume anything beyond a prompt.
    known: bool,
    /// What the model accepts: taken from the same tables the request is validated against.
    capabilities: Option<Value>,
    description: super::model_parameters::ModelDescription,
}

fn capabilities_of(
    settings: &crate::settings::Settings,
    kind: &MediaKind,
    provider_id: &str,
    model: &str,
) -> Option<Value> {
    let provider = settings.get_provider(provider_id)?;
    if provider.request.comfy.is_some() { return None; }
    match kind {
        MediaKind::Video => super::video_providers::capabilities(provider, model).map(|c| json!(c)),
        MediaKind::Image => Some(json!(super::image_providers::image_capabilities(
            provider, model
        ))),
        MediaKind::Speech | MediaKind::Transcribe | MediaKind::Edit | MediaKind::Text => None,
    }
}

pub(super) fn pool<'a>(
    settings: &'a crate::settings::Settings,
    kind: &MediaKind,
) -> &'a [crate::settings::DefaultModelSelection] {
    match kind {
        MediaKind::Image => &settings.workbench_media.image_models,
        MediaKind::Video => &settings.workbench_media.video_models,
        MediaKind::Speech => &settings.workbench_media.speech_models,
        MediaKind::Transcribe => &settings.workbench_media.transcribe_models,
        MediaKind::Edit | MediaKind::Text => &[],
    }
}

/// A pool member can be submitted right now: its provider is enabled and still lists the model.
/// This is the rule `start` enforces, so the default never points at something that would be refused.
pub(super) fn usable(
    settings: &crate::settings::Settings,
    entry: &crate::settings::DefaultModelSelection,
    kind: &MediaKind,
) -> bool {
    if entry.provider_id == "local" {
        return (*kind == MediaKind::Transcribe && entry.model == "whisperx-small")
            || (*kind == MediaKind::Speech && entry.model == super::local_tts::MODEL && super::local_tts::available())
            || (*kind == MediaKind::Edit && matches!(entry.model.as_str(), super::local_edit::MODEL_SUBTITLE | super::local_edit::MODEL_EDIT));
    }
    settings.providers.iter().any(|p| {
        p.id == entry.provider_id && p.enabled && p.enabled_models.contains(&entry.model)
            && match kind {
                MediaKind::Speech => super::speech_providers::configured(p, &entry.model),
                MediaKind::Transcribe => super::transcribe_configured(p, &entry.model),
                MediaKind::Image | MediaKind::Video => true,
                MediaKind::Edit | MediaKind::Text => false,
            }
    })
}

pub(super) fn members(settings: &crate::settings::Settings, kind: &MediaKind) -> Vec<crate::settings::DefaultModelSelection> {
    let mut members: Vec<_> = pool(settings,kind).iter().filter(|entry|usable(settings,entry,kind)).cloned().collect();
    if *kind == MediaKind::Speech && super::local_tts::available() && !members.iter().any(|m|m.provider_id == "local" && m.model == super::local_tts::MODEL) {
        members.push(crate::settings::DefaultModelSelection {provider_id:"local".into(),model:super::local_tts::MODEL.into()});
    }
    if *kind == MediaKind::Edit {
        for model in [super::local_edit::MODEL_SUBTITLE, super::local_edit::MODEL_EDIT] {
            if !members.iter().any(|member| member.provider_id == "local" && member.model == model) {
                members.push(crate::settings::DefaultModelSelection { provider_id: "local".into(), model: model.into() });
            }
        }
    }
    members
}

fn list_models(settings: &crate::settings::Settings, kind: Option<&MediaKind>) -> Vec<ModelEntry> {
    let kinds = match kind {
        Some(kind) => vec![kind.clone()],
        None => vec![MediaKind::Image, MediaKind::Video, MediaKind::Speech, MediaKind::Transcribe, MediaKind::Edit],
    };
    let local = super::local_provider();
    kinds
        .into_iter()
        .flat_map(|kind| {
            members(settings, &kind)
                .into_iter()
                .enumerate()
                .map(|(index, entry)| {
                    let capabilities =
                        capabilities_of(settings, &kind, &entry.provider_id, &entry.model);
                    let provider = if entry.provider_id == "local" { &local } else { settings.get_provider(&entry.provider_id).expect("usable provider") };
                    let description = super::model_parameters::describe(provider, &entry.model, &kind);
                    let known = match kind {
                        MediaKind::Image => description.arguments.contains_key("n"),
                        MediaKind::Video => capabilities.is_some(),
                        MediaKind::Speech | MediaKind::Transcribe | MediaKind::Edit | MediaKind::Text => true,
                    };
                    ModelEntry {
                        known,
                        capabilities: if known { capabilities } else { None },
                        description,
                        kind: kind.clone(),
                        id: format!("{}/{}", entry.provider_id, entry.model),
                        provider_id: entry.provider_id.clone(),
                        provider_name: if provider.name.trim().is_empty() { provider.id.clone() } else { provider.name.clone() },
                        model: entry.model.clone(),
                        default: index == 0,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Cloud models use the media pool; installed system TTS is an explicit local capability.
fn resolve_model(
    settings: &crate::settings::Settings,
    kind: &MediaKind,
    wanted: Option<&str>,
) -> Result<(String, String), String> {
    let label = match kind {
        MediaKind::Image => "图片",
        MediaKind::Video => "视频",
        MediaKind::Speech => "语音",
        MediaKind::Transcribe => "转写",
        MediaKind::Edit => "剪辑",
        MediaKind::Text => "文案",
    };
    let members = members(settings, kind);
    let first = members.first().ok_or_else(|| {
        if pool(settings, kind).is_empty() {
            format!("媒体创作里没有开启{label}模型，请先到「设置 > 媒体创作」打开")
        } else {
            format!("媒体创作里的{label}模型当前都不可用，请检查「设置 > 模型」里的供应商是否启用")
        }
    })?;
    let Some(wanted) = wanted.map(str::trim).filter(|w| !w.is_empty()) else {
        return Ok((first.provider_id.clone(), first.model.clone()));
    };
    if let Some(entry) = members
        .iter()
        .find(|e| format!("{}/{}", e.provider_id, e.model) == wanted)
    {
        return Ok((entry.provider_id.clone(), entry.model.clone()));
    }
    let matches: Vec<_> = members.iter().filter(|e| e.model == wanted).collect();
    match matches.as_slice() {
        [one] => Ok((one.provider_id.clone(), one.model.clone())),
        [] => Err(format!(
            "{label}模型 {wanted} 没有在媒体创作里开启，可用 `dsivio media models` 查看"
        )),
        _ => Err(format!("多个供应商都有 {wanted}，请用 `供应商/模型` 指定")),
    }
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

/// `cli/<source>` for listing; a key adds one more segment that identifies the paid submission.
fn origin_for(source: Option<&str>, key: Option<&str>) -> Result<String, String> {
    let source = source.unwrap_or("cli");
    if !valid_label(source) {
        return Err("--source 只能包含字母、数字、-、_、.，最长 64 个字符".into());
    }
    let mut origin = format!("cli/{source}");
    if let Some(key) = key {
        if !(key.len() <= 128
            && !key.is_empty()
            && key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':')))
        {
            return Err("--idempotency-key 只能包含字母、数字、-、_、.、:，最长 128 个字符".into());
        }
        origin.push('/');
        origin.push_str(key);
    }
    Ok(origin)
}

pub(super) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("无效路径")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = dir.join(format!(".{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut file = options.open(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&temp, path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Launcher on PATH

fn shim_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".kivio").join("bin"))
}

fn shim_script(exe: &Path) -> (&'static str, String) {
    let exe = exe.to_string_lossy();
    if cfg!(windows) {
        ("dsivio.cmd", format!("@\"{exe}\" %*\r\n"))
    } else {
        (
            "dsivio",
            format!("#!/bin/sh\nexec '{}' \"$@\"\n", exe.replace('\'', r"'\''")),
        )
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn git_bash_shim_script(exe: &Path) -> String {
    let exe = crate::utils::strip_windows_verbatim_prefix(exe.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    format!("#!/bin/sh\nexec '{}' \"$@\"\n", exe.replace('\'', r"'\''"))
}

fn write_shim(dir: &Path, name: &str, script: &str) -> bool {
    let path = dir.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(script) {
        return true;
    }
    let written = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, script));
    #[cfg(unix)]
    if written.is_ok() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    if let Err(error) = written {
        eprintln!("dsivio launcher not installed: {error}");
        return false;
    }
    true
}

/// Put `dsivio` on this process's PATH (inherited by chat shell, MCP and plugin processes) and
/// keep the launcher pointing at the App that is running now.
pub fn install_launcher() {
    let (Some(dir), Ok(exe)) = (shim_dir(), std::env::current_exe()) else {
        return;
    };
    let (name, script) = shim_script(&exe);
    if !write_shim(&dir, name, &script) {
        return;
    }
    // Git Bash (the Windows chat shell when installed) ignores PATHEXT and cannot run
    // `dsivio.cmd` as `dsivio`; an extensionless sh launcher covers it.
    #[cfg(windows)]
    let _ = write_shim(&dir, "dsivio", &git_bash_shim_script(&exe));
    let key = if cfg!(windows) { "Path" } else { "PATH" };
    let current = std::env::var_os(key).unwrap_or_default();
    if std::env::split_paths(&current).any(|p| p == dir) {
        return;
    }
    if let Ok(joined) =
        std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(&current)))
    {
        std::env::set_var(key, joined);
    }
}

// ---------------------------------------------------------------------------
// Client

const HELP: &str = "\
dsivio media —— 用 Dsivio「设置 > 媒体创作」里的模型生成图片、视频、语音及转写

  dsivio media models [--kind image|video|speech|transcribe|edit|matting] [--json]
  dsivio media image  (--prompt <文本> | --prompt-file <文件|->) [--model <供应商/模型>]
                      [--ref <图片>]... [--ratio 16:9] [--size 2K] [--quality high] [--n 1]
  dsivio media video  (--prompt <文本> | --prompt-file <文件|->) [--model <供应商/模型>]
                      [--first-frame <图片>] [--last-frame <图片>] [--ref <图片>]...
                      [--ref-video <文件>]... [--ref-audio <文件>]...
                      [--duration 5|auto] [--resolution 720p] [--ratio 9:16] [--audio|--no-audio]
  dsivio media speech (--text <文本> | --text-file <文件|->) [--model <供应商/模型>]
                      [--mode tts|clone] [--voice <音色>] [--voice-ref <音频>]
                      [--consent-attestation <声明文件>] [--instruction <文本> | --instruction-file <文件>]
                      [--output-format wav|mp3|flac|opus|aac]
  dsivio media transcribe <标准WAV> --language <语言> [--model local/whisperx-small|供应商/模型]
                      [--sample-frames <正安全整数>] [--timestamps word|segment]
  dsivio media subtitle <视频> --language <语言> [--burn] [--model local/ffmpeg-subtitle]
  dsivio media edit --plan-file <JSON>
  dsivio media asr status [--json]
  dsivio media asr install [--json]     按「设置 > 媒体创作 > 转写」的模型与语言安装
  dsivio media asr stop [--json]
  dsivio media cancel <任务ID> [--timeout <秒，默认30>] [--json]
  dsivio media status <任务ID> [--resume]
  dsivio media status --source <来源> --idempotency-key <key>  (只读找回任务，不重交)
  dsivio media wait   <任务ID> [--timeout <秒>]

image / video / speech / transcribe 通用：
  --no-wait                  提交后立即返回任务 ID
  --timeout <秒>             等待上限（图片/语音/转写默认 600，视频默认 1800）
  --idempotency-key <key>    同一 key 只提交一次，重复调用返回同一个任务
  --source <名称>            记录调用来源，例如 dsivio-video
  --options-json <JSON>      参数对象，仅接受模型 description 已声明的键
  --options-file <文件>      与 --options-json 互斥，从文件读取参数对象
  --description-revision <hash>  描述版本不匹配时付费前拒绝
  --json                     显式声明 JSON 输出（不改变输出形状）
  --out <目录>               成功后把结果复制到该目录

stdout 只输出 JSON。退出码：0 成功，2 参数错误/模型未开启（未提交），3 服务拒绝（未扣费），
4 提交后失败，5 提交结果不确定（不要重交，用 status 查询），6 Dsivio 未运行，7 已取消，124 等待超时。
cancel 成功交互退出0，请读取 outcome；asr install 退出0只表示安装已受理，不表示ready。
transcribe 必须指定 --language，且该语言已在转写设置中勾选；本地转写首次下载约 2–5 GB（视语言而定），中断后重试会续传。
";

struct Args {
    values: BTreeMap<String, Vec<String>>,
    flags: Vec<String>,
    positional: Vec<String>,
}

const BOOLEAN_FLAGS: &[&str] = &["no-wait", "audio", "no-audio", "resume", "help", "json", "burn"];

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        values: BTreeMap::new(),
        flags: vec![],
        positional: vec![],
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "-h" {
            parsed.flags.push("help".into());
            continue;
        }
        let Some(name) = arg.strip_prefix("--") else {
            parsed.positional.push(arg);
            continue;
        };
        if let Some((name, value)) = name.split_once('=') {
            if BOOLEAN_FLAGS.contains(&name) {
                return Err(format!("--{name} 不接受取值"));
            }
            parsed
                .values
                .entry(name.into())
                .or_default()
                .push(value.into());
        } else if BOOLEAN_FLAGS.contains(&name) {
            parsed.flags.push(name.into());
            if parsed.flags.iter().filter(|flag| flag.as_str() == name).count() > 1 {
                return Err(format!("--{name} 只能出现一次"));
            }
        } else {
            let value = iter.next().ok_or_else(|| format!("--{name} 缺少取值"))?;
            parsed.values.entry(name.into()).or_default().push(value);
        }
    }
    Ok(parsed)
}

impl Args {
    fn only(&self, allowed: &[&str]) -> Result<(), String> {
        for name in self.values.keys().chain(self.flags.iter()) {
            if !allowed.contains(&name.as_str()) && name != "help" && name != "json" {
                return Err(format!("不支持的参数 --{name}"));
            }
        }
        Ok(())
    }
    fn one(&self, name: &str) -> Result<Option<&str>, String> {
        match self.values.get(name).map(Vec::as_slice) {
            None | Some([]) => Ok(None),
            Some([value]) => Ok(Some(value)),
            Some(_) => Err(format!("--{name} 只能出现一次")),
        }
    }
    fn many(&self, name: &str) -> Vec<String> {
        self.values.get(name).cloned().unwrap_or_default()
    }
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }
    fn number(&self, name: &str) -> Result<Option<u64>, String> {
        self.one(name)?
            .map(|v| v.parse::<u64>().map_err(|_| format!("--{name} 需要正整数")))
            .transpose()
    }
}

/// The App resolves paths from its own working directory, so local files become absolute here.
fn absolute(value: &str, cwd: &Path) -> String {
    if ["http://", "https://", "data:", "mm_file://"]
        .iter()
        .any(|p| value.starts_with(p))
    {
        return value.into();
    }
    let path = Path::new(value);
    if path.is_absolute() {
        value.into()
    } else {
        cwd.join(path).to_string_lossy().into_owned()
    }
}

fn local_file(value: &str, cwd: &Path) -> Result<String, String> {
    if value == "-" || value.contains("://") || value.starts_with("data:") {
        return Err("需要本地文件；仅 --text-file - 可读取语音标准输入".into());
    }
    let path = Path::new(value);
    Ok(if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
        .to_string_lossy().into_owned())
}

fn read_speech_text(args: &Args, cwd: &Path, stdin: &mut dyn Read) -> Result<String, String> {
    let text = match (args.one("text")?, args.one("text-file")?) {
        (Some(_), Some(_)) => return Err("--text 和 --text-file 只能用一个".into()),
        (Some(text), None) => text.to_owned(),
        (None, Some("-")) => {
            let mut text = String::new();
            stdin.read_to_string(&mut text).map_err(|e| format!("读取标准输入失败：{e}"))?;
            text
        }
        (None, Some(file)) => std::fs::read_to_string(local_file(file, cwd)?)
            .map_err(|e| format!("读取语音文本失败：{e}"))?,
        (None, None) => return Err("需要 --text 或 --text-file".into()),
    };
    if text.trim().is_empty() { return Err("语音文本为空".into()); }
    Ok(text)
}

fn read_prompt(args: &Args, cwd: &Path, stdin: &mut dyn Read) -> Result<String, String> {
    let prompt = match (args.one("prompt")?, args.one("prompt-file")?) {
        (Some(_), Some(_)) => return Err("--prompt 和 --prompt-file 只能用一个".into()),
        (Some(text), None) => text.to_owned(),
        (None, Some("-")) => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .map_err(|e| format!("读取标准输入失败：{e}"))?;
            text
        }
        (None, Some(file)) => std::fs::read_to_string(absolute(file, cwd))
            .map_err(|e| format!("读取提示词文件失败：{e}"))?,
        (None, None) => return Err("需要 --prompt 或 --prompt-file".into()),
    };
    let prompt = prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("提示词为空".into());
    }
    Ok(prompt)
}

const SUBMIT_FLAGS: &[&str] = &[
    "model",
    "no-wait",
    "timeout",
    "idempotency-key",
    "source",
    "options-json",
    "options-file",
    "description-revision",
    "out",
];

fn duplicate_option(key: &str) -> String {
    serde_json::to_string(&super::model_parameters::ArgumentError {
        code: "MODEL_ARGUMENT_DUPLICATE", argument_path: key.into(), rule_id: "unique".into(),
        actual: json!(key), expected: json!("one spelling"),
        message: format!("MODEL_ARGUMENT_DUPLICATE: {key} simultaneously appears in flags and options"),
    }).expect("argument error is serializable")
}

fn build_submit(
    kind: MediaKind,
    args: &Args,
    cwd: &Path,
    stdin: &mut dyn Read,
) -> Result<Submit, String> {
    let mut options = Map::new();
    let mut images = vec![];
    let local = |values: Vec<String>| values.iter().map(|v| absolute(v, cwd)).collect::<Vec<_>>();
    match kind {
        MediaKind::Image => {
            let allowed = [SUBMIT_FLAGS, &["prompt", "prompt-file", "ref", "ratio", "size", "quality", "n"]].concat();
            args.only(&allowed)?;
            images = local(args.many("ref"));
            for (flag, key) in [
                ("ratio", "aspectRatio"),
                ("size", "size"),
                ("quality", "quality"),
            ] {
                if let Some(value) = args.one(flag)? {
                    options.insert(key.into(), json!(value));
                }
            }
            if let Some(n) = args.number("n")? {
                options.insert("n".into(), json!(n));
            }
        }
        MediaKind::Video => {
            let allowed = [
                SUBMIT_FLAGS,
                &[
                    "prompt",
                    "prompt-file",
                    "first-frame",
                    "last-frame",
                    "ref",
                    "ref-video",
                    "ref-audio",
                    "duration",
                    "resolution",
                    "ratio",
                    "audio",
                    "no-audio",
                ],
            ]
            .concat();
            args.only(&allowed)?;
            for (flag, key) in [("first-frame", "firstFrame"), ("last-frame", "lastFrame")] {
                if let Some(value) = args.one(flag)? {
                    options.insert(key.into(), json!(absolute(value, cwd)));
                }
            }
            for (flag, key) in [
                ("ref", "referenceImages"),
                ("ref-video", "referenceVideos"),
                ("ref-audio", "referenceAudios"),
            ] {
                let values = local(args.many(flag));
                if !values.is_empty() {
                    options.insert(key.into(), json!(values));
                }
            }
            for (flag, key) in [("resolution", "resolution"), ("ratio", "ratio")] {
                if let Some(value) = args.one(flag)? {
                    options.insert(key.into(), json!(value));
                }
            }
            if let Some(duration) = args.one("duration")? {
                let value = if duration == "auto" { json!("auto") } else { json!(duration.parse::<u32>().map_err(|_| "--duration requires seconds or auto")?) };
                options.insert("duration".into(), value);
            }
            if args.flag("audio") && args.flag("no-audio") { return Err("--audio and --no-audio are mutually exclusive".into()); }
            if args.flag("audio") || args.flag("no-audio") {
                options.insert("generateAudio".into(), json!(args.flag("audio")));
            }
        }
        MediaKind::Speech => {
            let allowed = [SUBMIT_FLAGS, &[
                "text", "text-file", "mode", "voice", "voice-ref", "consent-attestation",
                "instruction", "instruction-file", "output-format",
            ]].concat();
            args.only(&allowed)?;
            if args.one("text")?.is_some() || args.one("text-file")?.is_some() {
                options.insert("text".into(), json!(read_speech_text(args, cwd, stdin)?));
            }
            for (flag, key) in [("mode", "mode"), ("voice", "voice"), ("output-format", "outputFormat")] {
                if let Some(value) = args.one(flag)? { options.insert(key.into(), json!(value)); }
            }
            if let Some(file) = args.one("voice-ref")? {
                options.insert("voiceReference".into(), json!([{"source":local_file(file, cwd)?,"attributes":{}}]));
            }
            if let Some(file) = args.one("consent-attestation")? {
                options.insert("consentAttestation".into(), json!(local_file(file, cwd)?));
            }
            match (args.one("instruction")?, args.one("instruction-file")?) {
                (Some(_), Some(_)) => return Err("--instruction 和 --instruction-file 只能用一个".into()),
                (Some(text), None) => { options.insert("instruction".into(), json!(text)); }
                (None, Some(file)) => {
                    let text = std::fs::read_to_string(local_file(file, cwd)?)
                        .map_err(|e| format!("读取语音指令失败：{e}"))?;
                    options.insert("instruction".into(), json!(text));
                }
                (None, None) => {}
            }
        }
        MediaKind::Transcribe => {
            let allowed = [SUBMIT_FLAGS, &["language", "sample-frames", "timestamps"]].concat();
            args.only(&allowed)?;
            match args.positional.as_slice() {
                [] => {}
                [file] => { options.insert("audioFile".into(), json!(local_file(file, cwd)?)); }
                _ => return Err("需要一个本地标准 WAV 文件".into()),
            }
            if let Some(language) = args.one("language")? {
                options.insert("language".into(), json!(language));
            }
            if let Some(frames) = args.number("sample-frames")? {
                if frames == 0 || frames > 9_007_199_254_740_991 {
                    return Err("--sample-frames 需要正安全整数".into());
                }
                options.insert("sampleFrames".into(), json!(frames));
            }
            if let Some(timestamps) = args.one("timestamps")? {
                if !matches!(timestamps, "word" | "segment") {
                    return Err("--timestamps 只能是 word 或 segment".into());
                }
                options.insert("timestamps".into(), json!(timestamps));
            }
        }
        MediaKind::Edit => return Err("本地剪辑请用 subtitle / edit 命令".into()),
        MediaKind::Text => return Err("文案记录不从媒体命令提交".into()),
    }
    let extra = match (args.one("options-json")?, args.one("options-file")?) {
        (Some(_), Some(_)) => return Err("--options-json and --options-file are mutually exclusive".into()),
        (Some(text), None) => Some(text.to_owned()),
        (None, Some(path)) => Some(std::fs::read_to_string(absolute(path, cwd)).map_err(|e| format!("--options-file: {e}"))?),
        (None, None) => None,
    };
    if let Some(extra) = extra {
        let Value::Object(extra) = serde_json::from_str(&extra).map_err(|e| format!("options must be valid JSON: {e}"))? else { return Err("options must be a JSON object".into()); };
        let extra = super::model_parameters::normalize(extra.into_iter().collect())?;
        for (key, value) in extra {
            if options.contains_key(&key) {
                return Err(duplicate_option(&key));
            }
            options.insert(key, value);
        }
    }
    for name in ["firstFrame", "lastFrame", "referenceImages", "referenceVideos", "referenceAudios", "voiceReference"] {
        if let Some(value) = options.get_mut(name) {
            if let Some(source) = value.as_str() {
                *value = json!(if name == "voiceReference" { local_file(source, cwd)? } else { absolute(source, cwd) });
            } else if let Some(entries) = value.as_array_mut() {
                for entry in entries {
                    if let Some(source) = entry.as_str() {
                        *entry = json!(if name == "voiceReference" { local_file(source, cwd)? } else { absolute(source, cwd) });
                    } else if let Some(source) = entry.get("source").and_then(Value::as_str) {
                        entry["source"] = json!(if name == "voiceReference" { local_file(source, cwd)? } else { absolute(source, cwd) });
                    }
                }
            }
        }
    }
    for name in ["audioFile", "consentAttestation"] {
        if let Some(value) = options.get_mut(name) {
            if let Some(file) = value.as_str() { *value = json!(local_file(file, cwd)?); }
        }
    }
    if kind == MediaKind::Speech {
        if !options.get("text").and_then(Value::as_str).is_some_and(|text| !text.trim().is_empty()) {
            return Err("需要非空语音文本：--text / --text-file 或 options.text".into());
        }
        options.entry("mode").or_insert(json!("tts"));
        match options["mode"].as_str() {
            Some("tts") if options.contains_key("voice") && !options.contains_key("voiceReference") && !options.contains_key("consentAttestation") => {}
            Some("clone") if !options.contains_key("voice") && options.contains_key("voiceReference") && options.contains_key("consentAttestation") => {}
            Some("tts") => return Err("tts 需要 --voice，不能使用参考音频或授权声明".into()),
            Some("clone") => return Err("clone 需要一个 --voice-ref 和 --consent-attestation，不能同时使用 --voice".into()),
            _ => return Err("--mode 只能是 tts 或 clone".into()),
        }
    } else if kind == MediaKind::Transcribe {
        if !options.get("audioFile").and_then(Value::as_str).is_some_and(|file| !file.is_empty()) {
            return Err("需要一个本地标准 WAV 文件或 options.audioFile".into());
        }
        let language = options.get("language").and_then(Value::as_str).ok_or("需要 --language 或 options.language")?;
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|c| c.is_ascii_lowercase())
            || matches!(language, "auto" | "und") {
            return Err("--language 需要小写2–3字母语言代码，不能是auto/und".into());
        }
        if let Some(frames) = options.get("sampleFrames") {
            if !frames.as_u64().is_some_and(|frames| (1..=9_007_199_254_740_991).contains(&frames)) {
                return Err("--sample-frames 需要正安全整数".into());
            }
        }
        options.entry("timestamps").or_insert(json!("word"));
        if !options["timestamps"].as_str().is_some_and(|timestamps| matches!(timestamps, "word" | "segment")) {
            return Err("--timestamps 只能是 word 或 segment".into());
        }
    }
    if kind == MediaKind::Image {
        if let Some(value) = options.remove("images") {
            if args.values.contains_key("ref") { return Err(duplicate_option("images")); }
            let entries = value.as_array().ok_or("options.images 需要媒体列表")?;
            for (index, entry) in entries.iter().enumerate() {
                if entry.as_object().is_some_and(|object| object.keys().any(|key| key != "source" && key != "attributes"))
                    || entry.get("attributes").is_some_and(|attributes| !attributes.as_object().is_some_and(|attributes| attributes.is_empty())) {
                    return Err(serde_json::to_string(&super::model_parameters::ArgumentError {
                        code: "MODEL_ARGUMENT_UNSUPPORTED", argument_path: format!("images[{index}].attributes"),
                        rule_id: "image-source".into(), actual: json!("entry metadata"),
                        expected: json!("source with no additional attributes"),
                        message: "Image reference metadata cannot be represented by this request boundary".into(),
                    }).expect("argument error is serializable"));
                }
                let source = entry.as_str().or_else(|| entry.get("source").and_then(Value::as_str))
                    .ok_or("options.images 媒体条目需要 source")?;
                images.push(absolute(source, cwd));
            }
        }
    }
    let prompt = if matches!(kind, MediaKind::Image | MediaKind::Video) {
        let option_prompt = options.remove("prompt");
        if args.one("prompt")?.is_some() || args.one("prompt-file")?.is_some() {
            if option_prompt.is_some() { return Err(duplicate_option("prompt")); }
            read_prompt(args, cwd, stdin)?
        } else {
            let prompt = option_prompt.as_ref().and_then(Value::as_str)
                .ok_or("需要 --prompt / --prompt-file 或 options.prompt")?.trim();
            if prompt.is_empty() { return Err("提示词为空".into()); }
            prompt.to_owned()
        }
    } else { String::new() };
    Ok(Submit {
        kind: Some(kind.clone()),
        model: args.one("model")?.map(str::to_owned),
        prompt,
        images,
        options: options.into_iter().collect(),
        source: args.one("source")?.map(str::to_owned),
        idempotency_key: args.one("idempotency-key")?.map(str::to_owned),
        description_revision: args.one("description-revision")?.map(str::to_owned),
    })
}

fn exit_code(task: &MediaTask) -> u8 {
    match task.status {
        MediaStatus::Succeeded => exit::OK,
        MediaStatus::Running => exit::OK,
        MediaStatus::Cancelled => exit::CANCELLED,
        MediaStatus::Failed => match task.submission_state {
            Some(MediaSubmissionState::Rejected) => exit::REJECTED,
            Some(MediaSubmissionState::Uncertain) => exit::UNCERTAIN,
            // No public or private resumable receipt and no rejection: unknown, never resubmit.
            None if !task.can_resume && task.remote_id.is_none() && task.outputs.is_empty() => exit::UNCERTAIN,
            None => exit::FAILED,
        },
    }
}

struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn invalid(message: impl Into<String>) -> Self {
        Failure {
            code: exit::INVALID,
            message: message.into(),
        }
    }
}

fn call(op: Op) -> Result<Value, Failure> {
    call_with_timeout(op, Duration::from_secs(120))
}

fn call_with_timeout(op: Op, timeout: Duration) -> Result<Value, Failure> {
    let value = serde_json::to_value(&op).map_err(|error| Failure {
        code: exit::INTERNAL,
        message: format!("任务格式无效：{error}"),
    })?;
    let joined = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| crate::app_cli::CliFailure::internal(error.to_string()))?;
        runtime.block_on(crate::app_cli::call("media", value, timeout))
    })
    .join();
    match joined {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(failure)) => Err(Failure { code: failure.code, message: failure.message }),
        Err(_) => Err(Failure { code: exit::INTERNAL, message: "CLI 调用中断".into() }),
    }
}

fn task_from(value: Value) -> Result<MediaTask, Failure> {
    serde_json::from_value(value).map_err(|e| Failure {
        code: exit::INTERNAL,
        message: format!("任务格式无效：{e}"),
    })
}

fn status_operation(args: &Args) -> Result<Op, String> {
    match (args.one("source")?, args.one("idempotency-key")?) {
        (None, None) => match args.positional.as_slice() {
            [id] => Ok(Op::Status { id: id.clone(), resume: args.flag("resume") }),
            _ => Err("需要一个任务 ID".into()),
        },
        (Some(source), Some(key)) if args.positional.is_empty() && !args.flag("resume") => {
            origin_for(Some(source), Some(key))?;
            Ok(Op::Lookup { source: source.into(), idempotency_key: key.into() })
        }
        _ => Err("status 只能用任务 ID（可 --resume），或同时用 --source 和 --idempotency-key；key 查询不支持 --resume".into()),
    }
}

fn status(id: &str, resume: bool) -> Result<MediaTask, Failure> {
    task_from(call(Op::Status {
        id: id.into(),
        resume,
    })?)
}

fn wait_for(mut task: MediaTask, timeout: Duration) -> Result<(MediaTask, bool), Failure> {
    let deadline = Instant::now() + timeout;
    if task.status == MediaStatus::Running {
        eprintln!("等待任务 {} …", task.id);
    }
    while task.status == MediaStatus::Running {
        if Instant::now() >= deadline {
            return Ok((task, true));
        }
        std::thread::sleep(POLL);
        task = status(&task.id, false)?;
    }
    Ok((task, false))
}

fn copy_outputs(task: &mut MediaTask, dir: &Path) -> Result<(), Failure> {
    std::fs::create_dir_all(dir).map_err(|e| Failure {
        code: exit::INTERNAL,
        message: format!("无法创建输出目录：{e}"),
    })?;
    for output in &mut task.outputs {
        let source = Path::new(&output.path);
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let target = dir.join(format!("{}-{name}", task.id));
        std::fs::copy(source, &target).map_err(|e| Failure {
            code: exit::INTERNAL,
            message: format!("复制结果失败：{e}"),
        })?;
        output.path = target.to_string_lossy().into_owned();
    }
    Ok(())
}

fn print_json(value: &impl Serialize) {
    println!("{}", serde_json::to_string(value).unwrap_or_default());
}

fn finish(mut task: MediaTask, timed_out: bool, out: Option<PathBuf>) -> Result<u8, Failure> {
    if task.status == MediaStatus::Succeeded {
        if let Some(dir) = out {
            copy_outputs(&mut task, &dir)?;
        }
    }
    print_json(&task);
    if let Some(error) = task
        .error
        .as_deref()
        .filter(|_| task.status == MediaStatus::Failed)
    {
        eprintln!("{error}");
    }
    Ok(if timed_out {
        exit::TIMEOUT
    } else {
        exit_code(&task)
    })
}

fn run_command(argv: Vec<String>) -> Result<u8, Failure> {
    let (command, rest) = match argv.split_first() {
        Some((command, rest)) => (command.as_str(), rest.to_vec()),
        None => ("help", vec![]),
    };
    let args = parse_args(rest).map_err(Failure::invalid)?;
    if command == "help" || command == "--help" || command == "-h" || args.flag("help") {
        print!("{HELP}");
        return Ok(exit::OK);
    }
    let cwd = std::env::current_dir().map_err(|e| Failure::invalid(e.to_string()))?;
    let timeout = |default: u64| -> Result<Duration, Failure> {
        Ok(Duration::from_secs(
            args.number("timeout")
                .map_err(Failure::invalid)?
                .unwrap_or(default),
        ))
    };
    let single_id = |args: &Args| -> Result<String, Failure> {
        match args.positional.as_slice() {
            [id] => Ok(id.clone()),
            _ => Err(Failure::invalid("需要一个任务 ID")),
        }
    };
    let run_local = |submit: Submit, default_timeout: u64| -> Result<u8, Failure> {
        let duration = timeout(default_timeout)?;
        let out = args.one("out").map_err(Failure::invalid)?.map(|dir| PathBuf::from(absolute(dir, &cwd)));
        let task = task_from(call(Op::Submit(submit))?)?;
        if args.flag("no-wait") {
            return finish(task, false, None);
        }
        let (task, timed_out) = wait_for(task, duration)?;
        finish(task, timed_out, out)
    };
    match command {
        "models" => {
            args.only(&["kind"]).map_err(Failure::invalid)?;
            if !args.positional.is_empty() { return Err(Failure::invalid("models 不接受位置参数")); }
            let kind = args.one("kind").map_err(Failure::invalid)?;
            if kind.is_some_and(|kind| !["image", "video", "speech", "transcribe", "edit", "matting"].contains(&kind)) {
                return Err(Failure::invalid("--kind 只能是 image/video/speech/transcribe/edit/matting"));
            }
            let kind = kind.map(str::to_owned);
            print_json(&call(Op::Models { kind })?);
            Ok(exit::OK)
        }
        "subtitle" => {
            args.only(&["language", "burn", "model", "timeout", "no-wait", "out", "source", "idempotency-key"]).map_err(Failure::invalid)?;
            let video = match args.positional.as_slice() {
                [video] => absolute(video, &cwd),
                _ => return Err(Failure::invalid("需要一个视频文件")),
            };
            let language = args.one("language").map_err(Failure::invalid)?.ok_or_else(|| Failure::invalid("需要 --language"))?.to_owned();
            let mut options = BTreeMap::new();
            options.insert("video".into(), json!(video));
            options.insert("language".into(), json!(language));
            options.insert("burn".into(), json!(args.flag("burn")));
            let model = args.one("model").map_err(Failure::invalid)?.map(str::to_owned).unwrap_or_else(|| format!("local/{}", super::local_edit::MODEL_SUBTITLE));
            run_local(Submit {
                kind: Some(MediaKind::Edit),
                model: Some(model),
                prompt: String::new(),
                images: vec![],
                options,
                source: args.one("source").map_err(Failure::invalid)?.map(str::to_owned),
                idempotency_key: args.one("idempotency-key").map_err(Failure::invalid)?.map(str::to_owned),
                description_revision: None,
            }, 600)
        }
        "edit" => {
            args.only(&["plan-file", "model", "timeout", "no-wait", "out", "source", "idempotency-key"]).map_err(Failure::invalid)?;
            if !args.positional.is_empty() {
                return Err(Failure::invalid("edit 不接受位置参数"));
            }
            let file = args.one("plan-file").map_err(Failure::invalid)?.ok_or_else(|| Failure::invalid("需要 --plan-file"))?;
            let text = std::fs::read_to_string(absolute(file, &cwd)).map_err(|error| Failure::invalid(format!("读取剪辑计划失败：{error}")))?;
            let plan: Value = serde_json::from_str(&text).map_err(|error| Failure::invalid(format!("剪辑计划不是 JSON：{error}")))?;
            if !plan.is_object() {
                return Err(Failure::invalid("剪辑计划必须是 JSON 对象"));
            }
            let mut options = BTreeMap::new();
            options.insert("plan".into(), plan);
            let model = args.one("model").map_err(Failure::invalid)?.map(str::to_owned).unwrap_or_else(|| format!("local/{}", super::local_edit::MODEL_EDIT));
            run_local(Submit {
                kind: Some(MediaKind::Edit),
                model: Some(model),
                prompt: String::new(),
                images: vec![],
                options,
                source: args.one("source").map_err(Failure::invalid)?.map(str::to_owned),
                idempotency_key: args.one("idempotency-key").map_err(Failure::invalid)?.map(str::to_owned),
                description_revision: None,
            }, 600)
        }
        "image" | "video" | "speech" | "transcribe" => {
            let kind = match command {
                "image" => MediaKind::Image,
                "video" => MediaKind::Video,
                "speech" => MediaKind::Speech,
                _ => MediaKind::Transcribe,
            };
            let duration = timeout(if kind == MediaKind::Video { 1800 } else { 600 })?;
            let submit =
                build_submit(kind, &args, &cwd, &mut std::io::stdin()).map_err(Failure::invalid)?;
            if command != "transcribe" && !args.positional.is_empty() {
                return Err(Failure::invalid(format!(
                    "多余的参数：{}",
                    args.positional.join(" ")
                )));
            }
            let out = args
                .one("out")
                .map_err(Failure::invalid)?
                .map(|dir| PathBuf::from(absolute(dir, &cwd)));
            let task = task_from(call(Op::Submit(submit))?)?;
            if args.flag("no-wait") {
                return finish(task, false, None);
            }
            let (task, timed_out) = wait_for(task, duration)?;
            finish(task, timed_out, out)
        }
        "cancel" => {
            args.only(&["timeout"]).map_err(Failure::invalid)?;
            let id = single_id(&args)?;
            let result = call_with_timeout(Op::Cancel { id }, timeout(30)?)?;
            print_json(&result);
            Ok(exit::OK)
        }
        "asr" => {
            let [action] = args.positional.as_slice() else {
                return Err(Failure::invalid("需要 asr status/install/stop"));
            };
            let op = match action.as_str() {
                "status" => { args.only(&[]).map_err(Failure::invalid)?; Op::Asrstatus }
                "stop" => { args.only(&[]).map_err(Failure::invalid)?; Op::Asrstop }
                "install" => {
                    args.only(&["model", "language"]).map_err(Failure::invalid)?;
                    let model = args.one("model").map_err(Failure::invalid)?.map(str::to_owned);
                    Op::Asrinstall { model, languages: args.many("language") }
                }
                _ => return Err(Failure::invalid("需要 asr status/install/stop")),
            };
            print_json(&call_with_timeout(op, Duration::from_secs(30))?);
            Ok(exit::OK)
        }
        "status" => {
            args.only(&["resume", "source", "idempotency-key"]).map_err(Failure::invalid)?;
            let op = status_operation(&args).map_err(Failure::invalid)?;
            let task = task_from(call(op)?)?;
            finish(task, false, None)
        }
        "wait" => {
            args.only(&["timeout", "out"]).map_err(Failure::invalid)?;
            let out = args
                .one("out")
                .map_err(Failure::invalid)?
                .map(|dir| PathBuf::from(absolute(dir, &cwd)));
            let task = status(&single_id(&args)?, false)?;
            let duration = timeout(if task.kind == MediaKind::Video { 1800 } else { 600 })?;
            let (task, timed_out) = wait_for(task, duration)?;
            finish(task, timed_out, out)
        }
        other => Err(Failure::invalid(format!(
            "未知命令 {other}，运行 `dsivio media help` 查看用法"
        ))),
    }
}

/// Entry for `dsivio media ...`. Runs before any App initialization.
pub fn run(args: impl Iterator<Item = std::ffi::OsString>) -> ExitCode {
    let argv: Result<Vec<String>, _> = args.map(|a| a.into_string()).collect();
    let result = match argv {
        Ok(argv) => run_command(argv),
        Err(_) => Err(Failure::invalid("参数不是有效的 UTF-8")),
    };
    ExitCode::from(result.unwrap_or_else(|failure| {
        let details: Option<Value> = serde_json::from_str(&failure.message).ok();
        if let Some(Value::Object(mut details)) = details {
            eprintln!("{}", details.get("message").and_then(Value::as_str).unwrap_or(&failure.message));
            details.insert("exitCode".into(), json!(failure.code));
            print_json(&Value::Object(details));
        } else {
            eprintln!("{}", failure.message);
            print_json(&json!({ "error": failure.message, "exitCode": failure.code }));
        }
        failure.code
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> crate::settings::Settings {
        let mut settings = crate::settings::Settings::default();
        for (id, models) in [
            ("a", vec!["gpt-image-1"]),
            ("b", vec!["gpt-image-1", "seedream"]),
            ("v", vec!["seedance"]),
        ] {
            settings.providers.push(serde_json::from_value(json!({"id":id,"name":format!("P{id}"),"baseUrl":"https://example.test","enabled":true,"enabledModels":models,"apiKeys":["k"]})).unwrap());
        }
        let pick = |p: &str, m: &str| crate::settings::DefaultModelSelection {
            provider_id: p.into(),
            model: m.into(),
        };
        settings.workbench_media.image_models = vec![
            pick("a", "gpt-image-1"),
            pick("b", "gpt-image-1"),
            pick("b", "seedream"),
        ];
        settings.workbench_media.video_models = vec![pick("v", "seedance")];
        settings
    }

    #[test]
    fn models_come_only_from_the_media_pool_and_the_first_is_the_default() {
        let settings = settings();
        let image = MediaKind::Image;
        assert_eq!(
            resolve_model(&settings, &image, None).unwrap(),
            ("a".into(), "gpt-image-1".into())
        );
        assert_eq!(
            resolve_model(&settings, &image, Some("b/gpt-image-1")).unwrap(),
            ("b".into(), "gpt-image-1".into())
        );
        assert_eq!(
            resolve_model(&settings, &image, Some("seedream")).unwrap(),
            ("b".into(), "seedream".into())
        );
        assert!(resolve_model(&settings, &image, Some("gpt-image-1"))
            .unwrap_err()
            .contains("供应商/模型"));
        assert!(resolve_model(&settings, &image, Some("seedance"))
            .unwrap_err()
            .contains("没有在媒体创作里开启"));
        let mut empty = settings.clone();
        empty.workbench_media.video_models.clear();
        assert!(resolve_model(&empty, &MediaKind::Video, None)
            .unwrap_err()
            .contains("设置 > 媒体创作"));
        let listed = list_models(&settings, Some(&MediaKind::Video));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "v/seedance");
        assert!(listed[0].default);
    }

    #[test]
    fn pool_order_is_the_priority_and_unusable_members_are_never_the_default() {
        let mut settings = settings();
        let pick = |p: &str, m: &str| crate::settings::DefaultModelSelection {
            provider_id: p.into(),
            model: m.into(),
        };
        let image = MediaKind::Image;
        // Dragging seedream to the top (pool order) makes it the default, and `models` lists it first.
        settings.workbench_media.image_models = vec![
            pick("b", "seedream"),
            pick("a", "gpt-image-1"),
            pick("b", "gpt-image-1"),
        ];
        assert_eq!(
            resolve_model(&settings, &image, None).unwrap(),
            ("b".into(), "seedream".into())
        );
        let listed: Vec<_> = list_models(&settings, Some(&image))
            .into_iter()
            .map(|m| (m.id, m.default))
            .collect();
        assert_eq!(
            listed,
            [
                ("b/seedream".into(), true),
                ("a/gpt-image-1".into(), false),
                ("b/gpt-image-1".into(), false)
            ]
        );
        // A first entry whose provider is disabled is skipped, listed nowhere, and cannot be picked by name.
        settings
            .providers
            .iter_mut()
            .find(|p| p.id == "b")
            .unwrap()
            .enabled = false;
        assert_eq!(
            resolve_model(&settings, &image, None).unwrap(),
            ("a".into(), "gpt-image-1".into())
        );
        assert_eq!(list_models(&settings, Some(&image)).len(), 1);
        assert!(resolve_model(&settings, &image, Some("b/seedream")).is_err());
        // A provider that no longer lists the model is unusable too.
        settings
            .providers
            .iter_mut()
            .find(|p| p.id == "a")
            .unwrap()
            .enabled_models
            .clear();
        let error = resolve_model(&settings, &image, None).unwrap_err();
        assert!(error.contains("都不可用"), "{error}");
        assert!(list_models(&settings, Some(&image)).is_empty());
    }

    fn with_models(models: &[&str]) -> crate::settings::Settings {
        let mut settings = crate::settings::Settings::default();
        settings.providers.push(serde_json::from_value(json!({"id":"p","name":"P","baseUrl":"https://example.test","enabled":true,"enabledModels":models,"apiKeys":["k"]})).unwrap());
        let pick = |m: &str| crate::settings::DefaultModelSelection {
            provider_id: "p".into(),
            model: m.into(),
        };
        settings.workbench_media.video_models = models
            .iter()
            .filter(|m| !m.contains("image"))
            .map(|m| pick(m))
            .collect();
        settings.workbench_media.image_models = models
            .iter()
            .filter(|m| m.contains("image"))
            .map(|m| pick(m))
            .collect();
        settings
    }
    fn video_capabilities(settings: &crate::settings::Settings, model: &str) -> Value {
        let listed = list_models(settings, Some(&MediaKind::Video));
        serde_json::to_value(listed.iter().find(|m| m.model == model).unwrap()).unwrap()
    }

    #[test]
    fn models_expose_what_each_video_model_accepts_from_the_shared_catalog() {
        let settings = with_models(&[
            "doubao-seedance-2-5-260628",
            "doubao-seedance-2-0-fast-260128",
            "grok-imagine-video-1.5",
            "MiniMax-H3",
            "MiniMax-Hailuo-2.3",
            "veo-3.1-generate-preview",
            "my-private-video",
        ]);
        let seedance = video_capabilities(&settings, "doubao-seedance-2-5-260628");
        assert_eq!(seedance["known"], true);
        let c = &seedance["capabilities"];
        assert_eq!(
            (
                c["firstFrame"].clone(),
                c["lastFrame"].clone(),
                c["audioToggle"].clone()
            ),
            (json!(true), json!(true), json!(true))
        );
        assert_eq!(
            (
                c["maxReferenceImages"].clone(),
                c["maxReferenceVideos"].clone(),
                c["maxReferenceAudios"].clone()
            ),
            (json!(30), json!(10), json!(10))
        );
        assert_eq!(c["framesExcludeReferences"], true);
        assert_eq!(c["protocol"], "seedance");
        assert!(c["resolutions"]
            .as_array()
            .unwrap()
            .contains(&json!("1080p")));
        assert!(c["durations"].as_array().unwrap().contains(&json!(30)));
        assert_eq!(c["defaults"]["duration"], 5);
        // Seedance 2 fast: reference audio must come with a picture; the older one is smaller.
        let fast =
            &video_capabilities(&settings, "doubao-seedance-2-0-fast-260128")["capabilities"];
        assert_eq!(fast["maxReferenceImages"], 9);
        assert_eq!(fast["referenceAudioNeedsVisual"], true);
        // Grok 1.5 accepts first/last frames and reference images, but no reference video/audio.
        let grok = &video_capabilities(&settings, "grok-imagine-video-1.5")["capabilities"];
        assert_eq!(
            (grok["firstFrame"].clone(), grok["lastFrame"].clone()),
            (json!(true), json!(true))
        );
        assert_eq!(
            (
                grok["maxReferenceImages"].clone(),
                grok["maxReferenceVideos"].clone(),
                grok["maxReferenceAudios"].clone()
            ),
            (json!(7), json!(0), json!(0))
        );
        // MiniMax H3 accepts local reference media; a model without `reference` mode carries none.
        let h3 = &video_capabilities(&settings, "MiniMax-H3")["capabilities"];
        assert_eq!(
            (h3["lastFrame"].clone(), h3["localReferenceMedia"].clone()),
            (json!(true), json!(true))
        );
        let hailuo = &video_capabilities(&settings, "MiniMax-Hailuo-2.3")["capabilities"];
        assert_eq!(
            (
                hailuo["firstFrame"].clone(),
                hailuo["lastFrame"].clone(),
                hailuo["maxReferenceImages"].clone()
            ),
            (json!(true), json!(false), json!(0))
        );
        let veo = &video_capabilities(&settings, "veo-3.1-generate-preview")["capabilities"];
        assert_eq!(
            (
                veo["firstFrame"].clone(),
                veo["lastFrame"].clone(),
                veo["audioToggle"].clone()
            ),
            (json!(true), json!(false), json!(false))
        );
        // Unknown to the catalog: say so instead of guessing.
        let unknown = video_capabilities(&settings, "my-private-video");
        assert_eq!(
            (unknown["known"].clone(), unknown["capabilities"].clone()),
            (json!(false), Value::Null)
        );
    }

    #[test]
    fn a_chosen_protocol_changes_what_a_video_model_is_described_as_accepting() {
        let mut settings = with_models(&["MiniMax-H3"]);
        settings.providers[0].model_overrides.insert(
            "MiniMax-H3".into(),
            serde_json::from_value(json!({"videoProtocol":"minimax_hailuo"})).unwrap(),
        );
        let c = &video_capabilities(&settings, "MiniMax-H3")["capabilities"];
        // Sent as Hailuo, it has neither a last frame nor multimodal references, whatever the catalog says for H3.
        assert_eq!(
            (
                c["protocol"].clone(),
                c["lastFrame"].clone(),
                c["maxReferenceImages"].clone()
            ),
            (json!("minimax_hailuo"), json!(false), json!(0))
        );
    }

    #[test]
    fn image_models_expose_tiers_ratios_and_reference_limits() {
        let settings = with_models(&["gpt-image-2.5-flare", "grok-imagine-image-2.0"]);
        let listed = list_models(&settings, Some(&MediaKind::Image));
        let by =
            |m: &str| serde_json::to_value(listed.iter().find(|e| e.model == m).unwrap()).unwrap();
        let gpt = &by("gpt-image-2.5-flare");
        assert_eq!(gpt["known"], true);
        assert_eq!(gpt["capabilities"]["maxReferenceImages"], 16);
        assert_eq!(gpt["capabilities"]["sizes"], json!(["1K", "2K", "4K"]));
        assert!(gpt["capabilities"]["ratios"]
            .as_array()
            .unwrap()
            .contains(&json!("16:9")));
        assert_eq!(gpt["capabilities"]["maxCount"], 4);
        assert_eq!(
            by("grok-imagine-image-2.0")["capabilities"]["maxReferenceImages"],
            5
        );
    }

    #[test]
    fn idempotency_key_names_one_origin_and_labels_are_restricted() {
        assert_eq!(origin_for(None, None).unwrap(), "cli/cli");
        assert_eq!(
            origin_for(Some("hypit"), Some("run1:node-2")).unwrap(),
            "cli/hypit/run1:node-2"
        );
        assert!(origin_for(Some("../x"), None).is_err());
        assert!(origin_for(Some("hypit"), Some("a/b")).is_err());
        assert!(origin_for(Some("hypit"), Some("")).is_err());
    }

    #[test]
    fn an_idempotency_key_resubmits_only_after_a_definite_refusal() {
        let task = |id: &str, status: &str, state: Value, resume: bool| -> MediaTask {
            serde_json::from_value(json!({"id":id,"providerId":"p","model":"m","kind":"video",
                "status":status,"createdAt":"now","error":null,"remoteId":null,"outputs":[],
                "canResume":resume,"submissionState":state}))
            .unwrap()
        };
        assert!(submission_for_key(vec![]).is_none());
        assert!(submission_for_key(vec![task("refused", "failed", json!("rejected"), false)]).is_none());
        for kept in [
            task("running", "running", json!(null), false),
            task("done", "succeeded", json!(null), false),
            task("uncertain", "failed", json!("uncertain"), false),
            task("failed-after-accept", "failed", json!(null), false),
            task("lost", "failed", json!(null), true),
        ] {
            let id = kept.id.clone();
            assert_eq!(submission_for_key(vec![kept]).map(|t| t.id), Some(id));
        }
        // A retry after a refusal is itself found by the key next time.
        let found = submission_for_key(vec![
            task("refused", "failed", json!("rejected"), false),
            task("retry", "running", json!(null), false),
        ]);
        assert_eq!(found.map(|t| t.id).as_deref(), Some("retry"));
    }
    #[test]
    fn status_can_lookup_a_key_without_submitting_or_resuming() {
        let args = |argv: &[&str]| parse_args(argv.iter().map(|value| value.to_string())).unwrap();
        assert!(matches!(status_operation(&args(&["id", "--resume"])), Ok(Op::Status { resume: true, .. })));
        let op = status_operation(&args(&["--source", "dsvideo", "--idempotency-key", "run:1"])).unwrap();
        assert!(matches!(op, Op::Lookup { source, idempotency_key } if source == "dsvideo" && idempotency_key == "run:1"));
        for argv in [
            vec!["--source", "dsvideo"],
            vec!["--idempotency-key", "run:1"],
            vec!["id", "--source", "dsvideo", "--idempotency-key", "run:1"],
            vec!["--resume", "--source", "dsvideo", "--idempotency-key", "run:1"],
            vec!["--source", "../x", "--idempotency-key", "run:1"],
        ] { assert!(status_operation(&args(&argv)).is_err()); }
    }

    #[test]
    fn lookup_preserves_refused_and_uncertain_records_as_read_only_evidence() {
        let record = |id: &str, state: &str| -> MediaTask {
            serde_json::from_value(json!({"id":id,"providerId":"p","model":"m","kind":"video",
                "status":"failed","createdAt":"now","error":null,"remoteId":null,"outputs":[],
                "canResume":false,"submissionState":state})).unwrap()
        };
        assert!(lookup_task(vec![]).is_none());
        assert_eq!(lookup_task(vec![record("refused", "rejected")]).unwrap().id, "refused");
        assert_eq!(lookup_task(vec![record("refused", "rejected"), record("paid", "uncertain")]).unwrap().id, "paid");
    }

    #[test]
    fn submit_rejects_unknown_fields() {
        let unknown = r#"{"op":"submit","kind":"image","prompt":"p","apiKey":"x"}"#;
        assert!(serde_json::from_str::<Op>(unknown).is_err());
        assert!(matches!(serde_json::from_str::<Op>(r#"{"op":"status","id":"x"}"#).unwrap(), Op::Status { .. }));
    }

    fn submit_for(kind: MediaKind, argv: &[&str], stdin: &str) -> Result<Submit, String> {
        let args = parse_args(argv.iter().map(|s| s.to_string()))?;
        build_submit(kind, &args, Path::new("/work"), &mut stdin.as_bytes())
    }

    #[test]
    fn image_arguments_map_to_the_shared_request_with_absolute_paths() {
        let submit = submit_for(
            MediaKind::Image,
            &[
                "--prompt",
                " 白底主图 ",
                "--ref",
                "a.png",
                "--ref=/abs/b.png",
                "--ref",
                "https://x.test/c.png",
                "--ratio",
                "16:9",
                "--size",
                "2K",
                "--n",
                "2",
                "--model",
                "b/seedream",
                "--source",
                "hypit",
            ],
            "",
        )
        .unwrap();
        assert_eq!(submit.prompt, "白底主图");
        assert_eq!(
            submit.images,
            vec!["/work/a.png", "/abs/b.png", "https://x.test/c.png"]
        );
        assert_eq!(submit.options["size"], "2K");
        assert_eq!(submit.options["n"], 2);
        assert_eq!(submit.model.as_deref(), Some("b/seedream"));
        assert!(
            submit_for(MediaKind::Image, &["--prompt", "p", "--duration", "5"], "")
                .unwrap_err()
                .contains("--duration")
        );
        assert!(submit_for(MediaKind::Image, &["--prompt", "p", "--n", "two"], "").is_err());
    }

    #[test]
    fn video_arguments_use_explicit_frame_and_reference_fields() {
        let submit = submit_for(
            MediaKind::Video,
            &[
                "--prompt-file",
                "-",
                "--first-frame",
                "f.png",
                "--ref",
                "r.png",
                "--ref-video",
                "/v.mp4",
                "--duration",
                "5",
                "--resolution",
                "720p",
                "--ratio",
                "9:16",
                "--audio",
                "--idempotency-key",
                "k1",
            ],
            "镜头推进\n",
        )
        .unwrap();
        assert_eq!(submit.prompt, "镜头推进");
        assert!(submit.images.is_empty());
        assert_eq!(submit.options["firstFrame"], "/work/f.png");
        assert_eq!(submit.options["referenceImages"], json!(["/work/r.png"]));
        assert_eq!(submit.options["referenceVideos"], json!(["/v.mp4"]));
        assert_eq!(submit.options["duration"], 5);
        assert_eq!(submit.options["generateAudio"], true);
        assert_eq!(submit.idempotency_key.as_deref(), Some("k1"));
        let merged = submit_for(
            MediaKind::Video,
            &["--prompt", "p", "--options-json", r#"{"voiceIds":["a"]}"#],
            "",
        )
        .unwrap();
        assert_eq!(merged.options["voiceIds"], json!(["a"]));
        assert!(submit_for(
            MediaKind::Video,
            &[
                "--prompt",
                "p",
                "--duration",
                "5",
                "--options-json",
                r#"{"duration":6}"#
            ],
            ""
        )
        .is_err());
        assert!(submit_for(
            MediaKind::Video,
            &["--prompt", "p", "--prompt-file", "x"],
            ""
        )
        .is_err());
        assert!(submit_for(MediaKind::Video, &[], "")
            .unwrap_err()
            .contains("--prompt"));
    }

    #[test]
    fn adapter_can_explicitly_disable_audio_through_options_json() {
        let submit = submit_for(
            MediaKind::Video,
            &[
                "--prompt",
                "p",
                "--options-json",
                r#"{"generateAudio":false}"#,
            ],
            "",
        )
        .unwrap();
        assert_eq!(submit.options["generateAudio"], false);
        let mut options = json!(submit.options);
        options["prompt"] = json!(submit.prompt);
        let input: super::super::video_providers::VideoInput =
            serde_json::from_value(options).unwrap();
        assert_eq!(input.generate_audio, Some(false));
        assert!(submit_for(
            MediaKind::Video,
            &[
                "--prompt",
                "p",
                "--audio",
                "--options-json",
                r#"{"generateAudio":false}"#
            ],
            ""
        )
        .is_err());
    }

    fn task(
        status: MediaStatus,
        state: Option<MediaSubmissionState>,
        remote: Option<&str>,
    ) -> MediaTask {
        MediaTask {
            id: "t".into(),
            provider_id: "p".into(),
            model: "m".into(),
            kind: MediaKind::Video,
            status,
            created_at: String::new(),
            error: None,
            remote_id: remote.map(str::to_owned),
            outputs: vec![],
            can_resume: false,
            submission_state: state,
            origin: None,
            prompt: String::new(),
            result: None,
            request_hash: None,
            cancellation: None,
        }
    }

    #[test]
    fn exit_codes_never_invite_resubmitting_an_unknown_paid_request() {
        assert_eq!(
            exit_code(&task(MediaStatus::Succeeded, None, Some("r"))),
            exit::OK
        );
        assert_eq!(exit_code(&task(MediaStatus::Running, None, None)), exit::OK);
        let mut pending = task(MediaStatus::Running, None, Some("paid-receipt"));
        pending.can_resume = true;
        pending.error = Some("查询等待达到上限，勿重新提交".into());
        assert_eq!(
            exit_code(&pending),
            exit::OK,
            "a resumable query must not fail the Hypit build"
        );
        assert_eq!(
            exit_code(&task(
                MediaStatus::Failed,
                Some(MediaSubmissionState::Rejected),
                None
            )),
            exit::REJECTED
        );
        assert_eq!(
            exit_code(&task(
                MediaStatus::Failed,
                Some(MediaSubmissionState::Uncertain),
                None
            )),
            exit::UNCERTAIN
        );
        assert_eq!(
            exit_code(&task(MediaStatus::Failed, None, None)),
            exit::UNCERTAIN
        );
        assert_eq!(
            exit_code(&task(MediaStatus::Failed, None, Some("r"))),
            exit::FAILED
        );
        let mut saved_speech_receipt = task(MediaStatus::Failed, None, None);
        saved_speech_receipt.kind = MediaKind::Speech;
        saved_speech_receipt.can_resume = true;
        assert_eq!(exit_code(&saved_speech_receipt), exit::FAILED);
    }

    #[test]
    fn canonical_options_only_accepts_required_inputs_without_losing_boundary_fields() {
        let image = submit_for(MediaKind::Image, &["--options-json",
            r#"{"prompt":"icon","images":[{"source":"ref.png","attributes":{}}],"n":2}"#], "").unwrap();
        assert_eq!(image.prompt, "icon");
        assert_eq!(image.images, ["/work/ref.png"]);
        assert!(!image.options.contains_key("prompt") && !image.options.contains_key("images"));
        let speech = submit_for(MediaKind::Speech, &["--options-json",
            r#"{"text":"  hello\n","voice":"alloy","speed":0.75}"#], "").unwrap();
        assert_eq!(speech.options["text"], "  hello\n");
        let transcribe = submit_for(MediaKind::Transcribe, &["--options-json",
            r#"{"audioFile":"audio.wav","language":"zh","timestamps":"word"}"#], "").unwrap();
        assert_eq!(transcribe.options["audioFile"], "/work/audio.wav");
        for (kind, argv, path) in [
            (MediaKind::Image, vec!["--prompt", "icon", "--options-json", r#"{"prompt":"another"}"#], "prompt"),
            (MediaKind::Image, vec!["--prompt", "icon", "--ref", "ref.png", "--options-json", r#"{"images":[]}"#], "images"),
            (MediaKind::Transcribe, vec!["audio.wav", "--language", "zh", "--options-json", r#"{"audioFile":"another.wav"}"#], "audioFile"),
        ] {
            let error: Value = serde_json::from_str(&submit_for(kind, &argv, "").unwrap_err()).unwrap();
            assert_eq!(error["code"], "MODEL_ARGUMENT_DUPLICATE");
            assert_eq!(error["argumentPath"], path);
        }
    }

    #[test]
    fn speech_preserves_utf8_bytes_and_rejects_ambiguous_inputs() {
        let text = " \t欢迎\r\n😀  ";
        let submit = submit_for(MediaKind::Speech, &["--text-file", "-", "--voice", "alloy"], text).unwrap();
        assert_eq!(submit.options["text"], text);
        assert_eq!(submit.prompt, "");
        for argv in [
            vec!["--text", text, "--text-file", "-", "--voice", "alloy"],
            vec!["--text", text, "--voice", "alloy", "--voice", "echo"],
            vec!["--text", text, "--voice", "alloy", "--instruction-file", "-"],
            vec!["--text", text, "--mode", "clone", "--voice-ref", "-", "--consent-attestation", "consent.txt"],
            vec!["--text", text, "--voice", "alloy", "--options-json", r#"{"text":"changed"}"#],
            vec!["--text", text, "--voice", "alloy", "--options-json", r#"{"outputFormat":"wav","output_format":"mp3"}"#],
            vec!["--text", " \n ", "--voice", "alloy"],
            vec!["--text", text, "--voice", "alloy", "--no-wait", "--no-wait"],
        ] {
            assert!(submit_for(MediaKind::Speech, &argv, "").is_err(), "{argv:?}");
        }
        let clone = submit_for(MediaKind::Speech, &["--text", text, "--mode", "clone", "--voice-ref", "voice.wav", "--consent-attestation", "consent.txt"], "").unwrap();
        assert_eq!(clone.options["voiceReference"][0]["source"], "/work/voice.wav");
        assert_eq!(clone.options["consentAttestation"], "/work/consent.txt");
    }

    #[test]
    fn transcribe_requires_explicit_language_and_safe_sample_count() {
        for argv in [
            vec!["audio.wav"],
            vec!["audio.wav", "--language", "und"],
            vec!["audio.wav", "--language", "auto"],
            vec!["audio.wav", "--language", "EN"],
            vec!["audio.wav", "--language", "en", "--sample-frames", "0"],
            vec!["audio.wav", "--language", "en", "--sample-frames", "9007199254740992"],
            vec!["audio.wav", "--language", "en", "--timestamps", "sentence"],
            vec!["audio.wav", "extra.wav", "--language", "en"],
            vec!["-", "--language", "en"],
        ] {
            assert!(submit_for(MediaKind::Transcribe, &argv, "").is_err(), "{argv:?}");
        }
        let submit = submit_for(MediaKind::Transcribe, &["audio.wav", "--language", "zh", "--sample-frames", "9007199254740991", "--timestamps", "segment"], "").unwrap();
        assert_eq!(submit.options["audioFile"], "/work/audio.wav");
        assert_eq!(submit.options["sampleFrames"], 9_007_199_254_740_991u64);
        assert_eq!(submit.options["timestamps"], "segment");
    }

    #[test]
    fn cancelled_task_is_terminal_and_has_a_distinct_exit_code() {
        let cancelled = task(MediaStatus::Cancelled, None, None);
        assert_eq!(exit_code(&cancelled), exit::CANCELLED);
        let (observed, timed_out) = wait_for(cancelled, Duration::ZERO).unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(observed.status, MediaStatus::Cancelled);
        assert!(!timed_out);
        assert_eq!(finish(observed, false, None).unwrap_or_else(|e| panic!("{}", e.message)), 7);
    }

    #[test]
    fn idempotency_conflicts_on_changed_text_audio_or_consent_even_after_refusal() {
        let dir = std::env::temp_dir().join(format!("cli-hash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("audio.wav");
        let consent = dir.join("consent.txt");
        std::fs::write(&audio, b"original sample").unwrap();
        std::fs::write(&consent, b"I authorize this sample").unwrap();
        let mut request = MediaRequest {
            provider_id: "p".into(), model: "m".into(), kind: MediaKind::Speech,
            prompt: String::new(), images: vec![],
            options: serde_json::from_value(json!({"text":"hello","mode":"clone",
                "voiceReference":[{"source":audio,"attributes":{}}],"consentAttestation":consent})).unwrap(),
            origin: Some("cli/test/key".into()), description_revision: None,
        };
        let hash = super::super::request_hash(&request).unwrap();
        let mut existing = task(MediaStatus::Failed, Some(MediaSubmissionState::Rejected), None);
        existing.request_hash = Some(hash.clone());
        assert!(validate_existing_hashes(&[existing.clone()], &hash).is_ok());
        request.options.insert("text".into(), json!("changed"));
        assert!(validate_existing_hashes(&[existing.clone()], &super::super::request_hash(&request).unwrap()).is_err());
        request.options.insert("text".into(), json!("hello"));
        std::fs::write(&audio, b"different sample").unwrap();
        assert!(validate_existing_hashes(&[existing.clone()], &super::super::request_hash(&request).unwrap()).is_err());
        std::fs::write(&audio, b"original sample").unwrap();
        std::fs::write(&consent, b"Different authorization").unwrap();
        assert!(validate_existing_hashes(&[existing.clone()], &super::super::request_hash(&request).unwrap()).is_err());
        existing.request_hash = None;
        assert!(validate_existing_hashes(&[existing], &hash).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn launcher_quotes_the_app_path() {
        let (name, script) = shim_script(Path::new("/Applications/it's here/dsivio"));
        if cfg!(windows) {
            assert_eq!(name, "dsivio.cmd");
        } else {
            assert_eq!(name, "dsivio");
            assert_eq!(
                script,
                "#!/bin/sh\nexec '/Applications/it'\\''s here/dsivio' \"$@\"\n"
            );
        }
    }

    #[test]
    fn git_bash_launcher_uses_forward_slashes_and_quotes_the_app_path() {
        assert_eq!(
            git_bash_shim_script(Path::new(r"C:\Users\it's me\AppData\Local\dsivio\dsivio.exe")),
            "#!/bin/sh\nexec 'C:/Users/it'\\''s me/AppData/Local/dsivio/dsivio.exe' \"$@\"\n"
        );
    }

    fn local_asr(languages: &[&str], auto_install: bool) -> crate::settings::LocalAsrConfig {
        crate::settings::LocalAsrConfig {
            model: "small".into(),
            languages: languages.iter().map(|l| l.to_string()).collect(),
            auto_install,
        }
    }

    #[test]
    fn asr_install_uses_the_configuration_transcription_will_check() {
        let configured = local_asr(&["zh", "en", "pt"], true);
        // Bare `asr install` used to install en+zh, which transcription then rejected and reinstalled.
        assert_eq!(asr_install_config(&configured, None, vec![]).unwrap(), configured);
        assert_eq!(
            asr_install_config(&configured, Some("small".into()), vec!["pt".into(), "en".into(), "zh".into()]).unwrap(),
            configured
        );
        let error = asr_install_config(&configured, None, vec!["en".into(), "zh".into()]).unwrap_err();
        assert!(error.contains("设置 > 媒体创作 > 转写") && error.contains("zh"), "{error}");
        assert!(asr_install_config(&configured, Some("large".into()), vec![]).is_err());
    }

    #[test]
    fn asr_status_reports_settings_and_what_the_first_transcription_will_do() {
        let status = |state: &str, languages: &[&str]| json!({"state": state, "languages": languages});
        let automatic = asr_status_reply(status("notInstalled", &["en", "zh"]), &local_asr(&["zh", "en", "pt"], true));
        assert_eq!(automatic["settings"], json!({"model": "small", "languages": ["zh", "en", "pt"], "autoInstall": true}));
        let note = automatic["note"].as_str().unwrap();
        assert!(note.contains("自动安装") && note.contains("GB"), "{note}");

        let manual = asr_status_reply(status("notInstalled", &["en", "zh"]), &local_asr(&["zh"], false));
        assert!(manual["note"].as_str().unwrap().contains("asr install"));

        let changed = asr_status_reply(status("ready", &["en", "zh"]), &local_asr(&["zh", "en", "pt"], true));
        assert!(changed["note"].as_str().unwrap().contains("重新安装"));

        let ready = asr_status_reply(status("ready", &["zh", "en"]), &local_asr(&["en", "zh"], true));
        assert!(ready.get("note").is_none());
    }

}
