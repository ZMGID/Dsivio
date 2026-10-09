//! Markdown post rendering and stream edit planning. A finished stream edits the same bubble.

use crate::im::common::split_text;
use serde_json::{json, Value};

pub const CHUNK_BYTES: usize = 12_000;

pub fn chunk_text_limited(text: &str, max_bytes: usize) -> Vec<String> {
    split_text(text, max_bytes)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    New(String),
    Replace(String),
    Append(String),
}

pub struct Plan {
    pub steps: Vec<Step>,
    pub clear: bool,
    pub tail_sent: bool,
}

pub fn steps(
    existing: Option<&str>,
    tail_sent: bool,
    text: &str,
    finished: bool,
    streamed: bool,
    max_bytes: usize,
) -> Plan {
    let chunks = chunk_text_limited(text, max_bytes);
    if chunks.is_empty() {
        return Plan {
            steps: Vec::new(),
            clear: finished || !streamed,
            tail_sent,
        };
    }
    if !streamed {
        let mut out = vec![Step::New(chunks[0].clone())];
        out.extend(chunks.into_iter().skip(1).map(Step::Append));
        return Plan {
            steps: out,
            clear: true,
            tail_sent: true,
        };
    }
    let mut plan = Vec::new();
    let head = &chunks[0];
    match existing {
        None => plan.push(Step::New(head.clone())),
        Some(prev) if prev != head => plan.push(Step::Replace(head.clone())),
        Some(_) => {}
    }
    let mut tail_sent = tail_sent;
    if finished && !tail_sent && chunks.len() > 1 {
        plan.extend(chunks.into_iter().skip(1).map(Step::Append));
        tail_sent = true;
    }
    Plan {
        steps: plan,
        clear: finished,
        tail_sent,
    }
}

pub fn payloads(text: &str) -> (&'static str, String) {
    if looks_like_markdown(text) {
        (
            "post",
            serde_json::to_string(&json!({"zh_cn": {"content": post_rows(text)}}))
                .unwrap_or_else(|_| "{\"zh_cn\":{\"content\":[]}}".into()),
        )
    } else {
        (
            "text",
            serde_json::to_string(&json!({"text": text}))
                .unwrap_or_else(|_| "{\"text\":\"\"}".into()),
        )
    }
}

pub fn plain_payload(text: &str) -> String {
    serde_json::to_string(&json!({"text": strip_markdown(text)}))
        .unwrap_or_else(|_| "{\"text\":\"\"}".into())
}

pub fn media_payload(kind: &str, key: &str) -> String {
    let body = match kind {
        "image" => json!({"image_key": key}),
        _ => json!({"file_key": key}),
    };
    serde_json::to_string(&body).unwrap_or_else(|_| "{}".into())
}

pub fn post_rejected(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("content format") || lower.contains("post type")
}

fn looks_like_markdown(text: &str) -> bool {
    if text.contains("```") || text.contains("**") || text.contains("~~") || text.contains("](") {
        return true;
    }
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("# ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("---")
            || numbered(trimmed)
    })
}

fn numbered(line: &str) -> bool {
    let mut chars = line.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_digit()) && line.contains(". ")
}

fn post_rows(content: &str) -> Vec<Vec<Value>> {
    if !content.contains("```") {
        return vec![vec![json!({"tag": "md", "text": content})]];
    }
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut in_code = false;
    for line in content.split_inclusive('\n') {
        let fence = line.trim().starts_with("```");
        if fence && !in_code && !current.is_empty() {
            push_row(&mut rows, &mut current);
        }
        current.push(line);
        if fence {
            in_code = !in_code;
            if !in_code {
                push_row(&mut rows, &mut current);
            }
        }
    }
    push_row(&mut rows, &mut current);
    if rows.is_empty() {
        rows.push(vec![json!({"tag": "md", "text": content})]);
    }
    rows
}

fn push_row(rows: &mut Vec<Vec<Value>>, current: &mut Vec<&str>) {
    let text = current.concat();
    current.clear();
    if text.trim().is_empty() {
        return;
    }
    rows.push(vec![json!({"tag": "md", "text": text})]);
}

fn strip_markdown(text: &str) -> String {
    text.replace("**", "").replace("~~", "").replace("```", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_chunks_do_not_split_scalars() {
        let text = "\u{4f60}\u{4f60}\u{1f642}";
        let chunks = chunk_text_limited(text, 4);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|c| c.len() <= 4));
        assert!(chunks
            .iter()
            .all(|c| std::str::from_utf8(c.as_bytes()).is_ok()));
    }

    #[test]
    fn stream_final_edits_without_a_second_copy() {
        let first = steps(None, false, "hi", false, true, 8);
        assert_eq!(first.steps, vec![Step::New("hi".into())]);
        assert!(!first.clear);
        let grown = steps(Some("hi"), false, "hi there", false, true, 80);
        assert_eq!(grown.steps, vec![Step::Replace("hi there".into())]);
        let done = steps(Some("hi there"), false, "hi there", true, true, 80);
        assert!(done.steps.is_empty());
        assert!(done.clear);
        let only_final = steps(None, false, "done", true, true, 80);
        assert_eq!(only_final.steps, vec![Step::New("done".into())]);
        assert!(only_final.clear);
    }

    #[test]
    fn oversized_final_appends_the_tail_once() {
        let text = "\u{4f60}".repeat(6);
        let preview = steps(None, false, &text, false, true, 4);
        assert_eq!(preview.steps.len(), 1);
        let head = match &preview.steps[0] {
            Step::New(v) => v.clone(),
            _ => panic!("head"),
        };
        let again = steps(Some(&head), false, &text, false, true, 4);
        assert!(
            again.steps.is_empty(),
            "intermediate overflow must not resend the tail"
        );
        let finished = steps(Some(&head), false, &text, true, true, 4);
        assert!(finished.steps.iter().all(|s| matches!(s, Step::Append(_))));
        assert!(finished.tail_sent);
        let repeat = steps(Some(&head), true, &text, true, true, 4);
        assert!(repeat.steps.is_empty());
    }

    #[test]
    fn markdown_post_splits_fences_and_plain_fallback_strips() {
        let (kind, payload) = payloads("hello\n```\ncode\n```\n**end**");
        assert_eq!(kind, "post");
        let value: Value = serde_json::from_str(&payload).unwrap();
        let rows = value["zh_cn"]["content"].as_array().unwrap();
        assert!(rows.len() >= 2);
        let plain = plain_payload("**end**");
        assert!(!plain.contains("**"));
        assert!(post_rejected(
            "content format of the post type is incorrect"
        ));
    }
}
