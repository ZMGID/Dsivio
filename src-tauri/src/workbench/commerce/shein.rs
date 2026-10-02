//! SHEIN Open Platform adapter for 自运营 / 半托管 shops.
//!
//! Signing matches `shops::providers::shein_sign`: HMAC-SHA256 hex of
//! `{openKeyId}&{timestamp_ms}&{path}` keyed by `{secretKey}{random5}`, then
//! `x-lt-signature` is `{random5}{base64(hex)}`. Headers are the ones documented on
//! each API page (`x-lt-openKeyId`, `x-lt-timestamp`, `x-lt-signature`).
//!
//! Shop session mapping (CommerceCore / `open_shop_session`, not a second token store):
//! `partner_id` is `openKeyId`, `partner_key` is the already-decrypted `seller_secret`.
//! `region` is `supplierBusinessMode`, not a timezone. Order and return times are UTC+8.
//!
//! Docs (portal pages, retrieved 2026-10-02):
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001594> `POST /open-api/goods/query-category-tree`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001927> `POST /open-api/goods/query-attribute-template`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3002044> `POST /open-api/goods/query-publish-fill-in-standard`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001249> `POST /open-api/goods/query-site-list`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001900> `POST /open-api/goods/query-brand-list`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3002013> `GET /open-api/msc/warehouse/list`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001359> `POST /open-api/goods/upload-pic`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3002018> `POST /open-api/goods/product/publishOrEdit`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001368> `POST /open-api/goods/query-document-state`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001938> `POST /open-api/goods/searchProduct`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001921> `POST /open-api/order/order-list`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001915> `POST /open-api/order/order-detail`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001281> `POST /open-api/return-order/list`
//! - <https://open.sheincorp.com/zh/documents/apidoc/detail/3001282> `POST /open-api/return-order/details`
//!
//! Not used: `POST /open-api/goods/query-sku-sales` (doc 3001305) is SKU unit counts for
//! 全托管 / 自营 / 半托管 / POP and requires sku codes. It is not shop GMV.
//! Buyer ids are not on order-list or order-detail. `export-address` with `handleType=2`
//! changes order status, so metrics do not call it.

use super::transport::{Outbound, PushOutcome, ResolvedShop, Transport, TransportFault};
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceError, ListingAttribute,
    ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey, MetricRange, ShopMetrics,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

