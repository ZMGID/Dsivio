// Portions derived from Hermes WeCom media (MIT License, Copyright (c) 2025 Nous Research).
// Inbound CDN bytes are decrypted with the message AES key. Outbound files are chunked over the bot socket.

use std::future::Future;
use std::path::Path;

use aes::Aes256;
use base64::Engine;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use tauri::AppHandle;

use super::common::{download_media, http_client, save_media, MAX_MEDIA_BYTES};
use super::types::ImPlatform;

const IMAGE_MAX: usize = 10 * 1024 * 1024;
const VIDEO_MAX: usize = 10 * 1024 * 1024;
const VOICE_MAX: usize = 2 * 1024 * 1024;
const CHUNK: usize = 512 * 1024;
const MAX_CHUNKS: usize = 100;

pub(crate) trait WecomRpc {
    fn command<'a>(
        &'a self,
        cmd: &'a str,
        body: Value,
    ) -> impl Future<Output = Result<Value, String>> + Send + 'a;
}

#[derive(Debug)]
pub(crate) struct MediaRef {
    pub kind: String,
    pub url: String,
    pub aeskey: String,
    pub filename: String,
    pub inline_b64: String,
}

#[derive(Debug)]
pub(crate) struct Prepared {
    pub data: Vec<u8>,
    pub media_type: String,
    pub filename: String,
    pub downgrade: Option<String>,
}

pub(crate) fn refs_from_body(body: &Value) -> Vec<MediaRef> {
    let msgtype = text_at(body, "msgtype").to_ascii_lowercase();
    let mut refs = Vec::new();
    if msgtype == "mixed" {
        if let Some(items) = body.pointer("/mixed/msg_item").and_then(Value::as_array) {
            for item in items {
                if text_at(item, "msgtype").eq_ignore_ascii_case("image") {
                    push_ref(&mut refs, "image", item.get("image"));
                }
            }
        }
    } else {
        for kind in ["image", "file", "video", "voice"] {
            if msgtype == kind || body.get(kind).is_some() {
                push_ref(&mut refs, kind, body.get(kind));
            }
        }
        if msgtype == "appmsg" {
            if let Some(appmsg) = body.get("appmsg") {
                push_ref(&mut refs, "file", appmsg.get("file").or(Some(appmsg)));
            }
        }
    }
    if let Some(quote) = body.get("quote") {
        let quote_type = text_at(quote, "msgtype").to_ascii_lowercase();
        if matches!(quote_type.as_str(), "image" | "file" | "video" | "voice") {
            push_ref(&mut refs, &quote_type, quote.get(quote_type.as_str()));
        }
    }
    refs
}

fn push_ref(out: &mut Vec<MediaRef>, kind: &str, media: Option<&Value>) {
    let Some(media) = media.filter(|value| value.is_object()) else {
        return;
    };
    let filename = {
        let named = text_at(media, "filename");
        if named.is_empty() {
            text_at(media, "name")
        } else {
            named
        }
    };
    out.push(MediaRef {
        kind: kind.to_owned(),
        url: text_at(media, "url"),
        aeskey: text_at(media, "aeskey"),
        filename,
        inline_b64: text_at(media, "base64"),
    });
}

pub(crate) async fn store_inbound(
    app: &AppHandle,
    message_id: &str,
    media: &MediaRef,
) -> Option<String> {
    let bytes = if !media.inline_b64.is_empty() {
        decode_b64(&media.inline_b64).ok()?
    } else if media.url.starts_with("https://") {
        let client = http_client().ok()?;
        let encrypted = download_media(&client, &media.url, MAX_MEDIA_BYTES)
            .await
            .ok()?;
        if media.aeskey.is_empty() {
            encrypted
        } else {
            decrypt_media(&encrypted, &media.aeskey).ok()?
        }
    } else {
        return None;
    };
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
        return None;
    }
    let name = if media.filename.is_empty() {
        format!("wecom-{}.{}", media.kind, ext_for(&bytes, &media.kind))
    } else {
        media.filename.clone()
    };
    save_media(app, ImPlatform::Wecom, message_id, &name, &bytes).ok()
}

pub(crate) async fn load_outbound(path: &str) -> Result<(Vec<u8>, String), String> {
    let filename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment.bin")
        .to_owned();
    if path.starts_with("https://") {
        let client = http_client()?;
        return Ok((
            download_media(&client, path, MAX_MEDIA_BYTES).await?,
            filename,
        ));
    }
    if path.starts_with("http://") {
        return Err("企业微信附件只允许公共 HTTPS 地址".into());
    }
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|_| "找不到要发送的附件".to_string())?;
    if meta.len() as usize > MAX_MEDIA_BYTES {
        return Err("附件超过 20MB，未发送".into());
    }
    Ok((
        tokio::fs::read(path)
            .await
            .map_err(|_| "无法读取要发送的附件".to_string())?,
        filename,
    ))
}

