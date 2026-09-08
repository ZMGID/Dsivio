//! Native image adapters derived from dsimage's generation contracts.
//! Submissions are never automatically retried: a timeout may still be billable.
use super::{storage, types::*};
use crate::{settings::ModelProvider, state::AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{fs, time::Duration};
use tauri::{AppHandle, Manager};

pub enum Submission {
    Image(Vec<u8>),
    Pending(String),
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
    if cfg.protocol == "openai"
        && (!matches!(brief.ratio.as_str(), "1:1" | "2:3" | "3:2") || brief.resolution != "1k")
    {
        return Err("OpenAI 标准接口支持 1:1 / 2:3 / 3:2、标准尺寸；请调整画幅或切换兼容网关协议。交付尺寸可在导出时设置。".into());
    }
    if cfg.protocol == "grok"
        && (brief.resolution == "4k" || matches!(brief.ratio.as_str(), "4:5" | "5:4"))
    {
        return Err("Grok 不支持所选画幅或 4K，请调整设置".into());
    }
    Ok(())
}

fn provider(app: &AppHandle, cfg: &StudioConfig) -> Result<ModelProvider, String> {
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
    crate::provider_request::apply(req, p, Some(task_id))
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
        return Err(format!(
            "图片接口 HTTP {status}：{}",
            detail.chars().take(500).collect::<String>()
        ));
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
    task: &Task,
    plan: &ImagePlan,
) -> Result<Submission, String> {
    validate(cfg, &task.brief)?;
    let p = provider(app, cfg)?;
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
        "async" => "images/generations".into(),
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
        &task.id,
    );
    let b = &task.brief;
    if cfg.protocol == "openai" && refs {
        let size = openai_size(&b.ratio);
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
                json!({"model":cfg.model,"prompt":plan.prompt,"n":1,"size":openai_size(&b.ratio),"quality":"high"})
            }
            "grok" => {
                let mut v = json!({"model":cfg.model,"prompt":grok_prompt(&plan.prompt, images.len()),"n":1,"aspect_ratio":b.ratio,"resolution":b.resolution,"response_format":"b64_json","quality":"medium"});
                if images.len() == 1 {
                    v["image"] = json!({"url":images[0],"type":"image_url"});
                } else if refs {
                    v["images"] = json!(images
                        .iter()
                        .map(|u| json!({"url":u,"type":"image_url"}))
                        .collect::<Vec<_>>());
                }
                v
            }
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
            _ => {
                json!({"model":cfg.model,"prompt":plan.prompt,"n":1,"size":b.ratio,"resolution":b.resolution,"image_urls":images})
            }
        };
        req = req.json(&payload);
    }
    let v = json_response(req.send().await, &p).await?;
    if let Some(id) = v.pointer("/data/0/task_id").and_then(Value::as_str) {
        return Ok(Submission::Pending(id.into()));
    }
    Ok(Submission::Image(extract(app, &v).await?))
}
fn openai_size(ratio: &str) -> &'static str {
    match ratio {
        "2:3" => "1024x1536",
        "3:2" => "1536x1024",
        _ => "1024x1024",
    }
}

pub(super) fn grok_prompt(prompt: &str, count: usize) -> String {
    if count <= 1 || prompt.contains("<IMAGE_0>") {
        return prompt.into();
    }
    let mut text = prompt.to_string();
    for (i, ordinal) in ["first", "second", "third", "fourth", "fifth"]
        .iter()
        .take(count)
        .enumerate()
    {
        if let Ok(pattern) = regex::Regex::new(&format!("(?i)(?:the )?{ordinal} image")) {
            text = pattern
                .replace_all(&text, format!("<IMAGE_{i}>"))
                .into_owned();
        }
    }
    if !text.contains("<IMAGE_0>") {
        text.push_str(&format!(
            "\nReference images in order: {}",
            (0..count)
                .map(|i| format!("<IMAGE_{i}>"))
                .collect::<Vec<_>>()
                .join(" ")
        ));
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
            &endpoint(&p, &format!("tasks/{remote_id}"))?,
            cfg,
            task_id,
        )
        .send()
        .await,
        &p,
    )
    .await?;
    match v
        .pointer("/data/status")
        .and_then(Value::as_str)
        .unwrap_or("")
    {
        "completed" | "succeeded" | "success" => Ok(Some(extract(app, &v).await?)),
        "failed" | "cancelled" => {
            Err("远程图片任务失败，请在供应商后台核对原因；可单独重新生成该页。".into())
        }
        "pending" | "queued" | "running" | "processing" | "submitted" => Ok(None),
        _ => Err("无法识别远程任务状态。已保留任务 ID，可稍后恢复查询。".into()),
    }
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
    let u = v
        .pointer("/data/0/url")
        .or_else(|| v.pointer("/data/result/images/0/url"))
        .or_else(|| v.pointer("/data/result/images/0"))
        .and_then(Value::as_str)
        .ok_or("接口没有返回图片；请核对所选模型是否支持图片生成")?;
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

pub fn store_image(result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
    let img = storage::decode(bytes)?;
    let fmt = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let ext = match fmt {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        _ => "png",
    };
    let path = format!("results/{}.{}", result.id, ext);
    fs::write(storage::root()?.join(&path), bytes).map_err(|e| e.to_string())?;
    result.path = Some(path);
    result.width = img.width();
    result.height = img.height();
    result.error = None;
    Ok(())
}
