use super::{AppConfig, Credential, Platform, RemoteShop, ShopStatus};
use aes::Aes128;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;
use url::Url;

type HmacSha256 = Hmac<Sha256>;
const SHOPEE: &str = "https://partner.shopeemobile.com";
const TIKTOK_AUTH: &str = "https://auth.tiktok-shops.com";
const TIKTOK_API: &str = "https://open-api.tiktokglobalshop.com";
const MELI_API: &str = "https://api.mercadolibre.com";
const SHEIN_API: &str = "https://openapi.sheincorp.com";

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "无法创建店铺网络客户端".into())
}

fn hmac_hex(key: &str, input: &str) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).map_err(|_| "签名密钥无效")?;
    mac.update(input.as_bytes());
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn string<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn identifier(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| {
        x.as_str()
            .map(str::to_owned)
            .or_else(|| x.as_i64().map(|n| n.to_string()))
    })
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn response(request: reqwest::RequestBuilder, platform: &str) -> Result<Value, String> {
    let res = request
        .send()
        .await
        .map_err(|_| format!("{platform} 网络请求失败"))?;
    let status = res.status();
    let body = res.json::<Value>().await;
    if !status.is_success() {
        let message = body
            .as_ref()
            .ok()
            .and_then(|body| {
                string(body, "message")
                    .or_else(|| string(body, "error_description"))
                    .or_else(|| string(body, "error"))
            })
            .unwrap_or("平台拒绝请求");
        return Err(format!(
            "{platform} HTTP {}：{}",
            status.as_u16(),
            message.chars().take(160).collect::<String>()
        ));
    }
    body.map_err(|_| format!("{platform} 返回内容无法解析"))
}

fn platform_ok(v: &Value, platform: &str) -> Result<(), String> {
    let code = v.get("code");
    if code.is_some_and(|c| c != 0 && c != "0") {
        return Err(format!(
            "{platform}：{}",
            string(v, "message")
                .or_else(|| string(v, "msg"))
                .unwrap_or("平台接口返回失败")
        ));
    }
    if platform == "Shopee" && string(v, "error").is_some() {
        return Err(format!(
            "Shopee：{}",
            string(v, "message").unwrap_or("平台接口返回失败")
        ));
    }
    Ok(())
}

fn required_https(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "回调地址无效")?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || parsed.username() != ""
        || parsed.password().is_some()
    {
        return Err("回调地址必须是无凭据的 HTTPS 地址".into());
    }
    Ok(parsed)
}

