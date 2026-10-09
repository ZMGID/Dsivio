//! Inbound Feishu/Lark events: text, post, image, file, audio, and video, plus @ gating.
//! Mention behavior follows Hermes `normalize_feishu_message` / `_mentions_self` (Nous Research, MIT).

use crate::im::types::{ImPlatform, InboundMessage};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    File,
    Audio,
    Media,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub key: String,
    pub name: String,
    pub kind: ResourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub message: InboundMessage,
    pub downloads: Vec<Download>,
}

#[derive(Clone)]
pub struct BotIdentity {
    pub open_id: String,
    pub name: String,
}

impl Default for BotIdentity {
    fn default() -> Self {
        Self {
            open_id: String::new(),
            name: String::new(),
        }
    }
}

pub fn draft(event: &Value, bot: &BotIdentity, require_mention: bool) -> Option<Draft> {
    let header = event.get("header").unwrap_or(event);
    let event_type = header
        .get("event_type")
        .or_else(|| event.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event_type != "im.message.receive_v1" {
        return None;
    }
    let body = event.get("event").unwrap_or(event);
    let message = body.get("message")?;
    let sender = body.get("sender")?;
    let sender_type = sender
        .get("sender_type")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    if sender_type == "bot" || sender_type == "app" {
        return None;
    }
    let ids = sender.get("sender_id").unwrap_or(sender);
    let open_id = ids
        .get("open_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if open_id.is_empty() || (bot.open_id == open_id) {
        return None;
    }
    let message_id = message
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let chat_id = message
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if message_id.is_empty() || chat_id.is_empty() {
        return None;
    }
    let chat_type = message
        .get("chat_type")
        .and_then(|v| v.as_str())
        .unwrap_or("p2p");
    let is_group = chat_type != "p2p";
    let mentions = message.get("mentions").and_then(|v| v.as_array());
    let content_raw = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let message_type = message
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let mut parsed = parse_content(message_type, content_raw);
    if message_type == "text" {
        parsed.text = apply_mentions(&parsed.text, mentions, bot);
    }
    if is_group && require_mention && !mentions_bot(mentions, content_raw, bot, &parsed.at_ids) {
        return None;
    }
    let thread = message
        .get("thread_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let user_name = sender
        .get("name")
        .or_else(|| sender.get("sender_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();
    Some(Draft {
        message: InboundMessage {
            platform: ImPlatform::Feishu,
            message_id: message_id.to_owned(),
            chat_id: chat_id.to_owned(),
            user_id: open_id.to_owned(),
            user_name,
            is_group,
            thread_id: thread,
            reply_token: Some(message_id.to_owned()),
            text: parsed.text,
            attachments: Vec::new(),
        },
        downloads: parsed.downloads,
    })
}

pub fn event_id(event: &Value) -> String {
    event
        .get("header")
        .and_then(|h| h.get("event_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

struct Parsed {
    text: String,
    downloads: Vec<Download>,
    at_ids: Vec<String>,
}

fn parse_content(message_type: &str, raw: &str) -> Parsed {
    let value: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let mut parsed = Parsed {
        text: String::new(),
        downloads: Vec::new(),
        at_ids: Vec::new(),
    };
    match message_type {
        "text" => {
            parsed.text = normalize_text(
                value.get("text").and_then(|v| v.as_str()).unwrap_or(raw),
                &[],
            )
        }
        "post" => parse_post(&value, &mut parsed),
        "image" => push_image(
            &mut parsed,
            value
                .get("image_key")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        ),
        "file" => push_file(&mut parsed, &value, ResourceKind::File),
        "audio" => push_file(&mut parsed, &value, ResourceKind::Audio),
        "media" => push_file(&mut parsed, &value, ResourceKind::Media),
        _ => {
            parsed.text = value
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_owned()
        }
    }
    parsed
}

fn parse_post(value: &Value, parsed: &mut Parsed) {
    let node = post_locale(value);
    let mut lines = Vec::new();
    if let Some(title) = node
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        lines.push(title.to_owned());
    }
    if let Some(rows) = node.get("content").and_then(|v| v.as_array()) {
        for row in rows {
            let Some(items) = row.as_array() else {
                continue;
            };
            let mut line = String::new();
            for item in items {
                render_element(item, parsed, &mut line);
            }
            let line = normalize_text(&line, &[]);
            if !line.is_empty() {
                lines.push(line);
            }
        }
    }
    if let Some(files) = node.get("files").and_then(|v| v.as_array()) {
        for entry in files {
            if entry.get("is_folder").is_some_and(flagged) {
                continue;
            }
            push_file(parsed, entry, ResourceKind::File);
        }
    }
    parsed.text = lines.join("\n");
}

fn flagged(value: &Value) -> bool {
    value.as_bool() == Some(true)
        || value.as_i64() == Some(1)
        || value.as_str() == Some("true")
}

fn post_locale(value: &Value) -> &Value {
    if value.get("content").and_then(|v| v.as_array()).is_some() {
        return value;
    }
    let post = value.get("post").unwrap_or(value);
    for key in ["zh_cn", "zh_hk", "zh_tw", "en_us", "ja_jp"] {
        if let Some(node) = post.get(key) {
            if node.get("content").and_then(|v| v.as_array()).is_some() {
                return node;
            }
        }
    }
    if let Some(obj) = post.as_object() {
        for (_, node) in obj {
            if node.get("content").and_then(|v| v.as_array()).is_some() {
                return node;
            }
        }
    }
    value
}

fn render_element(item: &Value, parsed: &mut Parsed, line: &mut String) {
    let tag = item.get("tag").and_then(|v| v.as_str()).unwrap_or("text");
    match tag {
        "text" | "md" => line.push_str(item.get("text").and_then(|v| v.as_str()).unwrap_or("")),
        "a" => {
            let label = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let href = item.get("href").and_then(|v| v.as_str()).unwrap_or("");
            if href.is_empty() {
                line.push_str(label);
            } else {
                line.push_str(&format!("[{label}]({href})"));
            }
        }
        "at" => {
            let id = item
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_owned();
            if !id.is_empty() {
                parsed.at_ids.push(id.clone());
            }
            if id == "@_all" {
                line.push_str("@all");
            } else {
                let name = item
                    .get("user_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                line.push_str(&format!("@{name}"));
            }
        }
        "img" | "image" => push_image(
            parsed,
            item.get("image_key").and_then(|v| v.as_str()).unwrap_or(""),
        ),
        "media" | "file" | "audio" | "video" => {
            let kind = match tag {
                "audio" => ResourceKind::Audio,
                "video" | "media" => ResourceKind::Media,
                _ => ResourceKind::File,
            };
            push_file(parsed, item, kind);
        }
        "code" | "code_block" => {
            let code = item
                .get("text")
                .or_else(|| item.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            line.push_str(&format!("```\n{code}\n```"));
        }
        _ => {}
    }
}

fn push_image(parsed: &mut Parsed, key: &str) {
    let key = key.trim();
    if key.is_empty() || parsed.downloads.iter().any(|d| d.key == key) {
        return;
    }
    parsed.downloads.push(Download {
        key: key.to_owned(),
        name: format!("{key}.png"),
        kind: ResourceKind::Image,
    });
}

fn push_file(parsed: &mut Parsed, value: &Value, kind: ResourceKind) {
    let key = value
        .get("file_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if key.is_empty() || parsed.downloads.iter().any(|d| d.key == key) {
        return;
    }
    let name = value
        .get("file_name")
        .or_else(|| value.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let name = if name.is_empty() {
        match kind {
            ResourceKind::Audio => format!("{key}.opus"),
            ResourceKind::Media => format!("{key}.mp4"),
            ResourceKind::Image => format!("{key}.png"),
            ResourceKind::File => format!("{key}.bin"),
        }
    } else {
        name.to_owned()
    };
    parsed.downloads.push(Download {
        key: key.to_owned(),
        name,
        kind,
    });
}

fn mentions_bot(
    mentions: Option<&Vec<Value>>,
    raw: &str,
    bot: &BotIdentity,
    at_ids: &[String],
) -> bool {
    if raw.contains("@_all") {
        return true;
    }
    if at_ids
        .iter()
        .any(|id| id == "@_all" || (!bot.open_id.is_empty() && id == &bot.open_id))
    {
        return true;
    }
    mentions.is_some_and(|list| list.iter().any(|mention| mention_is_bot(mention, bot)))
}

fn mention_is_all(mention: &Value) -> bool {
    mention.get("key").and_then(|v| v.as_str()) == Some("@_all")
        || mention
            .get("id")
            .and_then(|id| id.get("user_id"))
            .and_then(|v| v.as_str())
            == Some("all")
}

fn mention_is_bot(mention: &Value, bot: &BotIdentity) -> bool {
    if mention_is_all(mention) {
        return true;
    }
    let id = mention.get("id").unwrap_or(mention);
    let open_id = id
        .get("open_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !bot.open_id.is_empty() && open_id == bot.open_id {
        return true;
    }
    let name = mention
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    !bot.name.is_empty() && !name.is_empty() && name == bot.name && open_id.is_empty()
}

fn normalize_text(text: &str, mentions: &[Value]) -> String {
    let mut out = text.to_owned();
    for mention in mentions {
        let key = mention.get("key").and_then(|v| v.as_str()).unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let name = mention
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        out = out.replace(key, &format!("@{name}"));
    }
    out.replace("@_all", "@all")
        .replace("\r\n", "\n")
        .trim()
        .to_owned()
}

pub fn apply_mentions(text: &str, mentions: Option<&Vec<Value>>, bot: &BotIdentity) -> String {
    let mut out = text.to_owned();
    if let Some(mentions) = mentions {
        for mention in mentions {
            let key = mention.get("key").and_then(|v| v.as_str()).unwrap_or("");
            if key.is_empty() {
                continue;
            }
            if mention_is_bot(mention, bot) && !mention_is_all(mention) {
                out = out.replace(key, "");
            } else {
                let name = mention
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                out = out.replace(key, &format!("@{name}"));
            }
        }
    }
    out.replace("@_all", "@all")
        .replace("\r\n", "\n")
        .trim()
        .to_owned()
}

/// Visible inbound text after downloads. `None` drops an empty non-attachment event.
pub fn compose_text(text: &str, saved: usize, failed: usize) -> Option<String> {
    if text.is_empty() && saved == 0 && failed == 0 {
        return None;
    }
    if text.is_empty() && saved == 0 {
        return Some("\u{9644}\u{4ef6}\u{4e0b}\u{8f7d}\u{5931}\u{8d25}".into());
    }
    Some(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bot() -> BotIdentity {
        BotIdentity {
            open_id: "ou_bot".into(),
            name: "Dsivio".into(),
        }
    }

    fn event(chat_type: &str, message_type: &str, content: &str, mentions: Value) -> Value {
        json!({
            "header": {"event_type": "im.message.receive_v1", "event_id": "e1", "token": "t"},
            "event": {
                "sender": {"sender_type": "user", "sender_id": {"open_id": "ou_user"}, "name": "Ada"},
                "message": {
                    "message_id": "om_1", "chat_id": "oc_1", "chat_type": chat_type,
                    "message_type": message_type, "content": content, "mentions": mentions
                }
            }
        })
    }

    #[test]
    fn group_requires_bot_open_id_or_all() {
        let mention = json!([{"key": "@_user_1", "id": {"open_id": "ou_bot"}, "name": "Dsivio"}]);
        let hit = draft(
            &event("group", "text", "{\"text\":\"@_user_1 hi\"}", mention),
            &bot(),
            true,
        )
        .unwrap();
        assert_eq!(hit.message.user_id, "ou_user");
        assert!(hit.message.is_group);
        assert_eq!(hit.message.reply_token.as_deref(), Some("om_1"));
        let other = json!([{"key": "@_user_1", "id": {"open_id": "ou_other"}, "name": "Other"}]);
        assert!(draft(
            &event("group", "text", "{\"text\":\"@_user_1 hi\"}", other),
            &bot(),
            true
        )
        .is_none());
        let all = json!([]);
        assert!(draft(
            &event("group", "text", "{\"text\":\"@_all hi\"}", all),
            &bot(),
            true
        )
        .is_some());
        assert!(draft(
            &event("p2p", "text", "{\"text\":\"hi\"}", json!([])),
            &bot(),
            true
        )
        .is_some());
        assert!(draft(
            &event("group", "text", "{\"text\":\"hi\"}", json!([])),
            &bot(),
            false
        )
        .is_some());
    }

    #[test]
    fn non_text_messages_keep_attachment_refs() {
        let image = draft(
            &event("p2p", "image", "{\"image_key\":\"img_1\"}", json!([])),
            &bot(),
            true,
        )
        .unwrap();
        assert_eq!(
            image.downloads,
            vec![Download {
                key: "img_1".into(),
                name: "img_1.png".into(),
                kind: ResourceKind::Image
            }]
        );
        let file = draft(
            &event(
                "p2p",
                "file",
                "{\"file_key\":\"f1\",\"file_name\":\"a.pdf\"}",
                json!([]),
            ),
            &bot(),
            true,
        )
        .unwrap();
        assert_eq!(file.downloads[0].kind, ResourceKind::File);
        assert_eq!(file.downloads[0].name, "a.pdf");
        let audio = draft(
            &event("p2p", "audio", "{\"file_key\":\"a1\"}", json!([])),
            &bot(),
            true,
        )
        .unwrap();
        assert_eq!(audio.downloads[0].kind, ResourceKind::Audio);
        let video = draft(
            &event(
                "p2p",
                "media",
                "{\"file_key\":\"v1\",\"file_name\":\"c.mp4\"}",
                json!([]),
            ),
            &bot(),
            true,
        )
        .unwrap();
        assert_eq!(video.downloads[0].kind, ResourceKind::Media);
        let post = "{\"zh_cn\":{\"title\":\"T\",\"content\":[[{\"tag\":\"img\",\"image_key\":\"img_2\"},{\"tag\":\"file\",\"file_key\":\"f2\",\"file_name\":\"b.txt\"}]]}}";
        let rich = draft(&event("p2p", "post", post, json!([])), &bot(), true).unwrap();
        assert_eq!(rich.downloads.len(), 2);
        assert!(rich.message.text.contains('T'));
    }

    #[test]
    fn failed_attachment_is_visible_and_empty_noise_is_dropped() {
        assert_eq!(
            compose_text("", 0, 1).as_deref(),
            Some("\u{9644}\u{4ef6}\u{4e0b}\u{8f7d}\u{5931}\u{8d25}")
        );
        assert!(compose_text("", 0, 0).is_none());
        assert_eq!(compose_text("hi", 0, 1).as_deref(), Some("hi"));
    }

    #[test]
    fn ignores_self_and_bots() {
        let mut ev = event("p2p", "text", "{\"text\":\"hi\"}", json!([]));
        ev["event"]["sender"]["sender_id"]["open_id"] = json!("ou_bot");
        assert!(draft(&ev, &bot(), true).is_none());
        ev["event"]["sender"]["sender_id"]["open_id"] = json!("ou_user");
        ev["event"]["sender"]["sender_type"] = json!("bot");
        assert!(draft(&ev, &bot(), true).is_none());
    }

    #[test]
    fn post_sibling_files_join_the_attachment_downloads() {
        let post = r#"{"zh_cn":{"title":"T","content":[[{"tag":"text","text":"hi"},{"tag":"file","file_key":"f_dup","file_name":"same.txt"}]],"files":[{"file_key":"f_sib","file_name":"extra.txt"},{"file_key":"f_dup","file_name":"same.txt"},{"file_key":"f_dir","file_name":"dir","is_folder":true},{"file_key":"f_one","is_folder":1},{"file_key":"f_str","is_folder":"true"},{"file_name":"missing-key.txt"},{"file_key":"  ","file_name":"blank.txt"}]}}"#;
        let rich = draft(&event("p2p", "post", post, json!([])), &bot(), true).unwrap();
        let keys: Vec<_> = rich.downloads.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, vec!["f_dup", "f_sib"]);
        assert_eq!(rich.downloads[1].name, "extra.txt");
        assert_eq!(rich.downloads[1].kind, ResourceKind::File);
        assert!(rich.message.text.contains("hi"));
    }
}