const HOST: &str = "https://openapi.sheincorp.com";
const BEIJING_OFFSET: i64 = 8 * 3600;
const ORDER_PAGE: i64 = 30;
const PRODUCT_PAGE: i64 = 10;
const MAX_PAGES: usize = 20;
const MAX_CHUNK_SECS: i64 = 48 * 3600 - 1;

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteProduct {
    pub remote_id: String,
    pub title: Option<String>,
    pub status: ListingStatus,
    pub reason: Option<String>,
    pub skus: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductPage {
    pub items: Vec<RemoteProduct>,
    pub page: u32,
    pub page_size: u32,
    pub total: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteOrder {
    pub remote_id: String,
    pub status: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderPage {
    pub items: Vec<RemoteOrder>,
    pub page: u32,
    pub page_size: u32,
    pub total: Option<i64>,
}

#[derive(Debug)]
enum CallFail {
    Auth(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
}

impl CallFail {
    fn text(&self) -> &str {
        match self {
            Self::Auth(message) | Self::Rejected(message) | Self::Failed(message) | Self::Uncertain(message) => message,
        }
    }
}

struct RawCategory {
    id: String,
    parent_id: String,
    name: String,
    leaf: bool,
    product_type_id: i64,
}

struct RawAttribute {
    id: i64,
    name: String,
    required: bool,
    input: AttributeInput,
    options: Vec<AttributeOption>,
    kind: i64,
    label: i64,
    mode: i64,
    dimension: i64,
    status: i64,
}

struct AttributeTemplate {
    main_attribute_status: i64,
    attributes: Vec<RawAttribute>,
}

struct FillStandard {
    default_language: String,
    supply_currency: Option<String>,
    brand_required: bool,
    brand_show: bool,
    supplier_code_on_spu: bool,
}

#[derive(Clone)]
struct SiteRow {
    main_site: String,
    sub_site: String,
    currency: String,
}

struct PreparedListing {
    body: Value,
    images: Vec<(String, Vec<u8>, i64)>,
}

/// Same construction as `shops::providers::shein_sign` (test vector `1aa34` + base64 hex).
pub(crate) fn sign_shein(
    open_key: &str,
    secret: &str,
    path: &str,
    timestamp_ms: i64,
    random: &str,
) -> Result<String, String> {
    crate::workbench::shops::sign_shein(open_key, secret, path, timestamp_ms, random)
}

fn random_key() -> String {
    static TICK: AtomicU64 = AtomicU64::new(1);
    let mut n = TICK.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..5)
        .map(|_| {
            n = n.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ALPHABET[(n >> 33) as usize % ALPHABET.len()] as char
        })
        .collect()
}

pub fn metric_window(range: MetricRange, now: i64) -> (i64, i64) {
    let local = now + BEIJING_OFFSET;
    let day = local - local.rem_euclid(86_400);
    let (start_local, end_local) = match range {
        MetricRange::Today => (day, local),
        MetricRange::Yesterday => (day - 86_400, day - 1),
        MetricRange::Last7 => (day - 6 * 86_400, local),
        MetricRange::Last30 => (day - 29 * 86_400, local),
    };
    (start_local - BEIJING_OFFSET, end_local - BEIJING_OFFSET)
}

fn time_chunks(start: i64, end: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut cursor = start;
    if end < start {
        return vec![(start, start)];
    }
    while cursor <= end {
        let chunk_end = (cursor + MAX_CHUNK_SECS - 1).min(end);
        out.push((cursor, chunk_end));
        if chunk_end >= end {
            break;
        }
        cursor = chunk_end + 1;
    }
    out
}

fn beijing_text(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix + BEIJING_OFFSET, 0)
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn ensure_shein(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform != Platform::Shein {
        return Err(CommerceError::unsupported("该平台尚未接入"));
    }
    if shop.partner_id.trim().is_empty() || shop.partner_key.trim().is_empty() {
        return Err(CommerceError::invalid(
            "SHEIN 会话缺少 openKeyId 或已解密的商家密钥（partner_id / partner_key）",
        ));
    }
    Ok(())
}

fn ensure_call(shop: &ResolvedShop) -> Result<(), CallFail> {
    ensure_shein(shop).map_err(|error| match error {
        CommerceError::Unsupported(message) => CallFail::Rejected(message),
        other => CallFail::Rejected(other.message().to_string()),
    })
}

pub async fn fetch_metrics(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    ensure_shein(shop)?;
    let (start, end) = metric_window(range, now);
    let mut heads = Vec::new();
    let mut order_error = None;
    let mut calls = 0usize;
    for (chunk_start, chunk_end) in time_chunks(start, end) {
        let mut page = 1i64;
        loop {
            if calls >= MAX_PAGES {
                order_error = Some("订单超过单次汇总上限，未用部分结果冒充完整指标".to_string());
                heads.clear();
                break;
            }
            calls += 1;
            match list_order_page(transport, shop, now, chunk_start, chunk_end, page).await {
                Ok((batch, total)) => {
                    let length = batch.len() as i64;
                    heads.extend(batch);
                    if length < ORDER_PAGE || page * ORDER_PAGE >= total.unwrap_or(i64::MAX) {
                        break;
                    }
                    page += 1;
                }
                Err(CallFail::Auth(message) | CallFail::Rejected(message)) => {
                    return Err(CommerceError::rejected(message));
                }
                Err(other) => {
                    order_error = Some(other.text().to_string());
                    heads.clear();
                    break;
                }
            }
        }
        if order_error.is_some() {
            break;
        }
    }
    let detailed = if order_error.is_none() {
        match attach_order_amounts(transport, shop, now, heads).await {
            Ok(rows) => Ok(rows),
            Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
            Err(other) => {
                order_error = Some(other.text().to_string());
                Err(())
            }
        }
    } else {
        Err(())
    };
    let returns = match list_returns(transport, shop, now, start, end).await {
        Ok(items) => Ok(items),
        Err(CallFail::Auth(message)) if order_error.is_none() => return Err(CommerceError::rejected(message)),
        Err(_) => Err(()),
    };
    let live = match live_product_count(transport, shop, now).await {
        Ok(count) => Some(count),
        Err(CallFail::Auth(message)) if order_error.is_none() => return Err(CommerceError::rejected(message)),
        Err(_) => None,
    };
    Ok(fold_metrics(shop_id, range, now, detailed, returns, live, order_error))
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: Result<Vec<OrderView>, ()>,
    returns: Result<Vec<ReturnView>, ()>,
    live: Option<f64>,
    order_error: Option<String>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    let mut metrics = super::types::zero_metrics(shop_id.to_string(), range, fetched_at);
    metrics.values.clear();
    let mut unsupported = Vec::new();
    if let Some(message) = order_error {
        unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
        metrics.error = Some(message);
    } else if let Ok(orders) = orders {
        let counted: Vec<&OrderView> = orders.iter().filter(|order| counts_order(&order.status)).collect();
        metrics.values.insert(MetricKey::Orders, counted.len() as f64);
        metrics.values.insert(
            MetricKey::PendingShipment,
            counted.iter().filter(|order| pending_shipment(&order.status)).count() as f64,
        );
        unsupported.push(MetricKey::Buyers);
        if counted.is_empty() {
            metrics.values.insert(MetricKey::Gmv, 0.0);
        } else if counted.iter().any(|order| order.amount.is_none()) {
            unsupported.push(MetricKey::Gmv);
        } else {
            let currencies: std::collections::BTreeSet<_> = counted.iter().filter_map(|order| order.currency.clone()).collect();
            if currencies.len() == 1 {
                metrics.currency = currencies.into_iter().next();
                metrics.values.insert(MetricKey::Gmv, counted.iter().filter_map(|order| order.amount).sum());
            } else {
                unsupported.push(MetricKey::Gmv);
            }
        }
    }
    match returns {
        Err(()) => unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]),
        Ok(items) => {
            metrics.values.insert(MetricKey::RefundOrders, items.len() as f64);
            if items.is_empty() {
                metrics.values.insert(MetricKey::RefundAmount, 0.0);
            } else if items.iter().any(|item| item.amount.is_none()) {
                unsupported.push(MetricKey::RefundAmount);
            } else {
                let currencies: std::collections::BTreeSet<_> =
                    items.iter().filter_map(|item| item.currency.clone()).collect();
                if currencies.len() == 1 {
                    metrics.values.insert(MetricKey::RefundAmount, items.iter().filter_map(|item| item.amount).sum());
                } else {
                    unsupported.push(MetricKey::RefundAmount);
                }
            }
        }
    }
    match live {
        Some(count) => {
            metrics.values.insert(MetricKey::ProductsLive, count);
        }
        None => unsupported.push(MetricKey::ProductsLive),
    }
    for key in unsupported {
        metrics.values.remove(&key);
        if !metrics.unsupported.contains(&key) {
            metrics.unsupported.push(key);
        }
    }
    metrics.unsupported.sort();
    metrics
}

struct OrderHead {
    order_no: String,
    status: String,
    created_at: Option<String>,
}

struct OrderView {
    status: String,
    amount: Option<f64>,
    currency: Option<String>,
}

struct ReturnView {
    amount: Option<f64>,
    currency: Option<String>,
}

fn counts_order(status: &str) -> bool {
    !matches!(status, "" | "6")
}

fn pending_shipment(status: &str) -> bool {
    matches!(status, "1" | "2" | "3" | "7")
}

fn counts_return(status: &str) -> bool {
    !matches!(status, "" | "1" | "3")
}

async fn list_order_page(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
    page: i64,
) -> Result<(Vec<OrderHead>, Option<i64>), CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/order/order-list",
        Some(json!({
            "queryType": 1,
            "startTime": beijing_text(start),
            "endTime": beijing_text(end),
            "page": page,
            "pageSize": ORDER_PAGE,
        })),
        None,
        false,
        false,
    )
    .await?;
    let info = &body["info"];
    let total = info.get("count").and_then(as_i64);
    let heads = info
        .get("orderList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let order_no = text(item, "orderNo")?;
            Some(OrderHead {
                order_no,
                status: status_text(item.get("orderStatus")),
                created_at: text(item, "orderCreateTime"),
            })
        })
        .collect();
    Ok((heads, total))
}

async fn attach_order_amounts(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    heads: Vec<OrderHead>,
) -> Result<Vec<OrderView>, CallFail> {
    let mut amounts = std::collections::HashMap::new();
    for batch in heads.chunks(ORDER_PAGE as usize) {
        let order_no_list: Vec<&str> = batch.iter().map(|item| item.order_no.as_str()).collect();
        let body = call(
            transport,
            shop,
            now,
            "POST",
            "/open-api/order/order-detail",
            Some(json!({ "orderNoList": order_no_list })),
            None,
            false,
            false,
        )
        .await?;
        let rows = body.get("info").and_then(Value::as_array).cloned().unwrap_or_default();
        for row in rows {
            if let Some(order_no) = text(&row, "orderNo") {
                amounts.insert(
                    order_no,
                    (
                        row.get("productTotalPrice").and_then(as_f64),
                        text(&row, "orderCurrency"),
                        status_text(row.get("orderStatus")),
                    ),
                );
            }
        }
    }
    Ok(heads
        .into_iter()
        .map(|head| {
            let (amount, currency, detail_status) = amounts.remove(&head.order_no).unwrap_or((None, None, String::new()));
            OrderView {
                status: if detail_status.is_empty() { head.status } else { detail_status },
                amount,
                currency,
            }
        })
        .collect())
}

