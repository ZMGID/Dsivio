//! Image provider wire formats: route selection, request bodies, sizing, transport and decoding.
//! Task persistence, receipts and polling belong to `media_generation`; chat input collection
//! belongs to `chat::image_generation`.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;

use crate::api::send_with_failover;
use crate::chat::model_metadata::normalize_model_name;
use crate::settings::{ModelProvider, ProviderApiFormat};
use crate::state::AppState;

pub(crate) mod async_task;

const DEFAULT_SIZE: &str = "auto";
const DEFAULT_QUALITY: &str = "auto";
const MAX_PROMPT_CHARS: usize = 8_000;
const MAX_IMAGE_BYTES: usize = 24 * 1024 * 1024;
const MAX_INPUT_IMAGES: usize = 4;
/// xAI Imagine image editing accepts five references (2026-08-28 contract).
const XAI_MAX_EDIT_IMAGES: usize = 5;
pub const IMAGE_GENERATION_TIMEOUT_MS: u64 = 300_000;
const IMAGE_GENERATION_HTTP_TIMEOUT: Duration = Duration::from_millis(IMAGE_GENERATION_TIMEOUT_MS);

/// 出图**端点选择**的唯一运行时枚举（不持久化，不进配置）。`resolve_image_route` 是端点
/// 判定的单一事实源，收敛此前散落的 `uses_*` 子串表；`generate_image_with_provider` 按此三分支
/// 调对应 `generate_with_*`。自愈只在 `Chat` ↔ `ImagesApi` 间摆动，`GeminiNative` 由 api_format 决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageRoute {
    GeminiNative,
    Chat,
    ImagesApi,
    /// Submit returns a receipt that is polled later (`async_task`).
    AsyncTask,
}

/// 归一化名字若命中这些 vendor 前缀（OpenRouter 风格 `vendor/model`），走 chat/completions 出图。
const OPENROUTER_CHAT_VENDOR_PREFIXES: [&str; 8] = [
    "black-forest-labs/",
    "bytedance-seed/",
    "google/",
    "microsoft/",
    "openai/gpt-5",
    "recraft/",
    "sourceful/",
    "x-ai/",
];

#[derive(Debug, Clone)]
struct ImageGenerationRequest {
    prompt: String,
    /// `auto`, `1K`/`2K`, or a validated `WIDTHxHEIGHT`.
    size: String,
    /// `auto` or an official ratio such as `16:9`.
    aspect_ratio: String,
    quality: String,
    n: usize,
    input_images: Vec<InputImage>,
}

#[derive(Debug, Clone)]
pub(crate) struct InputImage {
    pub(crate) mime_type: String,
    pub(crate) base64: String,
}

/// One generated image as returned by a provider (base64). The saved file's type is taken from
/// its signature, not from the provider's claim.
struct GeneratedImage {
    base64: String,
}

pub(crate) fn validate_generation(
    provider: &ModelProvider,
    model: &str,
    arguments: &Value,
    image_count: usize,
) -> Result<(), String> {
    let request = parse_request(arguments)?;
    validate_image_request(provider, model, &request)?;
    reject_excess_reference_images(provider, model, image_count)?;
    validate_provider(provider)
}

/// Decoded images from one synchronous generation, plus any text the model returned instead.
pub(crate) struct GeneratedBatch {
    pub images: Vec<Vec<u8>>,
    pub note: Option<String>,
}

pub(crate) async fn generate_image_with_provider(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    arguments: &Value,
    input_images: &[InputImage],
    retry_attempts: usize,
    operation: &str,
) -> Result<GeneratedBatch, String> {
    let mut request = parse_request(arguments)?;
    validate_image_request(provider, model, &request)?;
    reject_excess_reference_images(provider, model, input_images.len())?;
    request.input_images = input_images.to_vec();
    validate_provider(provider)?;

    // 端点选择：先查会话缓存（自愈学到的纠正结果），否则用单一解析器。
    let normalized_model = normalize_model_name(model);
    let cache_key = (provider.id.clone(), normalized_model);
    let cached_route = state.provider_runtime().image_route(&cache_key);
    let route = cached_route.unwrap_or_else(|| resolve_image_route(provider, model));

    let (images, note) = match call_image_route(
        state,
        provider,
        model,
        &request,
        retry_attempts,
        operation,
        route,
    )
    .await
    {
        Ok(result) => result,
        // 猜错自愈：选定 route 返回端点错配错误 → 换另一端点（Chat↔ImagesApi）重试一次；
        // 成功则记 (provider_id, normalized_model)→route 到会话缓存，下次同模型直达。
        Err(err) if is_endpoint_mismatch_error(&err) && alternate_route(route).is_some() => {
            let alt = alternate_route(route).expect("checked by guard");
            let result = call_image_route(
                state,
                provider,
                model,
                &request,
                retry_attempts,
                operation,
                alt,
            )
            .await?;
            state
                .provider_runtime()
                .remember_image_route(cache_key, alt);
            result
        }
        Err(err) => return Err(err),
    };
    let images = images
        .into_iter()
        .map(|image| {
            general_purpose::STANDARD
                .decode(image.base64.as_bytes())
                .map_err(|_| "图片编码无效".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Models sometimes answer with text (a refusal or question) instead of an image.
    let note = note.filter(|text| !text.trim().is_empty());
    if images.is_empty() {
        return Err(note.unwrap_or_else(|| "图片接口没有返回图片".into()));
    }
    Ok(GeneratedBatch { images, note })
}

/// 按选定 route 调对应 `generate_with_*`，统一返回 (图, 无图时的兜底文字)。
/// `ImagesApi` 无文字兜底（其响应体固定含图或报错）。
async fn call_image_route(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
    retry_attempts: usize,
    operation: &str,
    route: ImageRoute,
) -> Result<(Vec<GeneratedImage>, Option<String>), String> {
    match route {
        ImageRoute::GeminiNative => {
            generate_with_gemini_native(state, provider, model, request, retry_attempts, operation)
                .await
        }
        ImageRoute::Chat => {
            generate_with_openrouter_chat(
                state,
                provider,
                model,
                request,
                retry_attempts,
                operation,
            )
            .await
        }
        ImageRoute::ImagesApi => Ok((
            generate_with_images_api(state, provider, model, request, retry_attempts, operation)
                .await?,
            None,
        )),
        ImageRoute::AsyncTask => Err("异步图片任务须通过回执提交".into()),
    }
}

/// **端点选择的单一事实源**。优先级：① `api_format==Gemini` → GeminiNative；
/// ② base_url 含 `openrouter.ai` → Chat；③ 归一化名字启发式 → Chat | ImagesApi。
/// 收敛了旧 `uses_openrouter_chat_image_generation` 的全部判据（openrouter base、
/// api.openai.com / api.x.ai 早退、vendor 前缀表 + 裸名子串表合并为一处，均走归一化名）。
pub(crate) fn resolve_image_route(provider: &ModelProvider, model: &str) -> ImageRoute {
    if async_task::is_async_gateway(&provider.base_url, model) {
        return ImageRoute::AsyncTask;
    }
    if provider.api_format_kind() == ProviderApiFormat::Gemini {
        return ImageRoute::GeminiNative;
    }
    if is_openrouter_base_url(&provider.base_url) {
        return ImageRoute::Chat;
    }
    // 官方直连端点（OpenAI / xAI）不走 chat 仿 OpenRouter 出图，交给各自 images API。
    let base_url = provider.base_url.to_ascii_lowercase();
    if base_url.contains("api.openai.com") || base_url.contains("api.x.ai") {
        return ImageRoute::ImagesApi;
    }
    let normalized = normalize_model_name(model);
    if OPENROUTER_CHAT_VENDOR_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return ImageRoute::Chat;
    }
    // 裸名出图模型（通用 /v1 代理不带 vendor 前缀，如 `gemini-3.1-flash-image`）：这些代理
    // 不支持 /images/generations 出这些图，必须走 chat/completions 仿 OpenRouter 出图。
    // 注意：grok-imagine-image 相反——通用代理明确只支持 /v1/images/generations，故不在此列，
    // 交给 ImagesApi（body 变体由 uses_xai_images_api 命中，走 b64_json）。
    if (normalized.contains("gemini") && normalized.contains("image"))
        || normalized.contains("nano-banana")
        || normalized.starts_with("imagen")
    {
        return ImageRoute::Chat;
    }
    ImageRoute::ImagesApi
}

/// 自愈时的另一端点：Chat↔ImagesApi 互为备选；GeminiNative 由 api_format 决定，无备选。
fn alternate_route(route: ImageRoute) -> Option<ImageRoute> {
    match route {
        ImageRoute::Chat => Some(ImageRoute::ImagesApi),
        ImageRoute::ImagesApi => Some(ImageRoute::Chat),
        ImageRoute::GeminiNative | ImageRoute::AsyncTask => None,
    }
}

/// 端点错配错误识别：provider 明确报「此模型/端点用错」时的短语。命中即触发换端点自愈。
/// 错误串来自 `send_with_failover`（格式 `... Error: {status} - {body}`），故 body 里的短语可见。
fn is_endpoint_mismatch_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    // 覆盖两向真实代理错配错误：
    //   Chat 走错 → "only supported on /v1/images/generations and /v1/images/edits"
    //   ImagesApi 走错 → "... is not supported on /v1/images/generations or /v1/images/edits"
    // 用 "supported on /v1/images/" 同时匹配 only/not 两种措辞。
    lower.contains("supported on /v1/images/")
        || lower.contains("/chat/completions")
        || lower.contains("must be used with")
}

pub(crate) fn has_known_direct_image_generation_route(
    provider: &ModelProvider,
    model: &str,
) -> bool {
    // xAI 也算：`resolve_image_route` 已把 api.x.ai 判到 ImagesApi、`uses_xai_images_api`
    // 认得 grok-imagine，管子是通的，只差这道门。不放行的话，用 Grok 预设（xai_responses）
    // 的用户选 grok-imagine 模型直接打 prompt 会退化成一次普通文本请求。
    if provider.api_format_kind() == ProviderApiFormat::Gemini {
        return model.to_ascii_lowercase().contains("gemini")
            && model.to_ascii_lowercase().contains("image");
    }
    if !matches!(
        provider.api_format_kind(),
        ProviderApiFormat::OpenAiChat | ProviderApiFormat::XaiResponses
    ) {
        return false;
    }
    // 判据来源换成单一 resolver，但**不扩大直连范围**：Chat route 恒为已知直连；ImagesApi route
    // 仅当命中已知 images API 模型（xai grok-imagine / gpt-image / dall-e）才算已知直连，
    // 与旧 `openrouter_chat || xai_images || openai_images_model` 逐例等价。
    match resolve_image_route(provider, model) {
        ImageRoute::Chat | ImageRoute::AsyncTask => true,
        ImageRoute::ImagesApi => {
            uses_xai_images_api(provider, model) || uses_openai_images_api_model(model)
        }
        ImageRoute::GeminiNative => false,
    }
}

fn parse_request(arguments: &Value) -> Result<ImageGenerationRequest, String> {
    let prompt = arguments
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Image generation requires prompt".to_string())?;
    let prompt = truncate_chars(prompt, MAX_PROMPT_CHARS);
    let size = parse_size_arg(
        arguments
            .get("size")
            .and_then(|value| value.as_str())
            .unwrap_or(DEFAULT_SIZE),
    )?;
    let aspect_ratio = parse_aspect_ratio_arg(
        arguments
            .get("aspect_ratio")
            .and_then(|value| value.as_str())
            .unwrap_or("auto"),
    )?;
    let quality = match arguments
        .get("quality")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_QUALITY)
    {
        valid @ ("auto" | "low" | "medium" | "high" | "xhigh" | "max") => valid,
        other => return Err(format!("Unsupported image quality: {other}")),
    };
    let n = match arguments.get("n") {
        None | Some(Value::Null) => 1,
        Some(value) => value
            .as_u64()
            .filter(|n| (1..=4).contains(n))
            .ok_or("Image count must be an integer from 1 to 4")? as usize,
    };

    Ok(ImageGenerationRequest {
        prompt,
        size,
        aspect_ratio,
        quality: quality.to_string(),
        n,
        input_images: Vec::new(),
    })
}

