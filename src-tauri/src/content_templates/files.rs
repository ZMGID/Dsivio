use serde::{de::DeserializeOwned, Serialize};
use std::{fs, path::Path};
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T, String> {
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
        .map_err(|e| e.to_string())
}
pub fn write<T: Serialize>(p: &Path, value: &T) -> Result<(), String> {
    let tmp = p.with_extension(format!("{}.tmp", id()));
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, p).map_err(|e| e.to_string())
}
pub fn decode(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("单张图片不能超过 50 MB".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("无法读取图片（支持 PNG、JPEG、WebP）：{e}"))
}
