//! Per-platform dispatch. Each platform file exposes metrics, categories, attributes,
//! products, orders, listing push and status; see ADR 0011. This module owns dispatch.
use super::service::Runtime;
use super::shopee::{self, PushOutcome, ResolvedShop};
use super::types::{
    Category, CategoryAttribute, CommerceError, CommerceOrder, CommerceProduct, ListingDraft, ListingStatus, ListingTarget,
    MetricRange, OrderPage, ProductPage, ShopMetrics,
};
use super::{douyin, kuaishou, mercadolibre, pinduoduo, shein, taobao, tiktok, wechat};
use crate::workbench::shops::{self, Platform};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Credential snapshot for one connected shop. Refresh stays in `shops::shop_check`.
#[derive(Clone, Debug)]
pub struct ApiSession {
    pub shop_id: String,
    pub platform: Platform,
    pub remote_id: String,
    pub name: String,
    pub region: Option<String>,
    pub app_id: String,
    pub app_secret: String,
    pub access_token: String,
    pub refresh_token: String,
    pub open_key: String,
    pub seller_secret: String,
}

impl ApiSession {
    pub fn from_shop_session(session: &shops::ShopSession) -> Self {
        Self {
            shop_id: session.shop.id.clone(),
            platform: session.shop.platform,
            remote_id: session.shop.remote_id.clone(),
            name: session.shop.name.clone(),
            region: session.shop.region.clone(),
            app_id: session.partner_id.clone(),
            app_secret: session.partner_key.clone(),
            access_token: session.access_token.clone(),
            refresh_token: session.refresh_token.clone(),
            open_key: session.open_key.clone(),
            seller_secret: session.seller_secret.clone(),
        }
    }
}

pub async fn open_session(shop_id: &str) -> Result<ApiSession, CommerceError> {
    let session = shops::open_shop_session(shop_id).await.map_err(CommerceError::failed)?;
    Ok(ApiSession::from_shop_session(&session))
}

#[derive(Clone, Debug)]
pub struct MultipartFile {
    pub field: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum HttpBody {
    Empty,
    Json(Value),
    Form(Vec<(String, String)>),
    Multipart(MultipartFile),
}

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: HttpBody,
    pub mutating: bool,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum TransportFault {
    BeforeDispatch(String),
    AfterDispatch(String),
}

#[derive(Clone, Debug)]
pub enum ScriptStep {
    Json { status: u16, body: Value },
    Fault(TransportFault),
}

#[derive(Debug)]
pub struct ScriptHttp {
    queues: Mutex<HashMap<String, VecDeque<ScriptStep>>>,
    calls: Mutex<Vec<HttpRequest>>,
}

impl ScriptHttp {
    pub fn new() -> Self {
        Self { queues: Mutex::new(HashMap::new()), calls: Mutex::new(Vec::new()) }
    }

    pub fn push(&self, url: &str, step: ScriptStep) {
        self.queues.lock().expect("script queue").entry(url.to_string()).or_default().push_back(step);
    }

    pub fn calls(&self) -> Vec<HttpRequest> {
        self.calls.lock().expect("script calls").clone()
    }

    fn take(&self, url: &str) -> Result<ScriptStep, TransportFault> {
        let mut queues = self.queues.lock().expect("script queue");
        if let Some(step) = queues.get_mut(url).and_then(|queue| queue.pop_front()) {
            return Ok(step);
        }
        let path = url.split('?').next().and_then(|value| value.split("//").nth(1)).and_then(|value| value.find('/').map(|index| &value[index..]));
        if let Some(path) = path {
            if let Some(step) = queues.get_mut(path).and_then(|queue| queue.pop_front()) {
                return Ok(step);
            }
        }
        Err(TransportFault::BeforeDispatch(format!("测试脚本没有 {url} 的响应")))
    }
}

pub enum SharedTransport {
    Live(reqwest::Client),
    Script(Arc<ScriptHttp>),
}

impl SharedTransport {
    pub fn live() -> Result<Self, CommerceError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| CommerceError::internal(format!("无法创建 HTTP 客户端：{error}")))?;
        Ok(Self::Live(client))
    }

    pub fn script(script: Arc<ScriptHttp>) -> Self {
        Self::Script(script)
    }

    pub async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportFault> {
        match self {
            Self::Script(script) => {
                let url = request.url.clone();
                script.calls.lock().expect("script calls").push(request);
                match script.take(&url)? {
                    ScriptStep::Json { status, body } => Ok(HttpResponse { status, text: body.to_string(), body }),
                    ScriptStep::Fault(fault) => Err(fault),
                }
            }
            Self::Live(client) => live_send(client, request).await,
        }
    }
}