pub(crate) fn prepare_bytes(data: Vec<u8>, filename: &str) -> Result<Prepared, String> {
    if data.is_empty() {
        return Err("不能发送空附件".into());
    }
    if data.len() > MAX_MEDIA_BYTES {
        return Err("附件超过 20MB，未发送".into());
    }
    let (detected, _) = kind_of(filename);
    let (media_type, downgrade) = limit_kind(detected, data.len(), filename);
    Ok(Prepared {
        data,
        media_type,
        filename: filename.to_owned(),
        downgrade,
    })
}

pub(crate) async fn upload_media(
    rpc: &impl WecomRpc,
    data: &[u8],
    media_type: &str,
    filename: &str,
) -> Result<String, String> {
    if data.is_empty() {
        return Err("不能上传空附件".into());
    }
    let total_chunks = data.len().div_ceil(CHUNK);
    if total_chunks > MAX_CHUNKS {
        return Err("附件分片超过企业微信上限".into());
    }
    let init = rpc
        .command(
            "aibot_upload_media_init",
            json!({
                "type": media_type,
                "filename": filename,
                "total_size": data.len(),
                "total_chunks": total_chunks,
                "md5": format!("{:x}", Md5::digest(data)),
            }),
        )
        .await?;
    require_ok(&init)?;
    let upload_id = nested_str(&init, "upload_id")
        .ok_or("企业微信媒体上传未返回上传任务")?
        .to_owned();
    for (chunk_index, start) in (0..data.len()).step_by(CHUNK).enumerate() {
        let end = (start + CHUNK).min(data.len());
        let chunk = rpc.command("aibot_upload_media_chunk", json!({
            "upload_id": upload_id,
            "chunk_index": chunk_index,
            "base64_data": base64::engine::general_purpose::STANDARD.encode(&data[start..end]),
        })).await?;
        require_ok(&chunk)?;
    }
    let finish = rpc
        .command("aibot_upload_media_finish", json!({"upload_id": upload_id}))
        .await?;
    require_ok(&finish)?;
    nested_str(&finish, "media_id")
        .map(str::to_owned)
        .ok_or_else(|| "企业微信媒体上传未返回 media_id".into())
}

pub(crate) fn decrypt_media(encrypted: &[u8], aes_key: &str) -> Result<Vec<u8>, String> {
    if encrypted.is_empty() || encrypted.len() % 16 != 0 {
        return Err("媒体密文无效".into());
    }
    let key_bytes = decode_aes_key(aes_key)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&key_bytes[..16]);
    let plain = cbc::Decryptor::<Aes256>::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<NoPadding>(encrypted)
        .map_err(|_| "媒体解密失败".to_string())?;
    let pad = *plain.last().ok_or("媒体解密失败")? as usize;
    if pad == 0
        || pad > 32
        || pad > plain.len()
        || plain[plain.len() - pad..]
            .iter()
            .any(|byte| *byte as usize != pad)
    {
        return Err("媒体填充无效".into());
    }
    Ok(plain[..plain.len() - pad].to_vec())
}

pub(crate) fn require_ok(value: &Value) -> Result<(), String> {
    if let Some(code) = failure_code(value) {
        return Err(format!("企业微信媒体请求失败 (errcode={code})"));
    }
    Ok(())
}

fn failure_code(value: &Value) -> Option<i64> {
    [value.get("errcode"), value.pointer("/body/errcode")]
        .into_iter()
        .flatten()
        .find_map(Value::as_i64)
        .filter(|code| *code != 0)
}

fn limit_kind(detected: &str, len: usize, filename: &str) -> (String, Option<String>) {
    if detected == "voice" && !filename.to_ascii_lowercase().ends_with(".amr") {
        return ("file".into(), Some("语音不是 AMR，已改为文件发送".into()));
    }
    let (cap, label) = match detected {
        "image" => (IMAGE_MAX, "图片"),
        "video" => (VIDEO_MAX, "视频"),
        "voice" => (VOICE_MAX, "语音"),
        _ => return (detected.to_owned(), None),
    };
    if len > cap {
        (
            "file".into(),
            Some(format!("{label}超过大小限制，已改为文件发送")),
        )
    } else {
        (detected.to_owned(), None)
    }
}

fn kind_of(filename: &str) -> (&'static str, &'static str) {
    match Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => ("image", "image/png"),
        "jpg" | "jpeg" => ("image", "image/jpeg"),
        "gif" => ("image", "image/gif"),
        "webp" => ("image", "image/webp"),
        "bmp" => ("image", "image/bmp"),
        "mp4" | "mov" => ("video", "video/mp4"),
        "amr" => ("voice", "audio/amr"),
        _ => ("file", "application/octet-stream"),
    }
}