pub(super) fn authorize_url(
    config: &AppConfig,
    state: &str,
    code_verifier: Option<&str>,
) -> Result<String, String> {
    let redirect = required_https(&config.redirect_url)?;
    let mut url = match config.platform {
        Platform::Shopee => {
            let path = "/api/v2/shop/auth_partner";
            let timestamp = now();
            let id = config
                .app_id
                .parse::<u64>()
                .map_err(|_| "Shopee Partner ID 必须是数字")?;
            let sign = hmac_hex(&config.app_secret, &format!("{id}{path}{timestamp}"))?;
            let mut url = Url::parse(&format!("{SHOPEE}{path}")).unwrap();
            url.query_pairs_mut()
                .append_pair("partner_id", &id.to_string())
                .append_pair("timestamp", &timestamp.to_string())
                .append_pair("sign", &sign)
                .append_pair("redirect", redirect.as_str());
            url
        }
        Platform::Shein => {
            let mut url = Url::parse("https://openapi-sem.sheincorp.com/").unwrap();
            let encoded_redirect = STANDARD.encode(redirect.as_str());
            let fragment = format!(
                "/empower?appid={}&redirectUrl={}&state={}",
                url::form_urlencoded::byte_serialize(config.app_id.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(encoded_redirect.as_bytes())
                    .collect::<String>(),
                state
            );
            url.set_fragment(Some(&fragment));
            url
        }
        Platform::Tiktok => {
            let mut url =
                Url::parse(&config.authorize_url).map_err(|_| "TikTok Shop 授权链接无效")?;
            let host = url.host_str().unwrap_or_default();
            if url.scheme() != "https"
                || !matches!(
                    host,
                    "services.tiktokshop.com"
                        | "services.us.tiktokshop.com"
                        | "partner.tiktokshop.com"
                        | "partner.us.tiktokshop.com"
                )
                || url.path() != "/open/authorize"
                || !url
                    .query_pairs()
                    .any(|(key, value)| key == "service_id" && !value.is_empty())
            {
                return Err("请填写 TikTok Shop Partner Center 提供的卖家授权链接".into());
            }
            let pairs: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| key != "state")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
            url.set_query(None);
            url.query_pairs_mut().extend_pairs(pairs);
            url.query_pairs_mut().append_pair("state", state);
            url
        }
        Platform::Mercadolibre => {
            let host = match config.region.as_str() {
                "AR" => "auth.mercadolibre.com.ar",
                "BR" => "auth.mercadolivre.com.br",
                "MX" => "auth.mercadolibre.com.mx",
                "CL" => "auth.mercadolibre.cl",
                "CO" => "auth.mercadolibre.com.co",
                "UY" => "auth.mercadolibre.com.uy",
                _ => return Err("请选择美客多店铺所在国家或地区".into()),
            };
            let mut url = Url::parse(&format!("https://{host}/authorization")).unwrap();
            url.query_pairs_mut()
                .append_pair("response_type", "code")
                .append_pair("client_id", &config.app_id)
                .append_pair("redirect_uri", redirect.as_str())
                .append_pair("state", state);
            if let Some(verifier) = code_verifier {
                let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                url.query_pairs_mut()
                    .append_pair("code_challenge", &challenge)
                    .append_pair("code_challenge_method", "S256");
            }
            url
        }
    };
    url.set_username("").map_err(|_| "授权链接无效")?;
    Ok(url.to_string())
}

pub(super) fn callback_params(
    config: &AppConfig,
    callback: &str,
    expected_state: &str,
) -> Result<(String, Option<String>), String> {
    let expected = required_https(&config.redirect_url)?;
    let received = Url::parse(callback.trim()).map_err(|_| "请粘贴授权完成后的完整回调地址")?;
    if received.scheme() != expected.scheme()
        || received.host_str() != expected.host_str()
        || received.port_or_known_default() != expected.port_or_known_default()
        || received.username() != ""
        || received.password().is_some()
    {
        return Err("回调地址与本次授权设置不一致".into());
    }
    let shein_path_prefix = format!("{}/", expected.path().trim_end_matches('/'));
    if received.path() != expected.path()
        && (config.platform != Platform::Shein || !received.path().starts_with(&shein_path_prefix))
    {
        return Err("回调路径与本次授权设置不一致".into());
    }
    let params: std::collections::HashMap<_, _> = received.query_pairs().into_owned().collect();
    if let Some(error) = params.get("error") {
        return Err(format!(
            "平台拒绝授权：{}",
            error.chars().take(100).collect::<String>()
        ));
    }
    if config.platform != Platform::Shopee
        && params.get("state").map(String::as_str) != Some(expected_state)
    {
        return Err("授权 state 不匹配，请重新发起绑定".into());
    }
    let code = if config.platform == Platform::Shein {
        params.get("tempToken").cloned().or_else(|| {
            received
                .path()
                .strip_prefix(&shein_path_prefix)
                .map(str::to_owned)
        })
    } else {
        params.get("code").cloned()
    }
    .filter(|s| !s.is_empty() && s.len() <= 2048)
    .ok_or("回调中没有有效授权码")?;
    Ok((code, params.get("shop_id").cloned()))
}

