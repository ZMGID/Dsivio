use super::types::*;
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};
use tauri::Manager;
use tokio::sync::{mpsc, watch};

pub const MAX_MEDIA_BYTES: usize = 20 * 1024 * 1024;

pub struct PlatformContext {
    pub app: tauri::AppHandle,
    pub inbound: mpsc::Sender<InboundMessage>,
    pub outbound: mpsc::Receiver<OutboundMessage>,
    pub shutdown: watch::Receiver<bool>,
    pub status: mpsc::Sender<StatusUpdate>,
}
impl PlatformContext {
    pub async fn set_status(
        &self,
        platform: ImPlatform,
        state: ConnectionState,
        message: impl Into<String>,
    ) {
        let _ = self
            .status
            .send(StatusUpdate {
                platform,
                state,
                message: message.into(),
            })
            .await;
    }
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent("Dsivio/1.1 IM")
        .build()
        .map_err(|_| "无法创建 IM HTTP 客户端".into())
}

pub fn split_text(text: &str, max_bytes: usize) -> Vec<String> {
    let max_bytes = max_bytes.max(4);
    if text.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while remaining.len() > max_bytes {
        let mut end = max_bytes;
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        // Prefer a paragraph/line boundary without wasting most of the payload.
        if let Some(line) = remaining[..end].rfind('\n') {
            if line >= end / 2 {
                end = line + 1;
            }
        }
        chunks.push(remaining[..end].to_owned());
        remaining = &remaining[end..];
    }
    if !remaining.is_empty() {
        chunks.push(remaining.to_owned());
    }
    chunks
}

pub fn save_media(
    app: &tauri::AppHandle,
    platform: ImPlatform,
    message_id: &str,
    name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err("IM 附件超过 20 MB 限制".into());
    }
    let key = format!("{:x}", Sha256::digest(message_id.as_bytes()));
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|_| "IM 附件目录不可用")?
        .join("im")
        .join(platform.key())
        .join(&key[..24]);
    std::fs::create_dir_all(&dir).map_err(|_| "无法创建 IM 附件目录")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "无法保护 IM 附件目录")?;
    }
    let filename = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment.bin");
    let filename: String = filename
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':'))
        .take(160)
        .collect();
    let filename = if filename.is_empty() || filename == "." || filename == ".." {
        "attachment.bin"
    } else {
        &filename
    };
    let path = dir.join(format!("{}-{filename}", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|_| "无法保存 IM 附件")?;
    use std::io::Write;
    file.write_all(bytes).map_err(|_| "无法写入 IM 附件")?;
    Ok(path.to_string_lossy().into_owned())
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && !ip.is_unspecified()
        && a != 0
        && a != 255
        && !(a == 100 && (64..=127).contains(&b))
        && !(a == 192 && b == 0)
        && !(a == 198 && (b == 18 || b == 19))
        && !ip.is_documentation()
        && a < 240
}
fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let parts = ip.segments();
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && (parts[0] & 0xfe00) != 0xfc00
        && (parts[0] & 0xffc0) != 0xfe80
        && !(parts[0] == 0x2001 && parts[1] == 0xdb8)
        && (parts[0] & 0xe000) == 0x2000
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

/// Untrusted attachment URLs use a DNS-pinned client, not the platform's token-bearing client.
/// Every resolved address must be public; redirects cannot rebind into the user's local network.
pub async fn download_media(
    _client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let parsed = url::Url::parse(url).map_err(|_| "无效的 IM 附件地址")?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("IM 附件只允许公共 HTTPS 地址".into());
    }
    let host = parsed.host_str().ok_or("IM 附件地址缺少主机")?;
    let port = parsed.port_or_known_default().ok_or("无效的 IM 附件端口")?;
    let addresses: Vec<_> = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "IM 附件 DNS 查询超时")?
    .map_err(|_| "IM 附件 DNS 查询失败")?
    .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return Err("已拒绝非公共网络 IM 附件地址".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| "无法创建安全附件客户端")?;
    let mut response = client
        .get(parsed)
        .send()
        .await
        .map_err(|_| "IM 附件下载失败")?;
    if !response.status().is_success() {
        return Err(format!("IM 附件下载 HTTP {}", response.status().as_u16()));
    }
    let limit = max_bytes.min(MAX_MEDIA_BYTES);
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("IM 附件过大".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "IM 附件读取失败")? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("IM 附件过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_chunks_preserve_payload_and_byte_limit() {
        let text = "汉字🙂\n".repeat(700);
        let chunks = split_text(&text, 2048);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|s| s.len() <= 2048));
    }
    #[test]
    fn blocks_local_and_mapped_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.169.254",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
    }
}
