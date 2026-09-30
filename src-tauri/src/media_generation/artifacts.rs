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
}