pub fn classify_fault(mutating: bool, fault: TransportFault) -> CommerceError {
    match fault {
        TransportFault::BeforeDispatch(message) => CommerceError::failed(message),
        TransportFault::AfterDispatch(message) if mutating => CommerceError::uncertain(message),
        TransportFault::AfterDispatch(message) => CommerceError::failed(message),
    }
}

pub fn classify_http(mutating: bool, status: u16, message: String) -> Result<(), CommerceError> {
    if status >= 500 {
        return Err(if mutating { CommerceError::uncertain(message) } else { CommerceError::failed(message) });
    }
    if status >= 400 {
        return Err(CommerceError::rejected(message));
    }
    Ok(())
}

pub fn not_wired(platform: Platform, capability: &str) -> CommerceError {
    CommerceError::unsupported(format!("{} 尚未接入{capability}", platform.as_str()))
}

pub fn require_listing(platform: Platform) -> Result<(), CommerceError> {
    if super::types::capabilities_for(platform).listing {
        Ok(())
    } else {
        Err(not_wired(platform, "上架"))
    }
}

fn session_of(shop: &ResolvedShop, shop_id: &str) -> ApiSession {
    ApiSession {
        shop_id: shop_id.to_string(),
        platform: shop.platform,
        remote_id: shop.remote_id.clone(),
        name: shop.name.clone(),
        region: shop.region.clone(),
        app_id: shop.partner_id.clone(),
        app_secret: shop.partner_key.clone(),
        access_token: shop.access_token.clone(),
        refresh_token: String::new(),
        open_key: shop.partner_id.clone(),
        seller_secret: shop.partner_key.clone(),
    }
}

pub async fn metrics(runtime: &Runtime, shop_id: &str, range: MetricRange) -> Result<ShopMetrics, CommerceError> {
    let shop = runtime.shops.resolve(shop_id).await?;
    let now = runtime.now;
    match shop.platform {
        Platform::Shopee => shopee::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
        Platform::Tiktok => tiktok::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
        Platform::Shein => shein::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
        Platform::Kuaishou => kuaishou::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
        Platform::Pinduoduo => pinduoduo::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
        Platform::Douyin => douyin::metrics(&runtime.http, &session_of(&shop, shop_id), range, now).await,
        Platform::Mercadolibre => mercadolibre::metrics(&runtime.http, &session_of(&shop, shop_id), range, now).await,
        Platform::Wechat => wechat::metrics(&runtime.http, &session_of(&shop, shop_id), range, now).await,
        Platform::Taobao => taobao::fetch_metrics(&runtime.transport, &shop, shop_id, range, now).await,
    }
}

pub async fn categories(
    runtime: &Runtime,
    shop_id: &str,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    let shop = runtime.shops.resolve(shop_id).await?;
    let now = runtime.now;
    match shop.platform {
        Platform::Shopee => shopee::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
        Platform::Tiktok => tiktok::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
        Platform::Shein => shein::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
        Platform::Kuaishou => kuaishou::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
        Platform::Pinduoduo => pinduoduo::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
        Platform::Douyin => douyin::categories(&runtime.http, &session_of(&shop, shop_id), parent_id, now).await,
        Platform::Mercadolibre => mercadolibre::categories(&runtime.http, &session_of(&shop, shop_id), parent_id, now).await,
        Platform::Wechat => wechat::categories(&runtime.http, &session_of(&shop, shop_id), parent_id, now).await,
        Platform::Taobao => taobao::fetch_categories(&runtime.transport, &shop, now, parent_id).await,
    }
}