async fn list_returns(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<ReturnView>, CallFail> {
    let mut nos = Vec::new();
    let mut calls = 0usize;
    for (chunk_start, chunk_end) in time_chunks(start, end) {
        let mut page = 1i64;
        loop {
            if calls >= MAX_PAGES {
                return Err(CallFail::Failed("退货单超过单次汇总上限".into()));
            }
            calls += 1;
            let body = call(
                transport,
                shop,
                now,
                "POST",
                "/open-api/return-order/list",
                Some(json!({
                    "queryType": 1,
                    "startTime": beijing_text(chunk_start),
                    "endTime": beijing_text(chunk_end),
                    "page": page,
                    "pageSize": ORDER_PAGE,
                })),
                None,
                false,
                false,
            )
            .await?;
            let info = &body["info"];
            let total = info.get("count").and_then(as_i64);
            let batch = info.get("returnOrderList").and_then(Value::as_array).cloned().unwrap_or_default();
            let length = batch.len() as i64;
            for item in batch {
                if counts_return(&status_text(item.get("returnOrderStatus"))) {
                    if let Some(return_no) = text(&item, "returnOrderNo") {
                        nos.push(return_no);
                    }
                }
            }
            if length < ORDER_PAGE || page * ORDER_PAGE >= total.unwrap_or(i64::MAX) {
                break;
            }
            page += 1;
        }
    }
    let mut views = Vec::new();
    for batch in nos.chunks(ORDER_PAGE as usize) {
        let body = call(
            transport,
            shop,
            now,
            "POST",
            "/open-api/return-order/details",
            Some(json!({ "returnOrderNoList": batch })),
            None,
            false,
            false,
        )
        .await?;
        let rows = body.get("info").and_then(Value::as_array).cloned().unwrap_or_default();
        for row in rows {
            if row.get("noReturnGoodsSign").and_then(as_i64) == Some(1) || !counts_return(&status_text(row.get("returnOrderStatus")))
            {
                continue;
            }
            let goods = row.get("returnGoodsInfoList").and_then(Value::as_array);
            let (amount, currency) = match goods {
                Some(goods) if goods.iter().all(|item| item.get("estimateIncomeMoney").and_then(as_f64).is_some()) => {
                    let currencies: Vec<String> = goods.iter().filter_map(|item| text(item, "currency")).collect();
                    let unique: std::collections::BTreeSet<_> = currencies.iter().cloned().collect();
                    if unique.len() <= 1 {
                        (
                            Some(goods.iter().filter_map(|item| item.get("estimateIncomeMoney").and_then(as_f64)).sum()),
                            unique.into_iter().next(),
                        )
                    } else {
                        (None, None)
                    }
                }
                _ => (None, None),
            };
            views.push(ReturnView { amount, currency });
        }
    }
    Ok(views)
}

async fn live_product_count(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<f64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/searchProduct",
        Some(json!({"pageNum": 1, "pageSize": 1, "skcShelfStatus": 1, "languageList": ["en"]})),
        None,
        false,
        true,
    )
    .await?;
    body["info"]["meta"]
        .get("count")
        .and_then(as_f64)
        .ok_or_else(|| CallFail::Failed("SHEIN 未返回在售商品数量".into()))
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure_shein(shop)?;
    let mut categories = load_categories(transport, shop, now).await.map_err(fail_to_error)?;
    if let Some(parent) = parent_id.filter(|value| !value.is_empty()) {
        categories.retain(|category| category.parent_id == parent);
    }
    Ok(categories
        .into_iter()
        .map(|category| Category {
            id: category.id,
            name: category.name,
            parent_id: category.parent_id,
            leaf: category.leaf,
        })
        .collect())
}

pub async fn fetch_attributes(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure_shein(shop)?;
    if category_id.trim().is_empty() {
        return Err(CommerceError::invalid("缺少类目 ID"));
    }
    let categories = load_categories(transport, shop, now).await.map_err(fail_to_error)?;
    let product_type_id = categories
        .iter()
        .find(|category| category.id == category_id)
        .map(|category| category.product_type_id)
        .filter(|id| *id > 0)
        .ok_or_else(|| CommerceError::rejected("该类目不是末级或没有 product_type_id"))?;
    let template = load_template(transport, shop, now, product_type_id).await.map_err(fail_to_error)?;
    Ok(template
        .attributes
        .into_iter()
        .filter(|attribute| attribute.status != 1)
        .map(|attribute| CategoryAttribute {
            id: attribute.id.to_string(),
            name: attribute.name,
            required: attribute.required,
            input: attribute.input,
            options: attribute.options,
            unit: None,
        })
        .collect())
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    page: u32,
) -> Result<ProductPage, CommerceError> {
    ensure_shein(shop)?;
    if page == 0 {
        return Err(CommerceError::invalid("页码从 1 开始"));
    }
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/searchProduct",
        Some(json!({
            "pageNum": page,
            "pageSize": PRODUCT_PAGE,
            "languageList": ["en"],
        })),
        None,
        false,
        true,
    )
    .await
    .map_err(fail_to_error)?;
    let info = &body["info"];
    let items = info
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_product)
        .collect();
    Ok(ProductPage {
        items,
        page,
        page_size: PRODUCT_PAGE as u32,
        total: info.get("meta").and_then(|meta| meta.get("count")).and_then(as_i64),
    })
}

