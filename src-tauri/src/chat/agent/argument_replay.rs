//! Bounded file payloads in the model's working view. Audit records are immutable.
use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::chat::types::{ToolCallRecord, ToolCallStatus};

const LARGE_ARGUMENT_BYTES: usize = 8 * 1024;
const LARGE_FIELD_BYTES: usize = 1024;

/// Keep the newest assistant batch intact for immediate verification. Only older,
/// paired, successful native file calls with matching audit arguments qualify.
/// This operates on a send-view copy, never on persisted transcripts or records.
pub(crate) fn compact_completed_file_arguments<'a>(
    messages: &mut [Value],
    records: impl IntoIterator<Item = &'a ToolCallRecord>,
) -> usize {
    let mut eligible = HashMap::new();
    let mut duplicates = HashSet::new();
    for record in records {
        if eligible.insert(record.id.as_str(), record).is_some() {
            duplicates.insert(record.id.as_str());
        }
    }
    let Some(latest) = messages.iter().rposition(|m| m["role"] == "assistant") else {
        return 0;
    };
    let mut seen_calls = HashSet::new();
    let mut reused_calls = HashSet::new();
    for call in messages
        .iter()
        .flat_map(|m| m["tool_calls"].as_array().into_iter().flatten())
    {
        if let Some(id) = call["id"].as_str() {
            if !seen_calls.insert(id.to_owned()) {
                reused_calls.insert(id.to_owned());
            }
        }
    }
    let mut saved = 0;
    for index in 0..latest {
        if messages[index]["role"] != "assistant" || protected_replay(&messages[index]) {
            continue;
        }
        let paired: HashSet<String> = messages[index + 1..]
            .iter()
            .take_while(|m| m["role"] != "assistant" && m["role"] != "user")
            .filter(|m| m["role"] == "tool" && m["is_error"] != true)
            .filter_map(|m| m["tool_call_id"].as_str().map(str::to_owned))
            .collect();
        let Some(calls) = messages[index]["tool_calls"].as_array_mut() else {
            continue;
        };
        for call in calls {
            let Some(id) = call["id"].as_str() else {
                continue;
            };
            let Some(record) = eligible.get(id) else {
                continue;
            };
            if duplicates.contains(id)
                || reused_calls.contains(id)
                || !paired.contains(id)
                || record.source != "native"
                || record.server_id.is_some()
                || record.status != ToolCallStatus::Success
                || !matches!(record.name.as_str(), "write" | "edit")
                || call["function"]["name"] != record.name
                || !record
                    .structured_content
                    .as_ref()
                    .is_some_and(|v| v["ok"] == true)
            {
                continue;
            }
            let Some(raw) = call["function"]["arguments"].as_str() else {
                continue;
            };
            if raw.len() < LARGE_ARGUMENT_BYTES {
                continue;
            }
            let Ok(mut args) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            let Ok(original) = serde_json::from_str::<Value>(&record.arguments) else {
                continue;
            };
            if args != original {
                continue;
            }
            let changed = if record.name == "write" {
                args.get_mut("content").is_some_and(omit_large_field)
            } else {
                let mut changed = false;
                if let Some(edits) = args.get_mut("edits").and_then(Value::as_array_mut) {
                    for edit in edits {
                        changed |= edit.get_mut("old_string").is_some_and(omit_large_field);
                        changed |= edit.get_mut("new_string").is_some_and(omit_large_field);
                    }
                }
                changed
            };
            if changed {
                let compact = args.to_string();
                saved += raw.len().saturating_sub(compact.len());
                call["function"]["arguments"] = Value::String(compact);
            }
        }
    }
    saved
}

fn omit_large_field(value: &mut Value) -> bool {
    let Some(text) = value
        .as_str()
        .filter(|text| text.len() >= LARGE_FIELD_BYTES)
    else {
        return false;
    };
    *value = Value::String(format!(
        "[Kivio history: completed file payload omitted; {} UTF-8 bytes; sha256={:x}. Full arguments/diff remain in local history. Read the file for current content; do not execute this placeholder.]",
        text.len(), Sha256::digest(text.as_bytes())
    ));
    true
}

fn protected_replay(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "signature" | "thought_signature" | "encrypted_content" | "reasoning_items"
            ) || protected_replay(value)
        }),
        Value::Array(values) => values.iter().any(protected_replay),
        _ => false,
    }
}

/// Project a request without mutating a durable runtime or conversation.
pub(crate) fn send_view<'a>(
    messages: &[Value],
    records: impl IntoIterator<Item = &'a ToolCallRecord>,
) -> Vec<Value> {
    let mut view = messages.to_vec();
    compact_completed_file_arguments(&mut view, records);
    view
}

/// Compaction may drop messages, but retained calls must keep their full audit
/// arguments at checkpoint and compacted_history exits. The returned send view
/// is independent and stays bounded.
pub(crate) fn restore_retained_arguments(messages: &mut [Value], original: &[Value]) {
    let mut originals = HashMap::new();
    for call in original
        .iter()
        .flat_map(|m| m["tool_calls"].as_array().into_iter().flatten())
    {
        if let (Some(id), Some(name)) = (call["id"].as_str(), call["function"]["name"].as_str()) {
            originals
                .entry((id, name))
                .and_modify(|entry| *entry = None)
                .or_insert(Some(&call["function"]["arguments"]));
        }
    }
    for call in messages
        .iter_mut()
        .filter_map(|m| m["tool_calls"].as_array_mut())
        .flatten()
    {
        if let (Some(id), Some(name)) = (call["id"].as_str(), call["function"]["name"].as_str()) {
            if let Some(Some(raw)) = originals.get(&(id, name)) {
                call["function"]["arguments"] = (*raw).clone();
            }
        }
    }
}