fn shopee_sign(
    config: &AppConfig,
    path: &str,
    timestamp: i64,
    token: Option<&str>,
    shop_id: Option<&str>,
) -> Result<String, String> {
    let mut input = format!("{}{path}{timestamp}", config.app_id);
    if let Some(token) = token {
        input.push_str(token);
    }
    if let Some(shop_id) = shop_id {
        input.push_str(shop_id);
    }
    hmac_hex(&config.app_secret, &input)
}

async fn shopee_token(config: &AppConfig, path: &str, body: Value) -> Result<Value, String> {
    let timestamp = now();
    let sign = shopee_sign(config, path, timestamp, None, None)?;
    let url = format!("{SHOPEE}{path}");
    let data = response(
        client()?
            .post(url)
            .query(&[
                ("partner_id", config.app_id.as_str()),
                ("timestamp", &timestamp.to_string()),
                ("sign", &sign),
            ])
            .json(&body),
        "Shopee",
    )
    .await?;
    platform_ok(&data, "Shopee")?;
    Ok(data)
}

async fn shopee_info(
    credential: &Credential,
    shop_id: &str,
) -> Result<(String, Option<String>), String> {
    let path = "/api/v2/shop/get_shop_info";
    let timestamp = now();
    let sign = shopee_sign(
        &credential.config,
        path,
        timestamp,
        Some(&credential.access_token),
        Some(shop_id),
    )?;
    let data = response(
        client()?.get(format!("{SHOPEE}{path}")).query(&[
            ("partner_id", credential.config.app_id.as_str()),
            ("timestamp", &timestamp.to_string()),
            ("sign", &sign),
            ("access_token", &credential.access_token),
            ("shop_id", shop_id),
        ]),
        "Shopee",
    )
    .await?;
    platform_ok(&data, "Shopee")?;
    let info = &data["response"];
    Ok((
        string(info, "shop_name").unwrap_or(shop_id).to_owned(),
        string(info, "region").map(str::to_owned),
    ))
}

fn tiktok_sign(config: &AppConfig, path: &str, timestamp: i64) -> Result<String, String> {
    let input = format!(
        "{}{path}app_key{}timestamp{}{}",
        config.app_secret, config.app_id, timestamp, config.app_secret
    );
    hmac_hex(&config.app_secret, &input)
}

async fn tiktok_token(
    config: &AppConfig,
    path: &str,
    code_key: &str,
    code: &str,
    grant: &str,
) -> Result<Value, String> {
    let data = response(
        client()?.get(format!("{TIKTOK_AUTH}{path}")).query(&[
            ("app_key", config.app_id.as_str()),
            ("app_secret", config.app_secret.as_str()),
            (code_key, code),
            ("grant_type", grant),
        ]),
        "TikTok Shop",
    )
    .await?;
    platform_ok(&data, "TikTok Shop")?;
    let token = &data["data"];
    if !matches!(token["user_type"].as_i64(), Some(0 | 4 | 5)) {
        return Err("TikTok Shop 授权的不是卖家账号".into());
    }
    Ok(token.clone())
}

async fn tiktok_shops(
    credential: &Credential,
) -> Result<Vec<(String, String, Option<String>)>, String> {
    let path = "/authorization/202309/shops";
    let timestamp = now();
    let sign = tiktok_sign(&credential.config, path, timestamp)?;
    let data = response(
        client()?
            .get(format!("{TIKTOK_API}{path}"))
            .query(&[
                ("app_key", credential.config.app_id.as_str()),
                ("timestamp", &timestamp.to_string()),
                ("sign", &sign),
            ])
            .header("x-tts-access-token", &credential.access_token),
        "TikTok Shop",
    )
    .await?;
    platform_ok(&data, "TikTok Shop")?;
    let shops = data["data"]["shops"]
        .as_array()
        .ok_or("TikTok Shop 未返回店铺列表")?;
    Ok(shops
        .iter()
        .filter_map(|shop| {
            let id = identifier(shop, "id")?;
            let name = string(shop, "name").unwrap_or(&id).to_owned();
            let region = string(shop, "region").map(str::to_owned);
            Some((id, name, region))
        })
        .collect())
}

