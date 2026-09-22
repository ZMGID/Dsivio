//! Image project pages are linked to shared media tasks; this module owns no provider requests.
use super::{storage, types::*};
use crate::{media_generation::{self, MediaKind, MediaRequest, MediaStatus}, state::AppState};
use futures::{future::BoxFuture, FutureExt};
use std::fs;
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

pub(super) fn validate(cfg: &StudioConfig, _brief: &Brief) -> Result<(), String> {
    if cfg.provider_id.is_empty() || cfg.model.is_empty() { return Err("请选择工作台图片模型".into()); }
    Ok(())
}
pub(super) fn provider(app: &AppHandle, cfg: &StudioConfig) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings_read();
    if !settings.workbench_media.image_models.iter().any(|m| m.provider_id == cfg.provider_id && m.model == cfg.model) {
        return Err("请先把模型加入媒体创作图片模型池".into());
    }
    Ok(())
}
pub(super) fn resolved_gen_brief(
    cfg: &StudioConfig,
    brief: &Brief,
    plan: &ImagePlan,
) -> Result<Brief, String> {
    let mut resolved = brief.clone();
    if brief.ratio != "auto" && brief.resolution != "auto" {
        return Ok(resolved);
    }
    let output = plan.output.as_ref().ok_or("请先自动分配图片参数")?;
    let desired_ratio = if output.width > 0 && output.height > 0 {
        output.width as f64 / output.height as f64
    } else {
        let (w, h) = output.ratio.split_once(':').ok_or("无效画幅")?;
        w.parse::<f64>().map_err(|_| "无效画幅")? / h.parse::<f64>().map_err(|_| "无效画幅")?
    };
    let mut best: Option<(f64, &str, &str)> = None;
    for ratio in [
        "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9",
    ] {
        if brief.ratio != "auto" && brief.ratio != ratio {
            continue;
        }
        for resolution in ["1k", "2k", "4k"] {
            if brief.resolution != "auto" && brief.resolution != resolution {
                continue;
            }
            if !allowed_image_output(&cfg.model, &cfg.protocol, ratio, resolution) {
                continue;
            }
            let (w, h) = ratio.split_once(':').unwrap();
            let aspect = w.parse::<f64>().unwrap() / h.parse::<f64>().unwrap();
            let desired_level = if output.width > 0 {
                if output.width.max(output.height) > 2048 {
                    "4k"
                } else if output.width.max(output.height) > 1536 {
                    "2k"
                } else {
                    "1k"
                }
            } else {
                &output.resolution
            };
            let score = (aspect / desired_ratio).ln().abs() * 100.0
                + if resolution == desired_level {
                    0.0
                } else {
                    1.0
                };
            if best.is_none_or(|(previous, _, _)| score < previous) {
                best = Some((score, ratio, resolution));
            }
        }
    }
    let (_, ratio, resolution) = best.ok_or("当前模型不支持指定规格，请把比例和分辨率改为 Auto")?;
    resolved.ratio = ratio.into();
    resolved.resolution = resolution.into();
    Ok(resolved)
}

