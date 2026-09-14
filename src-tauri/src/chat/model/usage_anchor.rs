//! Applicability of a measured request, independent of mutable conversation metadata.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{GenerateRequest, OfficialDeepseekBuiltinHop};
use crate::settings::ModelProvider;

pub(crate) fn request_api_format(provider: &ModelProvider, builtin_search: bool) -> &str {
    match super::official_deepseek_builtin_hop(provider, builtin_search) {
        Some(OfficialDeepseekBuiltinHop::Responses) => "openai_responses",
        Some(OfficialDeepseekBuiltinHop::Anthropic) => "anthropic_messages",
        None => provider.api_format.as_str(),
    }
}

pub(crate) fn request_view(
    model: &str,
    messages: &[serde_json::Value],
    tools: &[crate::mcp::ChatToolDefinition],
    builtin_search: bool,
) -> GenerateRequest {
    super::generate_request_from_openai_messages(
        model,
        messages.to_vec(),
        Some(tools),
        super::GenerateOptions {
            builtin_web_search: builtin_search,
            ..Default::default()
        },
        "Context applicability",
        super::GenerateRequestContext::default(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRequestIdentity {
    pub configuration: String,
    pub input_prefix: String,
    pub input_messages: usize,
    pub api_format: String,
}

fn digest(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable request view"))
    )
}

impl UsageRequestIdentity {
    pub(crate) fn from_request(
        provider: &ModelProvider,
        request: &GenerateRequest,
    ) -> Option<Self> {
        // Arbitrary body overrides can change the final wire view. Do not certify
        // a logical view when its equivalence cannot be established.
        if provider
            .model_overrides
            .get(&request.model)
            .and_then(|info| info.extra_body.as_ref())
            .is_some_and(|body| body.as_object().is_none_or(|object| !object.is_empty()))
            || request
                .options
                .provider_options
                .as_object()
                .is_none_or(|object| !object.is_empty())
        {
            return None;
        }
        let api_format = request_api_format(provider, request.options.builtin_web_search).to_string();
        let endpoint = reqwest::Url::parse(&provider.base_url).ok().map(|mut url| {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        });
        // No keys, authorization headers, or mutable display names are included.
        let configuration = digest(&serde_json::json!({
            "version": 1, "provider": provider.id, "model": request.model,
            "api": api_format, "endpoint": endpoint, "system": request.system, "tools": request.tools,
            "builtin_search": request.options.builtin_web_search,
        }));
        Some(Self {
            configuration,
            input_prefix: digest(&request.messages),
            input_messages: request.messages.len(),
            api_format,
        })
    }

    pub(crate) fn applies_to(&self, provider: &ModelProvider, request: &GenerateRequest) -> bool {
        let Some(current) = Self::from_request(provider, request) else {
            return false;
        };
        current.configuration == self.configuration
            && request
                .messages
                .get(..self.input_messages)
                .is_some_and(|prefix| digest(&prefix) == self.input_prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::model::{
        generate_request_from_openai_messages, GenerateOptions, GenerateRequestContext,
    };
    #[test]
    fn measured_view_survives_append_and_reopen_but_rejects_configuration_or_history_changes() {
        let provider: ModelProvider = serde_json::from_value(serde_json::json!({
            "id": "test", "name": "Test", "apiFormat": "openai_chat", "baseUrl": "http://localhost", "apiKey": "not-recorded"
        })).unwrap();
        let request = generate_request_from_openai_messages(
            "model-a",
            vec![
                serde_json::json!({"role":"system", "content":"instructions"}),
                serde_json::json!({"role":"user", "content":"hello"}),
            ],
            None,
            GenerateOptions::default(),
            "test",
            GenerateRequestContext::default(),
        );
        let identity = UsageRequestIdentity::from_request(&provider, &request).unwrap();
        let stored = serde_json::to_string(&identity).unwrap();
        assert!(!stored.contains("not-recorded"));
        let reopened: UsageRequestIdentity = serde_json::from_str(&stored).unwrap();
        let mut appended = request.clone();
        appended.messages.extend(request.messages.clone());
        assert!(reopened.applies_to(&provider, &appended));
        for change in 0..7 {
            let mut changed = request.clone();
            let mut changed_provider = provider.clone();
            match change {
                0 => changed.model = "model-b".into(),
                1 => changed_provider.id = "other".into(),
                2 => changed_provider.api_format = "anthropic_messages".into(),
                3 => changed.system.push_str(" new instructions"),
                4 => changed.messages.clear(),
                5 => changed.tools.push(super::super::ModelTool::from(
                    &crate::mcp::types::native_write_file_tool(),
                )),
                _ => changed.options.builtin_web_search = true,
            }
            assert!(!reopened.applies_to(&changed_provider, &changed));
        }
        let mut with_tool = request.clone();
        with_tool.tools.push(super::super::ModelTool::from(
            &crate::mcp::types::native_write_file_tool(),
        ));
        let measured = UsageRequestIdentity::from_request(&provider, &with_tool).unwrap();
        let mut changed_tool = crate::mcp::types::native_write_file_tool();
        changed_tool.input_schema["properties"]["new_field"] = serde_json::json!({"type":"string"});
        with_tool.tools = vec![super::super::ModelTool::from(&changed_tool)];
        assert!(!measured.applies_to(&provider, &with_tool));
    }
}