async fn meli_token(
    config: &AppConfig,
    grant: &str,
    code: &str,
    code_verifier: Option<&str>,
) -> Result<Value, String> {
    let mut form = vec![
        ("grant_type", grant.to_owned()),
        ("client_id", config.app_id.clone()),
        ("client_secret", config.app_secret.clone()),
    ];
    if grant == "authorization_code" {
        form.push(("code", code.to_owned()));
        form.push(("redirect_uri", config.redirect_url.clone()));
        if let Some(verifier) = code_verifier {
            form.push(("code_verifier", verifier.to_owned()));
        }
    } else {
        form.push(("refresh_token", code.to_owned()));
    }
    response(
        client()?
            .post(format!("{MELI_API}/oauth/token"))
            .form(&form),
        "Mercado Libre",
    )
    .await
}

async fn meli_info(access_token: &str) -> Result<(String, String, Option<String>), String> {
    let data = response(
        client()?
            .get(format!("{MELI_API}/users/me"))
            .bearer_auth(access_token),
        "Mercado Libre",
    )
    .await?;
    let id = identifier(&data, "id").ok_or("Mercado Libre 未返回用户 ID")?;
    let name = string(&data, "nickname").unwrap_or(&id).to_owned();
    let region = string(&data, "site_id").map(str::to_owned);
    Ok((id, name, region))
}

fn shein_sign(
    open_key: &str,
    secret: &str,
    path: &str,
    timestamp: i64,
    random: &str,
) -> Result<String, String> {
    let digest = hmac_hex(
        &format!("{secret}{random}"),
        &format!("{open_key}&{timestamp}&{path}"),
    )?;
    Ok(format!("{random}{}", STANDARD.encode(digest)))
}

fn shein_secret(encrypted: &str, app_secret: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encrypted)
        .map_err(|_| "SHEIN 商家密钥格式无效")?;
    let mut key = [0u8; 16];
    let app_bytes = app_secret.as_bytes();
    key[..app_bytes.len().min(16)].copy_from_slice(&app_bytes[..app_bytes.len().min(16)]);
    let iv = *b"space-station-de";
    let plaintext = cbc::Decryptor::<Aes128>::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&bytes)
        .map_err(|_| "SHEIN 商家密钥解密失败")?;
    String::from_utf8(plaintext).map_err(|_| "SHEIN 商家密钥不是有效文本".into())
}

async fn shein_call(
    path: &str,
    open_key: &str,
    secret: &str,
    body: Value,
    app_auth: bool,
) -> Result<Value, String> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let random: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(5)
        .collect();
    let signature = shein_sign(open_key, secret, path, timestamp, &random)?;
    let header = if app_auth {
        "x-lt-appid"
    } else {
        "x-lt-openKeyId"
    };
    let data = response(
        client()?
            .post(format!("{SHEIN_API}{path}"))
            .header(header, open_key)
            .header("x-lt-timestamp", timestamp.to_string())
            .header("x-lt-signature", signature)
            .header("Content-Type", "application/json;charset=UTF-8")
            .json(&body),
        "SHEIN",
    )
    .await?;
    platform_ok(&data, "SHEIN")?;
    Ok(data)
}

async fn shein_info(
    credential: &Credential,
) -> Result<(String, String, ShopStatus, Option<String>), String> {
    let data = shein_call(
        "/open-api/openapi-business-backend/query-store-info",
        &credential.open_key,
        &credential.seller_secret,
        json!({}),
        false,
    )
    .await?;
    let info = &data["info"]["storeInfo"];
    let id = identifier(info, "supplierId").ok_or("SHEIN 未返回商家 ID")?;
    let name = string(info, "storeName").unwrap_or(&id).to_owned();
    let status = if info["storeStatus"].as_i64() == Some(0) {
        ShopStatus::Disabled
    } else {
        ShopStatus::Connected
    };
    let region = string(info, "supplierBusinessMode").map(str::to_owned);
    Ok((id, name, status, region))
}