fn map_product(item: &Value) -> Option<RemoteProduct> {
    let remote_id = text(item, "spuName")?;
    let shelf = item.get("spuShelfStatus").and_then(as_i64);
    let (status, reason) = match shelf {
        Some(1) => (ListingStatus::Live, None),
        Some(0) => (ListingStatus::Reviewing, Some("下架（含待上架与售罄）".into())),
        Some(other) => (ListingStatus::Reviewing, Some(format!("未识别的上架状态 {other}"))),
        None => (ListingStatus::Reviewing, Some("SHEIN 未返回上架状态".into())),
    };
    let title = item
        .get("skcList")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(|skc| skc.get("skcTitle"))
        .and_then(Value::as_array)
        .and_then(|titles| {
            titles
                .iter()
                .find(|title| text(title, "language").as_deref() == Some("en"))
                .or_else(|| titles.first())
                .and_then(|title| text(title, "title"))
        });
    let mut skus = Vec::new();
    if let Some(skc_list) = item.get("skcList").and_then(Value::as_array) {
        for skc in skc_list {
            if let Some(sku_list) = skc.get("skuList").and_then(Value::as_array) {
                for sku in sku_list {
                    if let Some(code) = text(sku, "skuCode") {
                        skus.push(code);
                    }
                }
            }
        }
    }
    Some(RemoteProduct { remote_id, title, status, reason, skus })
}

pub async fn fetch_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    range: MetricRange,
    page: u32,
) -> Result<OrderPage, CommerceError> {
    ensure_shein(shop)?;
    if page == 0 {
        return Err(CommerceError::invalid("页码从 1 开始"));
    }
    let (start, end) = metric_window(range, now);
    let chunks = time_chunks(start, end);
    if chunks.len() != 1 {
        return Err(CommerceError::invalid(
            "SHEIN 订单查询单次不能超过 48 小时；请用今天或昨天窗口分页，更长窗口只在指标汇总里拆开",
        ));
    }
    let (chunk_start, chunk_end) = chunks[0];
    let (heads, total) = list_order_page(transport, shop, now, chunk_start, chunk_end, page as i64)
        .await
        .map_err(fail_to_error)?;
    let identity: Vec<_> = heads.iter().map(|head| (head.order_no.clone(), head.created_at.clone())).collect();
    let detailed = attach_order_amounts(transport, shop, now, heads).await.map_err(fail_to_error)?;
    let items = detailed
        .into_iter()
        .zip(identity)
        .map(|(view, (remote_id, created_at))| RemoteOrder {
            remote_id,
            status: view.status,
            amount: view.amount,
            currency: view.currency,
            created_at,
        })
        .collect();
    Ok(OrderPage { items, page, page_size: ORDER_PAGE as u32, total })
}

pub async fn push_listing(
    transport: &Transport,
    shop: &ResolvedShop,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    now: i64,
    existing_remote: Option<&str>,
) -> PushOutcome {
    if let Err(error) = ensure_call(shop) {
        return from_fail(None, error);
    }
    if let Some(remote_id) = existing_remote.map(str::trim).filter(|value| !value.is_empty()) {
        return match document_state(transport, shop, now, remote_id).await {
            Ok((status, reason)) => outcome(Some(remote_id.to_string()), status, reason),
            Err(error) => from_fail(Some(remote_id.to_string()), error),
        };
    }
    let prepared = match prepare_listing(transport, shop, draft, target, images, now).await {
        Ok(prepared) => prepared,
        Err(error) => return from_fail(None, error),
    };
    let mut uploaded = Vec::new();
    for (name, bytes, image_type) in prepared.images {
        match upload_image(transport, shop, now, &name, &bytes, image_type).await {
            Ok(url) => uploaded.push((image_type, url)),
            Err(error) => return from_fail(None, error),
        }
    }
    let mut body = prepared.body;
    let image_info_list: Vec<Value> = uploaded
        .iter()
        .enumerate()
        .map(|(index, (image_type, url))| {
            json!({"image_sort": index as i64 + 1, "image_type": image_type, "image_url": url})
        })
        .collect();
    if let Some(skc) = body["skc_list"].as_array_mut().and_then(|list| list.get_mut(0)) {
        skc["image_info"] = json!({"image_info_list": image_info_list});
    }
    match call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/product/publishOrEdit",
        Some(body),
        None,
        true,
        true,
    )
    .await
    {
        Ok(response) => publish_outcome(&response),
        Err(error) => from_fail(None, error),
    }
}

pub async fn refresh_remote(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_shein(shop)?;
    document_state(transport, shop, now, remote_id).await.map_err(fail_to_error)
}

