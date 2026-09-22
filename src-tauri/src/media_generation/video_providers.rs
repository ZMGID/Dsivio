//! Native video protocol boundary. Catalog facts live only in videoModelCatalog.json.
//! This module formats, sends and decodes one request; the calling studio owns task persistence.
use crate::{settings::ModelProvider, state::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::LazyLock};
use ts_rs::TS;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    protocols: BTreeMap<String, Protocol>,
    models: Vec<Model>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Protocol {
    base_url: String,
    create_path: String,
    query_path: String,
    auth: String,
    headers: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Model {
    id: String,
    protocol: String,
    modes: Vec<String>,
    durations: Vec<u32>,
    resolutions: Vec<String>,
    ratios: Vec<String>,
    defaults: Value,
    max_prompt_length: Option<usize>,
    #[serde(default)]
    audio_toggle: bool,
    create_path: Option<String>,
    frame_duration: Option<u32>,
    frame_ratio: Option<String>,
    reference_task_type: Option<String>,
    reference_limits: Option<ReferenceLimits>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceLimits {
    images: usize,
    videos: usize,
    audios: usize,
    audio_only: bool,
}
static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../src/data/videoModelCatalog.json"))
        .expect("validated video model catalog")
});
fn model_profile(model: &str) -> Option<&'static Model> {
    CATALOG
        .models
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(model.trim()))
}
pub(crate) fn supports_mode(model: &str, mode: &str) -> bool {
    model_profile(model).is_none_or(|profile| profile.modes.iter().any(|value| value == mode))
}
pub(crate) fn input_with_defaults(model: &str, mut input: VideoInput) -> VideoInput {
    if let Some(profile) = model_profile(model) {
        if input.duration.is_none() {
            input.duration = profile.defaults["duration"].as_u64().map(|n| n as u32);
        }
        if input.resolution.is_none() {
            input.resolution = profile.defaults["resolution"].as_str().map(str::to_owned);
        }
        if input.ratio.is_none() {
            input.ratio = profile.defaults["ratio"].as_str().map(str::to_owned);
        }
    }
    input
}
pub(crate) fn is_video_model(provider: &ModelProvider, model: &str) -> bool {
    let info = provider.model_overrides.get(model);
    info.and_then(|i| i.capabilities.as_ref())
        .and_then(|c| c.video_generation)
        .unwrap_or_else(|| {
            info.and_then(|i| i.video_protocol.as_ref()).is_some() || model_profile(model).is_some()
        })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename = "VideoGenerationInput")]