pub(super) async fn exchange(
    config: AppConfig,
    code: &str,
    shop_id: Option<&str>,
    expected_state: &str,
    code_verifier: Option<&str>,
) -> Result<Vec<RemoteShop>, String> {
    match config.platform {
        Platform::Shopee => {
            let shop_id = shop_id.ok_or("Shopee 回调缺少 shop_id")?;
            shop_id.parse::<u64>().map_err(|_| "Shopee shop_id 无效")?;
            let data = shopee_token(
                &config,
                "/api/v2/auth/token/get",
                json!({
                    "code": code, "shop_id": shop_id.parse::<u64>().unwrap(),
                    "partner_id": config.app_id.parse::<u64>().map_err(|_| "Partner ID 无效")?,
                }),
            )
            .await?;
            let credential = Credential {
                access_token: string(&data, "access_token")
                    .ok_or("Shopee 未返回 access token")?
                    .to_owned(),
                refresh_token: string(&data, "refresh_token")
                    .ok_or("Shopee 未返回 refresh token")?
                    .to_owned(),
                expires_at: now() + data["expire_in"].as_i64().unwrap_or(14_400),
                ..Credential::new(config)
            };
            let (name, region) = shopee_info(&credential, shop_id).await?;
            Ok(vec![RemoteShop {
                remote_id: shop_id.into(),
                name,
                region,
                status: ShopStatus::Connected,
                credential,
            }])
        }
        Platform::Tiktok => {
            let token = tiktok_token(
                &config,
                "/api/v2/token/get",
                "auth_code",
                code,
                "authorized_code",
            )
            .await?;
            let credential = Credential {
                access_token: string(&token, "access_token")
                    .ok_or("TikTok Shop 未返回 access token")?
                    .into(),
                refresh_token: string(&token, "refresh_token")
                    .ok_or("TikTok Shop 未返回 refresh token")?
                    .into(),
                expires_at: token["access_token_expire_in"]
                    .as_i64()
                    .unwrap_or(now() + 86_400),
                ..Credential::new(config)
            };
            let shops = tiktok_shops(&credential).await?;
            if shops.is_empty() {
                return Err("TikTok Shop 授权成功，但没有可绑定的店铺".into());
            }
            Ok(shops
                .into_iter()
                .map(|(remote_id, name, region)| RemoteShop {
                    remote_id,
                    name,
                    region,
                    status: ShopStatus::Connected,
                    credential: credential.clone(),
                })
                .collect())
        }
        Platform::Mercadolibre => {
            let data = meli_token(&config, "authorization_code", code, code_verifier).await?;
            let credential = Credential {
                access_token: string(&data, "access_token")
                    .ok_or("Mercado Libre 未返回 access token")?
                    .into(),
                refresh_token: string(&data, "refresh_token")
                    .ok_or("Mercado Libre 未返回 refresh token")?
                    .into(),
                expires_at: now() + data["expires_in"].as_i64().unwrap_or(21_600),
                ..Credential::new(config)
            };
            let (remote_id, name, region) = meli_info(&credential.access_token).await?;
            if identifier(&data, "user_id").is_some_and(|id| id != remote_id) {
                return Err("Mercado Libre 用户 ID 与授权令牌不一致".into());
            }
            Ok(vec![RemoteShop {
                remote_id,
                name,
                region,
                status: ShopStatus::Connected,
                credential,
            }])
        }
        Platform::Shein => {
            let data = shein_call(
                "/open-api/auth/get-by-token",
                &config.app_id,
                &config.app_secret,
                json!({"tempToken": code}),
                true,
            )
            .await?;
            let info = &data["info"];
            if string(info, "state") != Some(expected_state) {
                return Err("SHEIN 授权 state 不匹配".into());
            }
            if string(info, "appid") != Some(config.app_id.as_str()) {
                return Err("SHEIN 应用 ID 与授权记录不一致".into());
            }
            let encrypted = string(info, "secretKey").ok_or("SHEIN 未返回商家密钥")?;
            let credential = Credential {
                open_key: string(info, "openKeyId")
                    .ok_or("SHEIN 未返回 openKeyId")?
                    .into(),
                seller_secret: shein_secret(encrypted, &config.app_secret)?,
                ..Credential::new(config)
            };
            let (remote_id, name, status, region) = shein_info(&credential).await?;
            if identifier(info, "supplierId").as_deref() != Some(remote_id.as_str()) {
                return Err("SHEIN 商家 ID 与授权记录不一致".into());
            }
            Ok(vec![RemoteShop {
                remote_id,
                name,
                region,
                status,
                credential,
            }])
        }
    }
}