async fn prepare_listing(
    transport: &Transport,
    shop: &ResolvedShop,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    now: i64,
) -> Result<PreparedListing, CallFail> {
    if draft.title.trim().chars().count() < 2 {
        return Err(CallFail::Rejected("标题至少需要 2 个字符".into()));
    }
    let weight_kg = draft.weight_kg.filter(|value| *value > 0.0).ok_or_else(|| {
        CallFail::Rejected("SHEIN 要求含包装重量，单位默认克，请填写 weightKg".into())
    })?;
    let dimensions = draft.dimensions_cm.clone().filter(|item| item.l > 0.0 && item.w > 0.0 && item.h > 0.0).ok_or_else(|| {
        CallFail::Rejected("SHEIN 要求含包装长宽高（厘米）".into())
    })?;
    if images.is_empty() || images.iter().any(|(_, bytes)| bytes.is_empty()) {
        return Err(CallFail::Rejected("SHEIN 上架需要至少一张本地图片，经 upload-pic 转成平台链接".into()));
    }
    if images.len() > 11 {
        return Err(CallFail::Rejected("主图最多 1 张，细节图最多 10 张".into()));
    }
    let skus = if draft.skus.is_empty() {
        return Err(CallFail::Rejected("SHEIN 发布需要至少一个带商家 SKU 编码的规格".into()));
    } else {
        draft.skus.clone()
    };
    if skus.iter().any(|sku| sku.code.as_deref().unwrap_or("").trim().is_empty()) {
        return Err(CallFail::Rejected("每个规格都需要商家 SKU 编码 supplier_sku".into()));
    }
    if skus.iter().any(|sku| sku.price.or(draft.price).filter(|price| *price > 0.0).is_none()) {
        return Err(CallFail::Rejected("每个规格需要大于 0 的价格".into()));
    }
    if skus.iter().any(|sku| sku.stock.or(draft.stock).is_none()) {
        return Err(CallFail::Rejected("每个规格需要库存，0 也要明确填写".into()));
    }
    let category_id = target.category_id.parse::<i64>().map_err(|_| CallFail::Rejected("类目 ID 必须是数字".into()))?;
    let categories = load_categories(transport, shop, now).await?;
    let leaf = categories.iter().find(|category| category.id == target.category_id).ok_or_else(|| {
        CallFail::Rejected("类目不在店铺末级分类树中".into())
    })?;
    if !leaf.leaf || leaf.product_type_id <= 0 {
        return Err(CallFail::Rejected("只能使用带 product_type_id 的末级类目".into()));
    }
    let template = load_template(transport, shop, now, leaf.product_type_id).await?;
    let standard = load_standard(transport, shop, now, category_id).await?;
    let sites = load_sites(transport, shop, now).await?;
    let warehouses = load_warehouses(transport, shop, now).await?;
    let merchant: Vec<_> = warehouses.into_iter().filter(|item| item.1 == 1).collect();
    let warehouse_id = match merchant.len() {
        0 => None,
        1 => Some(merchant[0].0.clone()),
        _ => {
            return Err(CallFail::Rejected(
                "店铺有多个商家仓，发布库存必须带 supplier_warehouse_id，当前上架草稿没有仓库字段".into(),
            ));
        }
    };
    let brand_code = if standard.brand_required || draft.brand.as_deref().is_some_and(|brand| !brand.trim().is_empty()) {
        if !standard.brand_show && !standard.brand_required {
            return Err(CallFail::Rejected("发布规范不允许传 brand_code".into()));
        }
        let wanted = draft.brand.as_deref().unwrap_or("").trim();
        if wanted.is_empty() {
            return Err(CallFail::Rejected("该类目必填品牌，请填写与店铺品牌列表一致的品牌名".into()));
        }
        Some(match_brand(transport, shop, now, wanted).await?)
    } else {
        None
    };
    let selected_sites = select_sites(&sites, draft.currency.as_deref())?;
    let pricing = if standard.supply_currency.is_some() {
        PriceMode::Supply { currency: standard.supply_currency.clone().unwrap_or_default() }
    } else if !selected_sites.is_empty() {
        PriceMode::Retail { sites: selected_sites.clone() }
    } else {
        return Err(CallFail::Rejected(
            "发布规范没有供货币种，站点列表也没有启用子站，无法确定售价或供货价".into(),
        ));
    };
    let product_attributes = product_attribute_payload(&template, &target.attributes)?;
    let (main_sale, secondary) = sale_plan(&template, &target.attributes, &skus)?;
    let supplier_code = skus[0].code.clone().unwrap_or_default();
    let language = standard.default_language.clone();
    let sku_list: Vec<Value> = skus
        .iter()
        .enumerate()
        .map(|(index, sku)| {
            let pair = secondary.as_ref().map(|(attribute_id, values)| (*attribute_id, values[index]));
            sku_body(sku, draft, &dimensions, weight_kg, pair, &pricing, warehouse_id.as_deref())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut body = json!({
        "category_id": category_id,
        "product_type_id": leaf.product_type_id,
        "source_system": "OpenAPI",
        "suit_flag": 0,
        "multi_language_name_list": [{"language": language.clone(), "name": draft.title.clone()}],
        "product_attribute_list": product_attributes,
        "skc_list": [{
            "supplier_code": supplier_code,
            "sale_attribute": {"attribute_id": main_sale.0, "attribute_value_id": main_sale.1},
            "sku_list": sku_list,
        }],
    });
    if let Some(description) = draft.description.as_deref().filter(|value| !value.trim().is_empty()) {
        body["multi_language_desc_list"] = json!([{"language": language, "name": description}]);
    }
    if let Some(brand_code) = brand_code {
        body["brand_code"] = json!(brand_code);
    }
    if standard.supplier_code_on_spu {
        body["supplier_code"] = json!(supplier_code);
    }
    if matches!(pricing, PriceMode::Retail { .. } | PriceMode::Supply { .. }) {
        if let PriceMode::Retail { sites } = &pricing {
            let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
            for site in sites {
                if let Some((_, subs)) = grouped.iter_mut().find(|(main, _)| main == &site.main_site) {
                    subs.push(site.sub_site.clone());
                } else {
                    grouped.push((site.main_site.clone(), vec![site.sub_site.clone()]));
                }
            }
            body["site_list"] = json!(grouped.into_iter().map(|(main, subs)| json!({"main_site": main, "sub_site_list": subs})).collect::<Vec<_>>());
        } else if !selected_sites.is_empty() {
            let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
            for site in &selected_sites {
                if let Some((_, subs)) = grouped.iter_mut().find(|(main, _)| main == &site.main_site) {
                    subs.push(site.sub_site.clone());
                } else {
                    grouped.push((site.main_site.clone(), vec![site.sub_site.clone()]));
                }
            }
            body["site_list"] = json!(grouped.into_iter().map(|(main, subs)| json!({"main_site": main, "sub_site_list": subs})).collect::<Vec<_>>());
        }
    }
    let images = images
        .iter()
        .enumerate()
        .map(|(index, (name, bytes))| {
            let image_type = if index == 0 { 1 } else { 2 };
            (name.clone(), bytes.clone(), image_type)
        })
        .collect();
    Ok(PreparedListing { body, images })
}

enum PriceMode {
    Retail { sites: Vec<SiteRow> },
    Supply { currency: String },
}

fn sku_body(
    sku: &ListingSku,
    draft: &ListingDraft,
    dimensions: &super::types::DimensionsCm,
    weight_kg: f64,
    secondary: Option<(i64, i64)>,
    pricing: &PriceMode,
    warehouse_id: Option<&str>,
) -> Result<Value, CallFail> {
    let price = sku.price.or(draft.price).unwrap_or(0.0);
    let stock = sku.stock.or(draft.stock).unwrap_or(0);
    let mut body = json!({
        "supplier_sku": sku.code.clone().unwrap_or_default(),
        "mall_state": 1,
        "weight": grams(weight_kg),
        "length": format_cm(dimensions.l),
        "width": format_cm(dimensions.w),
        "height": format_cm(dimensions.h),
    });
    if let Some((attribute_id, value_id)) = secondary {
        body["sale_attribute_list"] = json!([{"attribute_id": attribute_id, "attribute_value_id": value_id}]);
    } else {
        body["sale_attribute_list"] = json!([]);
    }
    match pricing {
        PriceMode::Retail { sites } => {
            body["price_info_list"] = json!(sites.iter().map(|site| json!({
                "base_price": price,
                "currency": site.currency,
                "sub_site": site.sub_site,
            })).collect::<Vec<_>>());
        }
        PriceMode::Supply { currency } => {
            body["cost_info"] = json!({"cost_price": format!("{price:.2}"), "currency": currency});
        }
    }
    let mut stock_row = json!({"inventory_num": stock});
    if let Some(warehouse_id) = warehouse_id {
        stock_row["supplier_warehouse_id"] = json!(warehouse_id);
    }
    body["stock_info_list"] = json!([stock_row]);
    Ok(body)
}

fn sale_plan(
    template: &AttributeTemplate,
    provided: &[ListingAttribute],
    skus: &[ListingSku],
) -> Result<((i64, i64), Option<(i64, Vec<i64>)>), CallFail> {
    let sales: Vec<&RawAttribute> = template.attributes.iter().filter(|attribute| attribute.kind == 1 && attribute.status != 1).collect();
    let default_value = sales.iter().find_map(|attribute| {
        attribute.options.iter().find(|option| option.name == "默认").map(|option| (attribute.id, option.id.parse::<i64>().unwrap_or(0)))
    });
    let main = match template.main_attribute_status {
        1 => default_value.filter(|(_, value)| *value > 0).ok_or_else(|| {
            CallFail::Rejected("该类目主销售属性必须使用【默认】，属性模板没有返回名为默认的属性值".into())
        })?,
        _ => {
            let mains: Vec<_> = sales.iter().copied().filter(|attribute| attribute.label == 1).collect();
            let chosen = mains.iter().find_map(|attribute| resolve_one(attribute, provided).map(|value| (attribute.id, value)));
            match chosen {
                Some(pair) => {
                    if template.main_attribute_status == 3 && default_value.is_some_and(|(_, value)| value == pair.1) {
                        return Err(CallFail::Rejected("该类目主销售属性不能使用【默认】".into()));
                    }
                    pair
                }
                None if template.main_attribute_status == 2 => default_value.filter(|(_, value)| *value > 0).ok_or_else(|| {
                    CallFail::Rejected("请在类目属性里填写主销售属性（attribute_label=1）的属性值".into())
                })?,
                None => {
                    return Err(CallFail::Rejected("请在类目属性里填写主销售属性（attribute_label=1）的属性值".into()));
                }
            }
        }
    };
    let secondary: Vec<_> = sales.iter().copied().filter(|attribute| attribute.label == 0 && attribute.status == 3).collect();
    if secondary.len() > 1 {
        return Err(CallFail::Rejected("该类目有多个必填次销售属性，当前规格只能对应一个次销售属性".into()));
    }
    let secondary = if let Some(attribute) = secondary.first() {
        let mut values = Vec::new();
        for sku in skus {
            values.push(resolve_sku_value(attribute, sku).ok_or_else(|| {
                CallFail::Rejected(format!(
                    "规格 {} 没有对上必填次销售属性 {} 的属性值 id 或名称",
                    sku.name, attribute.name
                ))
            })?);
        }
        Some((attribute.id, values))
    } else if skus.len() > 1 {
        return Err(CallFail::Rejected("没有必填次销售属性时，一个 SKC 只能有一个 SKU".into()));
    } else {
        None
    };
    Ok((main, secondary))
}

fn resolve_sku_value(attribute: &RawAttribute, sku: &ListingSku) -> Option<i64> {
    let token = sku.name.trim();
    if let Ok(id) = token.parse::<i64>() {
        if attribute.options.is_empty() || attribute.options.iter().any(|option| option.id == token) {
            return Some(id);
        }
    }
    attribute
        .options
        .iter()
        .find(|option| option.name.eq_ignore_ascii_case(token))
        .and_then(|option| option.id.parse().ok())
}

fn product_attribute_payload(template: &AttributeTemplate, provided: &[ListingAttribute]) -> Result<Vec<Value>, CallFail> {
    let mut rows = Vec::new();
    for attribute in &template.attributes {
        if attribute.status == 1 || !matches!(attribute.kind, 3 | 4) {
            if attribute.kind == 2 && attribute.status == 3 {
                return Err(CallFail::Rejected(format!(
                    "类目必填尺码属性 {}，当前上架草稿没有尺码表字段",
                    attribute.name
                )));
            }
            continue;
        }
        if attribute.dimension == 3 && attribute.required {
            return Err(CallFail::Rejected(format!(
                "类目属性 {} 绑定在 SKU 维度，当前草稿没有 SKU 属性位",
                attribute.name
            )));
        }
        let given = provided.iter().find(|item| item.id == attribute.id.to_string());
        let values = given.map(|item| {
            if item.values.is_empty() {
                vec![item.value.clone()]
            } else {
                item.values.clone()
            }
        });
        let values = values.unwrap_or_default().into_iter().filter(|value| !value.trim().is_empty()).collect::<Vec<_>>();
        if values.is_empty() {
            if attribute.required {
                return Err(CallFail::Rejected(format!("缺少必填类目属性 {}（{}）", attribute.name, attribute.id)));
            }
            continue;
        }
        if attribute.mode == 0 {
            rows.push(json!({"attribute_id": attribute.id, "attribute_extra_value": values[0]}));
            continue;
        }
        for value in values {
            let value_id = resolve_option(attribute, &value).ok_or_else(|| {
                CallFail::Rejected(format!("属性 {} 的值 {} 不在模板选项里", attribute.name, value))
            })?;
            let mut row = json!({"attribute_id": attribute.id, "attribute_value_id": value_id});
            if attribute.mode == 4 {
                row["attribute_extra_value"] = json!(value);
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

fn resolve_one(attribute: &RawAttribute, provided: &[ListingAttribute]) -> Option<i64> {
    let given = provided.iter().find(|item| item.id == attribute.id.to_string())?;
    let token = if given.value.trim().is_empty() { given.values.first()?.as_str() } else { given.value.as_str() };
    resolve_option(attribute, token)
}

fn resolve_option(attribute: &RawAttribute, token: &str) -> Option<i64> {
    let token = token.trim();
    if let Ok(id) = token.parse::<i64>() {
        if attribute.options.is_empty() || attribute.options.iter().any(|option| option.id == token) {
            return Some(id);
        }
    }
    attribute
        .options
        .iter()
        .find(|option| option.name.eq_ignore_ascii_case(token))
        .and_then(|option| option.id.parse().ok())
}

fn select_sites(sites: &[SiteRow], currency: Option<&str>) -> Result<Vec<SiteRow>, CallFail> {
    let enabled = sites.to_vec();
    if enabled.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(currency) = currency.map(str::trim).filter(|value| !value.is_empty()) {
        let matched: Vec<_> = enabled.into_iter().filter(|site| site.currency.eq_ignore_ascii_case(currency)).collect();
        if matched.is_empty() {
            return Err(CallFail::Rejected(format!("没有币种为 {currency} 的启用站点")));
        }
        return Ok(matched);
    }
    let currencies: std::collections::BTreeSet<_> = enabled.iter().map(|site| site.currency.to_ascii_uppercase()).collect();
    if currencies.len() > 1 {
        return Err(CallFail::Rejected("启用站点币种不一致，请在草稿里指定 currency".into()));
    }
    Ok(enabled)
}

async fn match_brand(transport: &Transport, shop: &ResolvedShop, now: i64, name: &str) -> Result<String, CallFail> {
    let body = call(transport, shop, now, "POST", "/open-api/goods/query-brand-list", Some(json!({})), None, false, true).await?;
    body["info"]
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| text(item, "brand_name").is_some_and(|brand| brand.eq_ignore_ascii_case(name)))
        .and_then(|item| text(item, "brand_code"))
        .ok_or_else(|| CallFail::Rejected(format!("品牌 {name} 不在店铺可用品牌列表中")))
}

async fn load_categories(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<Vec<RawCategory>, CallFail> {
    let body = call(transport, shop, now, "POST", "/open-api/goods/query-category-tree", Some(json!({})), None, false, true).await?;
    let mut out = Vec::new();
    if let Some(data) = body["info"].get("data").and_then(Value::as_array) {
        flatten_categories(data, &mut out);
    }
    Ok(out)
}

fn flatten_categories(nodes: &[Value], out: &mut Vec<RawCategory>) {
    for node in nodes {
        let Some(id) = identifier(node, "category_id") else { continue };
        out.push(RawCategory {
            id,
            parent_id: identifier(node, "parent_category_id").unwrap_or_else(|| "0".into()),
            name: text(node, "category_name").unwrap_or_default(),
            leaf: node.get("last_category").and_then(Value::as_bool).unwrap_or(false),
            product_type_id: node.get("product_type_id").and_then(as_i64).unwrap_or(0),
        });
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            flatten_categories(children, out);
        }
    }
}

async fn load_template(transport: &Transport, shop: &ResolvedShop, now: i64, product_type_id: i64) -> Result<AttributeTemplate, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/query-attribute-template",
        Some(json!({"product_type_id_list": [product_type_id]})),
        None,
        false,
        true,
    )
    .await?;
    let entry = body["info"]
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("product_type_id").and_then(as_i64) == Some(product_type_id))
        .cloned()
        .ok_or_else(|| CallFail::Rejected("属性模板没有返回该 product_type_id".into()))?;
    let attributes = entry
        .get("attribute_infos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_raw_attribute)
        .collect();
    Ok(AttributeTemplate {
        main_attribute_status: entry.get("main_attribute_status").and_then(as_i64).unwrap_or(3),
        attributes,
    })
}

fn map_raw_attribute(node: &Value) -> Option<RawAttribute> {
    let id = node.get("attribute_id").and_then(as_i64)?;
    let mode = node.get("attribute_mode").and_then(as_i64).unwrap_or(3);
    let status = node.get("attribute_status").and_then(as_i64).unwrap_or(2);
    let options = node
        .get("attribute_value_info_list")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter(|value| value.get("is_show").and_then(as_i64) != Some(2))
                .filter_map(|value| {
                    let id = identifier(value, "attribute_value_id").unwrap_or_default();
                    let name = text(value, "attribute_value").filter(|item| !item.is_empty())?;
                    Some(AttributeOption { id, name })
                })
                .collect()
        })
        .unwrap_or_default();
    let input = match mode {
        0 => AttributeInput::Number,
        1 | 4 => AttributeInput::MultiSelect,
        _ => AttributeInput::Select,
    };
    Some(RawAttribute {
        id,
        name: text(node, "attribute_name").unwrap_or_else(|| id.to_string()),
        required: status == 3,
        input,
        options,
        kind: node.get("attribute_type").and_then(as_i64).unwrap_or(4),
        label: node.get("attribute_label").and_then(as_i64).unwrap_or(0),
        mode,
        dimension: node.get("data_dimension").and_then(as_i64).unwrap_or(1),
        status,
    })
}

async fn load_standard(transport: &Transport, shop: &ResolvedShop, now: i64, category_id: i64) -> Result<FillStandard, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/query-publish-fill-in-standard",
        Some(json!({"category_id": category_id})),
        None,
        false,
        true,
    )
    .await?;
    let info = &body["info"];
    let language = text(info, "default_language").filter(|value| !value.is_empty()).ok_or_else(|| {
        CallFail::Rejected("发布规范未返回 default_language".into())
    })?;
    let brand = info
        .get("fill_in_standard_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| text(item, "field_key").as_deref() == Some("brand_code"));
    Ok(FillStandard {
        default_language: language,
        supply_currency: text(info, "currency").filter(|value| !value.is_empty()),
        brand_required: brand.and_then(|item| item.get("required")).and_then(Value::as_bool).unwrap_or(false),
        brand_show: brand.and_then(|item| item.get("show")).and_then(Value::as_bool).unwrap_or(true),
        supplier_code_on_spu: info.get("supplier_code_in_spu_dimension").and_then(Value::as_bool).unwrap_or(false),
    })
}