pub async fn submit(app: &AppHandle, cfg: &StudioConfig, _task_id: &str, brief: &Brief, plan: &ImagePlan) -> Result<Submission, String> {
    let output = resolved_gen_brief(cfg, brief, plan)?;
    let images = plan.refs.iter().map(|path| super::resolve_existing_image(path).map(|p| p.to_string_lossy().into_owned())).collect::<Result<Vec<_>, _>>()?;
    let origin = match brief.feature.as_str() { "gen" => "free-image", "replace" => "clone", "smart" => "template-set", "design" => "set-design", "client" => "batch-set", "workflow" => "template-builder", _ => return Err("未知图片功能".into()) };
    let task = media_generation::start_media_generation(app.clone(), MediaRequest {
        provider_id: cfg.provider_id.clone(), model: cfg.model.clone(), kind: MediaKind::Image,
        prompt: plan.prompt.clone(), images,
        options: [("aspect_ratio".into(), serde_json::json!(output.ratio)), ("size".into(), serde_json::json!(output.resolution.to_uppercase())), ("n".into(), serde_json::json!(1))].into(),
        origin: Some(format!("workbench/{origin}")),
    }).await?;
    Ok(Submission::Pending(format!("media:{}", task.id)))
}
pub async fn poll(app: &AppHandle, cfg: &StudioConfig, task_id: &str, remote: &str) -> Result<Option<Vec<u8>>, String> {
    if let Some(id) = remote.strip_prefix("media:") {
        let task = media_generation::get_media_task(app.clone(), id.into(), Some(true))?;
        return match task.status {
            MediaStatus::Running => Ok(None),
            MediaStatus::Failed => Err(task.error.unwrap_or("媒体任务失败；已有回执已保留".into())),
            MediaStatus::Succeeded => {
                let output = task.outputs.first().ok_or("媒体任务没有输出图片")?;
                fs::read(&output.path).map(Some).map_err(|e| e.to_string())
            }
        };
    }
    media_generation::image_gateway::poll(&app.state::<AppState>(), &media_generation::image_gateway::ReceiptConfig {
        provider_id: cfg.provider_id.clone(), model: cfg.model.clone(), protocol: cfg.protocol.clone(),
    }, task_id, remote).await
}
pub async fn download(app: &AppHandle, url: &str) -> Result<Vec<u8>, String> {
    media_generation::image_gateway::download(&app.state::<AppState>(), url).await
}
pub fn store_image(task_id: &str, result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
    store_image_in(&storage::load_task(task_id)?, result, bytes)
}

pub(super) fn store_image_in(
    task: &Task,
    result: &mut ImageResult,
    bytes: &[u8],
) -> Result<(), String> {
    let mut img = storage::decode(bytes)?;
    let output = task
        .plans
        .iter()
        .find(|plan| plan.product_id == result.product_id && plan.slot_id == result.slot_id)
        .and_then(|plan| plan.output.as_ref());
    let mut resized = std::io::Cursor::new(Vec::new());
    let bytes = if let Some(output) = output.filter(|o| {
        o.width > 0 && o.height > 0 && (o.width != img.width() || o.height != img.height())
    }) {
        img = img.resize_exact(
            output.width,
            output.height,
            image::imageops::FilterType::Lanczos3,
        );
        img.write_to(&mut resized, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        resized.get_ref().as_slice()
    } else {
        bytes
    };
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

fn allowed_image_output(model: &str, protocol: &str, ratio: &str, resolution: &str) -> bool {
    let name = model.to_ascii_lowercase();
    if name.contains("gpt-image-2") {
        return matches!(resolution, "1k" | "2k" | "4k")
            && matches!(
                ratio,
                "1:1" | "2:3" | "3:2" | "3:4" | "4:3" | "4:5" | "5:4" | "9:16" | "16:9"
            );
    }
    if name.contains("dall-e-3") {
        return resolution == "1k" && matches!(ratio, "1:1" | "9:16" | "16:9");
    }
    if name.contains("gpt-image") || name.starts_with("dall-e") || protocol == "openai" {
        return resolution == "1k" && matches!(ratio, "1:1" | "2:3" | "3:2");
    }
    if protocol == "grok" || name.starts_with("grok") || name.contains("grok-imagine") {
        return resolution != "4k"
            && matches!(
                ratio,
                "1:1" | "2:3" | "3:4" | "9:16" | "3:2" | "4:3" | "16:9"
            );
    }
    if name.starts_with("imagen") {
        return resolution != "4k" && matches!(ratio, "1:1" | "3:4" | "9:16" | "4:3" | "16:9");
    }
    if protocol == "gemini" || protocol == "gemini-chat" || name.contains("gemini") {
        let supported_resolution =
            if name.contains("gemini-2.5-flash-image") || name.contains("flash-lite-image") {
                resolution == "1k"
            } else {
                matches!(resolution, "1k" | "2k" | "4k")
            };
        return supported_resolution
            && matches!(
                ratio,
                "1:1" | "2:3" | "3:2" | "3:4" | "4:3" | "4:5" | "5:4" | "9:16" | "16:9"
            );
    }
    resolution != "4k" && matches!(ratio, "1:1" | "9:16" | "16:9")
}