pub async fn attributes(
    runtime: &Runtime,
    shop_id: &str,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    let shop = runtime.shops.resolve(shop_id).await?;
    let now = runtime.now;
    match shop.platform {
        Platform::Shopee => shopee::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
        Platform::Tiktok => tiktok::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
        Platform::Shein => shein::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
        Platform::Kuaishou => kuaishou::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
        Platform::Pinduoduo => pinduoduo::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
        Platform::Douyin => douyin::attributes(&runtime.http, &session_of(&shop, shop_id), category_id, now).await,
        Platform::Mercadolibre => mercadolibre::attributes(&runtime.http, &session_of(&shop, shop_id), category_id, now).await,
        Platform::Wechat => wechat::attributes(&runtime.http, &session_of(&shop, shop_id), category_id, now).await,
        Platform::Taobao => taobao::fetch_attributes(&runtime.transport, &shop, now, category_id).await,
    }
}

pub async fn products(runtime: &Runtime, shop_id: &str, cursor: Option<&str>) -> Result<ProductPage, CommerceError> {
    let shop = runtime.shops.resolve(shop_id).await?;
    let now = runtime.now;
    match shop.platform {
        Platform::Shopee => shopee::fetch_products(&runtime.transport, &shop, shop_id, cursor, now).await,
        Platform::Tiktok => tiktok::fetch_products(&runtime.transport, &shop, shop_id, cursor, now).await,
        Platform::Shein => shein_products(&runtime.transport, &shop, shop_id, cursor, now).await,
        Platform::Kuaishou => kuaishou_products(&runtime.transport, &shop, shop_id, cursor, now).await,
        Platform::Pinduoduo => pdd_products(&runtime.transport, &shop, shop_id, cursor, now).await,
        Platform::Douyin => douyin::products(&runtime.http, &session_of(&shop, shop_id), cursor, now).await,
        Platform::Mercadolibre => mercadolibre::products(&runtime.http, &session_of(&shop, shop_id), cursor, now).await,
        Platform::Wechat => wechat::products(&runtime.http, &session_of(&shop, shop_id), cursor, now).await,
        Platform::Taobao => taobao::fetch_products(&runtime.transport, &shop, shop_id, cursor, now).await,
    }
}

pub async fn orders(
    runtime: &Runtime,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
) -> Result<OrderPage, CommerceError> {
    let shop = runtime.shops.resolve(shop_id).await?;
    let now = runtime.now;
    match shop.platform {
        Platform::Shopee => shopee::fetch_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
        Platform::Tiktok => tiktok::fetch_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
        Platform::Shein => shein_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
        Platform::Kuaishou => kuaishou_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
        Platform::Pinduoduo => pdd_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
        Platform::Douyin => douyin::orders(&runtime.http, &session_of(&shop, shop_id), range, cursor, now).await,
        Platform::Mercadolibre => mercadolibre::orders(&runtime.http, &session_of(&shop, shop_id), range, cursor, now).await,
        Platform::Wechat => wechat::orders(&runtime.http, &session_of(&shop, shop_id), range, cursor, now).await,
        Platform::Taobao => taobao::fetch_orders(&runtime.transport, &shop, shop_id, range, cursor, now).await,
    }
}

pub async fn push_listing(
    runtime: &Runtime,
    shop: &ResolvedShop,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    remote: Option<&str>,
) -> PushOutcome {
    let now = runtime.now;
    let session = session_of(shop, &target.shop_id);
    match shop.platform {
        Platform::Shopee => shopee::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
        Platform::Tiktok => tiktok::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
        Platform::Shein => shein::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
        Platform::Kuaishou => kuaishou::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
        Platform::Pinduoduo => pinduoduo::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
        Platform::Douyin => douyin::push_listing(&runtime.http, &session, draft, target, images, now, remote).await,
        Platform::Mercadolibre => mercadolibre::push_listing(&runtime.http, &session, draft, target, images, now, remote).await,
        Platform::Wechat => wechat::push_listing(&runtime.http, &session, draft, target, images, now, remote).await,
        Platform::Taobao => taobao::push_listing(&runtime.transport, shop, draft, target, images, now, remote).await,
    }
}