pub struct VideoInput {
    pub prompt: String,
    #[ts(optional)]
    pub duration: Option<u32>,
    #[ts(optional)]
    pub resolution: Option<String>,
    #[ts(optional)]
    pub ratio: Option<String>,
    #[ts(optional)]
    pub first_frame: Option<String>,
    #[ts(optional)]
    pub last_frame: Option<String>,
    #[serde(default)]
    pub reference_images: Vec<String>,
    #[serde(default)]
    pub reference_videos: Vec<String>,
    #[serde(default)]
    pub reference_audios: Vec<String>,
    #[ts(optional)]
    pub generate_audio: Option<bool>,
    #[serde(default)]
    pub voice_ids: Vec<String>,
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "VideoRequestPreview")]
pub struct RequestPreview {
    pub method: &'static str,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    #[ts(type = "unknown")]
    pub body: Value,
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "VideoGenerationResult")]
pub struct VideoResult {
    pub remote_id: String,
    #[ts(type = "\"running\" | \"succeeded\" | \"failed\" | \"expired\" | \"cancelled\"")]
    pub status: String,
    pub download_url: Option<String>,
    pub file_id: Option<String>,
    pub error: Option<String>,
    pub download_requires_auth: bool,
}
fn protocol_definition(protocol: &str) -> Result<&'static Protocol, String> {
    CATALOG
        .protocols
        .get(protocol)
        .ok_or_else(|| "未知视频协议，请在模型详情中选择原生视频协议".into())
}
pub(crate) fn download_auth(protocol: &str) -> Result<&'static str, String> {
    Ok(&protocol_definition(protocol)?.auth)
}
fn endpoint(base: &str, path: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(base.trim()).map_err(|_| "视频 API 地址无效")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("视频 API 地址只能包含 HTTP(S) 域名和路径".into());
    }
    Ok(format!("{}{}", base.trim().trim_end_matches('/'), path))
}
fn valid_media(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("data:image/")
}
fn validate(protocol: &str, model: &str, input: &VideoInput) -> Result<(), String> {
    if model.trim().is_empty() || model.contains(['/', '?', '#']) {
        return Err("视频模型 ID 无效".into());
    }
    if input.prompt.trim().is_empty() {
        return Err("请填写视频提示词".into());
    }
    if input.last_frame.is_some() && input.first_frame.is_none() {
        return Err("尾帧模式必须同时提供首帧".into());
    }
    if input.duration == Some(0) {
        return Err("视频时长必须大于 0".into());
    }
    let reference = !input.reference_images.is_empty()
        || !input.reference_videos.is_empty()
        || !input.reference_audios.is_empty()
        || !input.voice_ids.is_empty();
    let mode = if reference {
        "reference"
    } else if input.last_frame.is_some() {
        "frames"
    } else if input.first_frame.is_some() {
        "image"
    } else {
        "text"
    };
    if let Some(profile) = model_profile(model).filter(|p| p.protocol == protocol) {
        if !profile.modes.iter().any(|m| m == mode) {
            return Err(format!("此模型当前适配入口不支持 {mode} 模式"));
        }
        if input
            .duration
            .is_some_and(|d| !profile.durations.is_empty() && !profile.durations.contains(&d))
        {
            return Err("此模型不支持所选时长".into());
        }
        for (value, allowed, name) in [
            (&input.resolution, &profile.resolutions, "分辨率"),
            (&input.ratio, &profile.ratios, "比例"),
        ] {
            if value
                .as_ref()
                .is_some_and(|v| !allowed.is_empty() && !allowed.contains(v))
            {
                return Err(format!("此模型不支持所选{name}"));
            }
        }
        if input.first_frame.is_some() || input.last_frame.is_some() {
            if profile
                .frame_duration
                .is_some_and(|d| input.duration.unwrap_or(d) != d)
            {
                return Err("此模型首尾帧模式不支持所选时长".into());
            }
            if profile
                .frame_ratio
                .as_ref()
                .is_some_and(|r| input.ratio.as_ref().is_some_and(|v| v != r))
            {
                return Err("此模型首尾帧模式只支持自适应比例".into());
            }
        }
        if let Some(limits) = &profile.reference_limits {
            if input.reference_images.len() > limits.images
                || input.reference_videos.len() > limits.videos
                || input.reference_audios.len() > limits.audios
            {
                return Err("参考素材数量超过模型限制".into());
            }
            if !limits.audio_only
                && !input.reference_audios.is_empty()
                && input.reference_images.is_empty()
                && input.reference_videos.is_empty()
            {
                return Err("此模型参考音频必须同时提供图片或视频".into());
            }
        }
        let length = if protocol == "runway" {
            input.prompt.encode_utf16().count()
        } else {
            input.prompt.chars().count()
        };
        if profile
            .max_prompt_length
            .is_some_and(|max| max > 0 && length > max)
        {
            return Err("提示词超过该模型长度限制".into());
        }
        if input.generate_audio.is_some() && !profile.audio_toggle && protocol != "seedance" {
            return Err("此模型入口不支持声音开关".into());
        }
    }
    if reference && !matches!(protocol, "minimax_h3" | "seedance" | "xai_video") {
        return Err("此协议入口不支持多模态参考，请使用其专用接口".into());
    }
    if matches!(protocol, "minimax_h3" | "seedance" | "xai_video") && reference {
        if input.first_frame.is_some() || input.last_frame.is_some() {
            return Err("首尾帧和多模态参考不能混用".into());
        }
    }
    if !input.voice_ids.is_empty()
        && (protocol != "xai_video"
            || input.voice_ids.len() > 3
            || input.generate_audio == Some(false))
    {
        return Err("音色仅支持有声 xAI 参考生成，最多 3 个".into());
    }
    if protocol == "xai_video"
        && reference
        && (input.reference_images.len() > 7
            || input.resolution.as_deref() == Some("1080p")
            || !input.reference_videos.is_empty()
            || !input.reference_audios.is_empty())
    {
        return Err("xAI 参考生成最多 7 张图片，最高 720p；不支持参考音视频文件".into());
    }
    if input.last_frame.is_some() && !matches!(protocol, "minimax_h3" | "seedance" | "luma") {
        return Err("此协议入口未支持尾帧".into());
    }
    if input.generate_audio.is_some()
        && !matches!(protocol, "xai_video" | "vidu" | "seedance" | "kling")
    {
        return Err("此协议不支持声音开关".into());
    }
    if protocol == "minimax_hailuo" && input.ratio.is_some() {
        return Err("Hailuo V1 不接受 ratio 参数".into());
    }
    if protocol == "runway" && input.resolution.is_some() {
        return Err("Runway 使用像素 ratio，不接受独立 resolution".into());
    }
    if protocol == "wan" && input.first_frame.is_some() {
        return Err("当前模型入口仅支持文生视频".into());
    }
    if protocol == "kling" && input.first_frame.is_some() && input.ratio.is_some() {
        return Err("Kling 图生视频输出跟随首帧，不接受独立比例".into());
    }
    for media in input
        .reference_videos
        .iter()
        .chain(input.reference_audios.iter())
    {
        if !(media.starts_with("https://")
            || media.starts_with("http://")
            || protocol == "minimax_h3"
                && (media.starts_with("data:") || media.starts_with("mm_file://")))
        {
            return Err("参考视频和音频需要可访问的 HTTP(S) URL".into());
        }
    }
    for media in input
        .first_frame
        .iter()
        .chain(input.last_frame.iter())
        .chain(input.reference_images.iter())
        .chain(input.reference_videos.iter())
        .chain(input.reference_audios.iter())
    {
        if !valid_media(media)
            && !(protocol == "minimax_h3"
                && (media.starts_with("data:video/")
                    || media.starts_with("data:audio/")
                    || media.starts_with("mm_file://")))
        {
            return Err(
                "素材须为可访问的 HTTP(S) URL 或协议支持的图片 data URL，不能直接发送本地路径"
                    .into(),
            );
        }
    }
    if protocol == "minimax_hailuo"
        && input.resolution.as_deref() == Some("1080P")
        && input.duration.is_some_and(|d| d != 6)
    {
        return Err("Hailuo 1080P 只支持 6 秒".into());
    }
    if protocol == "veo"
        && input.resolution.as_deref().is_some_and(|r| r != "720p")
        && input.duration.is_some_and(|d| d != 8)
    {
        return Err("Veo 高分辨率需要 8 秒".into());
    }
    Ok(())
}
fn put(body: &mut Value, key: &str, value: impl Serialize) {
    body[key] = serde_json::to_value(value).expect("serializable video input");
}
fn content(input: &VideoInput) -> Value {
    let mut items = vec![json!({"type":"text", "text":input.prompt})];
    for (urls, kind, role) in [
        (
            input.first_frame.iter().collect::<Vec<_>>(),
            "image_url",
            "first_frame",
        ),
        (input.last_frame.iter().collect(), "image_url", "last_frame"),
        (
            input.reference_images.iter().collect(),
            "image_url",
            "reference_image",
        ),
        (
            input.reference_videos.iter().collect(),
            "video_url",
            "reference_video",
        ),
        (
            input.reference_audios.iter().collect(),
            "audio_url",
            "reference_audio",
        ),
    ] {
        for url in urls {
            let mut item = json!({"type":kind,"role":role});
            item[kind] = json!({"url":url});
            items.push(item);
        }
    }
    json!(items)
}
fn luma_image(value: &str) -> Result<Value, String> {
    if value.starts_with("data:") {
        let (meta, data) = value
            .split_once(";base64,")
            .ok_or("图片 data URL 缺少 base64 内容")?;
        let mime = meta.strip_prefix("data:").ok_or("图片格式无效")?;
        if data.is_empty() {
            return Err("图片 base64 内容为空".into());
        }
        Ok(json!({"data":data,"media_type":mime}))
    } else {
        Ok(json!({"url":value}))
    }
}
pub(crate) fn prepare(
    protocol: &str,
    base: &str,
    model: &str,
    input: &VideoInput,
) -> Result<RequestPreview, String> {
    let definition = protocol_definition(protocol)?;
    validate(protocol, model, input)?;
    let profile = model_profile(model).filter(|p| p.protocol == protocol);
    let mut path = profile
        .and_then(|p| p.create_path.as_deref())
        .unwrap_or(&definition.create_path)
        .replace("{model}", model);
    let mut body = json!({"model":model});
    match protocol {
        "minimax_h3" | "seedance" => {
            body["content"] = content(input);
            if !input.reference_images.is_empty()
                || !input.reference_videos.is_empty()
                || !input.reference_audios.is_empty()
            {
                if let Some(task_type) = profile.and_then(|p| p.reference_task_type.as_ref()) {
                    body["omni_reference_task_type"] = json!(task_type);
                }
            }
            if protocol == "minimax_h3" && (input.duration.is_none() || input.resolution.is_none())
            {
                return Err("H3 必须填写时长和分辨率".into());
            }
            if let Some(v) = input.duration {
                put(&mut body, "duration", v);
            }
            if let Some(v) = &input.resolution {
                put(&mut body, "resolution", v);
            }
            if let Some(v) = &input.ratio {
                put(&mut body, "ratio", v);
            }
            if let Some(v) = input.generate_audio {
                put(&mut body, "generate_audio", v);
            }
        }
        "xai_video" | "minimax_hailuo" | "vidu" => {
            put(&mut body, "prompt", &input.prompt);
            if let Some(v) = input.duration {
                put(&mut body, "duration", v);
            }
            if let Some(v) = &input.resolution {
                put(&mut body, "resolution", v);
            }
            if let Some(v) = &input.ratio {
                put(&mut body, "aspect_ratio", v);
            }
            if let Some(v) = input.generate_audio {
                put(
                    &mut body,
                    if protocol == "vidu" {
                        "audio"
                    } else {
                        "generate_audio"
                    },
                    v,
                );
            }
            if protocol == "xai_video" {
                if !input.reference_images.is_empty() {
                    body["reference_images"] = json!(input
                        .reference_images
                        .iter()
                        .map(|url| json!({"url":url}))
                        .collect::<Vec<_>>());
                }
                if !input.voice_ids.is_empty() {
                    body["reference_audios"] = json!(input
                        .voice_ids
                        .iter()
                        .map(|id| json!({"voice_id":id}))
                        .collect::<Vec<_>>());
                }
            }
            if let Some(image) = &input.first_frame {
                match protocol {
                    "xai_video" => body["image"] = json!({"url":image}),
                    "minimax_hailuo" => put(&mut body, "first_frame_image", image),
                    _ => {
                        path = "/img2video".into();
                        body["images"] = json!([image]);
                    }
                }
            }
        }
        "veo" => {
            body = json!({"instances":[{"prompt":input.prompt}],"parameters":{}});
            let p = &mut body["parameters"];
            if let Some(v) = input.duration {
                put(p, "durationSeconds", v);
            }
            if let Some(v) = &input.resolution {
                put(p, "resolution", v);
            }
            if let Some(v) = &input.ratio {
                put(p, "aspectRatio", v);
            }
            if let Some(image) = &input.first_frame {
                let (meta, data) = image
                    .split_once(";base64,")
                    .ok_or("Veo 首帧须先转换为 base64 图片 data URL")?;
                let mime = meta
                    .strip_prefix("data:")
                    .filter(|m| matches!(*m, "image/png" | "image/jpeg" | "image/webp"))
                    .ok_or("Veo 首帧图片格式无效")?;
                body["instances"][0]["image"] = json!({"bytesBase64Encoded":data,"mimeType":mime});
            }
        }
        "wan" => {
            body["input"] = json!({"prompt":input.prompt});
            body["parameters"] = json!({});
            let p = &mut body["parameters"];
            if let Some(v) = input.duration {
                put(p, "duration", v);
            }
            if let Some(v) = &input.resolution {
                put(p, "resolution", v);
            }
            if let Some(v) = &input.ratio {
                put(p, "ratio", v);
            }
        }
        "runway" => {
            put(&mut body, "promptText", &input.prompt);
            put(
                &mut body,
                "duration",
                input.duration.ok_or("Runway 必须填写时长")?,
            );
            put(
                &mut body,
                "ratio",
                input.ratio.as_ref().ok_or("Runway 必须填写像素比例")?,
            );
            if let Some(image) = &input.first_frame {
                path = "/image_to_video".into();
                put(&mut body, "promptImage", image);
            }
        }
        "kling" => {
            body = json!({"prompt":input.prompt,"settings":{}});
            if let Some(image) = &input.first_frame {
                path = format!("/image-to-video/{model}");
                body = json!({"contents":[{"type":"prompt","text":input.prompt},{"type":"first_frame","url":image}],"settings":{}});
            }
            let p = &mut body["settings"];
            if let Some(v) = input.duration {
                put(p, "duration", v);
            }
            if let Some(v) = &input.resolution {
                put(p, "resolution", v);
            }
            if let Some(v) = &input.ratio {
                put(p, "aspect_ratio", v);
            }
            if let Some(v) = input.generate_audio {
                put(p, "audio", if v { "native" } else { "off" });
            }
        }
        "luma" => {
            body["type"] = json!("video");
            put(&mut body, "prompt", &input.prompt);
            if let Some(v) = &input.ratio {
                put(&mut body, "aspect_ratio", v);
            }
            body["video"] = json!({});
            let p = &mut body["video"];
            if let Some(v) = input.duration {
                put(p, "duration", format!("{v}s"));
            }
            if let Some(v) = &input.resolution {
                put(p, "resolution", v);
            }
            if let Some(v) = &input.first_frame {
                p["start_frame"] = luma_image(v)?;
            }
            if let Some(v) = &input.last_frame {
                p["end_frame"] = luma_image(v)?;
            }
        }
        _ => return Err("未实现的视频协议".into()),
    }
    if protocol == "minimax_h3"
        && serde_json::to_vec(&body).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024
    {
        return Err("参考素材请求超过 64 MB，请减少或压缩素材".into());
    }
    let mut headers = definition.headers.clone();
    headers.insert("Content-Type".into(), "application/json".into());
    let (header, value) = match definition.auth.as_str() {
        "google" => ("x-goog-api-key", "<API_KEY>"),
        "token" => ("Authorization", "Token <API_KEY>"),
        _ => ("Authorization", "Bearer <API_KEY>"),
    };
    headers.insert(header.into(), value.into());
    Ok(RequestPreview {
        method: "POST",
        url: endpoint(base, &path)?,
        headers,
        body,
    })
}
#[tauri::command]
pub fn preview_video_model_request(
    model: String,
    protocol: String,
    base_url: String,
) -> Result<RequestPreview, String> {
    let profile = model_profile(&model)
        .filter(|p| p.protocol == protocol)
        .or_else(|| CATALOG.models.iter().find(|p| p.protocol == protocol));
    let mut input = VideoInput {
        prompt: "A paper boat floating on a quiet pond".into(),
        ..Default::default()
    };
    if let Some(profile) = profile {
        input.duration = profile.defaults["duration"].as_u64().map(|n| n as u32);
        input.resolution = profile.defaults["resolution"].as_str().map(str::to_owned);
        input.ratio = profile.defaults["ratio"].as_str().map(str::to_owned);
        if !profile.modes.iter().any(|m| m == "text") {
            input.first_frame = Some("https://example.com/first-frame.png".into());
        }
    }
    let definition = protocol_definition(&protocol)?;
    prepare(
        &protocol,
        if base_url.trim().is_empty() {
            &definition.base_url
        } else {
            &base_url
        },
        &model,
        &input,
    )
}