async fn load_sites(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<Vec<SiteRow>, CallFail> {
    let body = call(transport, shop, now, "POST", "/open-api/goods/query-site-list", Some(json!({})), None, false, true).await?;
    let mut sites = Vec::new();
    let data = body["info"].get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    for main in data {
        let main_site = text(&main, "main_site").unwrap_or_else(|| "shein".into());
        let subs = main.get("sub_site_list").and_then(Value::as_array).cloned().unwrap_or_default();
        for sub in subs {
            if sub.get("site_status").and_then(as_i64) != Some(1) {
                continue;
            }
            let Some(sub_site) = text(&sub, "site_abbr") else { continue };
            let Some(currency) = text(&sub, "currency") else { continue };
            sites.push(SiteRow { main_site: main_site.clone(), sub_site, currency });
        }
    }
    Ok(sites)
}

async fn load_warehouses(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<Vec<(String, i64)>, CallFail> {
    let body = call(transport, shop, now, "GET", "/open-api/msc/warehouse/list", None, None, false, false).await?;
    Ok(body["info"]
        .get("list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let code = text(item, "warehouseCode")?;
            let kind = item.get("warehouseType").and_then(as_i64).or_else(|| text(item, "warehouseType")?.parse().ok())?;
            Some((code, kind))
        })
        .collect())
}

async fn upload_image(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    name: &str,
    bytes: &[u8],
    image_type: i64,
) -> Result<String, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/upload-pic",
        None,
        Some((name.to_string(), bytes.to_vec(), image_type.to_string())),
        true,
        true,
    )
    .await?;
    text(&body["info"], "image_url").filter(|url| url.starts_with("http")).ok_or_else(|| {
        CallFail::Rejected(text(&body["info"], "failure_reason").unwrap_or_else(|| "SHEIN 未返回图片链接".into()))
    })
}

