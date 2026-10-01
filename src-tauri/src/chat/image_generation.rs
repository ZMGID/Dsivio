//! Chat entry for image generation: resolve the conversation's image model, collect reference
//! images from the conversation, then hand the request to `media_generation`.
use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::AppHandle;

use crate::mcp::types::{ChatToolArtifact, McpToolCallResult};
use crate::media_generation::image_providers::{
    data_url_for_input, load_input_image_from_path, load_input_images_from_paths,
    parse_image_data_url, InputImage,
};
use crate::settings::ModelProvider;
use crate::state::AppState;

pub async fn tool_generate_image(
    app: &AppHandle,
    state: &AppState,
    conversation_id: Option<&str>,
    arguments: &Value,
) -> Result<McpToolCallResult, String> {
    let settings = state.settings_read().clone();
    let conversation = conversation_id.and_then(|conversation_id| {
        crate::chat::storage::load_conversation(app, conversation_id).ok()
    });
    let drafts = conversation_id
        .map(|conversation_id| crate::chat::draft_journal::latest_drafts_for(app, conversation_id))
        .unwrap_or_default();
    let session_ref = conversation
        .as_ref()
        .map(|conversation| crate::settings::SessionModel {
            provider_id: conversation.provider_id.as_str(),
            model: conversation.model.as_str(),
        });
    let (provider_id, model) =
        crate::chat::model_metadata::image_generation_model_for_session(&settings, session_ref)
            .ok_or_else(|| "Mixer image generation model is not configured".to_string())?;
    let provider = settings
        .get_provider(&provider_id)
        .cloned()
        .ok_or_else(|| "Mixer image generation provider is missing".to_string())?;
    let input_images = collect_mixer_input_images(app, conversation.as_ref(), &drafts, arguments)?;
    generate_shared(app, &provider, &model, arguments, &input_images).await
}

pub(crate) async fn generate_shared(
    app: &AppHandle,
    provider: &ModelProvider,
    model: &str,
    arguments: &Value,
    images: &[InputImage],
) -> Result<McpToolCallResult, String> {
    let mut options: std::collections::BTreeMap<String, Value> =
        serde_json::from_value(arguments.clone()).map_err(|e| e.to_string())?;
    let prompt = options
        .remove("prompt")
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or("请填写提示词")?;
    options.remove("paths");
    options.remove("artifact_ids");
    let task = crate::media_generation::start(
        app,
        crate::media_generation::MediaRequest {
            provider_id: provider.id.clone(),
            model: model.into(),
            kind: crate::media_generation::MediaKind::Image,
            prompt,
            images: images.iter().map(data_url_for_input).collect(),
            options,
            origin: Some("chat".into()),
            description_revision: None,
        },
    )
    .await?;
    Ok(crate::media_generation::tool_result(
        crate::media_generation::wait(
            app,
            &task.id,
            Some(std::time::Duration::from_secs(285)),
            &|| false,
        )
        .await?,
    ))
}

fn collect_mixer_input_images(
    app: &AppHandle,
    conversation: Option<&crate::chat::Conversation>,
    drafts: &[crate::chat::ChatMessage],
    arguments: &Value,
) -> Result<Vec<InputImage>, String> {
    let artifact_ids = crate::chat::artifacts::input_artifact_ids(arguments)?;
    let paths = string_list_arg(arguments, "paths");
    let mut images = Vec::new();
    let mut missing = Vec::new();

    let artifacts = if let Some(conversation) = conversation {
        artifact_ids
            .iter()
            .map(|id| crate::chat::artifacts::resolve(app, &conversation.id, id))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        resolve_mixer_artifacts(conversation, drafts, &artifact_ids)?
            .into_iter()
            .cloned()
            .collect()
    };
    if let Some(conversation) = conversation {
        for artifact in artifacts {
            match input_image_from_artifact(app, &conversation.id, &artifact) {
                Ok(image) => push_input_image(&mut images, image),
                Err(err) => missing.push(err),
            }
        }
    }

    for path in &paths {
        match resolve_mixer_image_path(app, conversation, path)
            .and_then(|resolved| load_input_image_from_path(&resolved))
        {
            Ok(image) => push_input_image(&mut images, image),
            Err(err) => missing.push(err),
        }
    }

    if !artifact_ids.is_empty() || !paths.is_empty() {
        if !missing.is_empty() {
            return Err(missing.join("; "));
        }
        return Ok(images);
    }

    let Some(conversation) = conversation else {
        return Ok(Vec::new());
    };
    let Some(last_user) = conversation
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
    else {
        return Ok(Vec::new());
    };
    let attachment_paths = crate::chat::attachments::stored_image_paths_for_attachments(
        app,
        &conversation.id,
        &last_user.attachments,
    )?;
    load_input_images_from_paths(&attachment_paths)
}

fn string_list_arg(arguments: &Value, name: &str) -> Vec<String> {
    let Some(values) = arguments.get(name).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for value in values {
        let Some(item) = value
            .as_str()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        else {
            continue;
        };
        if !items.iter().any(|existing| existing == item) {
            items.push(item.to_string());
        }
    }
    items
}

fn push_input_image(images: &mut Vec<InputImage>, image: InputImage) {
    if images
        .iter()
        .any(|existing| existing.base64 == image.base64)
    {
        return;
    }
    images.push(image);
}