fn read_string(value: &Value, pointer: &str) -> Option<String> {
    value.pointer(pointer).and_then(|v| {
        v.as_str()
            .map(str::to_owned)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
    })
}
fn decode(protocol: &str, value: &Value, id: &str) -> Result<VideoResult, String> {
    if value
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        .is_some_and(|c| c != 0)
    {
        return Err("MiniMax 视频接口返回业务错误；请检查模型权限、余额和参数".into());
    }
    if protocol == "kling" && value["code"].as_i64().is_some_and(|c| c != 0) {
        return Err("Kling 返回业务错误；请检查模型权限与参数".into());
    }
    let (id_path, status_path, url_path) = match protocol {
        "minimax_h3" => ("/task_id", "/task/status", "/task/content/url"),
        "minimax_hailuo" => ("/task_id", "/status", "/file/download_url"),
        "seedance" => ("/id", "/status", "/content/video_url"),
        "xai_video" => ("/request_id", "/status", "/video/url"),
        "wan" => (
            "/output/task_id",
            "/output/task_status",
            "/output/video_url",
        ),
        "runway" => ("/id", "/status", "/output/0"),
        "vidu" => ("/task_id", "/state", "/creations/0/url"),
        "luma" => ("/id", "/state", "/output/0/url"),
        "kling" if id.is_empty() => ("/data/id", "/data/status", "/data/outputs/0/url"),
        "kling" => ("/data/0/id", "/data/0/status", "/data/0/outputs/0/url"),
        "veo" => (
            "/name",
            "/status",
            "/response/generateVideoResponse/generatedSamples/0/video/uri",
        ),
        _ => return Err("未知视频协议".into()),
    };
    let remote_id = read_string(value, id_path).unwrap_or_else(|| id.to_owned());
    if remote_id.is_empty() {
        return Err(
            "供应商未返回视频任务 ID；提交结果不确定，请先在供应商控制台核查，勿自动重试".into(),
        );
    }
    let raw = if protocol == "veo" {
        if value.get("error").is_some() {
            "failed".into()
        } else if value["done"] == true {
            "succeeded".into()
        } else {
            "running".into()
        }
    } else {
        read_string(value, status_path)
            .unwrap_or_else(|| if id.is_empty() { "queued" } else { "unknown" }.into())
            .to_lowercase()
    };
    let status = match raw.as_str() {
        "submitted" | "queued" | "queueing" | "pending" | "created" | "preparing"
        | "processing" | "running" | "in_progress" | "throttled" => "running",
        "done" | "succeeded" | "success" | "completed" => "succeeded",
        "failed" | "fail" => "failed",
        "expired" => "expired",
        "cancelled" | "canceled" | "aborted" => "cancelled",
        _ => return Err(format!("视频任务返回未识别状态：{raw}")),
    }
    .to_owned();
    let url = read_string(value, url_path);
    let file_id = if protocol == "minimax_hailuo" {
        read_string(value, "/file_id")
    } else {
        None
    };
    if status == "succeeded" && url.is_none() && file_id.is_none() {
        return Err("视频任务结束但未返回可下载结果（可能被内容过滤），请核查供应商任务".into());
    }
    let error = if matches!(status.as_str(), "failed" | "expired" | "cancelled") {
        Some(format!("视频任务 {status}；请检查供应商任务详情"))
    } else {
        None
    };
    Ok(VideoResult {
        remote_id,
        status,
        download_url: url,
        file_id,
        error,
        download_requires_auth: protocol == "veo",
    })
}
pub(crate) fn selected<'a>(provider: &'a ModelProvider, model: &str) -> Result<&'a str, String> {
    if !provider.enabled
        || !provider.enabled_models.iter().any(|m| m == model)
        || !is_video_model(provider, model)
    {
        return Err("请先启用供应商与视频模型".into());
    }
    let protocol = provider
        .model_overrides
        .get(model)
        .and_then(|m| m.video_protocol.as_deref())
        .or_else(|| model_profile(model).map(|m| m.protocol.as_str()))
        .ok_or("请先为此视频模型指定协议")?;
    protocol_definition(protocol)?;
    Ok(protocol)
}
/// Preflight / definitive rejection is safe to correct; an ambiguous POST must not be replayed.
#[derive(Debug)]
pub(crate) struct RequestFailure {
    pub message: String,
    pub rejected: bool,
}
impl RequestFailure {
    fn uncertain(message: impl Into<String>) -> Self { Self { message: message.into(), rejected: false } }
}
impl From<String> for RequestFailure {
    fn from(message: String) -> Self { Self { message, rejected: true } }
}
impl From<&str> for RequestFailure {
    fn from(message: &str) -> Self { message.to_owned().into() }
}
impl std::fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.message.fmt(f) }
}