fn validate_provider(provider: &ModelProvider) -> Result<(), String> {
    if provider.request.oauth.is_some() {
        return Err("Account OAuth currently supports chat and vision input; choose an API Key provider for image generation".into());
    }
    match provider.api_format_kind() {
        // Gemini 原生走 generateContent 出图路径；OpenAI 系走 images/chat 路径。
        // xAI 出图走 `/images/generations`（`uses_xai_images_api`），与 OpenAI 系同路。
        ProviderApiFormat::OpenAiChat
        | ProviderApiFormat::OpenAiResponses
        | ProviderApiFormat::XaiResponses
        | ProviderApiFormat::Gemini => {}
        ProviderApiFormat::AnthropicMessages => {
            return Err("Mixer image generation requires an OpenAI-compatible provider".to_string())
        }
    }
    if !provider.has_credentials() {
        return Err(format!(
            "Image generation provider `{}` has no API key configured",
            provider.name
        ));
    }
    Ok(())
}

async fn generate_with_images_api(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
    retry_attempts: usize,
    operation: &str,
) -> Result<Vec<GeneratedImage>, String> {
    if !request.input_images.is_empty() {
        return generate_with_images_edits(
            state,
            provider,
            model,
            request,
            retry_attempts,
            operation,
        )
        .await;
    }
    let url = images_api_url(&provider.base_url, false);
    let body = images_generations_json_body(provider, model, request);

    let response = send_with_failover(
        state,
        operation,
        retry_attempts,
        &provider.id,
        &provider.api_keys,
        |key| {
            crate::provider_request::apply(
                state.client_for(provider).post(&url).bearer_auth(key),
                provider,
                None,
            )
            .timeout(IMAGE_GENERATION_HTTP_TIMEOUT)
            .json(&body)
            .send()
        },
    )
    .await?;
    read_images_api_response(provider, response).await
}

/// 有参考图时走 `/images/edits`：xAI 必须 JSON；官方 `api.openai.com` 必须 multipart；
/// 其余兼容中转跟 `/images/generations` 一样发 JSON data URL。
async fn generate_with_images_edits(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
    retry_attempts: usize,
    operation: &str,
) -> Result<Vec<GeneratedImage>, String> {
    let url = images_api_url(&provider.base_url, true);
    let response = if uses_xai_images_api(provider, model) {
        post_images_json(
            state,
            provider,
            &url,
            &xai_edits_body(model, request),
            retry_attempts,
            operation,
        )
        .await?
    } else if is_official_openai_images(provider) {
        let files = decoded_input_files(request)?;
        send_with_failover(
            state,
            operation,
            retry_attempts,
            &provider.id,
            &provider.api_keys,
            |key| {
                crate::provider_request::apply(
                    state.client_for(provider).post(&url).bearer_auth(key),
                    provider,
                    None,
                )
                .timeout(IMAGE_GENERATION_HTTP_TIMEOUT)
                .multipart(images_edits_form(model, request, &files))
                .send()
            },
        )
        .await?
    } else {
        let mut body = openai_compat_edits_json_body(model, request);
        let response =
            post_images_json(state, provider, &url, &body, retry_attempts, operation).await;
        match response {
            Err(error) if requires_images_image_url(&error) => {
                // The proxy explicitly rejected the reference field before
                // generation. Adapt once, retaining every reference and the
                // edit prompt; never turn an edit into text-to-image.
                body.as_object_mut()
                    .expect("edit body is an object")
                    .remove("image");
                body["images"] = Value::Array(
                    request
                        .input_images
                        .iter()
                        .map(|image| serde_json::json!({"image_url": data_url_for_input(image)}))
                        .collect(),
                );
                post_images_json(state, provider, &url, &body, retry_attempts, operation).await?
            }
            result => result?,
        }
    };
    read_images_api_response(provider, response).await
}

fn requires_images_image_url(error: &str) -> bool {
    error.contains("400 Bad Request") && error.contains("images[].image_url is required")
}

async fn post_images_json(
    state: &AppState,
    provider: &ModelProvider,
    url: &str,
    body: &Value,
    retry_attempts: usize,
    operation: &str,
) -> Result<reqwest::Response, String> {
    send_with_failover(
        state,
        operation,
        retry_attempts,
        &provider.id,
        &provider.api_keys,
        |key| {
            crate::provider_request::apply(
                state.client_for(provider).post(url).bearer_auth(key),
                provider,
                None,
            )
            .timeout(IMAGE_GENERATION_HTTP_TIMEOUT)
            .json(body)
            .send()
        },
    )
    .await
}

async fn read_images_api_response(
    provider: &ModelProvider,
    response: reqwest::Response,
) -> Result<Vec<GeneratedImage>, String> {
    let raw = response
        .text()
        .await
        .map_err(|err| format!("Mixer image generation read body: {err}"))?;
    let value: Value = serde_json::from_str(&raw).map_err(|err| {
        format!(
            "Mixer image generation parse JSON: {} (body: {})",
            err,
            raw.chars().take(500).collect::<String>()
        )
    })?;
    parse_images_api_response(provider, &value).await
}

fn images_api_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if let Some(message) = error
        .as_str()
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return Some(message.to_string());
    }
    if let Some(message) = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return Some(message.to_string());
    }
    Some(error.to_string())
}

async fn parse_images_api_response(
    provider: &ModelProvider,
    value: &Value,
) -> Result<Vec<GeneratedImage>, String> {
    let Some(data) = value.get("data").and_then(|value| value.as_array()) else {
        if let Some(message) = images_api_error_message(value) {
            return Err(format!("Image generation failed: {message}"));
        }
        return Err("Image generation response missing data array".to_string());
    };
    let mut images = Vec::new();
    for item in data {
        if let Some(b64) = item
            .get("b64_json")
            .or_else(|| item.get("b64Json"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            validate_base64_image(b64)?;
            images.push(GeneratedImage {
                base64: b64.to_string(),
            });
            continue;
        }
        if let Some(url) = item
            .get("url")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let base64 = fetch_image_url(provider, url).await?;
            images.push(GeneratedImage { base64 });
        }
    }
    Ok(images)
}

async fn generate_with_openrouter_chat(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
    retry_attempts: usize,
    operation: &str,
) -> Result<(Vec<GeneratedImage>, Option<String>), String> {
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": chat_user_content(request),
            }
        ],
        "modalities": openrouter_modalities(model),
        "stream": false,
    });
    if let Some(image_config) = openrouter_image_config(request) {
        body["image_config"] = image_config;
    }

    let mut all_images = Vec::new();
    let mut last_text = None;
    for _ in 0..request.n {
        let result = async {
            let response = send_with_failover(
                state,
                operation,
                retry_attempts,
                &provider.id,
                &provider.api_keys,
                |key| {
                    crate::provider_request::apply(
                        state.client_for(provider).post(&url).bearer_auth(key),
                        provider,
                        None,
                    )
                    .timeout(IMAGE_GENERATION_HTTP_TIMEOUT)
                    .json(&body)
                    .send()
                },
            )
            .await?;
            let raw = response
                .text()
                .await
                .map_err(|err| format!("Mixer image generation read body: {err}"))?;
            let value: Value = serde_json::from_str(&raw).map_err(|err| {
                format!(
                    "Mixer image generation parse JSON: {} (body: {})",
                    err,
                    raw.chars().take(500).collect::<String>()
                )
            })?;
            let images = parse_openrouter_response(&value)?;
            let text = value
                .get("choices")
                .and_then(|value| value.as_array())
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_str())
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            Ok::<_, String>((images, text))
        }
        .await;
        let (images, text) = match result {
            Ok(result) => result,
            Err(error) if !all_images.is_empty() => {
                last_text = Some(format!(
                    "Only {} of {} images completed: {error}",
                    all_images.len(),
                    request.n
                ));
                break;
            }
            Err(error) => return Err(error),
        };
        all_images.extend(images);
        if text.is_some() {
            last_text = text;
        }
    }
    Ok((all_images, last_text))
}

fn parse_openrouter_response(value: &Value) -> Result<Vec<GeneratedImage>, String> {
    let mut images = Vec::new();
    let choices = value
        .get("choices")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "OpenRouter image response missing choices array".to_string())?;
    for choice in choices {
        let Some(message) = choice.get("message") else {
            continue;
        };
        let Some(message_images) = message.get("images").and_then(|value| value.as_array()) else {
            continue;
        };
        for item in message_images {
            let Some(data_url) = item
                .get("image_url")
                .or_else(|| item.get("imageUrl"))
                .and_then(|value| value.get("url"))
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            let (_, base64) = parse_image_data_url(data_url)?;
            images.push(GeneratedImage { base64 });
        }
    }
    Ok(images)
}

