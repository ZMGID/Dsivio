//! 1688 product gateway contract, independently implemented for the native app.
//! Reference: next-1688/1688-product-find at 2c7d8ee (request format/signature).
use super::types::*;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use sha2::Sha256;
use std::{
    collections::{BTreeMap, HashSet},
    io::Cursor,
    time::Duration,
};

const PATH: &str = "/api/alibaba.1688.find.product/1.0.0/github";
const GATEWAY: &str = "https://gateway.1688.com";
const VERSION: &str = "1.7.0";
// A page remount or second window must not launch another paid request while one is running.
static SEARCH_SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

fn keys(raw: &str) -> Result<(String, String), String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("请先在「1688 接口设置」中配置 AK".into());
    }
    if raw.len() > 4096 {
        return Err("1688 AK 格式无效".into());
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(raw.trim_end_matches('='))
        .ok()
        .and_then(|b| String::from_utf8(b).ok());
    let value = decoded.as_deref().filter(|s| s.len() > 32).unwrap_or(raw);
    if !value.is_ascii()
        || value.len() <= 32
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("1688 AK 格式无效，请复制完整的 AK".into());
    }
    Ok((value[32..].into(), value[..32].into()))
}
fn signed_headers(ak: &str, body: &str, time: &str, nonce: &str) -> Result<HeaderMap, String> {
    let (id, secret) = keys(ak)?;
    let digest = STANDARD.encode(Md5::digest(body.as_bytes()));
    let values = BTreeMap::from([
        ("x-csk-ak", id),
        ("x-csk-time", time.into()),
        ("x-csk-nonce", nonce.into()),
        ("x-csk-content-md5", digest.clone()),
        ("x-csk-version", VERSION.into()),
    ]);
    let canonical = values
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect::<String>();
    let payload = format!("POST\n{digest}\napplication/json\n{time}\n{canonical}{PATH}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| "签名初始化失败")?;
    mac.update(payload.as_bytes());
    let mut headers = HeaderMap::new();
    for (k, v) in values.into_iter().chain([
        ("x-csk-sign", STANDARD.encode(mac.finalize().into_bytes())),
        ("content-type", "application/json".into()),
        ("x-skill-code", "1688-product-find".into()),
        ("x-skill-version", VERSION.into()),
        ("x-request-id", uuid::Uuid::new_v4().simple().to_string()),
    ]) {
        headers.insert(
            HeaderName::from_static(k),
            HeaderValue::from_str(&v).map_err(|_| "1688 AK 格式无效")?,
        );
    }
    Ok(headers)
}
fn image_base64(data: &str) -> Result<String, String> {
    if data.len() > 14 * 1024 * 1024 {
        return Err("单张图片不超过 10MB".into());
    }
    let (prefix, encoded) = data.split_once(',').ok_or("图片格式无效")?;
    if !matches!(
        prefix,
        "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
    ) {
        return Err("请选择 PNG、JPEG 或 WebP 图片".into());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| "图片编码无效")?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("单张图片不超过 10MB".into());
    }
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "图片格式无效")?;
    let (w, h) = reader.into_dimensions().map_err(|_| "图片内容无效")?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 40_000_000 {
        return Err("图片最多 4000 万像素".into());
    }
    let img = image::load_from_memory(&bytes)
        .map_err(|_| "图片解码失败")?
        .thumbnail(800, 800)
        .to_rgba8();
    let mut rgb = image::RgbImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let a = u32::from(p[3]);
        rgb.put_pixel(
            x,
            y,
            image::Rgb(
                [0, 1, 2].map(|c| ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8),
            ),
        );
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .map_err(|_| "图片压缩失败")?;
    Ok(STANDARD.encode(jpeg))
}
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().chars().take(1000).collect()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn web_url(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    let value = if s.starts_with("//") {
        format!("https:{s}")
    } else {
        s.to_string()
    };
    let u = reqwest::Url::parse(&value).ok()?;
    (value.len() <= 4096
        && matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none())
    .then_some(value)
}
fn response_products(value: Value) -> Result<Vec<SourcingProduct>, String> {
    if value.get("success") == Some(&Value::Bool(false)) {
        let code = value
            .get("msgCode")
            .and_then(Value::as_str)
            .filter(|code| !code.is_empty())
            .or_else(|| value.get("code").and_then(Value::as_str))
            .unwrap_or("");
        return Err(match code {
            "SignatureInvalid" => "1688 签名校验失败，请检查 AK 和系统时间",
            "QosAppFrequencyLimit" | "QosApiFrequencyLimit" => "1688 请求频率超限，请稍后重试",
            "1688_token_expired"
            | "1688_invalid_token"
            | "1688_token_revoked"
            | "1688_token_unauthorized"
            | "1688_no_scope_specified"
            | "1688_invalid_scope" => "1688 授权无效或权限不足，请到授权网站检查账号权限",
            _ => "1688 商品搜索失败，请检查接口授权或稍后重试",
        }
        .into());
    }
    let rows = value
        .get("model")
        .filter(|v| v.is_object())
        .or(value.get("data"))
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
        .ok_or("1688 返回结构异常，未获得商品列表")?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows.iter().take(100) {
        let Some(id) = scalar(&row["itemId"]).filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        else {
            continue;
        };
        let Some(title) = scalar(&row["title"]) else {
            continue;
        };
        let sku_id = scalar(&row["skuId"]);
        if !seen.insert((id.clone(), sku_id.clone())) {
            continue;
        }
        // Construct from the platform ID, so product navigation always points to 1688.
        out.push(SourcingProduct {
            url: format!("https://detail.1688.com/offer/{id}.html"),
            id,
            title,
            image_url: web_url(&row["imageUrl"]),
            price: scalar(&row["currentPrice"]),
            supplier: scalar(&row["company"]),
            sku_id,
            sku_title: scalar(&row["skuTitle"]),
            minimum_order: scalar(&row["quantityBegin"]),
            sold_count: scalar(&row["soldOut"]),
            stock: scalar(&row["storeAmount"]),
            relevance: row["score"].as_f64(),
        });
    }
    if !rows.is_empty() && out.is_empty() {
        return Err("1688 返回商品字段异常，请稍后重试".into());
    }
    Ok(out)
}
async fn request_products(
    client: &reqwest::Client,
    endpoint: &str,
    ak: &str,
    body: String,
) -> Result<Vec<SourcingProduct>, String> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let headers = signed_headers(
        ak,
        &body,
        &chrono::Utc::now().timestamp().to_string(),
        &nonce[..8],
    )?;
    // No automatic retries: searches may consume quota, even if a response is lost.
    let mut response = client
        .post(endpoint)
        .headers(headers)
        .body(body)
        .timeout(Duration::from_secs(45))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "1688 搜索超时，请重试"
            } else {
                "无法连接 1688，请检查网络后重试"
            }
        })?;
    if !response.status().is_success() {
        return Err(format!(
            "1688 请求失败（HTTP {}），请检查授权或稍后重试",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "读取 1688 响应失败")? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err("1688 返回内容过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    response_products(serde_json::from_slice(&bytes).map_err(|_| "1688 未返回有效 JSON")?)
}
pub(super) async fn search(
    ak: String,
    request: LookalikeRequest,
) -> Result<SourcingSearch, String> {
    let _permit = SEARCH_SLOT
        .try_acquire()
        .map_err(|_| "已有 1688 搜索正在进行，请稍后在搜索记录中查看结果")?;
    keys(&ak)?;
    if !(1..=50).contains(&request.limit)
        || !(1..=1_000_000).contains(&request.purchase_amount)
        || request.name.len() > 512
    {
        return Err("搜索参数无效：返回数量 1–50，采购件数 1–1000000".into());
    }
    let image = tauri::async_runtime::spawn_blocking(move || image_base64(&request.image))
        .await
        .map_err(|_| "处理图片失败")??;
    let mut body = json!({"imgBase64":image,"pageSize":request.limit,"purchaseAmount":request.purchase_amount,"scoreLevel":"high","tags":"4306497"});
    if let Some(sort) = match request.sort {
        ProductSort::Relevance => None,
        ProductSort::PriceAsc => Some("price_asc"),
        ProductSort::PriceDesc => Some("price_desc"),
        ProductSort::SalesDesc => Some("sold_desc"),
    } {
        body["sortType"] = json!(sort);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化 1688 连接")?;
    let products =
        request_products(&client, &format!("{GATEWAY}{PATH}"), &ak, body.to_string()).await?;
    Ok(SourcingSearch {
        id: uuid::Uuid::new_v4().to_string(),
        name: request.name,
        fetched_at: super::store::now(),
        sort: request.sort,
        purchase_amount: request.purchase_amount,
        products,
        history_warning: None,
    })
}
#[cfg(test)]
mod tests;

#[cfg(debug_assertions)]
pub(super) fn validate_key(raw: &str) -> Result<(), String> {
    keys(raw).map(|_| ())
}
