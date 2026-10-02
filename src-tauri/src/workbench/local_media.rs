//! Reads one dropped local image so upload fields can treat it like a picked file.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::path::Path;

const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalImagePayload {
    pub name: String,
    pub mime: String,
    pub base64: String,
}

fn mime_for(extension: &str) -> Option<&'static str> {
    match extension {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn read_image(path: &Path) -> Result<LocalImagePayload, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mime = mime_for(&extension).ok_or("不支持的图片格式")?;
    let metadata = std::fs::metadata(path).map_err(|error| format!("无法读取文件：{error}"))?;
    if !metadata.is_file() {
        return Err("拖入的不是文件".into());
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片超过 10MB".into());
    }
    let bytes = std::fs::read(path).map_err(|error| format!("无法读取文件：{error}"))?;
    Ok(LocalImagePayload {
        name: path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default(),
        mime: mime.into(),
        base64: STANDARD.encode(bytes),
    })
}

#[tauri::command]
pub async fn workbench_read_local_image(path: String) -> Result<LocalImagePayload, String> {
    tauri::async_runtime::spawn_blocking(move || read_image(Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dsivio-local-media-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn returns_name_mime_and_the_exact_bytes_of_an_image() {
        let path = scratch("Backpack.PNG");
        std::fs::write(&path, [1u8, 2, 3, 4]).unwrap();
        let image = read_image(&path).unwrap();
        assert_eq!(image.name, "Backpack.PNG");
        assert_eq!(image.mime, "image/png");
        assert_eq!(STANDARD.decode(image.base64).unwrap(), [1, 2, 3, 4]);
    }

    #[test]
    fn rejects_other_formats_directories_and_oversize_files() {
        let text = scratch("notes.txt");
        std::fs::write(&text, "hello").unwrap();
        assert!(read_image(&text).unwrap_err().contains("不支持"));

        let folder = scratch("folder.png");
        std::fs::create_dir(&folder).unwrap();
        assert!(read_image(&folder).unwrap_err().contains("不是文件"));

        let big = scratch("big.jpg");
        std::fs::File::create(&big).unwrap().set_len(MAX_IMAGE_BYTES + 1).unwrap();
        assert!(read_image(&big).unwrap_err().contains("10MB"));

        assert!(read_image(&scratch("missing.png")).is_err());
    }
}
