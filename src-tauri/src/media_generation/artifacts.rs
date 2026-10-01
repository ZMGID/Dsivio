//! Bounded, atomic media downloads shared by cloud providers and local workflows.
use super::{MediaKind, MediaOutput};
use std::path::Path;
use tokio::io::AsyncWriteExt;

const LIMIT: usize = 512 * 1024 * 1024;

/// Credentials attached to a result download. Only sent to the provider's own origin.
pub(crate) enum DownloadAuth<'a> {
    None,
    Bearer(&'a str),
    Token(&'a str),
    Google(&'a str),
}

/// Result downloads follow redirects by hand, so they use clients with redirects disabled,
/// one per proxy policy, built once.
pub(crate) fn download_client(
    provider: &crate::settings::ModelProvider,
) -> &'static reqwest::Client {
    static PROXIED: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    static DIRECT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let direct = !provider.request.use_system_proxy;
    (if direct { &DIRECT } else { &PROXIED }).get_or_init(|| {
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(20));
        let builder = if direct { builder.no_proxy() } else { builder };
        builder.build().unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// GET a generated result with the provider's proxy policy. Redirects are followed by hand so a
/// credential never leaves the original origin and HTTPS is never downgraded.
pub(crate) async fn fetch(
    client: &reqwest::Client,
    origin: &reqwest::Url,
    url: &str,
    auth: DownloadAuth<'_>,
    timeout: std::time::Duration,
) -> Result<reqwest::Response, String> {
    let relative = url.starts_with('/') && !url.starts_with("//");
    let mut url = if relative {
        origin.join(url).map_err(|_| "结果地址无效")?
    } else {
        reqwest::Url::parse(url).map_err(|_| "结果地址无效")?
    };
    let authenticated = !matches!(auth, DownloadAuth::None);
    if authenticated && origin.origin() != url.origin() {
        return Err("结果下载要求鉴权，但地址与原服务不一致，已阻止发送密钥".into());
    }
    for _ in 0..5 {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("结果地址必须是 HTTP(S)".into());
        }
        let mut request = client.get(url.clone()).timeout(timeout);
        if origin.origin() == url.origin() {
            request = match auth {
                DownloadAuth::None => request,
                DownloadAuth::Bearer(key) => request.bearer_auth(key),
                DownloadAuth::Token(key) => request.header("Authorization", format!("Token {key}")),
                DownloadAuth::Google(key) => request.header("x-goog-api-key", key),
            };
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("结果下载失败：{}；已受理的任务可继续查询", e.without_url()))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let target = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or("下载跳转缺少地址")?;
        let next = url.join(target).map_err(|_| "下载跳转地址无效")?;
        if url.scheme() == "https" && next.scheme() != "https" {
            return Err("下载跳转不能降低 HTTPS 安全性".into());
        }
        url = next;
    }
    Err("下载跳转次数过多".into())
}

/// Read a response body into memory with a size bound (small results such as images).
pub(crate) async fn read_bounded(
    mut response: reqwest::Response,
    max: usize,
) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("结果下载 HTTP {}", response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > max as u64)
    {
        return Err("结果文件过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "下载中断，可恢复")? {
        if bytes.len() + chunk.len() > max {
            return Err("结果文件过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) async fn save_response(
    mut response: reqwest::Response,
    path: &Path,
    kind: &MediaKind,
) -> Result<MediaOutput, String> {
    if !response.status().is_success() {
        return Err(format!(
            "媒体下载 HTTP {}，可继续查询恢复下载",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > LIMIT as u64)
    {
        return Err("结果超过 512 MB".into());
    }
    let temporary = path.with_extension("part");
    let result = async {
        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|e| e.to_string())?;
        let mut size = 0;
        let mut header = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "下载中断，可恢复")? {
            size += chunk.len();
            if size > LIMIT {
                return Err("结果超过 512 MB".into());
            }
            header.extend_from_slice(&chunk[..chunk.len().min(16 - header.len())]);
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        commit(file, &temporary, path, &header, kind).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

/// Image adapters return decoded bytes rather than a streaming HTTP response.
pub(crate) async fn save_image(bytes: &[u8], path: &Path) -> Result<MediaOutput, String> {
    if bytes.len() > LIMIT {
        return Err("结果超过 512 MB".into());
    }
    let temporary = path.with_extension("part");
    let result = async {
        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).await.map_err(|e| e.to_string())?;
        commit(file, &temporary, path, bytes, &MediaKind::Image).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

/// Audio is accepted only when its actual container agrees with the requested format.
/// Raw PCM is deliberately not a supported artifact.
pub(crate) async fn save_audio(bytes: &[u8], path: &Path, expected_format: &str) -> Result<MediaOutput, String> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err("音频为空或超过 512 MB".into());
    }
    let (extension, mime) = identify_audio(bytes).ok_or("结果不是支持的音频容器")?;
    if extension != expected_format {
        return Err(format!("音频格式不匹配：请求 {expected_format}，实际 {extension}"));
    }
    let temporary = path.with_extension("part");
    let result = async {
        let mut file = tokio::fs::File::create(&temporary).await.map_err(|e| e.to_string())?;
        file.write_all(bytes).await.map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        verify_audio(&temporary,expected_format).await?;
        let output = path.with_extension(extension);
        tokio::fs::rename(&temporary, &output).await.map_err(|e| e.to_string())?;
        Ok(MediaOutput { path: output.to_string_lossy().into_owned(), mime: mime.into() })
    }.await;
    if result.is_err() { let _ = tokio::fs::remove_file(&temporary).await; }
    result
}

async fn verify_audio(path: &Path, expected_format: &str) -> Result<(), String> {
    let tools = crate::media_runtime::runtime::tools_at(&crate::media_runtime::runtime::root()?)?;
    let probe = tools.get("ffprobe").ok_or("缺少内置 ffprobe，不能验证语音产物")?;
    let mut command = tokio::process::Command::new(probe);
    command.kill_on_drop(true).args(["-v", "error", "-show_streams", "-show_format", "-of", "json"]).arg(path);
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await.map_err(|_| "音频验证超时")?.map_err(|e| e.to_string())?;
    if !output.status.success() { return Err("音频容器无法解码".into()); }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|_| "音频验证结果无效")?;
    let streams = value["streams"].as_array().ok_or("音频缺少可解码流")?;
    if streams.iter().any(|s| s["codec_type"] == "video") || !streams.iter().any(|s| s["codec_type"] == "audio" && s["codec_name"].as_str().is_some_and(|s| s != "unknown")) {
        return Err("结果不是纯音频".into());
    }
    let codec_matches = streams.iter().any(|s| s["codec_type"] == "audio" && match expected_format {
        "wav" => value["format"]["format_name"] == "wav",
        "mp3" | "opus" | "aac" | "flac" => s["codec_name"] == expected_format,
        _ => false,
    });
    if !codec_matches {return Err("音频编码与请求格式不一致".into());}
    if !value["format"]["duration"].as_str().and_then(|s| s.parse::<f64>().ok()).is_some_and(|n| n.is_finite() && n > 0.0) {
        return Err("音频没有有效时长".into());
    }
    Ok(())
}

fn identify_audio(header: &[u8]) -> Option<(&'static str, &'static str)> {
    if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WAVE") {
        Some(("wav", "audio/wav"))
    } else if header.starts_with(b"fLaC") {
        Some(("flac", "audio/flac"))
    } else if header.starts_with(b"OggS") && header.windows(8).any(|w| w == b"OpusHead") {
        Some(("opus", "audio/ogg"))
    } else if header.starts_with(b"ID3") || header.len() >= 2 && header[0] == 0xff && header[1] & 0xe6 == 0xe2 {
        Some(("mp3", "audio/mpeg"))
    } else if header.len() >= 2 && header[0] == 0xff && header[1] & 0xf6 == 0xf0 {
        Some(("aac", "audio/aac"))
    } else {
        None
    }
}

/// Full transcript evidence is always delivered atomically, even when too large to inline.
pub(crate) fn save_transcript(value: &serde_json::Value, path: &Path) -> Result<MediaOutput, String> {
    crate::chat::storage::atomic_write(path, &serde_json::to_string(value).map_err(|e| e.to_string())?, "transcript")?;
    Ok(MediaOutput { path: path.to_string_lossy().into_owned(), mime: "application/json".into() })
}

async fn commit(
    file: tokio::fs::File,
    temporary: &Path,
    path: &Path,
    header: &[u8],
    kind: &MediaKind,
) -> Result<MediaOutput, String> {
    let (extension, mime) = identify(header, kind).ok_or("结果为空或不是支持的媒体文件")?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    let output = path.with_extension(extension);
    tokio::fs::rename(temporary, &output)
        .await
        .map_err(|e| e.to_string())?;
    Ok(MediaOutput {
        path: output.to_string_lossy().into_owned(),
        mime: mime.into(),
    })
}

fn identify(header: &[u8], kind: &MediaKind) -> Option<(&'static str, &'static str)> {
    // ComfyUI video workflows may explicitly produce animated GIFs.
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Some(("gif", "image/gif"));
    }
    match kind {
        MediaKind::Image if header.starts_with(b"\x89PNG\r\n\x1a\n") => Some(("png", "image/png")),
        MediaKind::Image if header.starts_with(&[0xff, 0xd8, 0xff]) => Some(("jpg", "image/jpeg")),
        MediaKind::Image if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") => {
            Some(("webp", "image/webp"))
        }
        MediaKind::Video if header.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) => {
            Some(("webm", "video/webm"))
        }
        MediaKind::Video if header.get(4..8) == Some(b"ftyp") => {
            Some(if header.get(8..12) == Some(b"qt  ") {
                ("mov", "video/quicktime")
            } else {
                ("mp4", "video/mp4")
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_must_match_requested_media_kind() {
        assert!(identify(b"<html>error</html>", &MediaKind::Video).is_none());
        assert!(identify(b"", &MediaKind::Image).is_none());
        assert!(identify(b"\x89PNG\r\n\x1a\n", &MediaKind::Video).is_none());
        assert_eq!(
            identify(b"\x00\x00\x00\x18ftypqt  ", &MediaKind::Video),
            Some(("mov", "video/quicktime"))
        );
        assert_eq!(
            identify(b"\x89PNG\r\n\x1a\n", &MediaKind::Image),
            Some(("png", "image/png"))
        );
    }
    #[tokio::test]
    async fn audio_container_rejects_mime_spoofing_and_format_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio");
        assert!(save_audio(b"<html>error</html>", &path, "mp3").await.is_err());
        assert!(save_audio(b"RIFF0000WAVEfmt ", &path, "mp3").await.is_err());
        assert!(save_audio(b"RIFF0000WAVEfmt ", &path, "wav").await.is_err());
        assert!(!path.with_extension("wav").exists());
        assert!(!path.with_extension("part").exists());
    }
}