async fn send(
    client: &reqwest::Client,
    provider: &ModelProvider,
    protocol: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<Value, RequestFailure> {
    let definition = protocol_definition(protocol)?;
    if provider.request.oauth.is_some() {
        return Err("视频接口需要供应商 API Key，不使用聊天 OAuth".into());
    }
    let key = provider
        .api_keys
        .get(provider.active_key_index)
        .or_else(|| provider.api_keys.first())
        .filter(|k| !k.trim().is_empty())
        .ok_or("Missing API Key")?;
    let mut request = if let Some(body) = body {
        client.post(url).json(body)
    } else {
        client.get(url)
    };
    request = crate::provider_request::apply(request, provider, None);
    request = match definition.auth.as_str() {
        "google" => request.header("x-goog-api-key", key),
        "token" => request.header("Authorization", format!("Token {key}")),
        _ => request.bearer_auth(key),
    };
    for (key, value) in &definition.headers {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| "视频协议请求头无效")?,
            value.parse().map_err(|_| "视频协议请求头值无效")?,
        );
        request = request.headers(headers);
    }
    // Preserve the app's proxy/custom-header policy. Never replay a paid POST on timeout.
    request = request.timeout(std::time::Duration::from_secs(120));
    let response = request.send().await.map_err(|e| {
        RequestFailure::uncertain(format!(
            "视频请求传输失败：{}；若为提交请求，请核查供应商控制台后再决定是否重试",
            e.without_url()
        ))
    })?;
    if !response.status().is_success() {
        return Err(RequestFailure {
            message: format!("视频接口 HTTP {}；请检查密钥、权限与请求参数", response.status().as_u16()),
            rejected: response.status().is_client_error() && response.status().as_u16() != 408,
        });
    }
    response.json().await.map_err(|_| {
        RequestFailure::uncertain("视频接口返回了无效 JSON；若为提交请求，结果不确定，请核查供应商控制台，勿自动重试")
    })
}

