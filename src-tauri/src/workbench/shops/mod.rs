//! Seller-owned marketplace connections. Platform protocols stay in `providers`;
//! this module owns the binding lifecycle and the only persisted shop snapshot.
mod providers;
mod store;

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Shopee,
    Shein,
    Tiktok,
    Mercadolibre,
}
impl Platform {
    fn as_str(self) -> &'static str {
        match self {
            Self::Shopee => "shopee",
            Self::Shein => "shein",
            Self::Tiktok => "tiktok",
            Self::Mercadolibre => "mercadolibre",
        }
    }
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "shopee" => Ok(Self::Shopee),
            "shein" => Ok(Self::Shein),
            "tiktok" => Ok(Self::Tiktok),
            "mercadolibre" => Ok(Self::Mercadolibre),
            _ => Err("不支持的店铺平台".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopStatus {
    Connected,
    Disabled,
    NeedsReauthorization,
    Error,
}
impl ShopStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disabled => "disabled",
            Self::NeedsReauthorization => "needs_reauthorization",
            Self::Error => "error",
        }
    }
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "connected" => Ok(Self::Connected),
            "disabled" => Ok(Self::Disabled),
            "needs_reauthorization" => Ok(Self::NeedsReauthorization),
            "error" => Ok(Self::Error),
            _ => Err("店铺状态无效".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shop {
    pub id: String,
    pub platform: Platform,
    pub remote_id: String,
    pub name: String,
    pub region: Option<String>,
    pub bound_at: String,
    pub checked_at: String,
    pub status: ShopStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    platform: Platform,
    app_id: String,
    app_secret: String,
    redirect_url: String,
    #[serde(default)]
    authorize_url: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    pkce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credential {
    config: AppConfig,
    #[serde(default)]
    authorization_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    open_key: String,
    seller_secret: String,
}
impl Credential {
    fn new(config: AppConfig) -> Self {
        Self {
            config,
            authorization_id: uuid::Uuid::new_v4().to_string(),
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            open_key: String::new(),
            seller_secret: String::new(),
        }
    }
}

struct RemoteShop {
    remote_id: String,
    name: String,
    region: Option<String>,
    status: ShopStatus,
    credential: Credential,
}

#[derive(Clone)]
struct Pending {
    config: AppConfig,
    state: String,
    code_verifier: Option<String>,
    created_at: i64,
}
static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}
static LIFECYCLE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
fn lifecycle_lock() -> &'static tokio::sync::Mutex<()> {
    LIFECYCLE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn credential_entry(id: &str) -> Result<keyring::Entry, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "店铺凭据引用无效")?;
    keyring::Entry::new("Dsivio.WorkbenchShop", id).map_err(|_| "系统凭据库不可用".into())
}
fn save_credential(id: &str, value: &Credential) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|_| "店铺凭据编码失败")?;
    credential_entry(id)?
        .set_password(&encoded)
        .map_err(|_| "店铺凭据保存到系统凭据库失败".into())
}
fn load_credential(id: &str) -> Result<Credential, String> {
    let encoded = credential_entry(id)?
        .get_password()
        .map_err(|_| "店铺凭据已丢失，请重新授权".to_string())?;
    serde_json::from_str(&encoded).map_err(|_| "店铺凭据损坏，请重新授权".into())
}
fn delete_credential(id: &str) -> Result<(), String> {
    match credential_entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法清除系统凭据库中的店铺凭据".into()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginResult {
    request_id: String,
    url: String,
}