fn resolve_mixer_artifacts<'a>(
    conversation: Option<&'a crate::chat::Conversation>,
    drafts: &'a [crate::chat::ChatMessage],
    artifact_ids: &[String],
) -> Result<Vec<&'a ChatToolArtifact>, String> {
    if artifact_ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(conversation) = conversation else {
        return Err("artifact_ids require an active conversation".to_string());
    };
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in artifact_ids {
        match crate::chat::artifacts::find_in_messages(
            drafts.iter().chain(conversation.messages.iter()),
            id,
        ) {
            Some(artifact) => found.push(artifact),
            None => missing.push(format!("Unknown artifact_id: {id}")),
        }
    }
    if !missing.is_empty() {
        return Err(missing.join("; "));
    }
    Ok(found)
}

fn input_image_from_artifact(
    app: &AppHandle,
    conversation_id: &str,
    artifact: &ChatToolArtifact,
) -> Result<InputImage, String> {
    if !artifact.mime_type.starts_with("image/") && !artifact.mime_type.is_empty() {
        return Err(format!(
            "Artifact `{}` is not an image",
            artifact.id.as_deref().unwrap_or(&artifact.name)
        ));
    }
    if artifact
        .path
        .as_deref()
        .is_some_and(|path| !path.is_empty())
    {
        let path = crate::chat::artifacts::file_path(app, conversation_id, artifact)?;
        if path.is_file() {
            return load_input_image_from_path(&path);
        }
        return Err(format!(
            "Artifact `{}` image file is missing: {}",
            artifact.id.as_deref().unwrap_or(&artifact.name),
            path.display()
        ));
    }
    if artifact.data_url.starts_with("data:") {
        let (mime_type, base64) = parse_image_data_url(&artifact.data_url)?;
        return Ok(InputImage { mime_type, base64 });
    }
    Err(format!(
        "Artifact `{}` has no readable image data",
        artifact.id.as_deref().unwrap_or(&artifact.name)
    ))
}

fn resolve_mixer_image_path(
    app: &AppHandle,
    conversation: Option<&crate::chat::Conversation>,
    raw: &str,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_file() {
        return Ok(path);
    }
    if let Some(conversation) = conversation {
        if let Ok(dir) = crate::chat::storage::conversation_attachments_dir(app, &conversation.id) {
            let name = Path::new(raw)
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| path.clone());
            let joined = dir.join(name);
            if joined.is_file() {
                return Ok(joined);
            }
        }
    }
    Err(format!("Input image not found: {raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation_from_messages(messages: Vec<Value>) -> crate::chat::Conversation {
        serde_json::from_value(serde_json::json!({
            "id": "conv_1",
            "title": "t",
            "provider_id": "p",
            "model": "m",
            "messages": messages,
            "created_at": 0,
            "updated_at": 0,
        }))
        .expect("conversation")
    }

    fn assistant_with_tool_artifact(artifact_id: &str, b64: &str) -> Value {
        serde_json::json!({
            "id": "msg_1",
            "role": "assistant",
            "content": "",
            "timestamp": 0,
            "tool_calls": [{
                "id": "tc_1",
                "name": "mixer_generate_image",
                "status": "success",
                "artifacts": [{
                    "id": artifact_id,
                    "name": "generated-image-1.png",
                    "mime_type": "image/png",
                    "data_url": format!("data:image/png;base64,{b64}"),
                }]
            }]
        })
    }

    #[test]
    fn mixer_finds_same_turn_artifact_on_draft_not_main_json() {
        let conversation = conversation_from_messages(vec![]);
        let drafts: Vec<crate::chat::ChatMessage> =
            vec![
                serde_json::from_value(assistant_with_tool_artifact("art_new", "aGVsbG8="))
                    .expect("draft"),
            ];
        let found = resolve_mixer_artifacts(Some(&conversation), &drafts, &["art_new".to_string()])
            .expect("draft artifact");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.as_deref(), Some("art_new"));
    }

    #[test]
    fn mixer_unknown_artifact_errors_even_if_another_image_is_known() {
        let conversation =
            conversation_from_messages(vec![assistant_with_tool_artifact("art_old", "aGVsbG8=")]);
        let err = resolve_mixer_artifacts(
            Some(&conversation),
            &[],
            &["art_old".to_string(), "art_new".to_string()],
        )
        .expect_err("must not silently drop the missing id");
        assert!(err.contains("art_new"), "{err}");
        assert!(!err.contains("art_old") || err.contains("Unknown artifact_id: art_new"));
    }

    #[test]
    fn mixer_artifact_ids_require_a_conversation() {
        let err = resolve_mixer_artifacts(None, &[], &["art_new".to_string()])
            .expect_err("no conversation");
        assert!(err.contains("active conversation"), "{err}");
    }

    #[test]
    fn string_list_arg_dedupes_and_skips_blanks() {
        let arguments = serde_json::json!({
            "paths": ["a.png", " ", "a.png", "b.png"],
            "artifact_ids": "not-an-array",
        });
        assert_eq!(
            string_list_arg(&arguments, "paths"),
            vec!["a.png".to_string(), "b.png".to_string()]
        );
        assert!(string_list_arg(&arguments, "artifact_ids").is_empty());
        assert!(string_list_arg(&arguments, "missing").is_empty());
    }
}