fn publish_outcome(body: &Value) -> PushOutcome {
    let info = &body["info"];
    let remote_id = text(info, "spu_name");
    let success = info.get("success").and_then(Value::as_bool).unwrap_or(remote_id.is_some());
    if success && remote_id.is_some() {
        return outcome(remote_id, ListingStatus::Reviewing, None);
    }
    let reason = pre_valid_messages(info).or_else(|| text(body, "msg")).unwrap_or_else(|| "SHEIN 未接受发布".into());
    outcome(remote_id, ListingStatus::Rejected, Some(reason))
}

fn pre_valid_messages(info: &Value) -> Option<String> {
    let messages = info
        .get("pre_valid_result")
        .and_then(Value::as_array)?
        .iter()
        .flat_map(|item| item.get("messages").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|item| item.as_str())
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("；"))
    }
}

async fn document_state(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    spu_name: &str,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/open-api/goods/query-document-state",
        Some(json!({"spuList": [{"spuName": spu_name}]})),
        None,
        false,
        true,
    )
    .await?;
    let skcs = body["info"]
        .get("data")
        .and_then(Value::as_array)
        .and_then(|data| data.first())
        .and_then(|item| item.get("skcList"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if skcs.is_empty() {
        return Err(CallFail::Failed("SHEIN 未返回审核状态".into()));
    }
    let states: Vec<(i64, Vec<String>)> = skcs
        .iter()
        .map(|skc| {
            let state = skc.get("documentState").and_then(as_i64).unwrap_or(1);
            let reasons = skc
                .get("failedReason")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| text(item, "content"))
                .collect();
            (state, reasons)
        })
        .collect();
    if let Some((_, reasons)) = states.iter().find(|(state, _)| *state == 3 || *state == -1) {
        return Ok((ListingStatus::Rejected, Some(reason_text(reasons, "审批失败"))));
    }
    if states.iter().any(|(state, _)| *state == 4) {
        return Ok((ListingStatus::Rejected, Some("已撤回".into())));
    }
    if states.iter().any(|(state, _)| *state == 1 || *state == 5) {
        let reason = if states.iter().any(|(state, _)| *state == 5) { Some("申诉中".into()) } else { None };
        return Ok((ListingStatus::Reviewing, reason));
    }
    if states.iter().all(|(state, _)| *state == 2) {
        return Ok((ListingStatus::Live, None));
    }
    Ok((ListingStatus::Reviewing, Some("未识别的公文状态".into())))
}

