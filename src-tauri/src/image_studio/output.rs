//! Human-readable, immutable task folders. Internal records stay in app data.
use super::{storage, types::*};
use std::path::{Path, PathBuf};

pub(super) fn root(config: &StudioConfig) -> Result<PathBuf, String> {
    if !config.output_root.trim().is_empty() {
        let path = PathBuf::from(config.output_root.trim());
        if !path.is_absolute() {
            return Err("图片保存位置必须是完整文件夹路径".into());
        }
        return Ok(path);
    }
    let dirs = directories::UserDirs::new().ok_or("无法定位系统图片目录")?;
    Ok(dirs
        .picture_dir()
        .unwrap_or(dirs.home_dir())
        .join("Dskivio")
        .join("Images"))
}

pub(super) fn name(value: &str) -> String {
    let clean: String = value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let clean = clean.trim_matches('_');
    if clean.is_empty() {
        "image".into()
    } else {
        clean.into()
    }
}

pub(super) fn product_folder(name_: &str, id: &str) -> String {
    format!("{}__{}", name(name_), name(id))
}

/// Directory stays fixed across task renames, revisions and default-root changes.
pub(super) fn prepare(task: &mut Task, config: &StudioConfig) -> Result<(), String> {
    if task.output_directory.is_none() {
        let date: String = task.created_at.chars().take(10).collect();
        let folder = format!(
            "{}_{}__{}",
            name(&date),
            name(&task.brief.name),
            storage::safe_id(&task.id)?
        );
        let path = root(config)?.join(folder);
        std::fs::create_dir_all(&path).map_err(|e| format!("无法创建图片保存目录：{e}"))?;
        task.output_directory = Some(
            path.canonicalize()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into(),
        );
    }
    Ok(())
}

pub(super) fn directory(task: &Task) -> Result<PathBuf, String> {
    task.output_directory
        .as_ref()
        .map(PathBuf::from)
        .ok_or("此旧任务尚未设置成图目录，请先保存任务".into())
}

pub(super) fn original_relative(task: &Task, result: &ImageResult, extension: &str) -> PathBuf {
    let product = task
        .brief
        .products
        .iter()
        .find(|p| p.id == result.product_id);
    Path::new("originals")
        .join(product_folder(
            product.map(|p| p.name.as_str()).unwrap_or("创作"),
            &result.product_id,
        ))
        .join(format!(
            "{}_r{:04}_{}.{}",
            name(&result.slot_id),
            result.revision,
            name(&result.id),
            extension
        ))
}

pub(super) fn resolve(parts: &[&str]) -> Result<PathBuf, String> {
    let id = parts.first().ok_or("缺少图片任务编号")?;
    let task = storage::load_task(storage::safe_id(id)?)?;
    let base = directory(&task)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = parts[1..].iter().fold(base.clone(), |p, s| p.join(s));
    let full = path.canonicalize().map_err(|e| e.to_string())?;
    if !full.starts_with(&base) {
        return Err("图片路径超出任务目录".into());
    }
    Ok(full)
}

pub(super) fn export(
    task: &Task,
    destination: &str,
    width: u32,
    height: u32,
    max_kb: u32,
) -> Result<String, String> {
    export_resolved(task, destination, width, height, max_kb, storage::resolve)
}

pub(super) fn export_resolved(
    task: &Task,
    destination: &str,
    width: u32,
    height: u32,
    max_kb: u32,
    resolve: impl Fn(&str) -> Result<PathBuf, String>,
) -> Result<String, String> {
    use serde_json::json;
    use std::{collections::BTreeMap, fs};
    if width > 8192 || height > 8192 || (width == 0) != (height == 0) {
        return Err("交付宽高须同时填写，且不超过 8192；填 0 导出原图".into());
    }
    let mut latest = BTreeMap::new();
    for result in &task.results {
        if result.revision == task.revision {
            latest.insert((&result.product_id, &result.slot_id), result);
        }
    }
    if !latest.values().any(|r| r.path.is_some()) {
        return Err("当前版本还没有可导出的成图".into());
    }
    let base = if destination.is_empty() {
        directory(task)?.join("deliveries")
    } else {
        PathBuf::from(destination)
    };
    if !base.is_absolute() {
        return Err("交付位置必须是完整文件夹路径".into());
    }
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let dest = base.join(format!(
        "{}_r{:04}_{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        task.revision,
        storage::id()
    ));
    fs::create_dir(&dest).map_err(|e| e.to_string())?;
    let mut manifest = vec![];
    for ((pid, sid), result) in latest {
        let Some(path) = &result.path else { continue };
        let source = resolve(path)?;
        let sku = task
            .brief
            .products
            .iter()
            .find(|p| &p.id == pid)
            .map(|p| p.name.as_str())
            .unwrap_or("创作");
        let folder = product_folder(sku, pid);
        fs::create_dir_all(dest.join(&folder)).map_err(|e| e.to_string())?;
        let unchanged = width == 0 && max_kb == 0;
        let ext = if unchanged {
            source.extension().and_then(|e| e.to_str()).unwrap_or("png")
        } else {
            "jpg"
        };
        // Include result identity so distinct slot IDs that sanitize alike cannot overwrite.
        let filename = format!("{folder}/{}_{}.{}", name(sid), name(&result.id), ext);
        if unchanged {
            fs::copy(&source, dest.join(&filename)).map_err(|e| e.to_string())?;
        } else {
            let img = storage::decode(&fs::read(&source).map_err(|e| e.to_string())?)?;
            let img = if width > 0 {
                img.resize(width, height, image::imageops::FilterType::Lanczos3)
            } else {
                img
            };
            let img = if width > 0 {
                let mut canvas =
                    image::RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]));
                let rgb = img.to_rgb8();
                image::imageops::overlay(
                    &mut canvas,
                    &rgb,
                    ((width - rgb.width()) / 2) as i64,
                    ((height - rgb.height()) / 2) as i64,
                );
                image::DynamicImage::ImageRgb8(canvas)
            } else {
                image::DynamicImage::ImageRgb8(img.to_rgb8())
            };
            let mut chosen = None;
            for quality in [95, 90, 85, 80, 75, 65, 55, 45, 35] {
                let mut bytes = vec![];
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
                    .encode_image(&img)
                    .map_err(|e| e.to_string())?;
                if max_kb == 0 || bytes.len() <= max_kb as usize * 1024 {
                    chosen = Some(bytes);
                    break;
                }
            }
            fs::write(
                dest.join(&filename),
                chosen.ok_or("无法满足大小限制，请降低尺寸或放宽限制。原图未更改。")?,
            )
            .map_err(|e| e.to_string())?;
        }
        manifest.push(json!({"productId":pid,"product":sku,"slot":sid,"file":filename,"sourceVersion":result.id,"prompt":result.prompt}));
    }
    storage::write(
        &dest.join("manifest.json"),
        &json!({"taskId":task.id,"task":task.brief.name,"revision":task.revision,"images":manifest}),
    )?;
    Ok(dest.to_string_lossy().into())
}
