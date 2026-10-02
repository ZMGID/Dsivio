//! Loopback transport shared by `dsivio media`, `dsivio ai`, and later commerce/publish.
//!
//! The client never reads provider settings. One JSON line goes to the running App; the App
//! dispatches on `service` and returns one JSON line. The endpoint file is
//! `<app_data>/run/media-cli.json`.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use std::process::ExitCode;

pub const AI_SUBCOMMAND: &str = "ai";

pub mod exit {
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
    /// The task was confirmed cancelled.
    pub const CANCELLED: u8 = 7;
    /// Still running when the wait ended.
    pub const TIMEOUT: u8 = 124;
}

#[derive(Debug)]
pub struct CliFailure {
    pub code: u8,
    pub message: String,
}

impl CliFailure {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self { code: exit::INVALID, message: message.into() }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self { code: exit::INTERNAL, message: message.into() }
    }
    fn unavailable(message: impl Into<String>) -> Self {
        Self { code: exit::UNAVAILABLE, message: message.into() }
    }
}

const MAX_LINE: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    token: String,
    service: String,
    op: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ErrorCode {
    Invalid,
    Unauthorized,
    Internal,
    Rejected,
    Failed,
    Uncertain,
    Cancelled,
    Timeout,
}

fn error_code_for(code: u8) -> ErrorCode {
    match code {
        exit::INTERNAL => ErrorCode::Internal,
        exit::REJECTED => ErrorCode::Rejected,
        exit::FAILED => ErrorCode::Failed,
        exit::UNCERTAIN => ErrorCode::Uncertain,
        exit::UNAVAILABLE => ErrorCode::Unauthorized,
        exit::CANCELLED => ErrorCode::Cancelled,
        exit::TIMEOUT => ErrorCode::Timeout,
        _ => ErrorCode::Invalid,
    }
}

fn exit_for(code: ErrorCode) -> u8 {
    match code {
        ErrorCode::Internal => exit::INTERNAL,
        ErrorCode::Rejected => exit::REJECTED,
        ErrorCode::Failed => exit::FAILED,
        ErrorCode::Uncertain => exit::UNCERTAIN,
        ErrorCode::Unauthorized => exit::UNAVAILABLE,
        ErrorCode::Cancelled => exit::CANCELLED,
        ErrorCode::Timeout => exit::TIMEOUT,
        ErrorCode::Invalid => exit::INVALID,
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Reply {
    Ok { ok: bool, result: Value },
    Err { ok: bool, code: ErrorCode, error: String },
}

impl Reply {
    fn ok(result: Value) -> Self {
        Reply::Ok { ok: true, result }
    }
    fn err(code: ErrorCode, error: impl Into<String>) -> Self {
        Reply::Err { ok: false, code, error: error.into() }
    }
    fn from_failure(failure: CliFailure) -> Self {
        Reply::err(error_code_for(failure.code), failure.message)
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

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("无效路径")?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let temp = dir.join(format!(".{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|error| error.to_string())?;
    drop(file);
    std::fs::rename(&temp, path).map_err(|error| error.to_string())
}

/// Accept CLI requests for the lifetime of the App. Failure only disables the CLI.
pub fn serve(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve_inner(app).await {
            eprintln!("dsivio CLI unavailable: {error}");
        }
    });
}

async fn serve_inner(app: tauri::AppHandle) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let token: String = rand::random::<[u8; 32]>().iter().map(|byte| format!("{byte:02x}")).collect();
    let path = endpoint_path().ok_or("无法定位应用数据目录")?;
    write_private(
        &path,
        &serde_json::to_vec(&Endpoint { port, token: token.clone(), pid: std::process::id() }).map_err(|error| error.to_string())?,
    )?;
    loop {
        let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            let _ = handle_connection(Some(app), stream, &token).await;
        });
    }
}

async fn handle_connection(
    app: Option<tauri::AppHandle>,
    stream: tokio::net::TcpStream,
    token: &str,
) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    tokio::io::BufReader::new(read.take(MAX_LINE)).read_line(&mut line).await?;
    let reply = match authorize(&line, token) {
        Ok((service, op)) => match dispatch(app.as_ref(), &service, op).await {
            Ok(result) => Reply::ok(result),
            Err(failure) => Reply::from_failure(failure),
        },
        Err(reply) => reply,
    };
    let mut bytes = serde_json::to_vec(&reply).unwrap_or_default();
    bytes.push(b'\n');
    write.write_all(&bytes).await?;
    write.shutdown().await
}