fn reason_text(reasons: &[String], fallback: &str) -> String {
    if reasons.is_empty() {
        fallback.to_string()
    } else {
        reasons.join("；")
    }
}

#[allow(clippy::too_many_arguments)]
async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    method: &'static str,
    path: &'static str,
    json_body: Option<Value>,
    file: Option<(String, Vec<u8>, String)>,
    mutating: bool,
    language: bool,
) -> Result<Value, CallFail> {
    let timestamp_ms = now.saturating_mul(1000);
    let random = random_key();
    let signature = sign_shein(&shop.partner_id, &shop.partner_key, path, timestamp_ms, &random).map_err(CallFail::Failed)?;
    let mut headers = vec![
        ("x-lt-openKeyId".into(), shop.partner_id.clone()),
        ("x-lt-timestamp".into(), timestamp_ms.to_string()),
        ("x-lt-signature".into(), signature),
    ];
    if language {
        headers.push(("language".into(), "en".into()));
    }
    if file.is_none() {
        headers.push(("Content-Type".into(), "application/json;charset=UTF-8".into()));
    }
    let (file_name, file_bytes, query) = match file {
        Some((name, bytes, image_type)) => (Some(name), Some(bytes), vec![("image_type".into(), image_type)]),
        None => (None, None, Vec::new()),
    };
    let request = Outbound {
        method,
        path: path.to_string(),
        query,
        json: json_body,
        file_name,
        file_bytes,
        mutating,
        host: HOST.to_string(),
        headers,
    };
    let inbound = match transport.execute(request).await {
        Ok(inbound) => inbound,
        Err(TransportFault::BeforeDispatch(message)) => return Err(CallFail::Failed(message)),
        Err(TransportFault::AfterDispatch(message)) if mutating => return Err(CallFail::Uncertain(message)),
        Err(TransportFault::AfterDispatch(message)) => return Err(CallFail::Failed(message)),
    };
    classify(mutating, inbound.status, &inbound.body)
}

fn classify(mutating: bool, status: u16, body: &Value) -> Result<Value, CallFail> {
    let code = body.get("code");
    let ok = match code {
        None => status < 400,
        Some(value) => value.as_i64() == Some(0) || value.as_str() == Some("0") || value.as_u64() == Some(0),
    };
    let message = text(body, "msg").unwrap_or_else(|| format!("HTTP {status}"));
    if status >= 500 {
        return if mutating { Err(CallFail::Uncertain(message)) } else { Err(CallFail::Failed(message)) };
    }
    if status >= 400 || !ok {
        if is_auth(&message) {
            return Err(CallFail::Auth(message));
        }
        return Err(CallFail::Rejected(message));
    }
    Ok(body.clone())
}

fn is_auth(message: &str) -> bool {
    let blob = message.to_ascii_lowercase();
    blob.contains("openkey") || blob.contains("signature") || blob.contains("签名") || blob.contains("secretkey")
}

fn from_fail(remote_id: Option<String>, error: CallFail) -> PushOutcome {
    let (status, reason) = match error {
        CallFail::Uncertain(message) => (ListingStatus::Uncertain, message),
        CallFail::Failed(message) => (ListingStatus::Failed, message),
        CallFail::Auth(message) | CallFail::Rejected(message) => (ListingStatus::Rejected, message),
    };
    outcome(remote_id, status, Some(reason))
}

fn outcome(remote_id: Option<String>, status: ListingStatus, reason: Option<String>) -> PushOutcome {
    PushOutcome { remote_id, status, reason }
}

fn fail_to_error(error: CallFail) -> CommerceError {
    match error {
        CallFail::Auth(message) | CallFail::Rejected(message) => CommerceError::rejected(message),
        CallFail::Failed(message) => CommerceError::failed(message),
        CallFail::Uncertain(message) => CommerceError::uncertain(message),
    }
}

fn grams(weight_kg: f64) -> f64 {
    (weight_kg * 1000.0 * 100.0).round() / 100.0
}

fn format_cm(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{}", (value * 100.0).round() / 100.0)
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).filter(|item| !item.is_empty()).map(str::to_owned)
}

fn identifier(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .or_else(|| item.as_i64().map(|number| number.to_string()))
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|item| item as i64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|item| item as f64))
        .or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn status_text(value: Option<&Value>) -> String {
    value.and_then(as_i64).map(|item| item.to_string()).or_else(|| value.and_then(Value::as_str).map(str::to_owned)).unwrap_or_default()
}

#[cfg(test)]
#[path = "shein_tests.rs"]
mod shein_tests;