/// Gemini 原生 `generateContent` 出图路径（api_format = gemini）。
/// n>1 时顺序多次调用；首次失败则透传错误，若已有成功图则返回已收集的图。
async fn generate_with_gemini_native(
    state: &AppState,
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
    retry_attempts: usize,
    operation: &str,
) -> Result<(Vec<GeneratedImage>, Option<String>), String> {
    let url = gemini_generate_content_url(&provider.base_url, model);
    let body = build_gemini_native_body(request);

    let mut images = Vec::new();
    let mut fallback_text: Option<String> = None;
    for idx in 0..request.n {
        let response = send_with_failover(
            state,
            operation,
            retry_attempts,
            &provider.id,
            &provider.api_keys,
            |key| {
                crate::provider_request::apply(
                    state
                        .client_for(provider)
                        .post(&url)
                        .header("x-goog-api-key", key),
                    provider,
                    None,
                )
                .timeout(IMAGE_GENERATION_HTTP_TIMEOUT)
                .json(&body)
                .send()
            },
        )
        .await;
        let response = match response {
            Ok(response) => response,
            Err(err) => {
                if images.is_empty() {
                    return Err(err);
                }
                eprintln!(
                    "Mixer image generation gemini call #{} failed: {err}",
                    idx + 1
                );
                break;
            }
        };
        let raw = match response.text().await {
            Ok(raw) => raw,
            Err(err) => {
                if images.is_empty() {
                    return Err(format!("Mixer image generation read body: {err}"));
                }
                eprintln!(
                    "Mixer image generation gemini read body #{} failed: {err}",
                    idx + 1
                );
                break;
            }
        };
        let value: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(err) => {
                let message = format!(
                    "Mixer image generation parse JSON: {} (body: {})",
                    err,
                    raw.chars().take(500).collect::<String>()
                );
                if images.is_empty() {
                    return Err(message);
                }
                eprintln!("{message}");
                break;
            }
        };
        match parse_gemini_native_response(&value) {
            Ok(mut parsed) => images.append(&mut parsed),
            Err(err) => {
                if images.is_empty() {
                    return Err(err);
                }
                eprintln!(
                    "Mixer image generation gemini parse #{} failed: {err}",
                    idx + 1
                );
                break;
            }
        }
        if fallback_text.is_none() {
            fallback_text = gemini_native_response_text(&value);
        }
    }
    Ok((images, fallback_text))
}

/// 从 Gemini 原生响应里拼出 candidates[*].content.parts[*].text（无图时透出的文字）。
fn gemini_native_response_text(value: &Value) -> Option<String> {
    let text = value
        .get("candidates")
        .and_then(|value| value.as_array())?
        .iter()
        .filter_map(|candidate| {
            candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(|parts| parts.as_array())
        })
        .flatten()
        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// 复刻 gemini.rs 的 `gemini_url`（私有，未编辑该文件）：base_url 去尾斜杠，
/// model 去重 "models/" 前缀，避免 "models/models/"。
fn gemini_generate_content_url(base_url: &str, model: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let model = model.trim_start_matches("models/");
    format!("{base}/models/{model}:generateContent")
}

fn build_gemini_native_body(request: &ImageGenerationRequest) -> Value {
    let mut generation_config = serde_json::json!({
        "responseModalities": ["TEXT", "IMAGE"],
    });
    if let Some(image_config) = gemini_image_config(request) {
        generation_config["imageConfig"] = image_config;
    }
    let mut parts = request
        .input_images
        .iter()
        .map(|image| {
            serde_json::json!({
                "inlineData": {
                    "mimeType": image.mime_type,
                    "data": image.base64,
                }
            })
        })
        .collect::<Vec<_>>();
    parts.push(serde_json::json!({ "text": request.prompt.as_str() }));
    serde_json::json!({
        "contents": [
            {
                "role": "user",
                "parts": parts,
            }
        ],
        "generationConfig": generation_config,
    })
}

fn parse_gemini_native_response(value: &Value) -> Result<Vec<GeneratedImage>, String> {
    let candidates = value
        .get("candidates")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "Gemini image response missing candidates array".to_string())?;
    let mut images = Vec::new();
    for candidate in candidates {
        let Some(parts) = candidate
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
        else {
            continue;
        };
        for part in parts {
            let Some(inline_data) = part.get("inlineData").or_else(|| part.get("inline_data"))
            else {
                continue;
            };
            let Some(base64) = inline_data
                .get("data")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            validate_base64_image(base64)?;
            images.push(GeneratedImage {
                base64: base64.to_string(),
            });
        }
    }
    Ok(images)
}

/// Download an image URL returned by a synchronous generation, with the provider's proxy policy
/// and no provider credentials.
async fn fetch_image_url(provider: &ModelProvider, url: &str) -> Result<String, String> {
    let origin = reqwest::Url::parse(provider.base_url.trim()).map_err(|_| "供应商地址无效")?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("图片接口返回了非 HTTP(S) 图片地址".into());
    }
    let response = super::artifacts::fetch(
        super::artifacts::download_client(provider),
        &origin,
        url,
        super::artifacts::DownloadAuth::None,
        IMAGE_GENERATION_HTTP_TIMEOUT,
    )
    .await?;
    let bytes = super::artifacts::read_bounded(response, MAX_IMAGE_BYTES).await?;
    Ok(general_purpose::STANDARD.encode(bytes))
}

pub(crate) fn parse_image_data_url(data_url: &str) -> Result<(String, String), String> {
    let trimmed = data_url.trim();
    let Some(rest) = trimmed.strip_prefix("data:") else {
        return Err("OpenRouter image response did not return a data URL".to_string());
    };
    let Some((metadata, payload)) = rest.split_once(',') else {
        return Err("Image data URL is malformed".to_string());
    };
    let mime_type = metadata
        .split(';')
        .next()
        .map(str::trim)
        .filter(|value| value.starts_with("image/"))
        .unwrap_or("image/png")
        .to_string();
    if !metadata
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        return Err("Image data URL is not base64 encoded".to_string());
    }
    validate_base64_image(payload.trim())?;
    Ok((mime_type, payload.trim().to_string()))
}

fn validate_base64_image(value: &str) -> Result<(), String> {
    let decoded_len = decoded_base64_len(value).unwrap_or(0);
    if decoded_len == 0 {
        return Err("Generated image base64 is empty".to_string());
    }
    if decoded_len as usize > MAX_IMAGE_BYTES {
        return Err("Generated image is too large to attach".to_string());
    }
    if general_purpose::STANDARD
        .decode(value)
        .map(|bytes| !bytes.is_empty())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("Generated image base64 is invalid".to_string())
    }
}

pub(crate) fn decoded_base64_len(value: &str) -> Option<u64> {
    let compact_len = value.chars().filter(|ch| !ch.is_whitespace()).count();
    if compact_len == 0 {
        return Some(0);
    }
    let padding = value
        .chars()
        .rev()
        .take_while(|ch| ch.is_whitespace() || *ch == '=')
        .filter(|ch| *ch == '=')
        .count()
        .min(2);
    Some(((compact_len * 3) / 4).saturating_sub(padding) as u64)
}

pub(crate) fn extension_for_mime(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        _ => "png",
    }
}

fn is_openrouter_base_url(base_url: &str) -> bool {
    base_url
        .trim()
        .to_ascii_lowercase()
        .contains("openrouter.ai")
}

/// Union of official ratios we accept from the tool. Each vendor only receives
/// ratios from its own allow-list.
const ASPECT_RATIOS: &[&str] = &[
    "1:1", "1:2", "2:1", "1:4", "4:1", "1:8", "8:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4",
    "5:2", "9:16", "16:9", "21:9", "19.5:9", "9:19.5", "20:9", "9:20",
];
/// Gemini `imageConfig.aspectRatio` (ai.google.dev image-generation).
const GEMINI_ASPECT_RATIOS: &[&str] = &[
    "1:1", "1:4", "1:8", "2:3", "3:2", "3:4", "4:1", "4:3", "4:5", "5:4", "8:1", "9:16", "16:9",
    "21:9",
];
/// xAI Imagine `aspect_ratio` (docs.x.ai images/generation).
const XAI_ASPECT_RATIOS: &[&str] = &[
    "1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "2:1", "1:2", "19.5:9", "9:19.5", "20:9",
    "9:20", "21:9", "5:2",
];

fn parse_size_arg(raw: &str) -> Result<String, String> {
    let size = raw.trim();
    if size.is_empty() || size.eq_ignore_ascii_case("auto") {
        return Ok("auto".to_string());
    }
    let lower = size.to_ascii_lowercase();
    if matches!(lower.as_str(), "512" | "0.5k" | "05k") {
        return Ok("512".to_string());
    }
    if lower == "1k" {
        return Ok("1K".to_string());
    }
    if lower == "2k" {
        return Ok("2K".to_string());
    }
    if lower == "4k" {
        return Ok("4K".to_string());
    }
    let Some((width, height)) = parse_pixel_pair(size) else {
        return Err(format!("Unsupported image size: {raw}"));
    };
    // Preserve exact dimensions; model-specific validation happens before submission.
    Ok(format!("{width}x{height}"))
}

fn parse_aspect_ratio_arg(raw: &str) -> Result<String, String> {
    let ratio = raw.trim().replace('/', ":");
    if ratio.is_empty() || ratio.eq_ignore_ascii_case("auto") {
        return Ok("auto".to_string());
    }
    ASPECT_RATIOS
        .iter()
        .find(|known| **known == ratio)
        .map(|known| (*known).to_string())
        .ok_or_else(|| format!("Unsupported aspect ratio: {raw}"))
}

