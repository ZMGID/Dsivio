//! `ImageRoute::AsyncTask`: gateways that accept an image job and return a receipt to poll
//! (apimart, ybw `gpt-image`/DALL·E). Also reads receipts saved by older Workbench tasks.
use crate::{settings::ModelProvider, state::AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Response;
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) struct ReceiptConfig {
    pub provider_id: String,
    pub model: String,
    pub protocol: String,
}
pub(crate) fn provider(state: &AppState, cfg: &ReceiptConfig) -> Result<ModelProvider, String> {
    state
        .settings_read()
        .get_provider(&cfg.provider_id)
        .filter(|p| p.enabled && p.preferred_api_key().is_some())
        .cloned()
        .ok_or("图片供应商未配置 API Key，或已被停用".into())
}

fn endpoint(p: &ModelProvider, suffix: &str) -> Result<String, String> {
    let mut u = reqwest::Url::parse(p.base_url.trim()).map_err(|_| "供应商地址无效")?;
    if !matches!(u.scheme(), "http" | "https") || !u.username().is_empty() || u.password().is_some()
    {
        return Err("供应商地址必须是 HTTP(S) URL".into());
    }
    let path = u.path().trim_end_matches('/');
    let path = if path.is_empty() { "/v1" } else { path };
    u.set_path(&format!("{path}/{suffix}"));
    u.set_query(None);
    u.set_fragment(None);
    Ok(u.to_string())
}

fn request(
    state: &AppState,
    p: &ModelProvider,
    method: reqwest::Method,
    url: &str,
    cfg: &ReceiptConfig,
    task_id: &str,
) -> reqwest::RequestBuilder {
    let req = state
        .client_for(p)
        .request(method, url)
        .timeout(Duration::from_secs(600));
    let req = if cfg.protocol == "gemini" {
        req.header("x-goog-api-key", p.preferred_api_key().unwrap_or_default())
    } else {
        req.bearer_auth(p.preferred_api_key().unwrap_or_default())
    };
    crate::media_generation::with_provider_headers(req, p, Some(task_id))
}