pub(super) async fn verify(
    remote_id: &str,
    credential: &mut Credential,
) -> Result<(String, Option<String>, ShopStatus), String> {
    let config = credential.config.clone();
    if credential.expires_at > 0 && credential.expires_at <= now() + 300 {
        match config.platform {
            Platform::Shopee => {
                let data = shopee_token(&config, "/api/v2/auth/access_token/get", json!({
                    "refresh_token": credential.refresh_token, "shop_id": remote_id.parse::<u64>().map_err(|_| "店铺 ID 无效")?,
                    "partner_id": config.app_id.parse::<u64>().map_err(|_| "Partner ID 无效")?,
                })).await?;
                credential.access_token = string(&data, "access_token")
                    .ok_or("Shopee 刷新未返回 access token")?
                    .into();
                credential.refresh_token = string(&data, "refresh_token")
                    .ok_or("Shopee 刷新未返回 refresh token")?
                    .into();
                credential.expires_at = now() + data["expire_in"].as_i64().unwrap_or(14_400);
            }
            Platform::Tiktok => {
                let data = tiktok_token(
                    &config,
                    "/api/v2/token/refresh",
                    "refresh_token",
                    &credential.refresh_token,
                    "refresh_token",
                )
                .await?;
                credential.access_token = string(&data, "access_token")
                    .ok_or("TikTok Shop 刷新未返回 access token")?
                    .into();
                credential.refresh_token = string(&data, "refresh_token")
                    .unwrap_or(&credential.refresh_token)
                    .to_owned();
                credential.expires_at = data["access_token_expire_in"]
                    .as_i64()
                    .unwrap_or(now() + 86_400);
            }
            Platform::Mercadolibre => {
                let data =
                    meli_token(&config, "refresh_token", &credential.refresh_token, None).await?;
                credential.access_token = string(&data, "access_token")
                    .ok_or("Mercado Libre 刷新未返回 access token")?
                    .into();
                credential.refresh_token = string(&data, "refresh_token")
                    .ok_or("Mercado Libre 刷新未返回 refresh token")?
                    .into();
                credential.expires_at = now() + data["expires_in"].as_i64().unwrap_or(21_600);
            }
            Platform::Shein => {}
        }
    }
    match config.platform {
        Platform::Shopee => {
            let (name, region) = shopee_info(credential, remote_id).await?;
            Ok((name, region, ShopStatus::Connected))
        }
        Platform::Tiktok => tiktok_shops(credential)
            .await?
            .into_iter()
            .find(|(id, _, _)| id == remote_id)
            .map(|(_, name, region)| (name, region, ShopStatus::Connected))
            .ok_or("TikTok Shop 授权列表已不包含这家店铺".into()),
        Platform::Mercadolibre => {
            let (id, name, region) = meli_info(&credential.access_token).await?;
            if id != remote_id {
                return Err("Mercado Libre 返回了另一位卖家".into());
            }
            Ok((name, region, ShopStatus::Connected))
        }
        Platform::Shein => {
            let (id, name, status, region) = shein_info(credential).await?;
            if id != remote_id {
                return Err("SHEIN 返回了另一家店铺".into());
            }
            Ok((name, region, status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shein_signature_has_random_prefix() {
        let sign = shein_sign("open", "secret", "/open-api/test", 1583398764000, "1aa34").unwrap();
        assert_eq!(sign, "1aa34NWI2NTVkODMwYzI3YWVkMjY4Y2YxNmRjMWY0MTNkYWVmNjAzODA3YTJkMjk1OWI3YzI3YmMxNzU5ZjczMWRkNw==");
    }
    #[test]
    fn callback_rejects_wrong_origin_and_state() {
        let config = AppConfig {
            platform: Platform::Mercadolibre,
            app_id: "1".into(),
            app_secret: "x".into(),
            redirect_url: "https://example.com/callback".into(),
            authorize_url: String::new(),
            region: "MX".into(),
            pkce: false,
        };
        assert!(callback_params(&config, "https://evil.com/callback?code=x&state=s", "s").is_err());
        assert!(callback_params(
            &config,
            "https://example.com/callback?code=x&state=wrong",
            "s"
        )
        .is_err());
        assert_eq!(
            callback_params(&config, "https://example.com/callback?code=x&state=s", "s")
                .unwrap()
                .0,
            "x"
        );
    }
    #[test]
    fn shein_callback_reads_temp_token_and_rejects_wrong_state() {
        let config = AppConfig {
            platform: Platform::Shein,
            app_id: "app".into(),
            app_secret: "secret".into(),
            redirect_url: "https://example.com/callback".into(),
            authorize_url: String::new(),
            region: String::new(),
            pkce: false,
        };
        assert_eq!(
            callback_params(
                &config,
                "https://example.com/callback?tempToken=t&state=s",
                "s"
            )
            .unwrap()
            .0,
            "t"
        );
        assert!(callback_params(
            &config,
            "https://example.com/callback?tempToken=t&state=other",
            "s"
        )
        .is_err());
        assert!(callback_params(
            &config,
            "https://example.com/elsewhere?tempToken=t&state=s",
            "s"
        )
        .is_err());
    }
    #[test]
    fn tiktok_authorize_url_requires_seller_service_link() {
        let mut config = AppConfig {
            platform: Platform::Tiktok,
            app_id: "app".into(),
            app_secret: "secret".into(),
            redirect_url: "https://example.com/callback".into(),
            authorize_url:
                "https://services.tiktokshop.com/open/authorize?service_id=123&state=old".into(),
            region: String::new(),
            pkce: false,
        };
        let url = Url::parse(&authorize_url(&config, "new", None).unwrap()).unwrap();
        assert_eq!(
            url.query_pairs().filter(|(key, _)| key == "state").count(),
            1
        );
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "state").unwrap().1,
            "new"
        );
        config.authorize_url = "https://services.tiktokshop.com/open/authorize?app_key=123".into();
        assert!(authorize_url(&config, "new", None).is_err());
    }
    #[test]
    fn meli_pkce_challenge_uses_s256() {
        let config = AppConfig {
            platform: Platform::Mercadolibre,
            app_id: "app".into(),
            app_secret: "secret".into(),
            redirect_url: "https://example.com/callback".into(),
            authorize_url: String::new(),
            region: "BR".into(),
            pkce: true,
        };
        let url =
            Url::parse(&authorize_url(&config, "state", Some("test-verifier")).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("auth.mercadolivre.com.br"));
        assert!(url.query_pairs().any(|(key, value)| key == "code_challenge"
            && value == "JBbiqONGWPaAmwXk_8bT6UnlPfrn65D32eZlJS-zGG0"));
        assert!(url
            .query_pairs()
            .any(|(key, value)| key == "code_challenge_method" && value == "S256"));
    }
}
