use crate::chat::types::AgentRuntimeConfig;
use tauri::AppHandle;

fn validate_runtime_change(
    current: &AgentRuntimeConfig,
    empty: bool,
    next: &AgentRuntimeConfig,
) -> Result<(), String> {
    if current.kind == crate::chat::types::AgentRuntimeKind::External
        || next.kind != crate::chat::types::AgentRuntimeKind::Builtin
    {
        return Err(
            "Local CLI support has been removed. Start a new Dsivio Agent conversation.".into(),
        );
    }
    if !empty && current.kind != next.kind {
        return Err("This conversation is already bound to its mode. Start a new conversation to change modes.".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_set_agent_runtime(
    app: AppHandle,
    conversation_id: String,
    agent_runtime: AgentRuntimeConfig,
) -> Result<serde_json::Value, String> {
    let conversation = crate::chat::repository::repository(&app)
        .mutate(&app, &conversation_id, |conversation| {
            validate_runtime_change(
                &conversation.agent_runtime,
                conversation.messages.is_empty(),
                &agent_runtime,
            )?;
            conversation.agent_runtime = AgentRuntimeConfig {
                kind: agent_runtime.kind,
                ..Default::default()
            };
            Ok(())
        })
        .await
        .map_err(crate::chat::repository::repository_error)?;
    Ok(serde_json::json!({ "success": true, "conversation": conversation }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::AgentRuntimeKind;
    #[test]
    fn legacy_chat_json_migrates_to_agent() {
        let runtime: AgentRuntimeConfig =
            serde_json::from_value(serde_json::json!({"kind":"chat"})).unwrap();
        assert_eq!(runtime.kind, AgentRuntimeKind::Builtin);
        assert_eq!(serde_json::to_value(&runtime).unwrap()["kind"], "builtin");
    }

    #[test]
    fn only_builtin_modes_can_be_selected() {
        let agent = AgentRuntimeConfig::default();
        let chat = AgentRuntimeConfig {
            kind: AgentRuntimeKind::Chat,
            ..Default::default()
        };
        let legacy = AgentRuntimeConfig {
            kind: AgentRuntimeKind::External,
            external_agent_id: Some("codex".into()),
            ..Default::default()
        };
        assert!(validate_runtime_change(&agent, true, &chat).is_err());
        assert!(validate_runtime_change(&agent, false, &chat).is_err());
        assert!(validate_runtime_change(&agent, true, &legacy).is_err());
        assert!(validate_runtime_change(&legacy, true, &agent).is_err());
        let malformed = AgentRuntimeConfig {
            kind: AgentRuntimeKind::External,
            ..Default::default()
        };
        assert!(validate_runtime_change(&agent, true, &malformed).is_err());
    }
}