fn authorize(line: &str, token: &str) -> Result<(String, Value), Reply> {
    let envelope: Envelope = serde_json::from_str(line.trim())
        .map_err(|error| Reply::err(ErrorCode::Invalid, format!("请求格式无效：{error}")))?;
    if !constant_time_eq(envelope.token.as_bytes(), token.as_bytes()) {
        return Err(Reply::err(ErrorCode::Unauthorized, "连接凭据已失效，请重新打开 Dsivio"));
    }
    if envelope.service.is_empty() {
        return Err(Reply::err(ErrorCode::Invalid, "缺少 service"));
    }
    Ok((envelope.service, envelope.op))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Service arms. Handler failures become `CliFailure`; a missing App is always exit 1.
/// Commerce and publish `Ok` payloads keep status in the body. Only `Err` uses `error.exit_code()`.
async fn dispatch(app: Option<&tauri::AppHandle>, service: &str, op: Value) -> Result<Value, CliFailure> {
    match service {
        "media" => {
            let app = app.ok_or_else(|| CliFailure::internal("missing app"))?;
            crate::media_generation::cli::handle(app, op).await.map_err(CliFailure::invalid)
        }
        "ai" => {
            let app = app.ok_or_else(|| CliFailure::internal("missing app"))?;
            crate::chat::ai_task::cli_handle(app, op).await.map_err(CliFailure::invalid)
        }
        "commerce" => {
            let app = app.ok_or_else(|| CliFailure::internal("missing app"))?;
            crate::workbench::commerce::handle(app, op).await.map_err(|error| CliFailure {
                code: error.exit_code(),
                message: error.to_string(),
            })
        }
        "publish" => {
            let app = app.ok_or_else(|| CliFailure::internal("missing app"))?;
            crate::workbench::publish::handle(app, op).await.map_err(|error| CliFailure {
                code: error.exit_code(),
                message: error.to_string(),
            })
        }
        other => Err(unknown_service(other)),
    }
}

fn unknown_service(service: &str) -> CliFailure {
    CliFailure::invalid(format!("unknown service: {service}"))
}

/// Client used by every `dsivio <service>` command.
pub async fn call(service: &str, op: Value, timeout: Duration) -> Result<Value, CliFailure> {
    let unavailable = |detail: String| {
        CliFailure::unavailable(format!("连接不到 Dsivio，请先打开 Dsivio 再重试（{detail}）"))
    };
    let path = endpoint_path().ok_or_else(|| unavailable("无法定位应用数据目录".into()))?;
    let endpoint: Endpoint = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| unavailable("未找到连接信息".into()))?;
    call_endpoint(endpoint.port, &endpoint.token, service, op, timeout)
        .await
        .map_err(|failure| {
            if failure.code == exit::UNAVAILABLE && !failure.message.contains("Dsivio") {
                unavailable(failure.message)
            } else {
                failure
            }
        })
}

async fn call_endpoint(
    port: u16,
    token: &str,
    service: &str,
    op: Value,
    timeout: Duration,
) -> Result<Value, CliFailure> {
    let timeout = timeout.max(Duration::from_millis(1));
    let exchange = async {
        let mut stream = tokio::time::timeout(
            timeout.min(Duration::from_secs(3)),
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)),
        )
        .await
        .map_err(|_| "连接超时".to_string())?
        .map_err(|error| error.to_string())?;
        let envelope = Envelope { token: token.to_owned(), service: service.to_owned(), op };
        let mut bytes = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tokio::time::timeout(timeout, stream.write_all(&bytes)).await.map_err(|_| "写入超时".to_string())?.map_err(|error| error.to_string())?;
        let mut line = String::new();
        tokio::time::timeout(timeout, stream.take(MAX_LINE).read_to_string(&mut line))
            .await
            .map_err(|_| "读取超时".to_string())?
            .map_err(|error| error.to_string())?;
        serde_json::from_str::<Reply>(line.trim()).map_err(|error| format!("Dsivio 响应无效：{error}"))
    };
    let reply = exchange.await.map_err(CliFailure::unavailable)?;
    match reply {
        Reply::Ok { result, .. } => Ok(result),
        Reply::Err { code, error, .. } => Err(CliFailure { code: exit_for(code), message: error }),
    }
}

const AI_HELP: &str = "\
dsivio ai —— 用正在运行的 Dsivio 执行一次工作台 AI 调用（不另开会话）

  dsivio ai run (--prompt <文本> | --prompt-file <文件|->)
                [--mode once|agent] [--system <文本>]
                [--image <图片>]... [--video <视频>]
                [--slot chat|vision|promptOptimize|videoAnalysis]
                [--model <供应商/模型>] [--timeout <秒>] [--json]

stdout 只输出 JSON。退出码：0 成功，2 参数错误，1 内部错误，6 Dsivio 未运行。
";