/// Submit once, return the remote receipt immediately. Caller must persist it before polling.
pub(crate) async fn submit_video_model_request(
    state: &AppState,
    provider: &ModelProvider,
    model: String,
    input: VideoInput,
) -> Result<VideoResult, RequestFailure> {
    let protocol = selected(&provider, &model)?;
    let request = prepare(protocol, &provider.base_url, &model, &input)?;
    decode(
        protocol,
        &send(
            state.client_for(&provider),
            &provider,
            protocol,
            &request.url,
            Some(&request.body),
        )
        .await?,
        "",
    ).map_err(RequestFailure::uncertain)
}

/// Query the original connection only; changing provider/model must never re-submit a task.
pub(crate) async fn query_video_model_request(
    state: &AppState,
    provider_id: String,
    model: String,
    protocol: String,
    base_url: String,
    remote_id: String,
) -> Result<VideoResult, String> {
    let provider = state
        .settings_read()
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned()
        .ok_or("原任务供应商不存在")?;
    if provider.base_url.trim_end_matches('/') != base_url.trim_end_matches('/')
        || selected(&provider, &model)? != protocol
    {
        return Err("原任务连接配置已变更；请恢复原供应商地址与协议后查询，勿重复提交".into());
    }
    if remote_id.is_empty()
        || !remote_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
        || remote_id.contains("..")
        || (protocol != "veo" && remote_id.contains('/'))
        || (protocol == "veo"
            && !(remote_id.starts_with("models/") || remote_id.starts_with("operations/")))
    {
        return Err("视频任务 ID 格式无效".into());
    }
    let definition = protocol_definition(&protocol)?;
    let url = endpoint(
        &base_url,
        &definition.query_path.replace("{id}", &remote_id),
    )?;
    let mut result = decode(
        &protocol,
        &send(
            state.client_for(&provider),
            &provider,
            &protocol,
            &url,
            None,
        )
        .await.map_err(|e| e.to_string())?,
        &remote_id,
    )?;
    if let Some(file) = &result.file_id {
        if !file.chars().all(|c| c.is_ascii_digit()) {
            return Err("MiniMax 返回无效文件 ID".into());
        }
        let url = endpoint(&base_url, &format!("/files/retrieve?file_id={file}"))?;
        let data = send(
            state.client_for(&provider),
            &provider,
            &protocol,
            &url,
            None,
        )
        .await.map_err(|e| e.to_string())?;
        result.download_url = read_string(&data, "/file/download_url");
        if result.download_url.is_none() {
            return Err("MiniMax 文件暂时无法下载；保留任务 ID 后重试查询".into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> VideoInput {
        VideoInput {
            prompt: "A paper boat".into(),
            duration: Some(6),
            ..Default::default()
        }
    }

    #[test]
    fn every_catalog_model_produces_a_valid_native_preview() {
        for profile in &CATALOG.models {
            let result = preview_video_model_request(
                profile.id.clone(),
                profile.protocol.clone(),
                String::new(),
            );
            assert!(result.is_ok(), "{}: {result:?}", profile.id);
            let preview = result.unwrap();
            assert!(!preview.url.contains("chat/completions"));
            assert!(preview.headers.values().any(|v| v.contains("<API_KEY>")));
        }
    }
    #[test]
    fn protocol_shapes_and_headers_follow_the_official_contracts() {
        let h3 =
            preview_video_model_request("MiniMax-H3".into(), "minimax_h3".into(), String::new())
                .unwrap();
        assert_eq!(h3.url, "https://api.minimax.cn/v2/video_generation");
        assert_eq!(h3.body["content"][0]["type"], "text");
        assert!(h3.body.get("prompt").is_none());
        let wan =
            preview_video_model_request("wan2.7-t2v".into(), "wan".into(), String::new()).unwrap();
        assert_eq!(wan.body["parameters"]["resolution"], "720P");
        assert!(wan.body["input"]["prompt"].is_string());
        assert_eq!(wan.headers["X-DashScope-Async"], "enable");
        let vidu =
            preview_video_model_request("viduq3-pro".into(), "vidu".into(), String::new()).unwrap();
        assert_eq!(vidu.headers["Authorization"], "Token <API_KEY>");
        let runway =
            preview_video_model_request("gen4_turbo".into(), "runway".into(), String::new())
                .unwrap();
        assert!(runway.url.ends_with("/image_to_video"));
        assert!(runway.body["promptImage"].is_string());
        assert_eq!(runway.headers["X-Runway-Version"], "2024-11-06");
        let veo = preview_video_model_request(
            "veo-3.1-generate-preview".into(),
            "veo".into(),
            String::new(),
        )
        .unwrap();
        assert!(veo.url.ends_with(":predictLongRunning"));
        assert_eq!(veo.headers["x-goog-api-key"], "<API_KEY>");
        assert_eq!(veo.body["parameters"]["durationSeconds"], 8);
        let luma =
            preview_video_model_request("ray-3.2".into(), "luma".into(), String::new()).unwrap();
        assert_eq!(luma.body["type"], "video");
        assert_eq!(luma.body["video"]["duration"], "5s");
    }
    #[test]
    fn references_do_not_silently_turn_into_text_only_generation() {
        let mut i = input();
        i.first_frame = Some("https://example.com/first.png".into());
        let grok = prepare(
            "xai_video",
            "https://gateway.example/v1",
            "grok-imagine-video-1.5",
            &i,
        )
        .unwrap();
        assert_eq!(grok.body["image"]["url"], "https://example.com/first.png");
        assert!(grok.url.starts_with("https://gateway.example/"));
        i.last_frame = Some("https://example.com/last.png".into());
        i.resolution = Some("768P".into());
        let h3 = prepare("minimax_h3", "https://api.minimax.cn", "MiniMax-H3", &i).unwrap();
        assert_eq!(h3.body["content"][2]["role"], "last_frame");
        i.reference_images
            .push("https://example.com/ref.png".into());
        assert!(prepare("minimax_h3", "https://api.minimax.cn", "MiniMax-H3", &i).is_err());
        assert!(prepare(
            "xai_video",
            "https://api.x.ai/v1",
            "grok-imagine-video-1.5",
            &i
        )
        .is_err());
    }
    #[test]
    fn rejects_unsupported_combinations_before_submission() {
        let mut i = input();
        i.duration = Some(10);
        i.resolution = Some("1080P".into());
        assert!(prepare(
            "minimax_hailuo",
            "https://api.minimax.io/v1",
            "MiniMax-Hailuo-2.3",
            &i
        )
        .is_err());
        i.resolution = Some("2K".into());
        assert!(prepare("minimax_h3", "https://api.minimax.cn", "MiniMax-H3-Max", &i).is_err());
        i = input();
        i.first_frame = Some("/Users/me/photo.png".into());
        assert!(prepare(
            "xai_video",
            "https://api.x.ai/v1",
            "grok-imagine-video-1.5",
            &i
        )
        .is_err());
        assert!(endpoint("https://example.com?api_key=secret", "/videos").is_err());
        assert!(prepare("openai_chat", "https://example.com", "gpt-4o", &input()).is_err());
    }
    #[test]
    fn decodes_submit_receipts_without_waiting_for_completion() {
        for (protocol, value, id) in [
            ("minimax_h3", json!({"task_id":"h3"}), "h3"),
            (
                "minimax_hailuo",
                json!({"task_id":"123","base_resp":{"status_code":0}}),
                "123",
            ),
            ("seedance", json!({"id":"ark"}), "ark"),
            ("xai_video", json!({"request_id":"xai"}), "xai"),
            (
                "veo",
                json!({"name":"models/veo/operations/op"}),
                "models/veo/operations/op",
            ),
            (
                "wan",
                json!({"output":{"task_id":"wan","task_status":"PENDING"}}),
                "wan",
            ),
            ("runway", json!({"id":"runway"}), "runway"),
            ("vidu", json!({"task_id":"vidu","state":"created"}), "vidu"),
            ("luma", json!({"id":"luma","state":"queued"}), "luma"),
            (
                "kling",
                json!({"code":0,"data":{"id":"kling","status":"submitted"}}),
                "kling",
            ),
        ] {
            let result = decode(protocol, &value, "").unwrap();
            assert_eq!(result.remote_id, id);
            assert_eq!(result.status, "running");
        }
        assert!(decode("xai_video", &json!({}), "")
            .unwrap_err()
            .contains("不确定"));
    }
    #[test]
    fn decodes_results_and_never_claims_success_without_an_artifact() {
        for (protocol, value) in [
            (
                "minimax_h3",
                json!({"task":{"status":"succeeded","content":{"url":"https://cdn/v.mp4"}}}),
            ),
            (
                "seedance",
                json!({"status":"succeeded","content":{"video_url":"https://cdn/v.mp4"}}),
            ),
            (
                "xai_video",
                json!({"status":"done","video":{"url":"https://cdn/v.mp4"}}),
            ),
            (
                "wan",
                json!({"output":{"task_status":"SUCCEEDED","video_url":"https://cdn/v.mp4"}}),
            ),
            (
                "runway",
                json!({"status":"SUCCEEDED","output":["https://cdn/v.mp4"]}),
            ),
            (
                "vidu",
                json!({"state":"success","creations":[{"url":"https://cdn/v.mp4"}]}),
            ),
            (
                "luma",
                json!({"state":"completed","output":[{"url":"https://cdn/v.mp4"}]}),
            ),
            (
                "kling",
                json!({"code":0,"data":[{"id":"saved-id","status":"succeeded","outputs":[{"type":"video","url":"https://cdn/v.mp4"}]}]}),
            ),
            (
                "veo",
                json!({"done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"https://cdn/v.mp4"}}]}}}),
            ),
        ] {
            let result = decode(protocol, &value, "saved-id").unwrap();
            assert_eq!(result.status, "succeeded");
            assert_eq!(result.download_url.as_deref(), Some("https://cdn/v.mp4"));
        }
        let file = decode(
            "minimax_hailuo",
            &json!({"status":"Success","file_id":"456"}),
            "123",
        )
        .unwrap();
        assert_eq!(file.file_id.as_deref(), Some("456"));
        assert!(decode("xai_video", &json!({"status":"done"}), "id").is_err());
        assert_eq!(
            decode("xai_video", &json!({"status":"expired"}), "id")
                .unwrap()
                .status,
            "expired"
        );
        assert_eq!(
            decode("veo", &json!({"error":{"code":400}}), "id")
                .unwrap()
                .status,
            "failed"
        );
        assert!(decode(
            "minimax_hailuo",
            &json!({"base_resp":{"status_code":1008}}),
            "id"
        )
        .is_err());
    }
    #[test]
    fn model_specific_constraints_and_image_formats_are_enforced() {
        let mut i = input();
        i.first_frame = Some("data:image/png;base64,aGVsbG8=".into());
        i.duration = Some(5);
        let luma = prepare("luma", "https://agents.lumalabs.ai/v1", "ray-3.2", &i).unwrap();
        assert_eq!(
            luma.body["video"]["start_frame"],
            json!({"data":"aGVsbG8=","media_type":"image/png"})
        );
        i.duration = Some(10);
        assert!(prepare("luma", "https://agents.lumalabs.ai/v1", "ray-3.2", &i).is_err());
        i.ratio = Some("16:9".into());
        assert!(prepare(
            "seedance",
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao-seedance-2-5-260628",
            &i
        )
        .is_err());
        i.ratio = Some("adaptive".into());
        assert!(prepare(
            "seedance",
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao-seedance-2-5-260628",
            &i
        )
        .is_ok());
        i.first_frame = None;
        i.reference_audios = vec!["https://example.com/audio.mp3".into()];
        assert!(prepare(
            "seedance",
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao-seedance-2-0-260128",
            &i
        )
        .is_err());
        let reference = prepare(
            "seedance",
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao-seedance-2-5-260628",
            &i,
        )
        .unwrap();
        assert_eq!(reference.body["omni_reference_task_type"], "reference");
        let turbo =
            preview_video_model_request("kling-3.0-turbo".into(), "kling".into(), String::new())
                .unwrap();
        assert!(turbo.url.ends_with("/image-to-video/kling-3.0-turbo"));
        assert_eq!(turbo.body["contents"][1]["type"], "first_frame");
        assert!(turbo.body.get("model").is_none());
        assert!(turbo.body["settings"].get("aspect_ratio").is_none());
    }

    #[tokio::test]
    async fn native_transport_sends_json_then_queries_without_replaying_failure() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut received = Vec::new();
            for response in [
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"task_id\":\"123\"}",
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 19\r\nConnection: close\r\n\r\n{\"state\":\"created\"}",
                "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 { break; }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(split) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..split]).to_lowercase();
                        let length = headers.lines().find_map(|line| line.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if bytes.len() >= split + 4 + length { break; }
                    }
                }
                received.push(String::from_utf8(bytes).unwrap());
                stream.write_all(response.as_bytes()).unwrap();
            }
            listener.set_nonblocking(true).unwrap();
            assert!(listener.accept().is_err(), "paid request was replayed");
            received
        });
        let provider: ModelProvider = serde_json::from_value(json!({"id":"p","name":"P","baseUrl":base,"apiKeys":["test-key"],"enabledModels":["viduq3-pro"]})).unwrap();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let request = prepare("vidu", &base, "viduq3-pro", &input()).unwrap();
        let receipt = send(
            &client,
            &provider,
            "vidu",
            &request.url,
            Some(&request.body),
        )
        .await
        .unwrap();
        assert_eq!(decode("vidu", &receipt, "").unwrap().remote_id, "123");
        let query_url = format!("{base}/tasks/123/creations");
        let queried = send(&client, &provider, "vidu", &query_url, None)
            .await
            .unwrap();
        assert_eq!(decode("vidu", &queried, "123").unwrap().status, "running");
        assert!(send(
            &client,
            &provider,
            "vidu",
            &request.url,
            Some(&request.body)
        )
        .await
        .unwrap_err()
        .message.contains("503"));
        let received = worker.join().unwrap();
        assert!(received[0].starts_with("POST /text2video HTTP/1.1"));
        assert!(received[0]
            .to_lowercase()
            .contains("authorization: token test-key"));
        assert!(received[0]
            .to_lowercase()
            .contains("content-type: application/json"));
        assert!(received[0].contains("\"model\":\"viduq3-pro\""));
        assert!(received[1].starts_with("GET /tasks/123/creations HTTP/1.1"));
    }

    #[test]
    fn user_capability_override_takes_precedence_over_catalog() {
        let mut p:ModelProvider=serde_json::from_value(json!({"id":"p","name":"P","baseUrl":"https://example.com","apiKeys":[],"enabledModels":["MiniMax-H3"]})).unwrap();
        assert!(is_video_model(&p, "MiniMax-H3"));
        p.model_overrides.insert(
            "MiniMax-H3".into(),
            serde_json::from_value(json!({"capabilities":{"videoGeneration":false}})).unwrap(),
        );
        assert!(!is_video_model(&p, "MiniMax-H3"));
        let encoded = serde_json::to_value(&p).unwrap();
        assert_eq!(
            encoded["modelOverrides"]["MiniMax-H3"]["capabilities"]["videoGeneration"],
            false
        );
    }
}