fn parse_pixel_pair(size: &str) -> Option<(u32, u32)> {
    let (width, height) = size.split_once('x')?;
    let width = width.trim().parse().ok()?;
    let height = height.trim().parse().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

fn snap16(value: u32) -> u32 {
    ((value + 8) / 16 * 16).max(16)
}

fn validate_openai_pixels(width: u32, height: u32) -> Result<(), String> {
    if width % 16 != 0 || height % 16 != 0 || width > 3840 || height > 3840 {
        return Err(format!(
            "Image size {width}x{height} must have edges that are multiples of 16 and at most 3840"
        ));
    }
    let pixels = u64::from(width) * u64::from(height);
    if !(655_360..=8_294_400).contains(&pixels) {
        return Err(format!(
            "Image size {width}x{height} is outside the official 655360–8294400 pixel range"
        ));
    }
    let (long, short) = if width >= height {
        (width, height)
    } else {
        (height, width)
    };
    if short == 0 || long > short * 3 {
        return Err(format!(
            "Image size {width}x{height} exceeds the official 3:1 aspect-ratio limit"
        ));
    }
    Ok(())
}

fn openai_size(request: &ImageGenerationRequest) -> Option<String> {
    match request.size.as_str() {
        "auto" if request.aspect_ratio == "auto" => None,
        "auto" => Some(pixels_for_tier_and_ratio("1K", &request.aspect_ratio)),
        "512" | "1K" | "2K" | "4K" => {
            let ratio = if request.aspect_ratio == "auto" {
                "1:1"
            } else {
                request.aspect_ratio.as_str()
            };
            Some(pixels_for_tier_and_ratio(&request.size, ratio))
        }
        pixels => Some(pixels.to_string()),
    }
}

fn pixels_for_tier_and_ratio(tier: &str, ratio: &str) -> String {
    let ratio = if aspect_ratio_value(ratio) > 3.0 || aspect_ratio_value(ratio) < 1.0 / 3.0 {
        if aspect_ratio_value(ratio) >= 1.0 {
            "3:1"
        } else {
            "1:3"
        }
    } else {
        ratio
    };
    // OpenAI gpt-image-2 has no 1K/2K field; these are official popular sizes
    // or 16-aligned equivalents inside the documented pixel limits.
    // 512 is below the official 655360 minimum, so it uses the 1K row.
    let tier = if tier == "512" { "1K" } else { tier };
    match (tier, ratio) {
        ("1K", "1:1") => "1024x1024",
        ("1K", "16:9") => "1536x864",
        ("1K", "9:16") => "864x1536",
        ("1K", "3:2") => "1536x1024",
        ("1K", "2:3") => "1024x1536",
        ("1K", "4:3") => "1024x768",
        ("1K", "3:4") => "768x1024",
        ("1K", "4:5") => "896x1120",
        ("1K", "5:4") => "1120x896",
        ("1K", "21:9") => "1536x656",
        ("1K", "2:1") => "1536x768",
        ("1K", "1:2") => "768x1536",
        ("1K", "3:1") => "1536x512",
        ("1K", "1:3") => "512x1536",
        ("2K", "1:1") => "2048x2048",
        ("2K", "16:9") => "2048x1152",
        ("2K", "9:16") => "1152x2048",
        ("2K", "3:2") => "2048x1360",
        ("2K", "2:3") => "1360x2048",
        ("2K", "4:3") => "2048x1536",
        ("2K", "3:4") => "1536x2048",
        ("2K", "4:5") => "1792x2240",
        ("2K", "5:4") => "2240x1792",
        ("2K", "21:9") => "2560x1104",
        ("2K", "2:1") => "2048x1024",
        ("2K", "1:2") => "1024x2048",
        ("2K", "3:1") => "2304x768",
        ("2K", "1:3") => "768x2304",
        ("4K", "1:1") => "2880x2880",
        ("4K", "16:9") => "3840x2160",
        ("4K", "9:16") => "2160x3840",
        ("4K", "3:2") => "3072x2048",
        ("4K", "2:3") => "2048x3072",
        ("4K", "4:3") => "3072x2304",
        ("4K", "3:4") => "2304x3072",
        ("4K", "4:5") => "2560x3200",
        ("4K", "5:4") => "3200x2560",
        ("4K", "21:9") => "3840x1648",
        ("4K", "2:1") => "3840x1920",
        ("4K", "1:2") => "1920x3840",
        _ => {
            let ratio = aspect_ratio_value(ratio);
            let edge = match tier {
                "4K" => 3840,
                "2K" => 2560,
                _ => 1536,
            };
            let (w, h) = if ratio >= 1.0 {
                (edge, snap16((edge as f64 / ratio) as u32))
            } else {
                (snap16((edge as f64 * ratio) as u32), edge)
            };
            return format!("{w}x{h}");
        }
    }
    .to_string()
}

fn requested_aspect_ratio(request: &ImageGenerationRequest) -> Option<String> {
    if request.aspect_ratio != "auto" {
        return Some(request.aspect_ratio.clone());
    }
    parse_pixel_pair(&request.size)
        .map(|(width, height)| nearest_allowed_aspect(ASPECT_RATIOS, width as f64 / height as f64))
}

fn vendor_aspect_ratio(request: &ImageGenerationRequest, allowed: &[&str]) -> Option<String> {
    requested_aspect_ratio(request).map(|ratio| {
        if allowed.contains(&ratio.as_str()) {
            ratio
        } else {
            nearest_allowed_aspect(allowed, aspect_ratio_value(&ratio))
        }
    })
}

fn nearest_allowed_aspect(allowed: &[&str], value: f64) -> String {
    allowed
        .iter()
        .copied()
        .min_by(|left, right| {
            (aspect_ratio_value(left) - value)
                .abs()
                .total_cmp(&(aspect_ratio_value(right) - value).abs())
        })
        .unwrap_or("1:1")
        .to_string()
}

fn aspect_ratio_value(ratio: &str) -> f64 {
    let (width, height) = ratio.split_once(':').unwrap_or(("1", "1"));
    width.parse::<f64>().unwrap_or(1.0) / height.parse::<f64>().unwrap_or(1.0)
}

fn requested_resolution_tier(request: &ImageGenerationRequest) -> Option<&'static str> {
    match request.size.as_str() {
        "512" => Some("512"),
        "1K" => Some("1K"),
        "2K" => Some("2K"),
        "4K" => Some("4K"),
        "auto" => None,
        pixels => parse_pixel_pair(pixels).map(|(width, height)| match width.max(height) {
            n if n >= 3000 => "4K",
            n if n >= 1920 => "2K",
            n if n >= 768 => "1K",
            _ => "512",
        }),
    }
}

/// Gemini official `imageSize`: `512` | `1K` | `2K` | `4K` (uppercase K).
fn gemini_image_size(request: &ImageGenerationRequest) -> Option<&'static str> {
    requested_resolution_tier(request)
}

/// xAI official `resolution`: `1k` | `2k` only.
fn xai_resolution(request: &ImageGenerationRequest) -> Option<&'static str> {
    match requested_resolution_tier(request)? {
        "1K" => Some("1k"),
        "2K" => Some("2k"),
        _ => None,
    }
}

fn gemini_image_config(request: &ImageGenerationRequest) -> Option<Value> {
    let aspect_ratio = vendor_aspect_ratio(request, GEMINI_ASPECT_RATIOS);
    let image_size = gemini_image_size(request);
    if aspect_ratio.is_none() && image_size.is_none() {
        return None;
    }
    let mut config = serde_json::Map::new();
    if let Some(aspect_ratio) = aspect_ratio {
        config.insert("aspectRatio".to_string(), Value::String(aspect_ratio));
    }
    if let Some(image_size) = image_size {
        config.insert(
            "imageSize".to_string(),
            Value::String(image_size.to_string()),
        );
    }
    Some(Value::Object(config))
}

fn openrouter_image_config(request: &ImageGenerationRequest) -> Option<Value> {
    let aspect_ratio = requested_aspect_ratio(request);
    let image_size = requested_resolution_tier(request);
    let quality = openai_quality(&request.quality);
    if aspect_ratio.is_none() && image_size.is_none() && quality.is_none() {
        return None;
    }
    let mut config = serde_json::Map::new();
    if let Some(aspect_ratio) = aspect_ratio {
        config.insert("aspect_ratio".to_string(), Value::String(aspect_ratio));
    }
    if let Some(image_size) = image_size {
        config.insert(
            "image_size".to_string(),
            Value::String(image_size.to_string()),
        );
    }
    if let Some(quality) = quality {
        config.insert("quality".to_string(), Value::String(quality.to_string()));
    }
    Some(Value::Object(config))
}

/// OpenAI GPT Image: `quality` is optional; default is `auto`.
fn openai_quality(quality: &str) -> Option<&str> {
    matches!(quality, "low" | "medium" | "high" | "xhigh" | "max").then_some(quality)
}

/// xAI Imagine: official `quality` is `low` | `medium` | `auto`. Omit `auto`.
fn xai_quality(quality: &str) -> Option<&str> {
    matches!(quality, "low" | "medium").then_some(quality)
}

fn uses_xai_images_api(provider: &ModelProvider, model: &str) -> bool {
    let descriptor =
        format!("{} {} {}", provider.base_url, provider.name, model).to_ascii_lowercase();
    descriptor.contains("api.x.ai") || descriptor.contains("grok-imagine-image")
}

fn uses_openai_images_api_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    uses_gpt_image_api_model(model) || lower.contains("dall-e")
}

fn uses_gpt_image_api_model(model: &str) -> bool {
    model.to_ascii_lowercase().contains("gpt-image")
}

/// Official Image APIs treat size/quality as optional (`auto` is the default).
/// Only send a concrete vendor field when the tool call asked for one.
fn images_generations_json_body(
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
) -> Value {
    let mut body = serde_json::json!({
        "model": model,
        "prompt": request.prompt.as_str(),
        "n": request.n,
    });
    if uses_xai_images_api(provider, model) {
        body["response_format"] = Value::String("b64_json".to_string());
        apply_xai_image_options(&mut body, request);
        return body;
    }
    apply_openai_image_options(&mut body, request);
    body
}

fn apply_openai_image_options(body: &mut Value, request: &ImageGenerationRequest) {
    let model = body["model"].as_str().unwrap_or("").to_ascii_lowercase();
    let size = if model.contains("dall-e") && request.size != "auto"
        || model.contains("dall-e") && request.aspect_ratio != "auto"
    {
        Some(if parse_pixel_pair(&request.size).is_some() {
            request.size.clone()
        } else if model.contains("dall-e-3") {
            match request.aspect_ratio.as_str() {
                "16:9" => "1792x1024",
                "9:16" => "1024x1792",
                _ => "1024x1024",
            }
            .into()
        } else if request.size == "512" {
            "512x512".into()
        } else {
            "1024x1024".into()
        })
    } else {
        openai_size(request)
    };
    if let Some(size) = size {
        body["size"] = Value::String(size);
    }
    if let Some(quality) = openai_quality(&request.quality) {
        body["quality"] = Value::String(quality.to_string());
    }
}

fn apply_xai_image_options(body: &mut Value, request: &ImageGenerationRequest) {
    if let Some(aspect_ratio) = vendor_aspect_ratio(request, XAI_ASPECT_RATIOS) {
        body["aspect_ratio"] = Value::String(aspect_ratio);
    }
    if let Some(resolution) = xai_resolution(request) {
        body["resolution"] = Value::String(resolution.to_string());
    }
    if let Some(quality) = xai_quality(&request.quality) {
        body["quality"] = Value::String(quality.to_string());
    }
}

fn openrouter_modalities(model: &str) -> Value {
    let lower = model.to_ascii_lowercase();
    let image_only = lower.contains("flux")
        || lower.contains("sourceful")
        || lower.contains("riverflow")
        || lower.contains("recraft")
        || lower.contains("seedream")
        || lower.contains("mai-image")
        || lower.contains("grok-imagine-image")
        || lower.contains("stable-diffusion")
        || lower.contains("sdxl")
        || lower.contains("imagen")
        || lower.contains("ideogram");
    if image_only {
        serde_json::json!(["image"])
    } else {
        serde_json::json!(["image", "text"])
    }
}

fn images_api_url(base_url: &str, has_input_images: bool) -> String {
    let path = if has_input_images {
        "images/edits"
    } else {
        "images/generations"
    };
    format!("{}/{path}", base_url.trim_end_matches('/'))
}

pub(crate) fn data_url_for_input(image: &InputImage) -> String {
    format!("data:{};base64,{}", image.mime_type, image.base64)
}