pub async fn listing_status(
    runtime: &Runtime,
    shop: &ResolvedShop,
    shop_id: &str,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    let now = runtime.now;
    let session = session_of(shop, shop_id);
    match shop.platform {
        Platform::Shopee => shopee::refresh_remote(&runtime.transport, shop, now, remote_id).await,
        Platform::Tiktok => tiktok::refresh_remote(&runtime.transport, shop, now, remote_id).await,
        Platform::Shein => shein::refresh_remote(&runtime.transport, shop, now, remote_id).await,
        Platform::Kuaishou => kuaishou::refresh_remote(&runtime.transport, shop, now, remote_id).await,
        Platform::Pinduoduo => pinduoduo::refresh_remote(&runtime.transport, shop, now, remote_id).await,
        Platform::Douyin => douyin::listing_status(&runtime.http, &session, remote_id, now).await,
        Platform::Mercadolibre => mercadolibre::listing_status(&runtime.http, &session, remote_id, now).await,
        Platform::Wechat => wechat::listing_status(&runtime.http, &session, remote_id, now).await,
        Platform::Taobao => taobao::refresh_remote(&runtime.transport, shop, now, remote_id).await,
    }
}

fn page_index(cursor: Option<&str>) -> Result<u32, CommerceError> {
    match cursor.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(1),
        Some(value) => value.parse().map_err(|_| CommerceError::invalid("分页游标无效")),
    }
}

fn counted_next(page: u32, page_size: u32, count: usize, total: Option<i64>) -> Option<String> {
    let covered = i64::from(page) * i64::from(page_size);
    let more = match total {
        Some(total) => covered < total && count > 0,
        None => count > 0 && count as u32 >= page_size,
    };
    more.then(|| (page + 1).to_string())
}

async fn shein_products(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    let page_no = page_index(cursor)?;
    let page = shein::fetch_products(transport, shop, now, page_no).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceProduct {
            id: item.remote_id,
            title: item.title.unwrap_or_default(),
            status: item.status.as_str().to_string(),
            price: None,
            currency: None,
            stock: None,
            sku: item.skus.into_iter().next(),
        })
        .collect::<Vec<_>>();
    let next_cursor = counted_next(page.page, page.page_size, items.len(), page.total);
    Ok(ProductPage { shop_id: shop_id.to_string(), items, next_cursor })
}

async fn shein_orders(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    let page_no = page_index(cursor)?;
    let page = shein::fetch_orders(transport, shop, now, range, page_no).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceOrder {
            id: item.remote_id,
            status: item.status,
            amount: item.amount,
            currency: item.currency,
            buyer: None,
            created_at: item.created_at,
            lines: Vec::new(),
        })
        .collect::<Vec<_>>();
    let next_cursor = counted_next(page.page, page.page_size, items.len(), page.total);
    Ok(OrderPage { shop_id: shop_id.to_string(), range, items, next_cursor })
}

async fn kuaishou_products(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    let page = kuaishou::fetch_products(transport, shop, now, cursor).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceProduct {
            id: item.remote_id,
            title: item.title,
            status: item.status.as_str().to_string(),
            price: item.price,
            currency: item.currency,
            stock: item.stock,
            sku: None,
        })
        .collect();
    Ok(ProductPage { shop_id: shop_id.to_string(), items, next_cursor: page.cursor })
}

async fn kuaishou_orders(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    let page = kuaishou::fetch_orders(transport, shop, now, range, cursor).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceOrder {
            id: item.remote_id,
            status: item.status,
            amount: item.amount,
            currency: item.currency,
            buyer: item.buyer,
            created_at: if item.created_at.is_empty() { None } else { Some(item.created_at) },
            lines: Vec::new(),
        })
        .collect();
    Ok(OrderPage { shop_id: shop_id.to_string(), range, items, next_cursor: page.cursor })
}

async fn pdd_products(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    let page_no = i64::from(page_index(cursor)?);
    let page = pinduoduo::fetch_products(transport, shop, now, page_no).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceProduct {
            id: item.id,
            title: item.title,
            status: item.status,
            price: item.price,
            currency: item.price.map(|_| "CNY".to_string()),
            stock: item.stock,
            sku: None,
        })
        .collect::<Vec<_>>();
    let next_cursor = page.has_next.then(|| (page.page + 1).to_string());
    Ok(ProductPage { shop_id: shop_id.to_string(), items, next_cursor })
}