fn ext_for(data: &[u8], kind: &str) -> &'static str {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "png";
    }
    if data.starts_with(b"\xff\xd8\xff") {
        return "jpg";
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return "gif";
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        return "webp";
    }
    match kind {
        "image" => "jpg",
        "video" => "mp4",
        "voice" => "amr",
        _ => "bin",
    }
}

fn decode_aes_key(aes_key: &str) -> Result<Vec<u8>, String> {
    // One percent-decode. Base64 keeps '+'; form-urlencoded '+' → space would corrupt the key.
    let decoded = percent_decode(aes_key.trim())?;
    let pad = (4 - decoded.len() % 4) % 4;
    let padded = format!("{decoded}{}", "=".repeat(pad));
    let key = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .map_err(|_| "媒体密钥无效".to_string())?;
    if key.len() != 32 {
        return Err("媒体密钥长度无效".into());
    }
    Ok(key)
}

fn decode_b64(data: &str) -> Result<Vec<u8>, String> {
    let payload = data.rsplit(',').next().unwrap_or(data).trim();
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "媒体内容无效".to_string())
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (from_hex(bytes[index + 1]), from_hex(bytes[index + 2]))
            {
                out.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).map_err(|_| "媒体密钥无效".to_string())
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn text_at(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn nested_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| value.get("body")?.get(key)?.as_str())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockEncryptMut;

    #[test]
    fn voice_that_is_not_amr_becomes_a_file() {
        let (kind, note) = limit_kind("voice", 100, "clip.ogg");
        assert_eq!(kind, "file");
        assert!(note.is_some());
        let (kind, note) = limit_kind("voice", 100, "clip.amr");
        assert_eq!(kind, "voice");
        assert!(note.is_none());
    }

    #[test]
    fn media_aes_roundtrip_keeps_unicode() {
        let key = b"0123456789abcdef0123456789abcdef";
        let plain = "你好-media".as_bytes();
        let amount = match plain.len() % 32 {
            0 => 32,
            rest => 32 - rest,
        };
        let mut padded = plain.to_vec();
        padded.extend(std::iter::repeat(amount as u8).take(amount));
        let encrypted = cbc::Encryptor::<Aes256>::new_from_slices(key, &key[..16])
            .unwrap()
            .encrypt_padded_vec_mut::<NoPadding>(&padded);
        let aes_key = base64::engine::general_purpose::STANDARD.encode(key);
        assert_eq!(decrypt_media(&encrypted, &aes_key).unwrap(), plain);
    }

    #[test]
    fn fb_key_raw_and_percent_encoded_restore_cipher() {
        let key = [0xfb_u8; 32];
        let raw = base64::engine::general_purpose::STANDARD.encode(key);
        assert!(raw.contains('+'), "fixture must exercise base64 '+'");
        let encoded = percent_encode_once(&raw);
        assert!(encoded.contains("%2B"));
        assert_ne!(encoded, raw);
        let plain = "真实密文-fb".as_bytes();
        let amount = match plain.len() % 32 {
            0 => 32,
            rest => 32 - rest,
        };
        let mut padded = plain.to_vec();
        padded.extend(std::iter::repeat(amount as u8).take(amount));
        let cipher = cbc::Encryptor::<Aes256>::new_from_slices(&key, &key[..16])
            .unwrap()
            .encrypt_padded_vec_mut::<NoPadding>(&padded);

        let raw_body = serde_json::json!({
            "msgtype": "image",
            "image": {"url": "https://cdn.example/a", "aeskey": raw}
        });
        let encoded_body = serde_json::json!({
            "msgtype": "image",
            "image": {"url": "https://cdn.example/a", "aeskey": encoded}
        });
        let raw_ref = &refs_from_body(&raw_body)[0];
        let encoded_ref = &refs_from_body(&encoded_body)[0];
        assert_eq!(raw_ref.aeskey, raw);
        assert_eq!(encoded_ref.aeskey, encoded);
        assert_eq!(decode_aes_key(&raw_ref.aeskey).unwrap(), key);
        assert_eq!(decode_aes_key(&encoded_ref.aeskey).unwrap(), key);
        assert_eq!(decrypt_media(&cipher, &raw_ref.aeskey).unwrap(), plain);
        assert_eq!(decrypt_media(&cipher, &encoded_ref.aeskey).unwrap(), plain);
        let twice = percent_encode_once(&encoded);
        assert!(decode_aes_key(&twice).is_err() || decode_aes_key(&twice).unwrap() != key);
    }

    fn percent_encode_once(input: &str) -> String {
        let mut out = String::new();
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char);
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }
}