fn chat_user_content(request: &ImageGenerationRequest) -> Value {
    if request.input_images.is_empty() {
        return Value::String(request.prompt.clone());
    }
    let mut parts = vec![serde_json::json!({
        "type": "text",
        "text": request.prompt.as_str(),
    })];
    for image in &request.input_images {
        parts.push(serde_json::json!({
            "type": "image_url",
            "image_url": { "url": data_url_for_input(image) }
        }));
    }
    Value::Array(parts)
}

fn is_official_openai_images(provider: &ModelProvider) -> bool {
    provider
        .base_url
        .to_ascii_lowercase()
        .contains("api.openai.com")
}

fn openai_compat_edits_json_body(model: &str, request: &ImageGenerationRequest) -> Value {
    let mut body = serde_json::json!({
        "model": model,
        "prompt": request.prompt.as_str(),
        "n": request.n,
    });
    apply_openai_image_options(&mut body, request);
    let urls = request
        .input_images
        .iter()
        .map(|image| Value::String(data_url_for_input(image)))
        .collect::<Vec<_>>();
    body["image"] = if urls.len() == 1 {
        urls.into_iter().next().unwrap_or(Value::Null)
    } else {
        Value::Array(urls)
    };
    body
}

fn xai_edits_body(model: &str, request: &ImageGenerationRequest) -> Value {
    let mut body = serde_json::json!({
        "model": model,
        "prompt": request.prompt.as_str(),
        "n": request.n,
        "response_format": "b64_json",
    });
    apply_xai_image_options(&mut body, request);
    let refs = request
        .input_images
        .iter()
        .map(|image| {
            serde_json::json!({
                "url": data_url_for_input(image),
                "type": "image_url",
            })
        })
        .collect::<Vec<_>>();
    if refs.len() == 1 {
        body["image"] = refs.into_iter().next().unwrap_or(Value::Null);
    } else {
        body["images"] = Value::Array(refs);
    }
    body
}

fn decoded_input_files(
    request: &ImageGenerationRequest,
) -> Result<Vec<(String, String, Vec<u8>)>, String> {
    request
        .input_images
        .iter()
        .enumerate()
        .map(|(idx, image)| {
            let bytes = general_purpose::STANDARD
                .decode(image.base64.as_bytes())
                .map_err(|_| "Input image base64 is invalid".to_string())?;
            if bytes.is_empty() {
                return Err("Input image is empty".to_string());
            }
            Ok((
                format!("image-{}.{}", idx + 1, extension_for_mime(&image.mime_type)),
                image.mime_type.clone(),
                bytes,
            ))
        })
        .collect()
}

fn images_edits_form(
    model: &str,
    request: &ImageGenerationRequest,
    files: &[(String, String, Vec<u8>)],
) -> reqwest::multipart::Form {
    let mut form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .text("prompt", request.prompt.clone())
        .text("n", request.n.to_string());
    let mut options = serde_json::json!({"model":model});
    apply_openai_image_options(&mut options, request);
    for name in ["size", "quality"] {
        if let Some(value) = options[name].as_str() {
            form = form.text(name, value.to_owned());
        }
    }
    let field = if uses_gpt_image_api_model(model) || files.len() > 1 {
        "image[]"
    } else {
        "image"
    };
    for (name, mime, bytes) in files {
        let part = reqwest::multipart::Part::bytes(bytes.clone()).file_name(name.clone());
        let part = part.mime_str(mime).unwrap_or_else(|_| {
            reqwest::multipart::Part::bytes(bytes.clone()).file_name(name.clone())
        });
        form = form.part(field, part);
    }
    form
}

pub(crate) fn load_input_images_from_paths(paths: &[PathBuf]) -> Result<Vec<InputImage>, String> {
    paths
        .iter()
        .map(|path| load_input_image_from_path(path))
        .collect()
}

/// What the image path accepts for a model, for callers that decide before submitting
/// (`dsivio media models`). Every value comes from the same tables the request uses.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCapabilities {
    pub max_reference_images: usize,
    /// Size tiers accepted by `--size`; a `WIDTHxHEIGHT` pixel size is accepted as well.
    pub sizes: Vec<&'static str>,
    pub ratios: Vec<&'static str>,
    /// Images generated per request (`--n`).
    pub max_count: u32,
    pub qualities: Vec<&'static str>,
    pub custom_pixel_size: bool,
}

pub(crate) fn image_capabilities(provider: &ModelProvider, model: &str) -> ImageCapabilities {
    let name = model.to_ascii_lowercase();
    let gateway = resolve_image_route(provider, model) == ImageRoute::AsyncTask;
    let mut c = ImageCapabilities {
        max_reference_images: max_reference_images(provider, model),
        sizes: vec![],
        ratios: vec![],
        max_count: 1,
        qualities: vec!["auto"],
        custom_pixel_size: false,
    };
    if uses_xai_images_api(provider, model) {
        c.sizes = vec!["1K", "2K"];
        c.ratios = XAI_ASPECT_RATIOS.to_vec();
        c.qualities = vec!["auto", "low", "medium"];
        c.max_count = 4;
    } else if uses_gpt_image_api_model(model) {
        c.sizes = vec!["1K"];
        c.custom_pixel_size = true;
        c.max_count = 4;
        c.qualities = vec!["auto", "low", "medium", "high"];
        if name.contains("gpt-image-2") {
            c.sizes = vec!["1K", "2K", "4K"];
            c.ratios = ASPECT_RATIOS
                .iter()
                .copied()
                .filter(|r| (1.0 / 3.0..=3.0).contains(&aspect_ratio_value(r)))
                .collect();
            if name.contains("gpt-image-2.5") {
                c.qualities.extend(["xhigh", "max"]);
            }
        } else {
            c.ratios = vec!["1:1", "3:2", "2:3"];
        }
    } else if name.contains("dall-e") {
        c.sizes = vec!["1K"];
        c.ratios = vec!["1:1"];
        c.custom_pixel_size = true;
        if name.contains("dall-e-3") {
            c.ratios = vec!["1:1", "16:9", "9:16"];
        } else {
            c.sizes = vec!["512", "1K"];
            c.max_count = 4;
        }
    } else if name.contains("gemini") || name.contains("nano-banana") {
        c.sizes = vec!["1K"];
        c.ratios = GEMINI_ASPECT_RATIOS
            .iter()
            .copied()
            .filter(|r| {
                name.contains("3.1-flash-image")
                    || name == "nano-banana-2"
                    || !matches!(*r, "1:4" | "4:1" | "1:8" | "8:1")
            })
            .collect();
        c.max_count = 4;
        if (name.contains("gemini-3")
            || name.contains("nano-banana-2")
            || name.contains("nano-banana-pro"))
            && !name.contains("lite")
        {
            c.sizes = vec!["1K", "2K", "4K"];
            if name.contains("flash-image") || name == "nano-banana-2" {
                c.sizes.insert(0, "512");
            }
        }
    } else if resolve_image_route(provider, model) == ImageRoute::Chat {
        // OpenRouter's models have endpoint-specific limits. Only advertise the basic path here.
        c.sizes = vec!["1K"];
        c.ratios = ASPECT_RATIOS.to_vec();
    }
    if gateway {
        c.max_count = 1;
    }
    c
}

fn validate_image_request(
    provider: &ModelProvider,
    model: &str,
    request: &ImageGenerationRequest,
) -> Result<(), String> {
    let c = image_capabilities(provider, model);
    if request.n > c.max_count as usize {
        return Err(format!(
            "{model} on this connection accepts at most {} image(s) per request",
            c.max_count
        ));
    }
    if !c.qualities.contains(&request.quality.as_str()) {
        return Err(format!(
            "{model} does not support quality {} (allowed: {})",
            request.quality,
            c.qualities.join(", ")
        ));
    }
    if request.aspect_ratio != "auto" && !c.ratios.contains(&request.aspect_ratio.as_str()) {
        return Err(format!(
            "{model} does not support aspect ratio {}",
            request.aspect_ratio
        ));
    }
    if request.size != "auto" {
        if let Some((w, h)) = parse_pixel_pair(&request.size) {
            if !c.custom_pixel_size {
                return Err(format!(
                    "{model} accepts size tiers, not exact pixel dimensions"
                ));
            }
            if model.to_ascii_lowercase().contains("gpt-image-2") {
                validate_openai_pixels(w, h)?;
            } else {
                let size = request.size.as_str();
                let allowed: &[&str] = if model.to_ascii_lowercase().contains("dall-e-3") {
                    &["1024x1024", "1792x1024", "1024x1792"]
                } else if model.to_ascii_lowercase().contains("dall-e") {
                    &["512x512", "1024x1024"]
                } else {
                    &["1024x1024", "1536x1024", "1024x1536"]
                };
                if !allowed.contains(&size) {
                    return Err(format!("{model} does not support pixel size {size}"));
                }
            }
        } else if !c.sizes.contains(&request.size.as_str()) {
            return Err(format!(
                "{model} does not support size {} (allowed: {})",
                request.size,
                c.sizes.join(", ")
            ));
        }
    }
    Ok(())
}

/// Shared by native and asynchronous OpenAI-compatible image requests.
pub(crate) fn image_api_payload(
    provider: &ModelProvider,
    model: &str,
    arguments: &Value,
    reference_count: usize,
) -> Result<Value, String> {
    validate_generation(provider, model, arguments, reference_count)?;
    Ok(images_generations_json_body(
        provider,
        model,
        &parse_request(arguments)?,
    ))
}

fn max_reference_images(provider: &ModelProvider, model: &str) -> usize {
    if uses_xai_images_api(provider, model) {
        XAI_MAX_EDIT_IMAGES
    } else if uses_gpt_image_api_model(model) {
        16
    } else {
        let model = model.to_ascii_lowercase();
        if model.contains("gemini-3")
            || model.contains("nano-banana-pro")
            || model.contains("nano-banana-2")
        {
            14
        } else if model.contains("gemini-2.5-flash-image") || model == "nano-banana" {
            3
        } else {
            MAX_INPUT_IMAGES
        }
    }
}

