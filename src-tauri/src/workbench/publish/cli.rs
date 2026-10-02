//! `dsivio publish` client. It only talks to the running app.
use crate::app_cli::{self, exit, CliFailure};
use serde_json::{json, Value};
use std::process::ExitCode;
use std::time::Duration;

const HELP: &str = "\
dsivio publish —— 用正在运行的 Dsivio 发布到 TikTok / YouTube（不读取密钥）

  dsivio publish accounts
  dsivio publish begin --platform tiktok|youtube --client-id <id> --client-secret <secret> [--redirect <url>]
  dsivio publish complete --request <id> [--callback <url>]
  dsivio publish submit --account <id> --video <path> --title <text> [--description <text>] [--privacy public|private|unlisted|friends] [--tag <tag>]...
  dsivio publish publish --account <id> --file <path> --title <text> [--description <text>] [--privacy public|private|unlisted|friends] [--tag <tag>]...
  dsivio publish records [--account <id>]
  dsivio publish refresh --record <id> | --account <id>
  dsivio publish status <id>
  dsivio publish retry --record <id>
  dsivio publish stats <id>
  dsivio publish unbind --account <id>

stdout 只输出 JSON。已产生记录时看 JSON 里的 status（多条取 uncertain > failed > rejected）：0 其余，3 被拒，4 失败，5 结果不确定。
调用失败、还没有记录：1 内部错误（含 App 缺失），2 参数、不支持或找不到，3 授权被拒绝，4 令牌或上传前 HTTP 失败，5 结果不确定（保留，当前不返回），6 Dsivio 未运行，124 本机授权回调超时。
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
    let value = runtime.block_on(app_cli::call("publish", op, Duration::from_secs(180)))?;
    println!("{}", serde_json::to_string(&value).unwrap_or_default());
    Ok(exit_code(&value))
}

fn parse(command: &str, args: &[String]) -> Result<Value, String> {
    let mut flags = Flags::new(args);
    let op = match command {
        "accounts" => json!({ "action": "accounts" }),
        "begin" => json!({
            "action": "begin",
            "config": {
                "platform": flags.required("platform")?,
                "clientId": flags.required("client-id")?,
                "clientSecret": flags.required("client-secret")?,
                "redirectUri": flags.optional("redirect")?.unwrap_or_default()
            }
        }),
        "complete" => json!({
            "action": "complete",
            "requestId": flags.required("request")?,
            "callbackUrl": flags.optional("callback")?.unwrap_or_default()
        }),
        "submit" | "publish" => {
            let accounts = flags.repeated("account")?;
            let tags = flags.repeated("tag")?;
            let video = flags.optional("video")?.or(flags.optional("file")?).ok_or("缺少 --file 或 --video")?;
            json!({
                "action": "submit",
                "request": {
                    "accountIds": accounts,
                    "videoPath": video,
                    "title": flags.required("title")?,
                    "description": flags.optional("description")?.unwrap_or_default(),
                    "privacy": flags.optional("privacy")?.unwrap_or_else(|| "private".into()),
                    "tags": tags
                }
            })
        }
        "records" => json!({
            "action": "records",
            "filter": { "accountId": flags.optional("account")? }
        }),
        "refresh" => {
            if let Some(id) = flags.optional("record")? {
                json!({ "action": "refresh", "id": id })
            } else {
                json!({ "action": "refreshAccount", "accountId": flags.required("account")? })
            }
        }
        "status" => json!({ "action": "refresh", "id": flags.optional("record")?.or_else(|| flags.positional()).ok_or("用法：dsivio publish status <id>")? }),
        "retry" => json!({ "action": "retry", "id": flags.required("record")? }),
        "stats" => json!({ "action": "stats", "id": flags.optional("record")?.or_else(|| flags.positional()).ok_or("用法：dsivio publish stats <id>")? }),
        "unbind" => json!({ "action": "unbind", "accountId": flags.required("account")? }),
        other => return Err(format!("未知命令 {other}，运行 `dsivio publish help` 查看用法")),
    };
    flags.finish()?;
    Ok(op)
}

