//! Shared asynchronous image gateway adapter, including legacy receipt recovery.
use crate::{settings::ModelProvider, state::AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Response};
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
    let req = crate::provider_request::apply(req, p, Some(task_id));
    // dsimage always sends a browser UA. Cloudflare on this gateway 403s
    // default library UAs; only add ours when the provider did not set one.
    if crate::provider_request::header_pairs(p, Some(task_id))
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
    {
        req
    } else {
        req.header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
    }
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
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v
            .pointer("/error/message")
            .or_else(|| v.pointer("/message"))
            .and_then(Value::as_str)
        {
            let clean: String = msg.chars().take(160).collect();
            if !clean.is_empty() {
                return format!("图片接口拒绝请求：{clean}");
            }
        }
    }
    format!("图片接口 HTTP {status}")
}

pub async fn poll(
    state: &AppState,
    cfg: &ReceiptConfig,
    task_id: &str,
    remote_id: &str,
) -> Result<Option<Vec<u8>>, String> {
    if remote_id.is_empty()
        || !remote_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("远程任务 ID 无效".into());
    }
    let p = provider(state, cfg)?;
    let v = json_response(
        request(
            state,
            &p,
            reqwest::Method::GET,
            &endpoint(&p, &async_poll_path(&p.base_url, &cfg.model, remote_id))?,
            cfg,
            task_id,
        )
        .send()
        .await,
        &p,
    )
    .await?;
    match remote_task_status(&v).as_str() {
        "completed" | "succeeded" | "success" => Ok(Some(extract(state, &v).await?)),
        "failed" | "cancelled" | "canceled" | "error" => {
            let detail = v
                .pointer("/error/message")
                .or_else(|| v.pointer("/data/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("请在供应商后台核对原因");
            Err(format!("远程图片任务失败：{detail}；可单独重新生成该页。"))
        }
        "pending" | "queued" | "running" | "processing" | "submitted" | "in_progress" => Ok(None),
        other => Err(if other.is_empty() {
            "无法识别远程任务状态。已保留任务 ID，可稍后恢复查询。".into()
        } else {
            format!("无法识别远程任务状态（{other}）。已保留任务 ID，可稍后恢复查询。")
        }),
    }
}

pub(crate) fn remote_task_status(v: &Value) -> String {
    ["/status", "/data/status", "/data/0/status"]
        .into_iter()
        .find_map(|path| v.pointer(path).and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

async fn extract(state: &AppState, v: &Value) -> Result<Vec<u8>, String> {
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
    download(state, u).await
}

pub(crate) async fn download(state: &AppState, u: &str) -> Result<Vec<u8>, String> {
    let url = reqwest::Url::parse(u).map_err(|_| "图片下载 URL 无效")?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("图片下载必须使用 HTTP(S) URL".into());
    }
    // Deliberately no provider headers / credentials on a CDN download.
    let client: Client = state.http.clone();
    let r = client
        .get(url)
        .timeout(Duration::from_secs(180))
        .send()
        .await
        .map_err(|_| "图片下载失败；已受理的任务请恢复查询，避免重复提交")?;
    if !r.status().is_success() {
        return Err(format!("图片下载 HTTP {}", r.status()));
    }
    body(r, 50 * 1024 * 1024).await
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

fn images_size(model: &str, ratio: &str, resolution: &str) -> String {
    let name = model.to_ascii_lowercase();
    if name.contains("gpt-image-2") {
        return gpt_image_size(ratio, resolution);
    }
    if name.contains("dall-e-3") {
        return match ratio {
            "9:16" => "1024x1792",
            "16:9" => "1792x1024",
            _ => "1024x1024",
        }
        .into();
    }
    match ratio {
        "2:3" | "3:4" | "4:5" | "9:16" => "1024x1536",
        "3:2" | "4:3" | "5:4" | "16:9" => "1536x1024",
        _ => "1024x1024",
    }
    .into()
}

/// Apimart-style gateways take `size` as a ratio. gpt-image / DALL·E on new-api
/// relays reject that (`size must be 16-multiple... : 1:1`) and want WxH.
pub(super) fn async_generation_payload(
    model: &str,
    prompt: &str,
    ratio: &str,
    resolution: &str,
    image_urls: &[String],
) -> Value {
    // Official dsimage sync body for gpt-image / DALL·E is model + prompt + n +
    // pixel size. It only adds quality when the user passes --quality. Do not
    // invent quality/high here — ybw-ai / new-api reject extra fields upstream.
    let mut v = if let Some(size) = async_images_size(model, ratio, resolution) {
        json!({"model":model,"prompt":prompt,"n":1,"size":size})
    } else {
        json!({"model":model,"prompt":prompt,"n":1,"size":ratio,"resolution":resolution})
    };
    if !image_urls.is_empty() {
        v["image_urls"] = json!(image_urls);
    }
    v
}

fn async_images_size(model: &str, ratio: &str, resolution: &str) -> Option<String> {
    let name = model.to_ascii_lowercase();
    if name.contains("gpt-image") || name.starts_with("dall-e") {
        return Some(images_size(model, ratio, resolution));
    }
    None
}

pub(super) fn remote_task_id(v: &Value) -> Option<String> {
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

pub(crate) fn uses_gateway(base: &str, model: &str) -> bool {
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
    media: &super::MediaRequest,
) -> Result<Value, String> {
    let images = super::image_inputs(&media.images)?;
    let cfg = ReceiptConfig {
        provider_id: p.id.clone(),
        model: media.model.clone(),
        protocol: "async".into(),
    };
    let ratio = media
        .options
        .get("aspect_ratio")
        .and_then(Value::as_str)
        .unwrap_or("1:1");
    let size = media
        .options
        .get("size")
        .and_then(Value::as_str)
        .unwrap_or("1K")
        .to_lowercase();
    if media.options.get("n").and_then(Value::as_u64).unwrap_or(1) != 1 {
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
    if !images.is_empty() && uses_openai_async_task(&p.base_url, &media.model) {
        let mut form = reqwest::multipart::Form::new()
            .text("model", media.model.clone())
            .text("prompt", media.prompt.clone())
            .text("n", "1")
            .text("size", images_size(&media.model, ratio, &size))
            .text("quality", "high")
            .text("output_format", "png");
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
            .map(crate::chat::image_generation::data_url_for_input)
            .collect::<Vec<_>>();
        req = req.json(&async_generation_payload(
            &media.model,
            &media.prompt,
            ratio,
            &size,
            &urls,
        ));
    }
    json_response(req.send().await, p).await
}
pub(crate) async fn immediate_image(state: &AppState, value: &Value) -> Result<Vec<u8>, String> {
    extract(state, value).await
}

pub(super) fn gpt_image_size(ratio: &str, resolution: &str) -> String {
    match (ratio, resolution) {
        ("1:1", "4k") => "2880x2880",
        ("1:1", "2k") => "2048x2048",
        ("2:3", "4k") => "2352x3520",
        ("2:3", "2k") => "1360x2048",
        ("2:3", _) => "1024x1536",
        ("3:2", "4k") => "3520x2352",
        ("3:2", "2k") => "2048x1360",
        ("3:2", _) => "1536x1024",
        ("3:4", "4k") => "2480x3312",
        ("3:4", "2k") => "1536x2048",
        ("3:4", _) => "1024x1360",
        ("4:3", "4k") => "3312x2480",
        ("4:3", "2k") => "2048x1536",
        ("4:3", _) => "1360x1024",
        ("4:5", "4k") => "2560x3200",
        ("4:5", "2k") => "1632x2048",
        ("4:5", _) => "1024x1280",
        ("5:4", "4k") => "3200x2560",
        ("5:4", "2k") => "2048x1632",
        ("5:4", _) => "1280x1024",
        ("9:16", "2k") => "1152x2048",
        ("9:16", "4k") => "2160x3840",
        ("9:16", _) => "864x1536",
        ("16:9", "2k") => "2048x1152",
        ("16:9", "4k") => "3840x2160",
        ("16:9", _) => "1536x864",
        _ => "1024x1024",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_routing_does_not_match_lookalike_hosts() {
        assert!(uses_gateway("https://api.ybw-ai.com/v1", "gpt-image-1"));
        assert!(uses_gateway("https://api.apimart.ai/v1", "gemini-image"));
        for url in [
            "https://ybw-ai.com.evil.test/v1",
            "https://elsewhere.test/ybw-ai.com",
            "https://api.openai.com/v1",
        ] {
            assert!(!uses_gateway(url, "gpt-image-1"));
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
