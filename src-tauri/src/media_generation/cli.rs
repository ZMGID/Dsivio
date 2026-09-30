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
use std::io::{BufRead, BufReader, Read, Write};
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
    pub const UNAVAILABLE: u8 = 6;
    /// Still running when the wait ended; continue with `wait <id>`.
    pub const TIMEOUT: u8 = 124;
}

const MAX_LINE: u64 = 4 * 1024 * 1024;
const POLL: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Wire format

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    token: String,
    #[serde(flatten)]
    op: Op,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Op {
    Models {
        #[serde(default)]
        kind: Option<MediaKind>,
    },
    Submit(Submit),
    Status {
        id: String,
        #[serde(default)]
        resume: bool,
    },
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

#[derive(Debug, Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
    pid: u32,
}

fn endpoint_path() -> Option<PathBuf> {
    crate::app_data::app_data_dir().map(|dir| dir.join("run").join("media-cli.json"))
}

// ---------------------------------------------------------------------------
// App side

/// Accept CLI requests for the lifetime of the App. Failure only disables the CLI.
pub fn serve(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve_inner(app).await {
            eprintln!("dsivio media CLI unavailable: {error}");
        }
    });
}

async fn serve_inner(app: tauri::AppHandle) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token: String = rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let path = endpoint_path().ok_or("无法定位应用数据目录")?;
    write_private(
        &path,
        &serde_json::to_vec(&Endpoint {
            port,
            token: token.clone(),
            pid: std::process::id(),
        })
        .map_err(|e| e.to_string())?,
    )?;
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            let _ = handle_connection(&app, stream, &token).await;
        });
    }
}

async fn handle_connection(
    app: &tauri::AppHandle,
    stream: tokio::net::TcpStream,
    token: &str,
) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    tokio::io::BufReader::new(read.take(MAX_LINE))
        .read_line(&mut line)
        .await?;
    let reply = match authorize(&line, token) {
        Ok(op) => dispatch(app, op).await,
        Err(reply) => reply,
    };
    let mut bytes = serde_json::to_vec(&reply).unwrap_or_default();
    bytes.push(b'\n');
    write.write_all(&bytes).await?;
    write.shutdown().await
}

fn authorize(line: &str, token: &str) -> Result<Op, Reply> {
    let envelope: Envelope = serde_json::from_str(line.trim())
        .map_err(|e| Reply::err(ErrorCode::Invalid, format!("请求格式无效：{e}")))?;
    if !constant_time_eq(envelope.token.as_bytes(), token.as_bytes()) {
        return Err(Reply::err(
            ErrorCode::Unauthorized,
            "连接凭据已失效，请重新打开 Dsivio",
        ));
    }
    Ok(envelope.op)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn dispatch(app: &tauri::AppHandle, op: Op) -> Reply {
    use tauri::Manager;
    match op {
        Op::Models { kind } => {
            let settings = app
                .state::<crate::state::AppState>()
                .settings_read()
                .clone();
            Reply::ok(json!(list_models(&settings, kind.as_ref())))
        }
        Op::Status { id, resume } => {
            if uuid::Uuid::parse_str(&id).is_err() {
                return Reply::err(ErrorCode::Invalid, format!("任务 ID 格式无效：{id}"));
            }
            match super::get_media_task(app.clone(), id.clone(), Some(resume)) {
                Ok(task) => Reply::ok(json!(task)),
                Err(error) => Reply::err(ErrorCode::Invalid, format!("找不到任务 {id}（{error}）")),
            }
        }
        Op::Submit(submit) => match submit_request(app, submit).await {
            Ok(task) => Reply::ok(json!(task)),
            Err(error) => Reply::err(ErrorCode::Invalid, error),
        },
    }
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
    if let Some(task) = submission_for_key(existing) {
        return Ok(task);
    }
    super::start_media_generation(app.clone(), request).await
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
}

fn capabilities_of(
    settings: &crate::settings::Settings,
    kind: &MediaKind,
    provider_id: &str,
    model: &str,
) -> Option<Value> {
    let provider = settings.get_provider(provider_id)?;
    match kind {
        MediaKind::Video => super::video_providers::capabilities(provider, model).map(|c| json!(c)),
        MediaKind::Image => Some(json!(super::image_providers::image_capabilities(
            provider, model
        ))),
    }
}

fn pool<'a>(
    settings: &'a crate::settings::Settings,
    kind: &MediaKind,
) -> &'a [crate::settings::DefaultModelSelection] {
    match kind {
        MediaKind::Image => &settings.workbench_media.image_models,
        MediaKind::Video => &settings.workbench_media.video_models,
    }
}

