//! Native image adapters derived from dsimage's generation contracts.
//! Submissions are never automatically retried: a timeout may still be billable.
use super::{storage, types::*};
use crate::{
    settings::{ModelProvider, ProviderApiFormat, Settings},
    state::AppState,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures::{future::BoxFuture, FutureExt};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{fs, time::Duration};
use tauri::{AppHandle, Manager};

pub enum Submission {
    Image(Vec<u8>),
    Pending(String),
    Download(String),
}

pub(super) struct NativeBackend<'a> {
    pub app: &'a AppHandle,
    pub cfg: &'a StudioConfig,
    pub task_id: &'a str,
    pub brief: &'a Brief,
}

impl super::generation::Backend for NativeBackend<'_> {
    fn submit<'a>(&'a self, plan: &'a ImagePlan) -> BoxFuture<'a, Result<Submission, String>> {
        submit(self.app, self.cfg, self.task_id, self.brief, plan).boxed()
    }

    fn poll<'a>(
        &'a self,
        cfg: &'a StudioConfig,
        remote: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, String>> {
        poll(self.app, cfg, self.task_id, remote).boxed()
    }

    fn download<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        download(self.app, url).boxed()
    }

    fn store(&self, result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
        store_image(self.task_id, result, bytes)
    }
}

/// Studio still stores a protocol string; resolve it from the live provider + model
/// so a stale `openai` config cannot keep gpt-image on a relay in a sync wait.
/// Grok / Gemini detection copies dsimage `detect_mode` + `_provider_from_model`.
pub fn resolve_protocol(provider: &ModelProvider, model: &str) -> String {
    let name = model.to_ascii_lowercase();
    let base = provider.base_url.to_ascii_lowercase();
    if provider.api_format_kind() == ProviderApiFormat::Gemini {
        return "gemini".into();
    }
    if provider.api_format_kind() == ProviderApiFormat::XaiResponses
        || name.starts_with("grok")
        || name.contains("grok-imagine")
        || host_matches(&base, "api.x.ai")
    {
        return "grok".into();
    }
    if (name.contains("gemini") && name.contains("image"))
        || name.contains("nano-banana")
        || name.starts_with("imagen")
    {
        return if host_matches(&base, "ybw-ai.com") {
            "gemini-chat".into()
        } else {
            "gemini".into()
        };
    }
    if uses_async_image_gateway(&base, &name) {
        return "async".into();
    }
    "openai".into()
}