#[tauri::command]
pub fn shop_begin(mut config: AppConfig) -> Result<BeginResult, String> {
    config.app_id = config.app_id.trim().to_owned();
    config.app_secret = config.app_secret.trim().to_owned();
    config.redirect_url = config.redirect_url.trim().to_owned();
    if config.app_id.is_empty()
        || config.app_secret.is_empty()
        || config.app_id.len() > 256
        || config.app_secret.len() > 2048
    {
        return Err("请填写有效的平台应用 ID 和密钥".into());
    }
    if config.redirect_url.len() > 2048 {
        return Err("回调地址过长".into());
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let state = uuid::Uuid::new_v4().simple().to_string();
    let code_verifier = (config.platform == Platform::Mercadolibre && config.pkce).then(|| {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    });
    let url = providers::authorize_url(&config, &state, code_verifier.as_deref())?;
    let mut entries = pending().lock().map_err(|_| "授权状态不可用")?;
    let now = chrono::Utc::now().timestamp();
    entries.retain(|_, p| now - p.created_at < 3600);
    if entries.len() >= 16 {
        return Err("待完成授权过多，请稍后重试".into());
    }
    entries.insert(
        request_id.clone(),
        Pending {
            config,
            state,
            code_verifier,
            created_at: now,
        },
    );
    Ok(BeginResult { request_id, url })
}

#[tauri::command]
pub async fn shop_complete(request_id: String, callback_url: String) -> Result<Vec<Shop>, String> {
    if callback_url.len() > 8192 {
        return Err("回调地址过长".into());
    }
    let p = pending()
        .lock()
        .map_err(|_| "授权状态不可用")?
        .get(&request_id)
        .cloned()
        .ok_or("本次授权已过期，请重新发起绑定")?;
    if chrono::Utc::now().timestamp() - p.created_at > 3600 {
        return Err("本次授权已过期，请重新发起绑定".into());
    }
    let (code, shop_id) = providers::callback_params(&p.config, &callback_url, &p.state)?;
    let remote = providers::exchange(
        p.config,
        &code,
        shop_id.as_deref(),
        &p.state,
        p.code_verifier.as_deref(),
    )
    .await?;
    let _guard = lifecycle_lock().lock().await;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let db = Store::open()?;
        let mut saved = Vec::new();
        for item in remote {
            let existing = db.existing_id(item.credential.config.platform, &item.remote_id)?;
            let (id, previous) = match existing {
                Some(id) => {
                    let shop = db.get(&id)?;
                    (id, Some(shop))
                }
                None => (uuid::Uuid::new_v4().to_string(), None),
            };
            let previous_credential = if previous.is_some() {
                load_credential(&id).ok()
            } else {
                None
            };
            save_credential(&id, &item.credential)?;
            let now = chrono::Utc::now().to_rfc3339();
            let shop = Shop {
                id: id.clone(),
                platform: item.credential.config.platform,
                remote_id: item.remote_id,
                name: item.name,
                region: item.region,
                bound_at: previous
                    .as_ref()
                    .map(|s| s.bound_at.clone())
                    .unwrap_or_else(|| now.clone()),
                checked_at: now,
                status: item.status,
                detail: None,
            };
            if let Err(error) = db.put(&shop) {
                if let Some(credential) = previous_credential {
                    let _ = save_credential(&id, &credential);
                } else {
                    let _ = delete_credential(&id);
                }
                return Err(error);
            }
            saved.push(shop);
        }
        Ok::<_, String>(saved)
    })
    .await
    .map_err(|_| "店铺保存任务失败")??;
    pending()
        .lock()
        .map_err(|_| "授权状态不可用")?
        .remove(&request_id);
    Ok(result)
}

#[tauri::command]
pub async fn shop_list() -> Result<Vec<Shop>, String> {
    tauri::async_runtime::spawn_blocking(|| Store::open()?.list())
        .await
        .map_err(|_| "店铺读取任务失败".to_string())?
}

#[tauri::command]
pub async fn shop_check(id: String) -> Result<Shop, String> {
    let _guard = lifecycle_lock().lock().await;
    let (mut shop, credential) = tauri::async_runtime::spawn_blocking({
        let id = id.clone();
        move || {
            let db = Store::open()?;
            let mut shop = db.get(&id)?;
            match load_credential(&id) {
                Ok(credential) => Ok::<_, String>((shop, Some(credential))),
                Err(error) => {
                    shop.status = ShopStatus::NeedsReauthorization;
                    shop.detail = Some(error);
                    shop.checked_at = chrono::Utc::now().to_rfc3339();
                    db.put(&shop)?;
                    Ok((shop, None))
                }
            }
        }
    })
    .await
    .map_err(|_| "店铺读取任务失败")??;
    let Some(mut credential) = credential else {
        return Ok(shop);
    };
    let result = providers::verify(&shop.remote_id, &mut credential).await;
    tauri::async_runtime::spawn_blocking(move || {
        // Refresh tokens can rotate before the follow-up identity request fails.
        // Persist the new token even when verification returns an error.
        save_credential(&shop.id, &credential)?;
        if shop.platform == Platform::Tiktok && !credential.authorization_id.is_empty() {
            for sibling in Store::open()?
                .list()?
                .into_iter()
                .filter(|s| s.id != shop.id && s.platform == Platform::Tiktok)
            {
                if let Ok(old) = load_credential(&sibling.id) {
                    if old.authorization_id == credential.authorization_id {
                        save_credential(&sibling.id, &credential)?;
                    }
                }
            }
        }
        match result {
            Ok((name, region, status)) => {
                shop.name = name;
                shop.region = region;
                shop.status = status;
                shop.detail = None;
                shop.checked_at = chrono::Utc::now().to_rfc3339();
            }
            Err(error) => {
                shop.status =
                    if error.contains("token") || error.contains("授权") || error.contains("401")
                    {
                        ShopStatus::NeedsReauthorization
                    } else {
                        ShopStatus::Error
                    };
                shop.detail = Some(error);
                shop.checked_at = chrono::Utc::now().to_rfc3339();
            }
        }
        Store::open()?.put(&shop)?;
        Ok::<_, String>(shop)
    })
    .await
    .map_err(|_| "店铺状态保存任务失败")?
}

#[tauri::command]
pub async fn shop_unbind(id: String) -> Result<(), String> {
    let _guard = lifecycle_lock().lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let db = Store::open()?;
        db.get(&id)?;
        let credential = load_credential(&id).ok();
        delete_credential(&id)?;
        if let Err(error) = db.delete(&id) {
            if let Some(credential) = credential {
                let _ = save_credential(&id, &credential);
            }
            return Err(error);
        }
        Ok(())
    })
    .await
    .map_err(|_| "店铺解绑任务失败")?
}