fn reject_excess_reference_images(
    provider: &ModelProvider,
    model: &str,
    count: usize,
) -> Result<(), String> {
    let max_refs = max_reference_images(provider, model);
    if count > max_refs {
        Err(format!(
            "This image model accepts at most {max_refs} reference images (got {count})"
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn load_input_image_from_path(path: &Path) -> Result<InputImage, String> {
    let bytes = fs::read(path).map_err(|err| format!("Read input image failed: {err}"))?;
    if bytes.is_empty() {
        return Err("Input image is empty".to_string());
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Input image is too large".to_string());
    }
    let (bytes, mime_type) = match crate::chat::image_prep::prepare_image_bytes_for_model(&bytes) {
        Some((prepared, mime)) => (prepared, mime.to_string()),
        None => (bytes, mime_for_image_path(path).to_string()),
    };
    Ok(InputImage {
        mime_type,
        base64: general_purpose::STANDARD.encode(bytes),
    })
}

fn mime_for_image_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "image/png",
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openrouter_image_data_url() {
        let value = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "images": [
                            {
                                "type": "image_url",
                                "image_url": {
                                    "url": "data:image/png;base64,aGVsbG8="
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let images = parse_openrouter_response(&value).expect("image should parse");

        assert_eq!(images.len(), 1);
        assert_eq!(images[0].base64, "aGVsbG8=");
    }

    #[test]
    fn openrouter_bare_gemini_image_uses_image_text_modality() {
        // 裸名 gemini-3.1-flash-image 含 "image" 但不在 image_only 列表 → ["image","text"]。
        assert_eq!(
            openrouter_modalities("gemini-3.1-flash-image"),
            serde_json::json!(["image", "text"])
        );
    }

    #[test]
    fn text_only_image_response_yields_no_image_but_has_text() {
        // 模型对笼统提示返回纯文字（澄清），无 images → 解析出空图 + 文字透出。
        let value = serde_json::json!({
            "candidates": [
                { "content": { "parts": [
                    { "text": "请提供更多细节，" },
                    { "text": "例如性别、发色。" }
                ] } }
            ]
        });
        assert!(parse_gemini_native_response(&value)
            .expect("parse ok")
            .is_empty());
        assert_eq!(
            gemini_native_response_text(&value).as_deref(),
            Some("请提供更多细节，例如性别、发色。")
        );

        // 真出图时不误判为文字-only。
        let with_image = serde_json::json!({
            "candidates": [
                { "content": { "parts": [
                    { "inlineData": { "mimeType": "image/png", "data": "aGVsbG8=" } }
                ] } }
            ]
        });
        assert_eq!(gemini_native_response_text(&with_image), None);
    }

    #[test]
    fn bare_image_model_names_route_through_chat_on_generic_proxy() {
        // 通用 /v1 代理（非 openrouter.ai / 非 api.openai.com / 非 api.x.ai）暴露的裸名出图模型
        // 必须走 chat/completions 仿 OpenRouter 出图，而非 /images/generations。
        let proxy = ModelProvider {
            id: "proxy".to_string(),
            name: "Proxy".to_string(),
            api_keys: vec!["k".to_string()],
            api_key_legacy: None,
            base_url: "https://cpa.xb1520.com/v1".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        };

        assert_eq!(
            resolve_image_route(&proxy, "gemini-3.1-flash-image"),
            ImageRoute::Chat
        );
        // grok-imagine-image 相反：通用代理只支持 /images/generations，
        // 故走 ImagesApi（uses_xai_images_api body 变体命中）。
        assert_eq!(
            resolve_image_route(&proxy, "grok-imagine-image"),
            ImageRoute::ImagesApi
        );
        assert!(uses_xai_images_api(&proxy, "grok-imagine-image"));
        assert_eq!(resolve_image_route(&proxy, "nano-banana"), ImageRoute::Chat);
        assert_eq!(resolve_image_route(&proxy, "imagen-4.0"), ImageRoute::Chat);
        // 改名 / 加前缀 / 大小写变体仍正确路由（归一化）。
        assert_eq!(
            resolve_image_route(&proxy, "Gemini-3.1-Flash-Image"),
            ImageRoute::Chat
        );
        assert_eq!(
            resolve_image_route(&proxy, "models/gemini-3.1-flash-image"),
            ImageRoute::Chat
        );
        // 也走已知直连路由总闸（OpenAiChat + chat 出图）。
        assert!(has_known_direct_image_generation_route(
            &proxy,
            "gemini-3.1-flash-image"
        ));
        // 普通文本模型不误判。
        assert_eq!(resolve_image_route(&proxy, "gpt-4o"), ImageRoute::ImagesApi);

        // api.openai.com / api.x.ai 上的裸名仍被早退排除（走各自 images API，不走 chat）。
        let openai = ModelProvider {
            base_url: "https://api.openai.com/v1".to_string(),
            active_key_index: 0,
            ..proxy.clone()
        };
        assert_eq!(
            resolve_image_route(&openai, "gemini-3.1-flash-image"),
            ImageRoute::ImagesApi
        );
        let xai = ModelProvider {
            base_url: "https://api.x.ai/v1".to_string(),
            active_key_index: 0,
            ..proxy.clone()
        };
        assert_eq!(
            resolve_image_route(&xai, "grok-imagine-image"),
            ImageRoute::ImagesApi
        );
    }

    #[test]
    fn resolve_image_route_gemini_provider_uses_native() {
        assert_eq!(
            resolve_image_route(&gemini_provider(), "gemini-3.1-flash-image"),
            ImageRoute::GeminiNative
        );
        // Gemini api_format 恒 GeminiNative，即便名字看似普通文本模型。
        assert_eq!(
            resolve_image_route(&gemini_provider(), "gemini-2.5-pro"),
            ImageRoute::GeminiNative
        );
    }

    #[test]
    fn resolve_image_route_openrouter_base_uses_chat() {
        let openrouter = ModelProvider {
            id: "or".to_string(),
            name: "OpenRouter".to_string(),
            api_keys: vec!["k".to_string()],
            api_key_legacy: None,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        };
        assert_eq!(
            resolve_image_route(&openrouter, "google/gemini-3.1-flash-image-preview"),
            ImageRoute::Chat
        );
        assert_eq!(
            resolve_image_route(&openrouter, "grok-imagine-image"),
            ImageRoute::Chat
        );
    }

    #[test]
    fn endpoint_mismatch_error_matches_provider_phrases_not_generic_errors() {
        assert!(is_endpoint_mismatch_error(
            "Mixer image generation Error: 400 - {\"error\":{\"message\":\"This model is only supported on /v1/images/generations\"}}"
        ));
        assert!(is_endpoint_mismatch_error(
            "Error: 400 - only supported on /v1/images/edits"
        ));
        assert!(is_endpoint_mismatch_error(
            "Error: 404 - This model must be used with the chat endpoint"
        ));
        assert!(is_endpoint_mismatch_error(
            "Error: 400 - image models require the /chat/completions endpoint"
        ));
        // 真实代理错误串（两向,由真机测试捕获）：
        //   grok 走错 chat/completions（Chat→ImagesApi 方向）
        assert!(is_endpoint_mismatch_error(
            "Chat image generation Error: 503 Service Unavailable - {\"error\":{\"message\":\"model grok-imagine-image is only supported on /v1/images/generations and /v1/images/edits\"}}"
        ));
        //   gemini-image 走错 /v1/images/generations（ImagesApi→Chat 方向，措辞是 "not supported on"）
        assert!(is_endpoint_mismatch_error(
            "Mixer image generation Error: 400 - {\"error\":{\"message\":\"Model gemini-3.1-flash-image is not supported on /v1/images/generations or /v1/images/edits. Use gpt-image-1.5\"}}"
        ));
        // 普通错误不触发换端点。
        assert!(!is_endpoint_mismatch_error(
            "Image generation response missing data array"
        ));
        assert!(!is_endpoint_mismatch_error(
            "Mixer image generation Error: 500 - internal server error"
        ));
        assert!(!is_endpoint_mismatch_error("connection reset by peer"));
    }

    #[test]
    fn alternate_route_swings_chat_and_images_only() {
        assert_eq!(
            alternate_route(ImageRoute::Chat),
            Some(ImageRoute::ImagesApi)
        );
        assert_eq!(
            alternate_route(ImageRoute::ImagesApi),
            Some(ImageRoute::Chat)
        );
        assert_eq!(alternate_route(ImageRoute::GeminiNative), None);
    }

    #[test]
    fn rejects_empty_prompt() {
        let err = parse_request(&serde_json::json!({ "prompt": " " })).unwrap_err();
        assert!(err.contains("prompt"));
    }

    #[test]
    fn rejects_out_of_range_image_count() {
        for n in [0, 99] {
            assert!(parse_request(&serde_json::json!({"prompt":"draw an icon","n":n})).is_err());
        }
    }

    #[test]
    fn parse_request_accepts_resolution_tier_and_custom_pixels() {
        let request = parse_request(&serde_json::json!({
            "prompt": "city skyline",
            "size": "2k",
            "aspect_ratio": "16:9",
        }))
        .expect("request should parse");
        assert_eq!(request.size, "2K");
        assert_eq!(request.aspect_ratio, "16:9");

        let custom = parse_request(&serde_json::json!({
            "prompt": "poster",
            "size": "1920x1080",
        }))
        .expect("custom size must be preserved");
        assert_eq!(custom.size, "1920x1080");
        assert_eq!(custom.aspect_ratio, "auto");

        let four_k = parse_request(&serde_json::json!({
            "prompt": "poster",
            "size": "4k",
            "aspect_ratio": "4:5",
        }))
        .expect("4K and 4:5 are official Gemini values");
        assert_eq!(four_k.size, "4K");
        assert_eq!(four_k.aspect_ratio, "4:5");
    }

    #[test]
    fn openrouter_flux_models_use_image_only_modality() {
        assert_eq!(
            openrouter_modalities("black-forest-labs/flux.2-pro"),
            serde_json::json!(["image"])
        );
        assert_eq!(
            openrouter_modalities("recraft/recraft-v4.1-pro"),
            serde_json::json!(["image"])
        );
        assert_eq!(
            openrouter_modalities("bytedance-seed/seedream-4.5"),
            serde_json::json!(["image"])
        );
        assert_eq!(
            openrouter_modalities("x-ai/grok-imagine-image-quality"),
            serde_json::json!(["image"])
        );
        assert_eq!(
            openrouter_modalities("google/gemini-3.1-flash-image-preview"),
            serde_json::json!(["image", "text"])
        );
    }

    #[test]
    fn xai_detection_matches_grok_imagine_models() {
        let provider = ModelProvider {
            id: "xai".to_string(),
            name: "xAI".to_string(),
            api_keys: Vec::new(),
            api_key_legacy: None,
            base_url: "https://api.x.ai/v1".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        };

        assert!(uses_xai_images_api(&provider, "grok-imagine-image-quality"));
        assert!(has_known_direct_image_generation_route(
            &provider,
            "grok-imagine-image-quality"
        ));
    }

    #[test]
    fn direct_route_detection_matches_known_provider_routes() {
        let openai = ModelProvider {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            api_keys: Vec::new(),
            api_key_legacy: None,
            base_url: "https://api.openai.com/v1".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        };
        let openrouter = ModelProvider {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            active_key_index: 0,
            ..openai.clone()
        };
        let openrouter_compatible_relay = ModelProvider {
            base_url: "https://relay.example.com/v1".to_string(),
            active_key_index: 0,
            ..openai.clone()
        };

        assert!(has_known_direct_image_generation_route(
            &openai,
            "gpt-image-1.5"
        ));
        assert!(has_known_direct_image_generation_route(
            &openrouter,
            "google/gemini-3.1-flash-image-preview"
        ));
        assert!(has_known_direct_image_generation_route(
            &openrouter_compatible_relay,
            "black-forest-labs/flux.2-pro"
        ));
        assert!(!has_known_direct_image_generation_route(
            &openai,
            "gemini-3.1-flash-image-preview"
        ));
        assert!(!has_known_direct_image_generation_route(
            &openai,
            "openai/gpt-5-image"
        ));
    }

    fn gemini_provider() -> ModelProvider {
        ModelProvider {
            id: "gemini".to_string(),
            name: "Gemini".to_string(),
            api_keys: vec!["k".to_string()],
            api_key_legacy: None,
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "gemini".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        }
    }

    #[test]
    fn gemini_native_body_shape() {
        let request = ImageGenerationRequest {
            prompt: "draw a cat".to_string(),
            size: "1024x1024".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: Vec::new(),
        };
        let body = build_gemini_native_body(&request);
        assert_eq!(
            body["generationConfig"]["responseModalities"],
            serde_json::json!(["TEXT", "IMAGE"])
        );
        assert_eq!(
            body["generationConfig"]["imageConfig"]["aspectRatio"],
            serde_json::json!("1:1")
        );
        assert_eq!(
            body["contents"][0]["parts"].as_array().map(|p| p.len()),
            Some(1)
        );
        assert_eq!(body["contents"][0]["parts"][0]["text"], "draw a cat");

        let auto = ImageGenerationRequest {
            size: "auto".to_string(),
            ..request
        };
        let auto_body = build_gemini_native_body(&auto);
        assert!(auto_body["generationConfig"].get("imageConfig").is_none());
    }

    #[test]
    fn gemini_native_url_dedupes_models_prefix() {
        assert_eq!(
            gemini_generate_content_url(
                "https://generativelanguage.googleapis.com/v1beta/",
                "gemini-3.1-flash-image"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-image:generateContent"
        );
        assert_eq!(
            gemini_generate_content_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "models/gemini-3.1-flash-image"
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-image:generateContent"
        );
    }

    #[test]
    fn gemini_native_parses_inline_data() {
        let value = serde_json::json!({
            "candidates": [
                {
                    "content": {
                        "parts": [
                            { "text": "here is your image" },
                            {
                                "inlineData": {
                                    "mimeType": "image/png",
                                    "data": "aGVsbG8="
                                }
                            }
                        ]
                    }
                }
            ]
        });
        let images = parse_gemini_native_response(&value).expect("image should parse");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].base64, "aGVsbG8=");
    }

    #[test]
    fn validate_provider_accepts_gemini_rejects_anthropic() {
        assert!(validate_provider(&gemini_provider()).is_ok());

        let anthropic = ModelProvider {
            api_format: "anthropic_messages".to_string(),
            active_key_index: 0,
            ..gemini_provider()
        };
        assert!(validate_provider(&anthropic).is_err());
    }

    fn sample_input_image() -> InputImage {
        InputImage {
            mime_type: "image/png".to_string(),
            base64: "aGVsbG8=".to_string(),
        }
    }

    #[test]
    fn images_api_switches_to_edits_when_input_images_present() {
        assert_eq!(
            images_api_url("https://api.openai.com/v1/", false),
            "https://api.openai.com/v1/images/generations"
        );
        assert_eq!(
            images_api_url("https://api.openai.com/v1", true),
            "https://api.openai.com/v1/images/edits"
        );
    }

    #[test]
    fn chat_content_stays_plain_text_without_input_images() {
        let request = ImageGenerationRequest {
            prompt: "a red mug".to_string(),
            size: "auto".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: Vec::new(),
        };
        assert_eq!(
            chat_user_content(&request),
            Value::String("a red mug".to_string())
        );
    }

    #[test]
    fn chat_content_includes_input_images_as_data_urls() {
        let request = ImageGenerationRequest {
            prompt: "make it night".to_string(),
            size: "auto".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: vec![sample_input_image()],
        };
        let content = chat_user_content(&request);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "make it night");
        assert_eq!(
            content[1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
    }

    #[test]
    fn gemini_body_prepends_inline_input_images() {
        let request = ImageGenerationRequest {
            prompt: "make it night".to_string(),
            size: "auto".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: vec![sample_input_image()],
        };
        let body = build_gemini_native_body(&request);
        assert_eq!(
            body["contents"][0]["parts"][0]["inlineData"]["data"],
            "aGVsbG8="
        );
        assert_eq!(body["contents"][0]["parts"][1]["text"], "make it night");
    }

    #[test]
    fn xai_edits_json_uses_image_object_for_one_and_images_array_for_many() {
        let one = ImageGenerationRequest {
            prompt: "sketch".to_string(),
            size: "1024x1024".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: vec![sample_input_image()],
        };
        let body = xai_edits_body("grok-imagine-image", &one);
        assert_eq!(body["image"]["type"], "image_url");
        assert_eq!(body["image"]["url"], "data:image/png;base64,aGVsbG8=");
        assert!(body.get("images").is_none());
        assert_eq!(body["aspect_ratio"], "1:1");

        let many = ImageGenerationRequest {
            input_images: vec![sample_input_image(), sample_input_image()],
            ..one
        };
        let body = xai_edits_body("grok-imagine-image", &many);
        assert!(body.get("image").is_none());
        assert_eq!(body["images"].as_array().map(|v| v.len()), Some(2));
    }

    #[test]
    fn openai_compat_edits_json_puts_data_urls_on_image() {
        let request = ImageGenerationRequest {
            prompt: "make it night".to_string(),
            size: "1024x1024".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "high".to_string(),
            n: 1,
            input_images: vec![sample_input_image()],
        };
        let body = openai_compat_edits_json_body("gpt-image-1.5", &request);
        assert_eq!(body["image"], "data:image/png;base64,aGVsbG8=");
        assert_eq!(body["size"], "1024x1024");
        assert_eq!(body["quality"], "high");

        let many = ImageGenerationRequest {
            input_images: vec![sample_input_image(), sample_input_image()],
            ..request.clone()
        };
        let body = openai_compat_edits_json_body("gpt-image-1.5", &many);
        assert_eq!(body["image"].as_array().map(|v| v.len()), Some(2));

        let auto = ImageGenerationRequest {
            size: "auto".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            ..request
        };
        let auto_body = openai_compat_edits_json_body("gpt-image-2", &auto);
        assert!(auto_body.get("size").is_none());
        assert!(auto_body.get("quality").is_none());
    }

    fn images_provider(base_url: &str) -> ModelProvider {
        ModelProvider {
            id: "img".to_string(),
            name: "Images".to_string(),
            api_keys: vec!["k".to_string()],
            api_key_legacy: None,
            base_url: base_url.to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        }
    }

    #[test]
    fn image_bodies_omit_optional_auto_and_map_official_sizes() {
        let auto = ImageGenerationRequest {
            prompt: "wallpaper".to_string(),
            size: "auto".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "auto".to_string(),
            n: 1,
            input_images: Vec::new(),
        };
        let landscape = ImageGenerationRequest {
            size: "1536x1024".to_string(),
            aspect_ratio: "auto".to_string(),
            quality: "high".to_string(),
            ..auto.clone()
        };
        let openai = images_provider("https://api.openai.com/v1");
        let openai_auto = images_generations_json_body(&openai, "gpt-image-2", &auto);
        assert!(openai_auto.get("size").is_none());
        assert!(openai_auto.get("quality").is_none());
        assert!(openai_auto.get("background").is_none());
        let openai_size = images_generations_json_body(&openai, "gpt-image-2", &landscape);
        assert_eq!(openai_size["size"], "1536x1024");
        assert_eq!(openai_size["quality"], "high");

        let xai = images_provider("https://api.x.ai/v1");
        let xai_auto = images_generations_json_body(&xai, "grok-imagine-image-2.0", &auto);
        assert_eq!(xai_auto["response_format"], "b64_json");
        assert!(xai_auto.get("aspect_ratio").is_none());
        assert!(xai_auto.get("size").is_none());
        assert!(xai_auto.get("quality").is_none());
        let xai_size = images_generations_json_body(&xai, "grok-imagine-image-2.0", &landscape);
        assert_eq!(xai_size["aspect_ratio"], "3:2");
        assert_eq!(xai_size["resolution"], "1k");
        assert!(xai_size.get("size").is_none());
        assert!(
            xai_size.get("quality").is_none(),
            "xAI official quality is low|medium|auto; high is omitted"
        );

        let widescreen = ImageGenerationRequest {
            size: "2K".to_string(),
            aspect_ratio: "16:9".to_string(),
            quality: "auto".to_string(),
            ..auto.clone()
        };
        let openai_2k = images_generations_json_body(&openai, "gpt-image-2", &widescreen);
        assert_eq!(openai_2k["size"], "2048x1152");
        let openrouter = openrouter_image_config(&widescreen).expect("chat image_config");
        assert_eq!(openrouter["aspect_ratio"], "16:9");
        assert_eq!(openrouter["image_size"], "2K");
        let xai_2k = images_generations_json_body(&xai, "grok-imagine-image-2.0", &widescreen);
        assert_eq!(xai_2k["aspect_ratio"], "16:9");
        assert_eq!(xai_2k["resolution"], "2k");
        let custom = ImageGenerationRequest {
            size: parse_size_arg("1920x1080").expect("exact pixels"),
            ..auto.clone()
        };
        assert_eq!(
            images_generations_json_body(&openai, "gpt-image-2", &custom)["size"],
            "1920x1080"
        );
        let xai_medium = ImageGenerationRequest {
            quality: "medium".to_string(),
            ..landscape.clone()
        };
        let xai_medium_body =
            images_generations_json_body(&xai, "grok-imagine-image-2.0", &xai_medium);
        assert_eq!(xai_medium_body["quality"], "medium");

        let gemini_auto = build_gemini_native_body(&auto);
        assert!(gemini_auto["generationConfig"].get("imageConfig").is_none());
        let gemini_size = build_gemini_native_body(&landscape);
        assert_eq!(
            gemini_size["generationConfig"]["imageConfig"]["aspectRatio"],
            "3:2"
        );
        assert_eq!(
            gemini_size["generationConfig"]["imageConfig"]["imageSize"],
            "1K"
        );
        let gemini_2k = build_gemini_native_body(&widescreen);
        assert_eq!(
            gemini_2k["generationConfig"]["imageConfig"]["aspectRatio"],
            "16:9"
        );
        assert_eq!(
            gemini_2k["generationConfig"]["imageConfig"]["imageSize"],
            "2K"
        );

        let four_k = ImageGenerationRequest {
            size: "4K".to_string(),
            aspect_ratio: "16:9".to_string(),
            ..auto.clone()
        };
        assert_eq!(
            images_generations_json_body(&openai, "gpt-image-2", &four_k)["size"],
            "3840x2160"
        );
        let xai_4k = images_generations_json_body(&xai, "grok-imagine-image-2.0", &four_k);
        assert_eq!(xai_4k["aspect_ratio"], "16:9");
        assert!(xai_4k.get("resolution").is_none());
        assert!(validate_generation(
            &xai,
            "grok-imagine-image-2.0",
            &serde_json::json!({"prompt":"test","size":"4K"}),
            0
        )
        .is_err());
        let gemini_4k = build_gemini_native_body(&four_k);
        assert_eq!(
            gemini_4k["generationConfig"]["imageConfig"]["imageSize"],
            "4K"
        );
        let gemini_phone = ImageGenerationRequest {
            size: "auto".to_string(),
            aspect_ratio: "4:5".to_string(),
            ..auto.clone()
        };
        assert_eq!(
            build_gemini_native_body(&gemini_phone)["generationConfig"]["imageConfig"]
                ["aspectRatio"],
            "4:5"
        );
    }

    #[test]
    fn images_api_error_message_reads_provider_error_object() {
        let value = serde_json::json!({
            "error": {
                "type": "image_generation_user_error",
                "message": "Invalid value: 'auto'. Size must satisfy the documented pixel constraints."
            }
        });
        let message = images_api_error_message(&value).expect("error message");
        assert!(message.contains("Size must satisfy"));
    }

    #[tokio::test]
    async fn edits_preserve_reference_when_proxy_requires_images_image_url() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                let header_end = loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                while request.len() < header_end + length {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                let body: Value =
                    serde_json::from_slice(&request[header_end..header_end + length]).unwrap();
                let (status, response) = if attempt == 0 {
                    assert_eq!(body["image"], "data:image/png;base64,aGVsbG8=");
                    (
                        "400 Bad Request",
                        r#"{"error":{"message":"images[].image_url is required","type":"invalid_request_error"}}"#,
                    )
                } else {
                    assert!(body.get("image").is_none());
                    assert_eq!(
                        body["images"][0]["image_url"],
                        "data:image/png;base64,aGVsbG8="
                    );
                    assert_eq!(body["prompt"], "make it dark blue");
                    ("200 OK", r#"{"data":[{"b64_json":"aGVsbG8="}]}"#)
                };
                socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            }
        });
        let mut provider = gemini_provider();
        provider.base_url = format!("http://{address}/v1");
        provider.api_format = "openai_responses".into();
        let state = crate::state::test_app_state();
        let request = ImageGenerationRequest {
            prompt: "make it dark blue".into(),
            size: "1024x1024".into(),
            aspect_ratio: "auto".into(),
            quality: "auto".into(),
            n: 1,
            input_images: vec![sample_input_image()],
        };
        let result = generate_with_images_edits(
            &state,
            &provider,
            "gpt-image-2",
            &request,
            1,
            "edit regression",
        )
        .await;
        if result.is_err() {
            server.abort();
        }
        assert!(
            result.is_ok(),
            "Reference-preserving edit failed: {}",
            result.err().unwrap_or_default()
        );
        server.await.unwrap();
    }

    #[test]
    fn edit_schema_adaptation_does_not_retry_timeouts_or_unrelated_errors() {
        assert!(!requires_images_image_url("request timed out"));
        assert!(!requires_images_image_url(
            "500 Internal Server Error: images[].image_url is required"
        ));
        assert!(!requires_images_image_url(
            "400 Bad Request: image is invalid"
        ));
    }

    #[test]
    fn loads_input_image_from_temp_file() {
        let dir = std::env::temp_dir().join(format!("kivio-img-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("photo.jpg");
        fs::write(&path, b"fake-jpeg-bytes").expect("write");
        let image = load_input_image_from_path(&path).expect("load");
        assert_eq!(image.mime_type, "image/jpeg");
        assert_eq!(
            general_purpose::STANDARD.decode(image.base64).expect("b64"),
            b"fake-jpeg-bytes"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reference_limits_follow_the_selected_image_model() {
        let provider = gemini_provider();
        for (model, cap) in [
            ("gpt-image-1", 16),
            ("gpt-image-1.5", 16),
            ("gemini-3.1-flash-image", 14),
            ("gemini-3-pro-image-preview", 14),
            ("nano-banana-pro", 14),
            ("gemini-2.5-flash-image", 3),
            ("unknown-image", 4),
        ] {
            assert_eq!(max_reference_images(&provider, model), cap, "{model}");
            assert!(
                reject_excess_reference_images(&provider, model, cap).is_ok(),
                "{model}"
            );
            assert!(
                reject_excess_reference_images(&provider, model, cap + 1).is_err(),
                "{model}"
            );
        }
    }

    #[test]
    fn xai_reference_image_cap_is_five_and_does_not_silently_drop() {
        let xai = ModelProvider {
            id: "xai".to_string(),
            name: "xAI".to_string(),
            api_keys: vec!["k".to_string()],
            api_key_legacy: None,
            base_url: "https://api.x.ai/v1".to_string(),
            available_models: Vec::new(),
            enabled_models: Vec::new(),
            enabled: true,
            api_format: "xai_responses".to_string(),
            model_overrides: std::collections::HashMap::new(),
            compress_request_body: false,
            request: Default::default(),
            active_key_index: 0,
        };
        assert_eq!(max_reference_images(&xai, "grok-imagine-image"), 5);
        assert_eq!(
            max_reference_images(&gemini_provider(), "gemini-3.1-flash-image"),
            14
        );
        let err = reject_excess_reference_images(&xai, "grok-imagine-image", 6)
            .expect_err("xAI must refuse a 6th reference");
        assert!(err.contains("at most 5"), "{err}");
        assert!(err.contains("got 6"), "{err}");
        assert!(reject_excess_reference_images(&xai, "grok-imagine-image", 3).is_ok());
        assert!(
            reject_excess_reference_images(&gemini_provider(), "gemini-3.1-flash-image", 4).is_ok()
        );
        assert!(
            reject_excess_reference_images(&gemini_provider(), "gemini-3.1-flash-image", 15)
                .is_err()
        );

        let four = vec![
            sample_input_image(),
            InputImage {
                mime_type: "image/png".to_string(),
                base64: "two".to_string(),
            },
            InputImage {
                mime_type: "image/png".to_string(),
                base64: "three".to_string(),
            },
            InputImage {
                mime_type: "image/png".to_string(),
                base64: "four".to_string(),
            },
        ];
        let body = xai_edits_body(
            "grok-imagine-image",
            &ImageGenerationRequest {
                prompt: "sketch".to_string(),
                size: "auto".to_string(),
                aspect_ratio: "auto".to_string(),
                quality: "auto".to_string(),
                n: 1,
                input_images: four[..3].to_vec(),
            },
        );
        assert_eq!(body["images"].as_array().map(|v| v.len()), Some(3));
    }
    #[test]
    fn ratio_regression_openai_async_tier_preserves_portrait_dimensions() {
        let provider = images_provider("https://ybw-ai.com");
        let body = image_api_payload(
            &provider,
            "gpt-image-2.5-flare",
            &serde_json::json!({
                "prompt": "A perfume bottle on a windowsill",
                "size": "1K",
                "aspect_ratio": "9:16",
                "quality": "low",
            }),
            1,
        )
        .unwrap();
        assert_eq!(body["size"], "864x1536");
        assert_eq!(body["quality"], "low");
        // OpenAI-compatible async generations and multipart edits express ratio in size.
        assert!(body.get("aspect_ratio").is_none());
        println!("OpenAI async request: {body}");
    }

    #[test]
    fn every_advertised_gpt_tier_and_ratio_produces_valid_pixels() {
        let provider = images_provider("https://api.openai.com/v1");
        let c = image_capabilities(&provider, "gpt-image-2.5-flare");
        for tier in &c.sizes {
            for ratio in &c.ratios {
                let body = image_api_payload(
                    &provider,
                    "gpt-image-2.5-flare",
                    &serde_json::json!({"prompt":"test","size":tier,"aspect_ratio":ratio}),
                    0,
                )
                .unwrap();
                let (w, h) = parse_pixel_pair(body["size"].as_str().unwrap()).unwrap();
                validate_openai_pixels(w, h).unwrap();
                assert!(
                    (w as f64 / h as f64 / aspect_ratio_value(ratio) - 1.0).abs() < 0.02,
                    "{tier} {ratio}: {w}x{h}"
                );
            }
        }
        assert!(validate_generation(
            &provider,
            "gpt-image-2",
            &serde_json::json!({"prompt":"test","size":"1920x1080"}),
            0
        )
        .is_err());
    }

    #[test]
    fn official_media_contract_regressions_reject_silent_changes_and_accept_current_options() {
        let xai = images_provider("https://api.x.ai/v1");
        assert!(validate_generation(
            &xai,
            "grok-imagine-image-2.0",
            &serde_json::json!({"prompt":"test", "size":"4K"}),
            0
        )
        .is_err());
        assert!(validate_generation(
            &xai,
            "grok-imagine-image-2.0",
            &serde_json::json!({"prompt":"test", "quality":"high"}),
            0
        )
        .is_err());
        assert!(validate_generation(
            &xai,
            "grok-imagine-image-2.0",
            &serde_json::json!({"prompt":"test"}),
            5
        )
        .is_ok());
        let openai = images_provider("https://api.openai.com/v1");
        assert!(validate_generation(
            &openai,
            "gpt-image-1.5",
            &serde_json::json!({"prompt":"test", "size":"2K"}),
            0
        )
        .is_err());
        assert!(validate_generation(
            &openai,
            "gpt-image-2.5-flare",
            &serde_json::json!({"prompt":"test", "quality":"max"}),
            0
        )
        .is_ok());
        assert!(validate_generation(
            &openai,
            "gpt-image-2",
            &serde_json::json!({"prompt":"test", "quality":"max"}),
            0
        )
        .is_err());
        assert!(has_known_direct_image_generation_route(
            &gemini_provider(),
            "gemini-3.1-flash-image"
        ));
        assert!(validate_generation(
            &gemini_provider(),
            "gemini-3-pro-image-preview",
            &serde_json::json!({"prompt":"test", "size":"512"}),
            0
        )
        .is_err());
        assert!(validate_generation(
            &gemini_provider(),
            "gemini-3.1-flash-lite-image",
            &serde_json::json!({"prompt":"test", "size":"2K"}),
            0
        )
        .is_err());
        let gateway = images_provider("https://ybw-ai.com/v1");
        assert_eq!(
            image_capabilities(&gateway, "gpt-image-2.5-flare").max_count,
            1
        );
        assert!(validate_generation(
            &gateway,
            "gpt-image-2.5-flare",
            &serde_json::json!({"prompt":"test", "n":2}),
            0
        )
        .is_err());
    }
}