/// Entry for `dsivio ai ...`. Runs before any App initialization.
pub fn run_ai(args: impl Iterator<Item = std::ffi::OsString>) -> ExitCode {
    let argv: Result<Vec<String>, _> = args.map(|arg| arg.into_string()).collect();
    let result = match argv {
        Ok(argv) => run_ai_command(argv),
        Err(_) => Err(CliFailure::invalid("参数不是有效的 UTF-8")),
    };
    ExitCode::from(result.unwrap_or_else(|failure| {
        eprintln!("{}", failure.message);
        println!("{}", serde_json::to_string(&json!({"error": failure.message, "exitCode": failure.code})).unwrap_or_default());
        failure.code
    }))
}

fn run_ai_command(argv: Vec<String>) -> Result<u8, CliFailure> {
    let (command, rest) = match argv.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("help", &[][..]),
    };
    if command == "help" || command == "--help" || command == "-h" || rest.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{AI_HELP}");
        return Ok(exit::OK);
    }
    if command != "run" {
        return Err(CliFailure::invalid(format!("未知命令 {command}，运行 `dsivio ai help` 查看用法")));
    }
    let cwd = std::env::current_dir().map_err(|error| CliFailure::invalid(error.to_string()))?;
    let op = parse_ai_run(rest, &cwd, &mut std::io::stdin()).map_err(CliFailure::invalid)?;
    let timeout = ai_socket_timeout(&op);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliFailure::internal(error.to_string()))?;
    let value = runtime.block_on(call("ai", op, timeout))?;
    println!("{}", serde_json::to_string(&value).unwrap_or_default());
    Ok(exit::OK)
}

fn ai_socket_timeout(op: &Value) -> Duration {
    let default = if op["mode"].as_str() == Some("agent") { 630 } else { 90 };
    let secs = op["timeout"].as_u64().map(|value| value.saturating_add(30)).unwrap_or(default);
    Duration::from_secs(secs.max(1))
}

fn parse_ai_run(args: &[String], cwd: &Path, stdin: &mut dyn Read) -> Result<Value, String> {
    let mut prompt = None;
    let mut prompt_file = None;
    let mut mode = "once".to_string();
    let mut system = None;
    let mut images = Vec::new();
    let mut video = None;
    let mut slot = "chat".to_string();
    let mut model = None;
    let mut timeout = None;
    let mut iter = args.iter().cloned();
    while let Some(arg) = iter.next() {
        let Some(name) = arg.strip_prefix("--") else {
            return Err(format!("多余的参数：{arg}"));
        };
        let (name, inline) = name.split_once('=').map(|(name, value)| (name, Some(value.to_owned()))).unwrap_or((name, None));
        let value = |inline: Option<String>, iter: &mut std::iter::Cloned<std::slice::Iter<String>>| {
            inline.or_else(|| iter.next()).ok_or_else(|| format!("--{name} 缺少取值"))
        };
        match name {
            "json" => {
                if inline.is_some() {
                    return Err("--json 不接受取值".into());
                }
            }
            "prompt" => prompt = Some(value(inline, &mut iter)?),
            "prompt-file" => prompt_file = Some(value(inline, &mut iter)?),
            "mode" => mode = value(inline, &mut iter)?,
            "system" => system = Some(value(inline, &mut iter)?),
            "image" => images.push(absolute(&value(inline, &mut iter)?, cwd)),
            "video" => {
                if video.is_some() {
                    return Err("--video 只能出现一次".into());
                }
                video = Some(absolute(&value(inline, &mut iter)?, cwd));
            }
            "slot" => slot = value(inline, &mut iter)?,
            "model" => model = Some(value(inline, &mut iter)?),
            "timeout" => {
                let raw = value(inline, &mut iter)?;
                let parsed = raw.parse::<u64>().map_err(|_| "--timeout 需要正整数".to_string())?;
                if parsed == 0 || parsed > u64::from(u32::MAX) {
                    return Err("--timeout 需要正整数".into());
                }
                timeout = Some(parsed);
            }
            other => return Err(format!("不支持的参数 --{other}")),
        }
    }
    if prompt.is_some() && prompt_file.is_some() {
        return Err("--prompt 和 --prompt-file 只能用一个".into());
    }
    let prompt = match (prompt, prompt_file) {
        (Some(text), None) => text,
        (None, Some(file)) if file == "-" => {
            let mut text = String::new();
            stdin.read_to_string(&mut text).map_err(|error| format!("读取标准输入失败：{error}"))?;
            text
        }
        (None, Some(file)) => std::fs::read_to_string(absolute(&file, cwd)).map_err(|error| format!("读取提示词文件失败：{error}"))?,
        (None, None) => return Err("需要 --prompt 或 --prompt-file".into()),
        (Some(_), Some(_)) => unreachable!(),
    };
    let prompt = prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("提示词为空".into());
    }
    if !matches!(mode.as_str(), "once" | "agent") {
        return Err("--mode 只能是 once 或 agent".into());
    }
    if !matches!(slot.as_str(), "chat" | "vision" | "promptOptimize" | "videoAnalysis") {
        return Err("--slot 只能是 chat、vision、promptOptimize 或 videoAnalysis".into());
    }
    if let Some(model) = &model {
        let (provider, name) = model.split_once('/').ok_or("--model 需要 供应商/模型")?;
        if provider.is_empty() || name.is_empty() {
            return Err("--model 需要 供应商/模型".into());
        }
    }
    Ok(json!({
        "op": "run",
        "prompt": prompt,
        "mode": mode,
        "system": system,
        "images": images,
        "video": video,
        "slot": slot,
        "model": model,
        "timeout": timeout,
    }))
}