async fn pdd_orders(
    transport: &super::transport::Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    let (start, end) = pdd_window(range, now, pdd_offset(shop.region.as_deref()));
    let chunks = pdd_chunks(start, end);
    let (index, page_no) = pdd_order_cursor(cursor, chunks.len())?;
    if chunks.is_empty() || index >= chunks.len() {
        return Ok(OrderPage { shop_id: shop_id.to_string(), range, items: Vec::new(), next_cursor: None });
    }
    let (chunk_start, chunk_end) = chunks[index];
    let page = pinduoduo::fetch_orders(transport, shop, now, chunk_start, chunk_end, page_no).await?;
    let items = page
        .items
        .into_iter()
        .map(|item| CommerceOrder {
            id: item.id,
            status: item.status,
            amount: item.amount,
            currency: item.currency,
            buyer: item.buyer,
            created_at: item.created_at.and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0)).map(|time| time.to_rfc3339()),
            lines: Vec::new(),
        })
        .collect();
    let next_cursor = if page.has_next {
        Some(format!("{index}:{}", page.page + 1))
    } else if index + 1 < chunks.len() {
        Some(format!("{}:1", index + 1))
    } else {
        None
    };
    Ok(OrderPage { shop_id: shop_id.to_string(), range, items, next_cursor })
}

fn pdd_offset(region: Option<&str>) -> i64 {
    match region.unwrap_or("CN").trim().to_ascii_uppercase().as_str() {
        "TH" | "VN" | "ID" | "KH" => 7 * 3600,
        "JP" | "KR" => 9 * 3600,
        _ => 8 * 3600,
    }
}

fn pdd_window(range: MetricRange, now: i64, offset: i64) -> (i64, i64) {
    let local = now + offset;
    let day = local - local.rem_euclid(86_400);
    let (start_local, end_local) = match range {
        MetricRange::Today => (day, local),
        MetricRange::Yesterday => (day - 86_400, day - 1),
        MetricRange::Last7 => (day - 6 * 86_400, local),
        MetricRange::Last30 => (day - 29 * 86_400, local),
    };
    (start_local - offset, end_local - offset)
}

fn pdd_chunks(start: i64, end: i64) -> Vec<(i64, i64)> {
    if end < start {
        return vec![(start, start)];
    }
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let chunk_end = (cursor + 86_399).min(end);
        out.push((cursor, chunk_end));
        if chunk_end >= end {
            break;
        }
        cursor = chunk_end + 1;
    }
    out
}

fn pdd_order_cursor(cursor: Option<&str>, windows: usize) -> Result<(usize, i64), CommerceError> {
    let Some(cursor) = cursor.filter(|value| !value.is_empty()) else {
        return Ok((0, 1));
    };
    let (index, page) = cursor.split_once(':').unwrap_or(("0", cursor));
    let index = index.parse::<usize>().map_err(|_| CommerceError::invalid("订单分页游标无效"))?;
    let page = page.parse::<i64>().map_err(|_| CommerceError::invalid("订单分页游标无效"))?;
    if (windows > 0 && index >= windows) || page < 1 {
        return Err(CommerceError::invalid("订单分页游标无效"));
    }
    Ok((index, page))
}

async fn live_send(client: &reqwest::Client, request: HttpRequest) -> Result<HttpResponse, TransportFault> {
    let mut builder = match request.method {
        "POST" => client.post(&request.url),
        "PUT" => client.put(&request.url),
        "DELETE" => client.delete(&request.url),
        _ => client.get(&request.url),
    };
    builder = builder.query(&request.query);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    match request.body {
        HttpBody::Empty => {}
        HttpBody::Json(body) => builder = builder.json(&body),
        HttpBody::Form(form) => {
            let encoded = form
                .iter()
                .map(|(key, value)| format!("{}={}", form_encode(key), form_encode(value)))
                .collect::<Vec<_>>()
                .join("&");
            builder = builder.header("content-type", "application/x-www-form-urlencoded").body(encoded);
        }
        HttpBody::Multipart(file) => {
            let part = reqwest::multipart::Part::bytes(file.bytes).file_name(file.file_name);
            builder = builder.multipart(reqwest::multipart::Form::new().part(file.field, part));
        }
    }
    let response = match builder.send().await {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            return Err(if error.is_connect() {
                TransportFault::BeforeDispatch(message)
            } else {
                TransportFault::AfterDispatch(message)
            });
        }
    };
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|error| TransportFault::AfterDispatch(error.to_string()))?;
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": "http", "message": text.chars().take(300).collect::<String>() }));
    Ok(HttpResponse { status, body, text })
}

fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