/// A pool member can be submitted right now: its provider is enabled and still lists the model.
/// This is the rule `start` enforces, so the default never points at something that would be refused.
fn usable(
    settings: &crate::settings::Settings,
    entry: &crate::settings::DefaultModelSelection,
) -> bool {
    settings
        .providers
        .iter()
        .any(|p| p.id == entry.provider_id && p.enabled && p.enabled_models.contains(&entry.model))
}

fn list_models(settings: &crate::settings::Settings, kind: Option<&MediaKind>) -> Vec<ModelEntry> {
    let kinds = match kind {
        Some(kind) => vec![kind.clone()],
        None => vec![MediaKind::Image, MediaKind::Video],
    };
    kinds
        .into_iter()
        .flat_map(|kind| {
            pool(settings, &kind)
                .iter()
                .filter(|entry| usable(settings, entry))
                .enumerate()
                .map(|(index, entry)| {
                    let capabilities =
                        capabilities_of(settings, &kind, &entry.provider_id, &entry.model);
                    ModelEntry {
                        known: capabilities.is_some(),
                        capabilities,
                        kind: kind.clone(),
                        id: format!("{}/{}", entry.provider_id, entry.model),
                        provider_id: entry.provider_id.clone(),
                        provider_name: settings
                            .get_provider(&entry.provider_id)
                            .map(|p| {
                                if p.name.trim().is_empty() {
                                    p.id.clone()
                                } else {
                                    p.name.clone()
                                }
                            })
                            .unwrap_or_else(|| entry.provider_id.clone()),
                        model: entry.model.clone(),
                        default: index == 0,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Only models enabled under 设置 > 媒体创作 are usable. No model means the pool's first entry.
fn resolve_model(
    settings: &crate::settings::Settings,
    kind: &MediaKind,
    wanted: Option<&str>,
) -> Result<(String, String), String> {
    let label = if *kind == MediaKind::Image {
        "图片"
    } else {
        "视频"
    };
    let members: Vec<_> = pool(settings, kind)
        .iter()
        .filter(|entry| usable(settings, entry))
        .cloned()
        .collect();
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

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
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

/// Put `dsivio` on this process's PATH (inherited by chat shell, MCP and plugin processes) and
/// keep the launcher pointing at the App that is running now.
pub fn install_launcher() {
    let (Some(dir), Ok(exe)) = (shim_dir(), std::env::current_exe()) else {
        return;
    };
    let (name, script) = shim_script(&exe);
    let path = dir.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(script.as_str()) {
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &script));
        #[cfg(unix)]
        if written.is_ok() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        if let Err(error) = written {
            eprintln!("dsivio launcher not installed: {error}");
            return;
        }
    }
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
dsivio media —— 用 Dsivio「设置 > 媒体创作」里开启的模型生成图片、视频

  dsivio media models [--kind image|video]
  dsivio media image  (--prompt <文本> | --prompt-file <文件|->) [--model <供应商/模型>]
                      [--ref <图片>]... [--ratio 16:9] [--size 2K] [--quality high] [--n 1]
  dsivio media video  (--prompt <文本> | --prompt-file <文件|->) [--model <供应商/模型>]
                      [--first-frame <图片>] [--last-frame <图片>] [--ref <图片>]...
                      [--ref-video <文件>]... [--ref-audio <文件>]...
                      [--duration 5] [--resolution 720p] [--ratio 9:16] [--audio]
  dsivio media status <任务ID> [--resume]
  dsivio media wait   <任务ID> [--timeout <秒>]

image / video 通用：
  --no-wait                  提交后立即返回任务 ID
  --timeout <秒>             等待上限（图片默认 600，视频默认 1800）
  --idempotency-key <key>    同一 key 只提交一次，重复调用返回同一个任务
  --source <名称>            记录调用来源，例如 hypit
  --options-json <JSON>      额外参数，原样合并进请求
  --out <目录>               成功后把结果复制到该目录

stdout 只输出 JSON。退出码：0 成功，2 参数错误/模型未开启（未提交），3 服务拒绝（未扣费），
4 提交后失败，5 提交结果不确定（不要重交，用 status 查询），6 Dsivio 未运行，124 等待超时。
";

struct Args {
    values: BTreeMap<String, Vec<String>>,
    flags: Vec<String>,
    positional: Vec<String>,
}

const BOOLEAN_FLAGS: &[&str] = &["no-wait", "audio", "resume", "help"];

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
            parsed
                .values
                .entry(name.into())
                .or_default()
                .push(value.into());
        } else if BOOLEAN_FLAGS.contains(&name) {
            parsed.flags.push(name.into());
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
            if !allowed.contains(&name.as_str()) && name != "help" {
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
    "prompt",
    "prompt-file",
    "model",
    "no-wait",
    "timeout",
    "idempotency-key",
    "source",
    "options-json",
    "out",
];

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
            let allowed = [SUBMIT_FLAGS, &["ref", "ratio", "size", "quality", "n"]].concat();
            args.only(&allowed)?;
            images = local(args.many("ref"));
            for (flag, key) in [
                ("ratio", "aspect_ratio"),
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
                    "first-frame",
                    "last-frame",
                    "ref",
                    "ref-video",
                    "ref-audio",
                    "duration",
                    "resolution",
                    "ratio",
                    "audio",
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
            if let Some(duration) = args.number("duration")? {
                options.insert("duration".into(), json!(duration));
            }
            if args.flag("audio") {
                options.insert("generateAudio".into(), json!(true));
            }
        }
    }
    if let Some(extra) = args.one("options-json")? {
        let Value::Object(extra) = serde_json::from_str(extra)
            .map_err(|e| format!("--options-json 不是有效 JSON：{e}"))?
        else {
            return Err("--options-json 必须是 JSON 对象".into());
        };
        for (key, value) in extra {
            if options.insert(key.clone(), value).is_some() {
                return Err(format!("{key} 同时出现在参数和 --options-json 里"));
            }
        }
    }
    Ok(Submit {
        kind: Some(kind),
        model: args.one("model")?.map(str::to_owned),
        prompt: read_prompt(args, cwd, stdin)?,
        images,
        options: options.into_iter().collect(),
        source: args.one("source")?.map(str::to_owned),
        idempotency_key: args.one("idempotency-key")?.map(str::to_owned),
    })
}

fn exit_code(task: &MediaTask) -> u8 {
    match task.status {
        MediaStatus::Succeeded => exit::OK,
        MediaStatus::Running => exit::OK,
        MediaStatus::Failed => match task.submission_state {
            Some(MediaSubmissionState::Rejected) => exit::REJECTED,
            Some(MediaSubmissionState::Uncertain) => exit::UNCERTAIN,
            // No receipt and no explicit rejection: conservatively unknown, never resubmit.
            None if task.remote_id.is_none() && task.outputs.is_empty() => exit::UNCERTAIN,
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
    let unavailable = |detail: String| Failure {
        code: exit::UNAVAILABLE,
        message: format!("连接不到 Dsivio，请先打开 Dsivio 再重试（{detail}）"),
    };
    let path = endpoint_path().ok_or_else(|| unavailable("无法定位应用数据目录".into()))?;
    let endpoint: Endpoint = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| unavailable("未找到连接信息".into()))?;
    let reply = exchange(
        endpoint.port,
        &Envelope {
            token: endpoint.token,
            op,
        },
    )
    .map_err(unavailable)?;
    match reply {
        Reply::Ok { result, .. } => Ok(result),
        Reply::Err {
            code: ErrorCode::Unauthorized,
            error,
            ..
        } => Err(Failure {
            code: exit::UNAVAILABLE,
            message: error,
        }),
        Reply::Err {
            code: ErrorCode::Invalid,
            error,
            ..
        } => Err(Failure::invalid(error)),
        Reply::Err {
            code: ErrorCode::Internal,
            error,
            ..
        } => Err(Failure {
            code: exit::INTERNAL,
            message: error,
        }),
    }
}

fn exchange(port: u16, envelope: &Envelope) -> Result<Reply, String> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let mut bytes = serde_json::to_vec(envelope).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    stream.write_all(&bytes).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE))
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| format!("Dsivio 响应无效：{e}"))
}

fn task_from(value: Value) -> Result<MediaTask, Failure> {
    serde_json::from_value(value).map_err(|e| Failure {
        code: exit::INTERNAL,
        message: format!("任务格式无效：{e}"),
    })
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
    match command {
        "models" => {
            args.only(&["kind"]).map_err(Failure::invalid)?;
            let kind = match args.one("kind").map_err(Failure::invalid)? {
                None => None,
                Some("image") => Some(MediaKind::Image),
                Some("video") => Some(MediaKind::Video),
                Some(other) => {
                    return Err(Failure::invalid(format!(
                        "--kind 只能是 image 或 video，收到 {other}"
                    )))
                }
            };
            print_json(&call(Op::Models { kind })?);
            Ok(exit::OK)
        }
        "image" | "video" => {
            let kind = if command == "image" {
                MediaKind::Image
            } else {
                MediaKind::Video
            };
            let default_timeout = if kind == MediaKind::Image { 600 } else { 1800 };
            let submit =
                build_submit(kind, &args, &cwd, &mut std::io::stdin()).map_err(Failure::invalid)?;
            if !args.positional.is_empty() {
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
            let (task, timed_out) = wait_for(task, timeout(default_timeout)?)?;
            finish(task, timed_out, out)
        }
        "status" => {
            args.only(&["resume"]).map_err(Failure::invalid)?;
            let task = status(&single_id(&args)?, args.flag("resume"))?;
            finish(task, false, None)
        }
        "wait" => {
            args.only(&["timeout", "out"]).map_err(Failure::invalid)?;
            let out = args
                .one("out")
                .map_err(Failure::invalid)?
                .map(|dir| PathBuf::from(absolute(dir, &cwd)));
            let (task, timed_out) = wait_for(status(&single_id(&args)?, false)?, timeout(1800)?)?;
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
        eprintln!("{}", failure.message);
        print_json(&json!({ "error": failure.message, "exitCode": failure.code }));
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
        assert_eq!(list_models(&settings, None).len(), 4);
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
    fn requests_need_the_current_token() {
        let line = serde_json::to_string(&Envelope {
            token: "right".into(),
            op: Op::Status {
                id: "x".into(),
                resume: false,
            },
        })
        .unwrap();
        assert!(matches!(authorize(&line, "right"), Ok(Op::Status { .. })));
        assert!(matches!(
            authorize(&line, "wrong"),
            Err(Reply::Err {
                code: ErrorCode::Unauthorized,
                ..
            })
        ));
        assert!(matches!(
            authorize("not json", "right"),
            Err(Reply::Err {
                code: ErrorCode::Invalid,
                ..
            })
        ));
        let unknown = r#"{"token":"right","op":"submit","kind":"image","prompt":"p","apiKey":"x"}"#;
        assert!(authorize(unknown, "right").is_err());
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
        assert_eq!(submit.options["aspect_ratio"], "16:9");
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
    fn client_and_server_share_one_line_framing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let reply = match authorize(&line, "secret") {
                Ok(_) => Reply::ok(json!({"seen": true})),
                Err(reply) => reply,
            };
            let mut stream = stream;
            stream
                .write_all(format!("{}\n", serde_json::to_string(&reply).unwrap()).as_bytes())
                .unwrap();
        });
        let reply = exchange(
            port,
            &Envelope {
                token: "secret".into(),
                op: Op::Models { kind: None },
            },
        )
        .unwrap();
        server.join().unwrap();
        assert!(matches!(reply, Reply::Ok { result, .. } if result["seen"] == true));
    }
}