fn absolute(value: &str, cwd: &Path) -> String {
    let path = Path::new(value);
    if path.is_absolute() { value.to_owned() } else { cwd.join(path).to_string_lossy().into_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workbench_local_service_dispatch() {
        let unknown = dispatch(None, "nope", json!({})).await.unwrap_err();
        assert_eq!(unknown.code, exit::INVALID);
        assert!(unknown.message.contains("unknown service"), "{}", unknown.message);
        let commerce = dispatch(None, "commerce", json!({})).await.unwrap_err();
        assert_eq!(commerce.code, exit::INTERNAL);
        assert!(commerce.message.contains("missing app"), "{}", commerce.message);
        let publish = dispatch(None, "publish", json!({})).await.unwrap_err();
        assert_eq!(publish.code, exit::INTERNAL);
        assert!(publish.message.contains("missing app"), "{}", publish.message);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = "tok".to_string();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(None, stream, &token).await.unwrap();
        });
        let failure = call_endpoint(port, "tok", "nope", json!({"op": "run"}), Duration::from_secs(2)).await.unwrap_err();
        assert_eq!(failure.code, exit::INVALID);
        assert!(failure.message.contains("unknown service"), "{}", failure.message);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(None, stream, "right").await.unwrap();
        });
        let failure = call_endpoint(port, "wrong", "ai", json!({"op": "run"}), Duration::from_secs(2)).await.unwrap_err();
        assert_eq!(failure.code, exit::UNAVAILABLE);
    }

    #[test]
    fn workbench_local_exit_codes_roundtrip() {
        for code in [
            exit::INVALID,
            exit::INTERNAL,
            exit::REJECTED,
            exit::FAILED,
            exit::UNCERTAIN,
            exit::UNAVAILABLE,
            exit::CANCELLED,
            exit::TIMEOUT,
        ] {
            assert_eq!(exit_for(error_code_for(code)), code);
        }
    }

    #[test]
    fn workbench_local_ai_args() {
        let cwd = Path::new("/work");
        let op = parse_ai_run(
            &[
                "--prompt".into(),
                " hi ".into(),
                "--mode".into(),
                "agent".into(),
                "--model".into(),
                "p/m".into(),
                "--slot".into(),
                "vision".into(),
                "--timeout".into(),
                "9".into(),
                "--system".into(),
                "sys".into(),
                "--image".into(),
                "a.png".into(),
                "--video".into(),
                "/abs/v.mp4".into(),
                "--json".into(),
            ],
            cwd,
            &mut std::io::empty(),
        )
        .unwrap();
        assert_eq!(op["op"], "run");
        assert_eq!(op["prompt"], "hi");
        assert_eq!(op["mode"], "agent");
        assert_eq!(op["model"], "p/m");
        assert_eq!(op["slot"], "vision");
        assert_eq!(op["timeout"], 9);
        assert_eq!(op["system"], "sys");
        assert_eq!(op["images"][0], "/work/a.png");
        assert_eq!(op["video"], "/abs/v.mp4");
        assert!(parse_ai_run(&[], cwd, &mut std::io::empty()).unwrap_err().contains("prompt"));
        assert!(parse_ai_run(&["--prompt".into(), "hi".into(), "--prompt-file".into(), "a.txt".into()], cwd, &mut std::io::empty()).is_err());
        assert!(parse_ai_run(&["--prompt".into(), "hi".into(), "--mode".into(), "nope".into()], cwd, &mut std::io::empty()).is_err());
        assert!(parse_ai_run(&["--prompt".into(), "hi".into(), "--model".into(), "only".into()], cwd, &mut std::io::empty()).is_err());
    }
}
