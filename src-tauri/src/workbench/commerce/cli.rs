//! `dsivio commerce` client. It only talks to the running app and never reads shop credentials.
use crate::app_cli::{self, exit, CliFailure};
use serde_json::{json, Value};
use std::process::ExitCode;
use std::time::Duration;

pub const SUBCOMMAND: &str = "commerce";

const HELP: &str = "\
dsivio commerce —— 用正在运行的 Dsivio 查询店铺并提交 Shopee 上架（不读取密钥）

  dsivio commerce shops
  dsivio commerce capabilities <shop>
  dsivio commerce metrics <shop> [--range today|yesterday|last7|last30]
  dsivio commerce categories <shop> [--parent <id>]
  dsivio commerce attributes <shop> <category>
  dsivio commerce products <shop> [--cursor <token>]
  dsivio commerce orders <shop> [--range today|yesterday|last7|last30] [--cursor <token>]
  dsivio commerce listings [--shop <id>] [--group <id>] [--status <status>]
  dsivio commerce submit --file <json>
  dsivio commerce resubmit <id> [--file <json>]
  dsivio commerce status <id>

stdout 只输出 JSON。退出码：0 成功，1 内部错误，2 参数/不支持/重复，3 被拒，4 失败，5 结果不确定，6 Dsivio 未运行。
";

pub fn run(args: impl Iterator<Item = std::ffi::OsString>) -> ExitCode {
    let argv: Result<Vec<String>, _> = args.map(|arg| arg.into_string()).collect();
    let result = match argv {
        Ok(argv) => run_command(argv),
        Err(_) => Err(CliFailure::invalid("参数不是有效的 UTF-8")),
    };
    ExitCode::from(result.unwrap_or_else(|failure| {
        eprintln!("{}", failure.message);
        println!(
            "{}",
            serde_json::to_string(&json!({"error": failure.message, "exitCode": failure.code})).unwrap_or_default()
        );
        failure.code
    }))
}

fn run_command(argv: Vec<String>) -> Result<u8, CliFailure> {
    let (command, rest) = match argv.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("help", &[][..]),
    };
    if command == "help" || command == "--help" || command == "-h" {
        print!("{HELP}");
        return Ok(exit::OK);
    }
    let op = parse(command, rest).map_err(CliFailure::invalid)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliFailure::internal(error.to_string()))?;
    let value = runtime.block_on(app_cli::call("commerce", op, Duration::from_secs(180)))?;
    println!("{}", serde_json::to_string(&value).unwrap_or_default());
    Ok(exit_from_value(&value))
}

pub async fn handle(app: &tauri::AppHandle, op: Value) -> Result<Value, super::CommerceError> {
    let _ = app;
    super::execute(&op).await
}

pub(crate) fn parse(command: &str, args: &[String]) -> Result<Value, String> {
    let mut positionals = Vec::new();
    let mut flags = std::collections::HashMap::<String, String>::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(name) = arg.strip_prefix("--") {
            let (name, inline) = name.split_once('=').map(|(name, value)| (name, Some(value.to_string()))).unwrap_or((name, None));
            if name.is_empty() || flags.contains_key(name) {
                return Err(format!("--{name} 重复或无效"));
            }
            let value = if let Some(value) = inline {
                value
            } else {
                index += 1;
                args.get(index).cloned().ok_or_else(|| format!("缺少 --{name} 的值"))?
            };
            flags.insert(name.to_string(), value);
        } else {
            positionals.push(arg.clone());
        }
        index += 1;
    }
    let flag = |name: &str| flags.get(name).cloned();
    let op = match command {
        "shops" => {
            if !positionals.is_empty() {
                return Err("shops 不接受额外参数".into());
            }
            json!({ "action": "shops" })
        }
        "capabilities" => json!({ "action": "capabilities", "shopId": one_positional(command, &positionals)? }),
        "metrics" => json!({
            "action": "metrics",
            "shopId": one_positional(command, &positionals)?,
            "range": flag("range").unwrap_or_else(|| "today".into())
        }),
        "categories" => json!({
            "action": "categories",
            "shopId": one_positional(command, &positionals)?,
            "parentId": flag("parent")
        }),
        "attributes" => {
            if positionals.len() != 2 {
                return Err("用法：attributes <shop> <category>".into());
            }
            json!({ "action": "attributes", "shopId": positionals[0], "categoryId": positionals[1] })
        }
        "products" => json!({
            "action": "products",
            "shopId": one_positional(command, &positionals)?,
            "cursor": flag("cursor")
        }),
        "orders" => json!({
            "action": "orders",
            "shopId": one_positional(command, &positionals)?,
            "range": flag("range").unwrap_or_else(|| "today".into()),
            "cursor": flag("cursor")
        }),
        "listings" => {
            if !positionals.is_empty() {
                return Err("listings 的筛选请使用 --shop、--group、--status".into());
            }
            json!({
                "action": "listings",
                "filter": { "shopId": flag("shop"), "groupId": flag("group"), "status": flag("status") }
            })
        }
        "submit" => {
            if !positionals.is_empty() {
                return Err("submit 只接受 --file".into());
            }
            let path = flag("file").ok_or("缺少 --file")?;
            let mut body = read_json(&path)?;
            body["action"] = json!("submit");
            if let Some(group) = flag("group") {
                body["groupId"] = json!(group);
            }
            if body.get("draft").is_none() {
                return Err("提交文件需要 draft 和 targets".into());
            }
            body
        }
        "resubmit" => {
            let id = one_positional(command, &positionals)?;
            let mut body = json!({ "action": "resubmit", "id": id });
            if let Some(path) = flag("file") {
                let file = read_json(&path)?;
                if let Some(draft) = file.get("draft") {
                    body["draft"] = draft.clone();
                }
                if let Some(target) = file.get("target") {
                    body["target"] = target.clone();
                }
            }
            body
        }
        "status" => json!({ "action": "status", "id": one_positional(command, &positionals)? }),
        other => return Err(format!("未知命令 {other}，运行 `dsivio commerce help` 查看用法")),
    };
    let allowed: &[&str] = match command {
        "metrics" => &["range"],
        "orders" => &["range", "cursor"],
        "products" => &["cursor"],
        "categories" => &["parent"],
        "listings" => &["shop", "group", "status"],
        "submit" => &["file", "group"],
        "resubmit" => &["file"],
        _ => &[],
    };
    if let Some(unknown) = flags.keys().find(|name| !allowed.contains(&name.as_str())) {
        return Err(format!("不认识的参数 --{unknown}"));
    }
    Ok(op)
}

fn one_positional(command: &str, positionals: &[String]) -> Result<String, String> {
    if positionals.len() != 1 || positionals[0].is_empty() {
        return Err(format!("用法：{command} 需要一个参数"));
    }
    Ok(positionals[0].clone())
}

fn read_json(path: &str) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|_| format!("读不到文件 {path}"))?;
    if bytes.len() > 1_000_000 {
        return Err("提交文件过大".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("提交文件不是 JSON：{error}"))
}

fn exit_from_value(value: &Value) -> u8 {
    if let Some(status) = value.get("status").and_then(Value::as_str) {
        return status_exit(status);
    }
    if let Some(items) = value.as_array() {
        return items.iter().map(|item| status_exit(item.get("status").and_then(Value::as_str).unwrap_or(""))).max().unwrap_or(exit::OK);
    }
    exit::OK
}

fn status_exit(status: &str) -> u8 {
    match status {
        "rejected" => exit::REJECTED,
        "failed" => exit::FAILED,
        "uncertain" => exit::UNCERTAIN,
        _ => exit::OK,
    }
}