fn exit_code(value: &Value) -> u8 {
    let status = value
        .get("status")
        .and_then(|item| item.as_str())
        .or_else(|| value["records"].as_array().and_then(worst_status));
    match status {
        Some("rejected") => exit::REJECTED,
        Some("failed") => exit::FAILED,
        Some("uncertain") => exit::UNCERTAIN,
        _ => exit::OK,
    }
}

fn worst_status(records: &Vec<Value>) -> Option<&str> {
    let statuses: Vec<&str> = records.iter().filter_map(|record| record["status"].as_str()).collect();
    ["uncertain", "failed", "rejected", "processing", "uploading", "published"]
        .into_iter()
        .find(|status| statuses.contains(status))
}

struct Flags<'a> {
    args: &'a [String],
}
impl<'a> Flags<'a> {
    fn new(args: &'a [String]) -> Self {
        Self { args }
    }
    fn required(&mut self, name: &str) -> Result<String, String> {
        self.optional(name)?.ok_or_else(|| format!("缺少 --{name}"))
    }
    fn optional(&mut self, name: &str) -> Result<Option<String>, String> {
        let mut found = None;
        let mut index = 0;
        while index < self.args.len() {
            let arg = &self.args[index];
            if arg == &format!("--{name}") || arg.starts_with(&format!("--{name}=")) {
                if found.is_some() {
                    return Err(format!("--{name} 重复了"));
                }
                found = Some(if let Some(inline) = arg.strip_prefix(&format!("--{name}=")) {
                    inline.to_string()
                } else {
                    index += 1;
                    self.args.get(index).cloned().ok_or_else(|| format!("--{name} 缺少取值"))?
                });
            }
            index += 1;
        }
        Ok(found)
    }
    fn repeated(&mut self, name: &str) -> Result<Vec<String>, String> {
        let mut found = Vec::new();
        let mut index = 0;
        while index < self.args.len() {
            let arg = &self.args[index];
            if arg == &format!("--{name}") || arg.starts_with(&format!("--{name}=")) {
                found.push(if let Some(inline) = arg.strip_prefix(&format!("--{name}=")) {
                    inline.to_string()
                } else {
                    index += 1;
                    self.args.get(index).cloned().ok_or_else(|| format!("--{name} 缺少取值"))?
                });
            }
            index += 1;
        }
        Ok(found)
    }
    fn positional(&self) -> Option<String> {
        let mut index = 0;
        while index < self.args.len() {
            let arg = &self.args[index];
            if arg.starts_with("--") {
                index += if arg.contains('=') { 1 } else { 2 };
                continue;
            }
            return Some(arg.clone());
        }
        None
    }
    fn finish(&self) -> Result<(), String> {
        let known = [
            "platform", "client-id", "client-secret", "redirect", "request", "callback", "account", "video", "file", "title",
            "description", "privacy", "tag", "record",
        ];
        for arg in self.args {
            if let Some(name) = arg.strip_prefix("--") {
                let name = name.split('=').next().unwrap_or(name);
                if !known.contains(&name) {
                    return Err(format!("未知参数 --{name}"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn publish_file_status_and_stats_use_the_assignment_aliases() {
        let submit = parse(
            "publish",
            &[
                "--account".into(),
                "acct".into(),
                "--file".into(),
                "/tmp/clip.mp4".into(),
                "--title".into(),
                "标题".into(),
            ],
        )
        .unwrap();
        assert_eq!(submit["action"], "submit");
        assert_eq!(submit["request"]["videoPath"], "/tmp/clip.mp4");
        assert_eq!(submit["request"]["accountIds"][0], "acct");

        let status = parse("status", &["rec-1".into()]).unwrap();
        assert_eq!(status["action"], "refresh");
        assert_eq!(status["id"], "rec-1");

        let stats = parse("stats", &["rec-2".into()]).unwrap();
        assert_eq!(stats["action"], "stats");
        assert_eq!(stats["id"], "rec-2");

        let refresh = parse("refresh", &["--account".into(), "acct".into()]).unwrap();
        assert_eq!(refresh["action"], "refreshAccount");
        assert_eq!(refresh["accountId"], "acct");
    }
}