fn host_matches(base: &str, domain: &str) -> bool {
    let host = base
        .split("://")
        .nth(1)
        .unwrap_or(base)
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn uses_async_image_gateway(base: &str, model: &str) -> bool {
    if base.contains("apimart") {
        return true;
    }
    let official_openai = base.contains("api.openai.com");
    let images_api_model = model.contains("gpt-image") || model.starts_with("dall-e");
    images_api_model && !official_openai
}

pub fn apply_resolved_protocol(cfg: &mut StudioConfig, provider: &ModelProvider) {
    cfg.protocol = resolve_protocol(provider, &cfg.model);
}

/// Rewrite a stored studio protocol from the live provider + model.
/// Returns true when the on-disk value was stale.
pub fn sync_protocol_from_settings(settings: &Settings, cfg: &mut StudioConfig) -> bool {
    let Some(p) = settings.get_provider(&cfg.provider_id) else {
        return false;
    };
    let next = resolve_protocol(p, &cfg.model);
    if cfg.protocol == next {
        return false;
    }
    cfg.protocol = next;
    true
}

pub fn validate(cfg: &StudioConfig, brief: &Brief) -> Result<(), String> {
    if cfg.model.trim().is_empty() || cfg.provider_id.is_empty() {
        return Err("请先在图片设置中选择图片供应商和模型".into());
    }
    if !matches!(
        cfg.protocol.as_str(),
        "openai" | "grok" | "gemini" | "gemini-chat" | "async"
    ) {
        return Err("请选择图片接口协议".into());
    }
    if !matches!(
        brief.ratio.as_str(),
        "1:1" | "2:3" | "3:2" | "3:4" | "4:3" | "4:5" | "5:4" | "9:16" | "16:9"
    ) {
        return Err("不支持的画幅".into());
    }
    if !matches!(brief.resolution.as_str(), "1k" | "2k" | "4k") {
        return Err("不支持的生成清晰度".into());
    }
    if !allowed_image_output(&cfg.model, &cfg.protocol, &brief.ratio, &brief.resolution) {
        let name = cfg.model.to_ascii_lowercase();
        if name.contains("gpt-image-2") {
            return Err("gpt-image-2 请使用官方尺寸：1:1、2:3、3:2、9:16、16:9".into());
        }
        if cfg.protocol == "openai" {
            return Err("OpenAI 标准接口支持 1:1 / 2:3 / 3:2、标准尺寸；请调整画幅或切换兼容网关协议。交付尺寸可在导出时设置。".into());
        }
        if cfg.protocol == "grok" {
            return Err("Grok 不支持所选画幅或 4K，请调整设置".into());
        }
        return Err("请选择当前模型支持的分辨率".into());
    }
    Ok(())
}

fn allowed_image_output(model: &str, protocol: &str, ratio: &str, resolution: &str) -> bool {
    let name = model.to_ascii_lowercase();
    if name.contains("gpt-image-2") {
        return matches!(
            (ratio, resolution),
            ("1:1", "1k")
                | ("1:1", "2k")
                | ("2:3", "1k")
                | ("3:2", "1k")
                | ("9:16", "1k")
                | ("9:16", "2k")
                | ("9:16", "4k")
                | ("16:9", "1k")
                | ("16:9", "2k")
                | ("16:9", "4k")
        );
    }
    if name.contains("dall-e-3") {
        return resolution == "1k" && matches!(ratio, "1:1" | "9:16" | "16:9");
    }
    if name.contains("gpt-image") || name.starts_with("dall-e") || protocol == "openai" {
        return resolution == "1k" && matches!(ratio, "1:1" | "2:3" | "3:2");
    }
    if protocol == "grok" || name.starts_with("grok") || name.contains("grok-imagine") {
        return matches!(
            grok_ratio(ratio).as_str(),
            "1:1" | "2:3" | "3:4" | "9:16" | "3:2" | "4:3" | "16:9"
        ) && grok_resolution(resolution) != "4k";
    }
    if protocol == "gemini"
        || protocol == "gemini-chat"
        || name.contains("gemini")
        || name.starts_with("imagen")
    {
        return resolution != "4k" && matches!(ratio, "1:1" | "3:4" | "9:16" | "4:3" | "16:9");
    }
    resolution != "4k" && matches!(ratio, "1:1" | "9:16" | "16:9")
}

pub(super) fn provider(app: &AppHandle, cfg: &StudioConfig) -> Result<ModelProvider, String> {
    app.state::<AppState>()
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
    app: &AppHandle,
    p: &ModelProvider,
    method: reqwest::Method,
    url: &str,
    cfg: &StudioConfig,
    task_id: &str,
) -> reqwest::RequestBuilder {
    let state = app.state::<AppState>();
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
fn data_uri(path: &str) -> Result<String, String> {
    let bytes = fs::read(storage::resolve(path)?).map_err(|e| e.to_string())?;
    let fmt = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    let mime = match fmt {
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
pub async fn submit(
    app: &AppHandle,
    cfg: &StudioConfig,
    task_id: &str,
    brief: &Brief,
    plan: &ImagePlan,
) -> Result<Submission, String> {
    let p = provider(app, cfg)?;
    let mut resolved = cfg.clone();
    apply_resolved_protocol(&mut resolved, &p);
    let cfg = &resolved;
    validate(cfg, brief)?;
    if plan.refs.len() > if cfg.protocol == "grok" { 5 } else { 16 } {
        return Err("参考图数量超出该接口限制，请减少参考素材".into());
    }
    let images: Vec<String> = plan
        .refs
        .iter()
        .map(|p| data_uri(p))
        .collect::<Result<_, _>>()?;
    let refs = !images.is_empty();
    let suffix = match cfg.protocol.as_str() {
        "gemini" => format!(
            "models/{}:generateContent",
            cfg.model.rsplit('/').next().unwrap_or(&cfg.model)
        ),
        "gemini-chat" => "chat/completions".into(),
        "async" => async_submit_path(&p.base_url, &cfg.model).into(),
        _ => {
            if refs {
                "images/edits".into()
            } else {
                "images/generations".into()
            }
        }
    };
    let mut req = request(
        app,
        &p,
        reqwest::Method::POST,
        &endpoint(&p, &suffix)?,
        cfg,
        task_id,
    );
    let b = brief;
    if cfg.protocol == "openai" && refs {
        let size = images_size(&cfg.model, &b.ratio, &b.resolution);
        let mut form = reqwest::multipart::Form::new()
            .text("model", cfg.model.clone())
            .text("prompt", plan.prompt.clone())
            .text("n", "1")
            .text("size", size)
            .text("quality", "high");
        for (i, path) in plan.refs.iter().enumerate() {
            let bytes = fs::read(storage::resolve(path)?).map_err(|e| e.to_string())?;
            let mime = images[i]
                .split(';')
                .next()
                .unwrap_or("data:image/png")
                .trim_start_matches("data:");
            let ext = if mime == "image/jpeg" {
                "jpg"
            } else if mime == "image/webp" {
                "webp"
            } else {
                "png"
            };
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(format!("reference-{i}.{ext}"))
                .mime_str(mime)
                .map_err(|e| e.to_string())?;
            form = form.part(
                if plan.refs.len() == 1 {
                    "image"
                } else {
                    "image[]"
                },
                part,
            );
        }
        req = req.multipart(form);
    } else {
        let payload = match cfg.protocol.as_str() {
            "openai" => {
                json!({"model":cfg.model,"prompt":plan.prompt,"n":1,"size":images_size(&cfg.model, &b.ratio, &b.resolution),"quality":"high"})
            }
            "grok" => grok_generation_payload(&cfg.model, &plan.prompt, &b.ratio, &b.resolution, &images),
            "gemini" => {
                let mut parts = vec![json!({"text":plan.prompt})];
                for u in &images {
                    let (header, data) = u.split_once(',').ok_or("图片编码失败")?;
                    parts.push(json!({"inline_data":{"mime_type":header.trim_start_matches("data:").trim_end_matches(";base64"),"data":data}}));
                }
                json!({"contents":[{"parts":parts}],"generationConfig":{"responseModalities":["TEXT","IMAGE"],"responseFormat":{"image":{"aspectRatio":b.ratio,"imageSize":b.resolution.to_uppercase()}}}})
            }
            "gemini-chat" => {
                let mut content = vec![json!({"type":"text","text":plan.prompt})];
                for u in &images {
                    content.push(json!({"type":"image_url","image_url":{"url":u}}));
                }
                json!({"model":cfg.model,"messages":[{"role":"user","content":content}],"stream":false,"generationConfig":{"responseModalities":["IMAGE"],"imageConfig":{"aspectRatio":b.ratio,"imageSize":b.resolution.to_uppercase()}}})
            }
            _ => async_generation_payload(&cfg.model, &plan.prompt, &b.ratio, &b.resolution, &images),
        };
        req = req.json(&payload);
    }
    let v = json_response(req.send().await, &p).await?;
    if let Some(url) = image_download_url(&v) {
        return Ok(Submission::Download(url.into()));
    }
    if let Some(id) = remote_task_id(&v) {
        return Ok(Submission::Pending(id));
    }
    Ok(Submission::Image(extract(app, &v).await?))
}
pub(super) fn explain_image_http_error(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if status == 504 || lower.contains("gateway timeout") || lower.contains("error code: 504") {
        return "图片生成超时，兼容网关无法同步等待出图。请重新生成。".into();
    }
    if lower.contains("16-multiple")
        || lower.contains("image-2 size")
        || (lower.contains("invalid_request") && lower.contains("size") && lower.contains("1:1"))
    {
        return "该模型需要像素尺寸（如 1024x1024），不能把画幅比例直接当作 size。请重新生成。".into();
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

/// Official gpt-image-2 sizes from the OpenAI image-prompting guide, plus `1536x864`.
pub(super) fn gpt_image_size(ratio: &str, resolution: &str) -> String {
    match (ratio, resolution) {
        ("1:1", "2k") => "2048x2048",
        ("2:3", _) => "1024x1536",
        ("3:2", _) => "1536x1024",
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

/// dsimage `GROK_RATIO_FALLBACK` + `GROK_RATIOS`. Unknown ratios become 1:1.
pub(super) fn grok_ratio(ratio: &str) -> String {
    let mapped = match ratio {
        "5:4" => "4:3",
        "4:5" => "3:4",
        "9:21" => "9:16",
        other => other,
    };
    match mapped {
        "1:1" | "16:9" | "9:16" | "4:3" | "3:4" | "3:2" | "2:3" | "2:1" | "1:2" | "19.5:9"
        | "9:19.5" | "20:9" | "9:20" | "21:9" | "5:2" | "auto" => mapped.into(),
        _ => "1:1".into(),
    }
}

/// dsimage `grok_resolution`: official Imagine tops out at 2k.
pub(super) fn grok_resolution(resolution: &str) -> String {
    if resolution == "4k" {
        "2k".into()
    } else {
        resolution.into()
    }
}

/// dsimage `build_grok_payload`. Do not invent `quality` — relays reject extra fields.
pub(super) fn grok_generation_payload(
    model: &str,
    prompt: &str,
    ratio: &str,
    resolution: &str,
    image_urls: &[String],
) -> Value {
    let mut v = json!({
        "model": model,
        "prompt": grok_prompt(prompt, image_urls.len()),
        "n": 1,
        "aspect_ratio": grok_ratio(ratio),
        "resolution": grok_resolution(resolution),
        "response_format": "b64_json",
    });
    if image_urls.len() == 1 {
        v["image"] = json!({"url": image_urls[0], "type": "image_url"});
    } else if image_urls.len() > 1 {
        v["images"] = json!(image_urls
            .iter()
            .map(|u| json!({"url": u, "type": "image_url"}))
            .collect::<Vec<_>>());
    }
    v
}

/// dsimage `grok_tagged_prompt`: xAI multi-image edits name refs as `<IMAGE_0>`…
pub(super) fn grok_prompt(prompt: &str, count: usize) -> String {
    if count <= 1 || prompt.contains("<IMAGE_0>") {
        return prompt.into();
    }
    let mut text = prompt.to_string();
    for (i, phrase) in [
        "the first image",
        "the second image",
        "the third image",
        "the fourth image",
        "the fifth image",
    ]
    .iter()
    .take(count)
    .enumerate()
    {
        if let Ok(pattern) = regex::Regex::new(&format!("(?i){}", regex::escape(phrase))) {
            text = pattern
                .replace_all(&text, format!("<IMAGE_{i}>"))
                .into_owned();
        }
    }
    if !text.contains("<IMAGE_0>") {
        let tags = (0..count)
            .map(|i| format!("<IMAGE_{i}>"))
            .collect::<Vec<_>>()
            .join(" ");
        text = format!("{text}\nUse reference images in order: {tags}.");
    }
    text
}

pub async fn poll(
    app: &AppHandle,
    cfg: &StudioConfig,
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
    let p = provider(app, cfg)?;
    let v = json_response(
        request(
            app,
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
        "completed" | "succeeded" | "success" => Ok(Some(extract(app, &v).await?)),
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

pub(super) fn remote_task_status(v: &Value) -> String {
    ["/status", "/data/status", "/data/0/status"]
        .into_iter()
        .find_map(|path| v.pointer(path).and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

async fn extract(app: &AppHandle, v: &Value) -> Result<Vec<u8>, String> {
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
    download(app, u).await
}

pub(super) async fn download(app: &AppHandle, u: &str) -> Result<Vec<u8>, String> {
    let url = reqwest::Url::parse(u).map_err(|_| "图片下载 URL 无效")?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("图片下载必须使用 HTTP(S) URL".into());
    }
    // Deliberately no provider headers / credentials on a CDN download.
    let client: Client = app.state::<AppState>().http.clone();
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

pub(super) fn image_download_url(v: &Value) -> Option<&str> {
    v.pointer("/data/0/url")
        .or_else(|| v.pointer("/result/data/0/url"))
        .or_else(|| v.pointer("/image_url"))
        // dsimage async returns images[].url as an array, not a string.
        .or_else(|| v.pointer("/data/result/images/0/url/0"))
        .or_else(|| v.pointer("/data/result/images/0/url"))
        .or_else(|| v.pointer("/data/result/images/0"))
        .and_then(Value::as_str)
}

pub(super) fn remote_task_id(v: &Value) -> Option<String> {
    ["id", "task_id"]
        .into_iter()
        .filter_map(|k| v.get(k).and_then(Value::as_str))
        .chain(
            [
                "/data/0/task_id",
                "/data/task_id",
                "/data/0/id",
                "/data/id",
            ]
            .into_iter()
            .filter_map(|p| v.pointer(p).and_then(Value::as_str)),
        )
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

fn uses_openai_async_task(base_url: &str, model: &str) -> bool {
    let base = base_url.to_ascii_lowercase();
    let name = model.to_ascii_lowercase();
    !base.contains("apimart") && (name.contains("gpt-image") || name.starts_with("dall-e"))
}

pub(super) fn async_submit_path(base_url: &str, model: &str) -> &'static str {
    if uses_openai_async_task(base_url, model) {
        "images/generations/async"
    } else {
        "images/generations"
    }
}

pub(super) fn async_poll_path(base_url: &str, model: &str, remote_id: &str) -> String {
    if uses_openai_async_task(base_url, model) {
        format!("images/tasks/{remote_id}")
    } else {
        format!("tasks/{remote_id}")
    }
}

pub fn store_image(task_id: &str, result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
    store_image_in(&storage::load_task(task_id)?, result, bytes)
}

pub(super) fn store_image_in(
    task: &Task,
    result: &mut ImageResult,
    bytes: &[u8],
) -> Result<(), String> {
    let img = storage::decode(bytes)?;
    let fmt = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let ext = match fmt {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        _ => "png",
    };
    let relative = super::output::original_relative(task, result, ext);
    let base = super::output::directory(task)?;
    let target = base.join(&relative);
    fs::create_dir_all(target.parent().ok_or("无效成图路径")?).map_err(|e| e.to_string())?;
    let canonical_base = base.canonicalize().map_err(|e| e.to_string())?;
    if !target
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(canonical_base)
    {
        return Err("成图目录超出任务文件夹".into());
    }
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    let path = format!(
        "outputs/{}/{}",
        task.id,
        relative.to_string_lossy().replace('\\', "/")
    );
    result.path = Some(path);
    result.width = img.width();
    result.height = img.height();
    result.error = None;
    Ok(())
}