async fn body(mut r: Response, max: usize) -> Result<Vec<u8>, String> {
    if r.content_length().unwrap_or(0) > max as u64 {
        return Err("接口返回内容过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| "下载图片中断，可尝试恢复远程任务")?
    {
        if bytes.len() + chunk.len() > max {
            return Err("接口返回内容过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn json_response(
    r: Result<Response, reqwest::Error>,
    p: &ModelProvider,
) -> Result<Value, String> {
    let r = r.map_err(|_| {
        "图片接口连接中断或超时。服务端可能已受理，请检查供应商记录后再重试，避免重复计费。"
    })?;
    let status = r.status();
    let bytes = body(r, 80 * 1024 * 1024).await?;
    if !status.is_success() {
        let mut detail = String::from_utf8_lossy(&bytes).to_string();
        for key in &p.api_keys {
            if !key.is_empty() {
                detail = detail.replace(key, "[redacted]");
            }
        }
        return Err(explain_image_http_error(status.as_u16(), &detail));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "图片接口没有返回 JSON，请核对协议和供应商地址".into())
}

pub(crate) fn explain_image_http_error(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if status == 504 || lower.contains("gateway timeout") || lower.contains("error code: 504") {
        return "图片生成超时，提交结果未确认，请先核对供应商记录，避免重复计费。".into();
    }
    if lower.contains("16-multiple")
        || lower.contains("image-2 size")
        || (lower.contains("invalid_request") && lower.contains("size") && lower.contains("1:1"))
    {
        return "该模型需要像素尺寸（如 1024x1024），不能把画幅比例直接当作 size。请重新生成。"
            .into();
    }
    // Keep "HTTP {status}" parseable: submission failures are classified by status code.
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/message"))
                .and_then(Value::as_str)
                .map(|msg| msg.chars().take(160).collect::<String>())
        })
        .filter(|msg| !msg.is_empty());
    match detail {
        Some(detail) => format!("图片接口 HTTP {status}：{detail}"),
        None => format!("图片接口 HTTP {status}"),
    }
}

/// One read of an accepted asynchronous image receipt.
pub(crate) enum ReceiptRead {
    Pending,
    /// The provider did not return this receipt (404) or was briefly unavailable.
    Unseen(u16),
    Ready(Vec<u8>),
    /// The provider reported a terminal failure; querying again will not change it.
    Failed(String),
}

pub(crate) async fn poll(
    state: &AppState,
    cfg: &ReceiptConfig,
    task_id: &str,
    remote_id: &str,
) -> Result<ReceiptRead, String> {
    if remote_id.is_empty()
        || !remote_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("远程任务 ID 无效".into());
    }
    let p = provider(state, cfg)?;
    let response = request(
        state,
        &p,
        reqwest::Method::GET,
        &endpoint(&p, &async_poll_path(&p.base_url, &cfg.model, remote_id))?,
        cfg,
        task_id,
    )
    .send()
    .await;
    if let Ok(r) = &response {
        let code = r.status().as_u16();
        if matches!(code, 404 | 408 | 429 | 500..=599) {
            return Ok(ReceiptRead::Unseen(code));
        }
    }
    let v = json_response(response, &p).await?;
    Ok(match remote_task_status(&v).as_str() {
        "completed" | "succeeded" | "success" => ReceiptRead::Ready(extract(&p, &v).await?),
        "pending" | "queued" | "running" | "processing" | "submitted" | "in_progress" => {
            ReceiptRead::Pending
        }
        "failed" | "cancelled" | "canceled" | "error" => {
            let detail = v
                .pointer("/error/message")
                .or_else(|| v.pointer("/data/error/message"))
                .or_else(|| v.pointer("/data/fail_reason"))
                .and_then(Value::as_str)
                .unwrap_or("供应商未说明原因");
            ReceiptRead::Failed(format!("远程图片任务失败：{detail}"))
        }
        other => ReceiptRead::Failed(format!("远程图片任务返回未识别状态（{other}）")),
    })
}

pub(crate) fn remote_task_status(v: &Value) -> String {
    ["/status", "/data/status", "/data/0/status"]
        .into_iter()
        .find_map(|path| v.pointer(path).and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

const IMAGE_RESULT_LIMIT: usize = 50 * 1024 * 1024;

async fn extract(p: &ModelProvider, v: &Value) -> Result<Vec<u8>, String> {
    if let Some(s) = v.pointer("/data/0/b64_json").and_then(Value::as_str) {
        return STANDARD
            .decode(s)
            .map_err(|_| "接口图片 Base64 无效".into());
    }
    if let Some(parts) = v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
    {
        for part in parts {
            if let Some(s) = part
                .pointer("/inlineData/data")
                .or_else(|| part.pointer("/inline_data/data"))
                .and_then(Value::as_str)
            {
                return STANDARD.decode(s).map_err(|_| "Gemini 图片编码无效".into());
            }
        }
    }
    if let Some(s) = v
        .pointer("/predictions/0/bytesBase64Encoded")
        .and_then(Value::as_str)
    {
        return STANDARD.decode(s).map_err(|_| "Imagen 图片编码无效".into());
    }
    if let Some(s) = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
    {
        if let Some(start) = s.find(";base64,") {
            let encoded: String = s[start + 8..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
                .collect();
            return STANDARD
                .decode(encoded)
                .map_err(|_| "Gemini Chat 图片编码无效".into());
        }
    }
    let u = image_download_url(v).ok_or("接口没有返回图片；请核对所选模型是否支持图片生成")?;
    download(p, u).await
}

/// Download a gateway image result. CDN results get no provider credentials.
pub(crate) async fn download(p: &ModelProvider, u: &str) -> Result<Vec<u8>, String> {
    let origin = reqwest::Url::parse(p.base_url.trim()).map_err(|_| "供应商地址无效")?;
    let response = crate::media_generation::artifacts::fetch(
        crate::media_generation::artifacts::download_client(p),
        &origin,
        u,
        crate::media_generation::artifacts::DownloadAuth::None,
        Duration::from_secs(180),
    )
    .await?;
    crate::media_generation::artifacts::read_bounded(response, IMAGE_RESULT_LIMIT).await
}

pub(crate) fn image_download_url(v: &Value) -> Option<&str> {
    v.pointer("/data/0/url")
        .or_else(|| v.pointer("/result/data/0/url"))
        .or_else(|| v.pointer("/image_url"))
        // dsimage async returns images[].url as an array, not a string.
        .or_else(|| v.pointer("/data/result/images/0/url/0"))
        .or_else(|| v.pointer("/data/result/images/0/url"))
        .or_else(|| v.pointer("/data/result/images/0"))
        .and_then(Value::as_str)
}

fn uses_openai_async_task(base_url: &str, model: &str) -> bool {
    let base = base_url.to_ascii_lowercase();
    let name = model.to_ascii_lowercase();
    !base.contains("apimart") && (name.contains("gpt-image") || name.starts_with("dall-e"))
}

pub(crate) fn async_poll_path(base_url: &str, model: &str, remote_id: &str) -> String {
    if uses_openai_async_task(base_url, model) {
        format!("images/tasks/{remote_id}")
    } else {
        format!("tasks/{remote_id}")
    }
}

pub(crate) fn remote_task_id(v: &Value) -> Option<String> {
    ["id", "task_id"]
        .into_iter()
        .filter_map(|k| v.get(k).and_then(Value::as_str))
        .chain(
            ["/data/0/task_id", "/data/task_id", "/data/0/id", "/data/id"]
                .into_iter()
                .filter_map(|p| v.pointer(p).and_then(Value::as_str)),
        )
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

pub(super) fn async_submit_path(
    base_url: &str,
    model: &str,
    has_reference_images: bool,
) -> &'static str {
    if uses_openai_async_task(base_url, model) {
        if has_reference_images {
            "images/edits/async"
        } else {
            "images/generations/async"
        }
    } else {
        "images/generations"
    }
}

pub(super) fn openai_edit_image_field(model: &str, image_count: usize) -> &'static str {
    if model.to_ascii_lowercase().contains("gpt-image") || image_count > 1 {
        "image[]"
    } else {
        "image"
    }
}

pub(super) fn is_async_gateway(base: &str, model: &str) -> bool {
    let host = reqwest::Url::parse(base)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
        .to_lowercase();
    let domain = |v: &str| host == v || host.ends_with(&format!(".{v}"));
    domain("apimart.ai")
        || (domain("ybw-ai.com") && (model.contains("gpt-image") || model.starts_with("dall-e")))
}
pub(crate) async fn submit(
    state: &AppState,
    p: &ModelProvider,
    id: &str,
    media: &crate::media_generation::MediaRequest,
) -> Result<Value, String> {
    let images = crate::media_generation::image_inputs(&media.images)?;
    let cfg = ReceiptConfig {
        provider_id: p.id.clone(),
        model: media.model.clone(),
        protocol: "async".into(),
    };
    // Public arguments are canonical; only the adapter translates vendor field names.
    let arguments = crate::media_generation::image_arguments(p, media)?;
    let ratio = arguments["aspectRatio"]
        .as_str()
        .unwrap_or("1:1")
        .to_owned();
    let size = arguments["size"].as_str().unwrap_or("1K").to_lowercase();
    if arguments["n"].as_u64().unwrap_or(1) != 1 {
        return Err("异步图片任务每次生成一张，请按任务分别提交".into());
    }
    let mut req = request(
        state,
        p,
        reqwest::Method::POST,
        &endpoint(
            p,
            async_submit_path(&p.base_url, &media.model, !images.is_empty()),
        )?,
        &cfg,
        id,
    );
    let body = super::image_api_payload(p, &media.model, &arguments, images.len())?;
    if !images.is_empty() && uses_openai_async_task(&p.base_url, &media.model) {
        let mut form = reqwest::multipart::Form::new();
        for key in ["model", "prompt", "n", "size", "quality", "background", "output_format"] {
            if let Some(value) = body.get(key) {
                form = form.text(
                    key,
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                );
            }
        }
        for (i, image) in images.iter().enumerate() {
            let part = reqwest::multipart::Part::bytes(
                STANDARD.decode(&image.base64).map_err(|_| "无效图片编码")?,
            )
            .file_name(format!("reference-{i}.png"))
            .mime_str(&image.mime_type)
            .map_err(|e| e.to_string())?;
            form = form.part(openai_edit_image_field(&media.model, images.len()), part);
        }
        req = req.multipart(form);
    } else {
        let urls = images
            .iter()
            .map(super::data_url_for_input)
            .collect::<Vec<_>>();
        let mut payload = body;
        if !uses_openai_async_task(&p.base_url, &media.model) {
            payload["size"] = json!(ratio);
            payload["resolution"] = json!(size);
        }
        if !urls.is_empty() {
            payload["image_urls"] = json!(urls);
        }
        req = req.json(&payload);
    }
    json_response(req.send().await, p).await
}
pub(crate) async fn immediate_image(p: &ModelProvider, value: &Value) -> Result<Vec<u8>, String> {
    extract(p, value).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_routing_does_not_match_lookalike_hosts() {
        assert!(is_async_gateway("https://api.ybw-ai.com/v1", "gpt-image-1"));
        assert!(is_async_gateway(
            "https://api.apimart.ai/v1",
            "gemini-image"
        ));
        for url in [
            "https://ybw-ai.com.evil.test/v1",
            "https://elsewhere.test/ybw-ai.com",
            "https://api.openai.com/v1",
        ] {
            assert!(!is_async_gateway(url, "gpt-image-1"));
        }
        assert_eq!(
            async_poll_path("https://ybw-ai.com", "gpt-image-1", "r1"),
            "images/tasks/r1"
        );
        assert_eq!(
            async_poll_path("https://apimart.ai", "gemini-image", "r1"),
            "tasks/r1"
        );
        assert_eq!(
            remote_task_id(&json!({"data":[{"task_id":"receipt"}]})).as_deref(),
            Some("receipt")
        );
    }
}
