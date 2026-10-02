//! Mercado Libre local listings and Global Selling.
//!
//! Local items: <https://developers.mercadolibre.com.ar/en_us/list-products>
//! Pictures: <https://developers.mercadolibre.com.ar/en_us/categories-and-attributes/working-with-pictures>
//! Variations and `SELLER_SKU`: <https://developers.mercadolibre.com.ar/en_us/variations>
//! User Products (`user_product_seller`, `family_name`): <https://developers.mercadolibre.com.ar/en_us/price-per-variation>
//! Seller item search: <https://developers.mercadolibre.com.ar/en_us/items-and-searches>
//! Orders and date filters: <https://developers.mercadolibre.com.ar/en_us/manage-sales>
//! Global Selling publish and `GET /marketplace/users/{id}`: <https://global-selling.mercadolibre.com/devsite/global-listing>
//! Fully Managed is used only when that users payload contains `business_model` `CBT CN Fulfillment Managed`:
//! <https://global-selling.mercadolibre.com/devsite/devsite/fully-managed-product-publishing>

use super::adapter::{ApiSession, HttpBody, HttpRequest, MultipartFile, SharedTransport, TransportFault};
use super::shopee::PushOutcome;
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities, CommerceError, CommerceOrder,
    CommerceOrderLine, CommerceProduct, ListingAttribute, ListingDraft, ListingSku, ListingStatus, ListingTarget,
    MetricKey, MetricRange, OrderPage, ProductPage, ShopMetrics,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

const HOST: &str = "https://api.mercadolibre.com";
const PAGE: i64 = 50;
const MULTIGET: usize = 20;
const MAX_PAGES: usize = 20;
const FULLY_MANAGED: &str = "CBT CN Fulfillment Managed";
const NEW_CONDITION: &str = "2230284";

#[derive(Clone, Debug)]
struct Account {
    site_id: String,
    user_product: bool,
    markets: Vec<Market>,
    seller_id: String,
}

#[derive(Clone, Debug)]
struct Market {
    user_id: String,
    site_id: String,
    logistic_type: String,
    net_proceeds: bool,
    fully_managed: bool,
}

struct Prepared {
    account: Account,
    currency: String,
    variation_attribute: Option<String>,
    price: Option<f64>,
    stock: i64,
}

struct Snap {
    status: Option<String>,
    sub_status: Vec<String>,
    tags: Vec<String>,
}

struct Created {
    ids: Vec<String>,
    snaps: Vec<Snap>,
}

#[derive(Clone, Debug)]
struct OrderHit {
    status: String,
    amount: Option<f64>,
    currency: Option<String>,
    buyer: Option<String>,
    pending: Option<bool>,
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

pub(crate) fn site_offset_secs(site: &str) -> i64 {
    match site.trim().to_ascii_uppercase().as_str() {
        "MLA" | "AR" | "MLB" | "BR" | "MLU" | "UY" => -3 * 3600,
        "MLC" | "CL" | "MLV" | "VE" => -4 * 3600,
        "MCO" | "CO" | "MPE" | "PE" | "MEC" | "EC" => -5 * 3600,
        "MLM" | "MX" => -6 * 3600,
        _ => 0,
    }
}

pub(crate) fn metric_bounds(range: MetricRange, now: i64, offset: i64) -> (i64, i64) {
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

/// Hour floor. Order search drops minutes, seconds, and milliseconds.
pub(crate) fn format_from(unix: i64, offset_secs: i64) -> String {
    let local = unix + offset_secs;
    let floor = local - local.rem_euclid(3600);
    format_zoned(floor - offset_secs, offset_secs)
}

/// Next hour when the instant is not already on the hour, so the current hour survives minute dropping.
pub(crate) fn format_to(unix: i64, offset_secs: i64) -> String {
    let local = unix + offset_secs;
    let floor = local - local.rem_euclid(3600);
    let hour = if local == floor { floor } else { floor + 3600 };
    format_zoned(hour - offset_secs, offset_secs)
}

fn format_zoned(unix: i64, offset_secs: i64) -> String {
    let offset = chrono::FixedOffset::east_opt(offset_secs as i32)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).expect("utc"));
    let local = chrono::DateTime::from_timestamp(unix, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .with_timezone(&offset);
    let hours = offset_secs.div_euclid(3600);
    let sign = if hours < 0 { '-' } else { '+' };
    format!(
        "{stamp}{sign}{hours:02}:00",
        stamp = local.format("%Y-%m-%dT%H:00:00.000"),
        sign = sign,
        hours = hours.abs(),
    )
}

fn normalize_site(value: &str) -> Option<String> {
    let key = value.trim().to_ascii_uppercase();
    Some(
        match key.as_str() {
            "AR" => "MLA",
            "BR" => "MLB",
            "MX" => "MLM",
            "CL" => "MLC",
            "CO" => "MCO",
            "UY" => "MLU",
            "PE" => "MPE",
            "EC" => "MEC",
            "VE" => "MLV",
            "MLA" | "MLB" | "MLM" | "MLC" | "MCO" | "MLU" | "MPE" | "MEC" | "MLV" | "CBT" => return Some(key),
            _ => return None,
        }
        .to_string(),
    )
}

pub fn capabilities() -> CommerceCapabilities {
    CommerceCapabilities {
        platform: Platform::Mercadolibre,
        metrics: vec![
            MetricKey::Gmv,
            MetricKey::Orders,
            MetricKey::Buyers,
            MetricKey::PendingShipment,
            MetricKey::ProductsLive,
        ],
        listing: true,
        categories: true,
        products: true,
        orders: true,
        notes: vec![
            "本地站点 POST /items，描述另 POST /items/{id}/description。CBT 读 marketplace/users 后 POST /global/items，物流类型按账号原样复制。只有 business_model 为 CBT CN Fulfillment Managed 才走 Fully Managed。user_product_seller 按 SKU 各发一条 family_name。退款不在卖家 orders/search 中，记为不支持。".into(),
        ],
    }
}

pub async fn metrics(
    http: &SharedTransport,
    session: &ApiSession,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    if session.platform != Platform::Mercadolibre {
        return Err(CommerceError::unsupported("该平台尚未接入店铺指标"));
    }
    let account = match load_account(http, session).await {
        Ok(account) => account,
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(error) => {
            return Ok(blank_metrics(&session.shop_id, range, now, Some(error.text().to_string()), MetricKey::ALL.to_vec()));
        }
    };
    let (start, end) = metric_bounds(range, now, site_offset_secs(&account.site_id));
    let orders = match list_orders(http, session, &account, start, end).await {
        Ok(orders) => Ok(orders),
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(error) => Err(error.text().to_string()),
    };
    let live = match live_count(http, session, &account).await {
        Ok(count) => Some(count),
        Err(CallFail::Auth(message)) if orders.as_ref().ok().is_some_and(|items| items.is_empty()) => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => None,
    };
    Ok(fold_metrics(&session.shop_id, range, now, orders, live))
}

fn blank_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    error: Option<String>,
    unsupported: Vec<MetricKey>,
) -> ShopMetrics {
    let mut metrics = fold_metrics(shop_id, range, now, Err(error.clone().unwrap_or_default()), None);
    metrics.error = error;
    for key in unsupported {
        metrics.values.insert(key, 0.0);
        if !metrics.unsupported.contains(&key) {
            metrics.unsupported.push(key);
        }
    }
    metrics.unsupported.sort();
    metrics
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: Result<Vec<OrderHit>, String>,
    live: Option<f64>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    let mut metrics = super::types::zero_metrics(shop_id.to_string(), range, fetched_at);
    let mut unsupported = vec![MetricKey::RefundAmount, MetricKey::RefundOrders];
    match orders {
        Err(message) => {
            unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
            if !message.is_empty() {
                metrics.error = Some(message);
            }
        }
        Ok(orders) => fold_orders(&mut metrics, &mut unsupported, &orders),
    }
    match live {
        Some(count) => {
            metrics.values.insert(MetricKey::ProductsLive, count);
        }
        None => unsupported.push(MetricKey::ProductsLive),
    }
    for key in unsupported {
        metrics.values.insert(key, 0.0);
        if !metrics.unsupported.contains(&key) {
            metrics.unsupported.push(key);
        }
    }
    metrics.unsupported.sort();
    metrics
}

fn fold_orders(metrics: &mut ShopMetrics, unsupported: &mut Vec<MetricKey>, orders: &[OrderHit]) {
    let counted: Vec<&OrderHit> = orders.iter().filter(|order| order.status == "paid").collect();
    metrics.values.insert(MetricKey::Orders, counted.len() as f64);
    if counted.is_empty() {
        metrics.values.insert(MetricKey::Gmv, 0.0);
        metrics.values.insert(MetricKey::Buyers, 0.0);
        metrics.values.insert(MetricKey::PendingShipment, 0.0);
        return;
    }
    let currencies: BTreeSet<&str> = counted.iter().filter_map(|order| order.currency.as_deref()).collect();
    let amounts_complete = counted.iter().all(|order| order.amount.is_some() && order.currency.is_some());
    if amounts_complete && currencies.len() == 1 {
        metrics.currency = currencies.iter().next().map(|currency| (*currency).to_string());
        metrics.values.insert(MetricKey::Gmv, counted.iter().filter_map(|order| order.amount).sum());
    } else {
        unsupported.push(MetricKey::Gmv);
    }
    if counted.iter().any(|order| order.buyer.is_none()) {
        unsupported.push(MetricKey::Buyers);
    } else {
        let buyers: BTreeSet<&str> = counted.iter().filter_map(|order| order.buyer.as_deref()).collect();
        metrics.values.insert(MetricKey::Buyers, buyers.len() as f64);
    }
    if counted.iter().any(|order| order.pending.is_none()) {
        unsupported.push(MetricKey::PendingShipment);
    } else {
        let pending = counted.iter().filter(|order| order.pending == Some(true)).count() as f64;
        metrics.values.insert(MetricKey::PendingShipment, pending);
    }
}

pub async fn categories(
    http: &SharedTransport,
    session: &ApiSession,
    parent_id: Option<&str>,
    _now: i64,
) -> Result<Vec<Category>, CommerceError> {
    ensure_platform(session)?;
    let site = resolve_site(http, session).await.map_err(fail_to_error)?;
    if let Some(parent) = parent_id.filter(|value| !value.is_empty()) {
        let body = call(http, session, "GET", format!("/categories/{parent}"), Vec::new(), None, None, false)
            .await
            .map_err(fail_to_error)?;
        let mut categories = Vec::new();
        for child in children_of(&body) {
            let detail = call(http, session, "GET", format!("/categories/{child}"), Vec::new(), None, None, false)
                .await
                .map_err(fail_to_error)?;
            if let Some(category) = map_category_node(&detail, parent) {
                categories.push(category);
            }
        }
        return Ok(categories);
    }
    let body = call(http, session, "GET", format!("/sites/{site}/categories"), Vec::new(), None, None, false)
        .await
        .map_err(fail_to_error)?;
    Ok(map_site_roots(&body))
}

fn children_of(body: &Value) -> Vec<String> {
    body.get("children_categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|child| text(child, "id"))
        .collect()
}

fn map_site_roots(body: &Value) -> Vec<Category> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = text(item, "id")?;
            let name = text(item, "name").unwrap_or_else(|| id.clone());
            Some(Category { id, name, parent_id: "0".into(), leaf: false })
        })
        .collect()
}

fn map_category_node(body: &Value, parent_id: &str) -> Option<Category> {
    let id = text(body, "id")?;
    let name = text(body, "name").unwrap_or_else(|| id.clone());
    let leaf = body.get("children_categories").and_then(Value::as_array).is_some_and(|children| children.is_empty());
    Some(Category { id, name, parent_id: parent_id.to_string(), leaf })
}

pub async fn attributes(
    http: &SharedTransport,
    session: &ApiSession,
    category_id: &str,
    _now: i64,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure_platform(session)?;
    if category_id.trim().is_empty() {
        return Err(CommerceError::invalid("缺少类目"));
    }
    let body = call(
        http,
        session,
        "GET",
        format!("/categories/{category_id}/attributes"),
        Vec::new(),
        None,
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    attribute_rows(body).iter().filter_map(map_one_attribute).collect()
}

fn attribute_rows(body: &Value) -> Vec<Value> {
    if let Some(rows) = body.as_array() {
        return rows.clone();
    }
    body.get("attributes").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn map_one_attribute(row: &Value) -> Option<CategoryAttribute> {
    let id = text(row, "id")?;
    let name = text(row, "name").unwrap_or_else(|| id.clone());
    let tags = row.get("tags").cloned().unwrap_or(Value::Null);
    let required = tag_flag(&tags, "required") || tag_flag(&tags, "new_required");
    let multivalued = tag_flag(&tags, "multivalued");
    let value_type = text(row, "value_type").unwrap_or_default();
    let options = row
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let name = text(value, "name")?;
                    let id = text(value, "id").unwrap_or_default();
                    Some(AttributeOption { id, name })
                })
                .collect()
        })
        .unwrap_or_default();
    let unit = text(row, "default_unit").or_else(|| {
        row.get("allowed_units")
            .and_then(Value::as_array)
            .and_then(|units| units.first())
            .and_then(|unit| text(unit, "id").or_else(|| text(unit, "name")))
    });
    Some(CategoryAttribute { id, name, required, input: map_input(&value_type, multivalued), options, unit })
}

fn map_input(value_type: &str, multivalued: bool) -> AttributeInput {
    match value_type {
        "number" | "number_unit" => AttributeInput::Number,
        "list" | "boolean" if multivalued => AttributeInput::MultiSelect,
        "list" | "boolean" => AttributeInput::Select,
        _ => AttributeInput::Text,
    }
}

fn tag_flag(tags: &Value, name: &str) -> bool {
    match tags {
        Value::Object(map) => map.get(name).and_then(Value::as_bool).unwrap_or(false),
        Value::Array(items) => items.iter().any(|item| item.as_str() == Some(name)),
        _ => false,
    }
}

pub fn map_item_status(status: &str, sub_status: &[String], _tags: &[String]) -> (ListingStatus, Option<String>) {
    let detail = if sub_status.is_empty() {
        status.to_string()
    } else {
        format!("{status}:{}", sub_status.join(","))
    };
    let banned = sub_status.iter().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("forbidden") || value.contains("banned")
    });
    if banned && status != "closed" {
        return (ListingStatus::Banned, Some(detail));
    }
    if status == "inactive" && sub_status.iter().any(|value| value == "deleted") {
        return (ListingStatus::Rejected, Some("deleted".into()));
    }
    match status {
        "active" => (ListingStatus::Live, None),
        "paused" | "under_review" | "inactive" | "payment_required" | "not_yet_active" => {
            (ListingStatus::Reviewing, Some(detail))
        }
        "closed" => (ListingStatus::Rejected, Some("closed".into())),
        other => (ListingStatus::Reviewing, Some(format!("未识别的商品状态 {other}"))),
    }
}

pub async fn push_listing(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    _now: i64,
    existing_remote: Option<&str>,
) -> PushOutcome {
    if session.platform != Platform::Mercadolibre {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    if let Some(remote_id) = existing_remote.filter(|value| !value.is_empty()) {
        return continue_existing(http, session, draft, remote_id).await;
    }
    if let Some(reason) = preflight(draft, target, images) {
        return outcome(None, ListingStatus::Rejected, Some(reason));
    }
    let prepared = match prepare(http, session, draft, target).await {
        Ok(prepared) => prepared,
        Err(error) => return from_fail(None, error),
    };
    let mut picture_ids = Vec::new();
    for (name, bytes) in images {
        match upload_picture(http, session, name, bytes).await {
            Ok(id) => picture_ids.push(id),
            Err(error) => return from_fail(None, error),
        }
    }
    let created = match create_items(http, session, draft, target, &prepared, &picture_ids).await {
        Ok(created) => created,
        Err(error) => return from_fail(None, error),
    };
    if created.ids.is_empty() {
        return outcome(None, ListingStatus::Rejected, Some("Mercado Libre 未返回商品 ID".into()));
    }
    let remote_id = created.ids.join(",");
    if !prepared.account.global() {
        if let Some(description) = plain_description(draft) {
            for id in &created.ids {
                if let Err(error) = post_description(http, session, id, &description).await {
                    return from_fail(Some(remote_id), error);
                }
            }
        }
    }
    if let Some((status, reason)) = fold_snaps(&created) {
        return outcome(Some(remote_id), status, reason);
    }
    match refresh_ids(http, session, &created.ids).await {
        Ok((status, reason)) => outcome(Some(remote_id), status, reason),
        Err(error) => from_fail(Some(remote_id), error),
    }
}

async fn continue_existing(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    remote_id: &str,
) -> PushOutcome {
    let ids: Vec<String> = split_ids(remote_id);
    let global = ids.iter().any(|id| id.starts_with("CBT"));
    if !global {
        if let Some(description) = plain_description(draft) {
            for id in &ids {
                if let Err(error) = post_description(http, session, id, &description).await {
                    return from_fail(Some(remote_id.to_string()), error);
                }
            }
        }
    }
    match refresh_ids(http, session, &ids).await {
        Ok((status, reason)) => outcome(Some(remote_id.to_string()), status, reason),
        Err(error) => from_fail(Some(remote_id.to_string()), error),
    }
}

pub async fn listing_status(
    http: &SharedTransport,
    session: &ApiSession,
    remote_id: &str,
    _now: i64,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_platform(session)?;
    let ids = split_ids(remote_id);
    if ids.is_empty() {
        return Err(CommerceError::invalid("缺少远程商品 ID"));
    }
    refresh_ids(http, session, &ids).await.map_err(fail_to_error)
}

fn split_ids(remote_id: &str) -> Vec<String> {
    remote_id.split(',').filter(|id| !id.is_empty()).map(str::to_owned).collect()
}

async fn refresh_ids(
    http: &SharedTransport,
    session: &ApiSession,
    ids: &[String],
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let mut mapped = Vec::new();
    for id in ids {
        let path = if id.starts_with("CBT") {
            format!("/marketplace/items/{id}")
        } else {
            format!("/items/{id}")
        };
        let body = call(http, session, "GET", path, Vec::new(), None, None, false).await?;
        let status = text(&body, "status").ok_or_else(|| CallFail::Failed("Mercado Libre 未返回商品状态".into()))?;
        mapped.push(map_item_status(&status, &string_list(body.get("sub_status")), &string_list(body.get("tags"))));
    }
    if mapped.iter().all(|(status, _)| *status == ListingStatus::Live) {
        return Ok((ListingStatus::Live, None));
    }
    Ok(mapped.into_iter().find(|(status, _)| *status != ListingStatus::Live).unwrap_or((ListingStatus::Reviewing, None)))
}

pub async fn products(
    http: &SharedTransport,
    session: &ApiSession,
    cursor: Option<&str>,
    _now: i64,
) -> Result<ProductPage, CommerceError> {
    ensure_platform(session)?;
    let offset = parse_offset(cursor)?;
    let account = load_account(http, session).await.map_err(fail_to_error)?;
    let body = call(
        http,
        session,
        "GET",
        format!("/users/{}/items/search", account.seller_id),
        vec![("limit".into(), PAGE.to_string()), ("offset".into(), offset.to_string())],
        None,
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let ids = string_list(body.get("results"));
    let mut products = Vec::new();
    for chunk in ids.chunks(MULTIGET) {
        let details = call(
            http,
            session,
            "GET",
            "/items".into(),
            vec![("ids".into(), chunk.join(","))],
            None,
            None,
            false,
        )
        .await
        .map_err(fail_to_error)?;
        products.extend(map_multiget(&details, chunk));
    }
    let next = next_cursor(body.get("paging"), offset, ids.len());
    Ok(ProductPage { shop_id: session.shop_id.clone(), items: products, next_cursor: next })
}

pub async fn orders(
    http: &SharedTransport,
    session: &ApiSession,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    ensure_platform(session)?;
    let account = load_account(http, session).await.map_err(fail_to_error)?;
    let sellers = account.order_sellers();
    let (seller_index, offset) = parse_order_cursor(cursor, sellers.len())?;
    let seller = sellers.get(seller_index).cloned().unwrap_or_else(|| account.seller_id.clone());
    let (start, end) = metric_bounds(range, now, site_offset_secs(&account.site_id));
    let body = search_orders(http, session, &seller, start, end, offset, &account.site_id).await.map_err(fail_to_error)?;
    let orders = map_orders(&body);
    let page_len = body.get("results").and_then(Value::as_array).map(|rows| rows.len()).unwrap_or(0);
    let next = match next_cursor(body.get("paging"), offset, page_len) {
        Some(next_offset) if sellers.len() > 1 => Some(format!("{seller_index}:{next_offset}")),
        Some(next_offset) => Some(next_offset),
        None if seller_index + 1 < sellers.len() => Some(format!("{}:0", seller_index + 1)),
        None => None,
    };
    Ok(OrderPage { shop_id: session.shop_id.clone(), range, items: orders, next_cursor: next })
}

fn map_orders(body: &Value) -> Vec<CommerceOrder> {
    body.get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(CommerceOrder {
                id: identifier(row, "id")?,
                status: text(row, "status").unwrap_or_default(),
                created_at: text(row, "date_created"),
                amount: row.get("total_amount").and_then(as_f64),
                currency: text(row, "currency_id"),
                buyer: row.get("buyer").and_then(|buyer| identifier(buyer, "id")),
                lines: order_lines(row),
            })
        })
        .collect()
}

fn order_lines(row: &Value) -> Vec<CommerceOrderLine> {
    row.get("order_items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|line| {
                    let item = line.get("item").unwrap_or(line);
                    CommerceOrderLine {
                        title: text(item, "title").unwrap_or_default(),
                        quantity: line.get("quantity").and_then(as_i64).unwrap_or(0),
                        sku: text(item, "seller_sku").or_else(|| text(item, "seller_custom_field")),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn map_multiget(body: &Value, ids: &[String]) -> Vec<CommerceProduct> {
    let rows = body.as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return ids.iter().map(|id| empty_product(id)).collect();
    }
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let code = row.get("code").and_then(Value::as_u64).unwrap_or(200);
            let item = row.get("body").cloned().unwrap_or(row);
            let remote_id = text(&item, "id").or_else(|| ids.get(index).cloned()).unwrap_or_default();
            if code >= 400 {
                return empty_product(&remote_id);
            }
            CommerceProduct {
                id: remote_id,
                title: text(&item, "title").or_else(|| text(&item, "family_name")).unwrap_or_default(),
                status: text(&item, "status").unwrap_or_default(),
                price: item.get("price").and_then(as_f64),
                currency: text(&item, "currency_id"),
                stock: item.get("available_quantity").and_then(as_i64),
                sku: seller_sku(&item),
            }
        })
        .collect()
}

fn empty_product(id: &str) -> CommerceProduct {
    CommerceProduct {
        id: id.to_string(),
        title: String::new(),
        status: String::new(),
        price: None,
        currency: None,
        stock: None,
        sku: None,
    }
}

fn fold_snaps(created: &Created) -> Option<(ListingStatus, Option<String>)> {
    if created.snaps.len() != created.ids.len() || created.snaps.iter().any(|snap| snap.status.is_none()) {
        return None;
    }
    let mapped: Vec<_> = created
        .snaps
        .iter()
        .map(|snap| map_item_status(snap.status.as_deref().unwrap_or(""), &snap.sub_status, &snap.tags))
        .collect();
    if mapped.iter().all(|(status, _)| *status == ListingStatus::Live) {
        return Some((ListingStatus::Live, None));
    }
    mapped.into_iter().find(|(status, _)| *status != ListingStatus::Live)
}

fn snap_of(body: &Value) -> Snap {
    Snap {
        status: text(body, "status"),
        sub_status: string_list(body.get("sub_status")),
        tags: string_list(body.get("tags")),
    }
}

fn seller_sku(item: &Value) -> Option<String> {
    item.get("attributes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|attribute| text(attribute, "id").as_deref() == Some("SELLER_SKU"))
        .and_then(|attribute| text(attribute, "value_name"))
}

fn next_cursor(paging: Option<&Value>, offset: i64, page_len: usize) -> Option<String> {
    let total = paging.and_then(|value| value.get("total")).and_then(as_i64);
    let limit = paging.and_then(|value| value.get("limit")).and_then(as_i64).unwrap_or(PAGE);
    match total {
        Some(total) if offset + page_len as i64 >= total || page_len == 0 => None,
        Some(_) => Some((offset + page_len as i64).to_string()),
        None if page_len as i64 >= limit && page_len > 0 => Some((offset + page_len as i64).to_string()),
        None => None,
    }
}

fn parse_offset(cursor: Option<&str>) -> Result<i64, CommerceError> {
    match cursor.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(0),
        Some(value) => value.parse::<i64>().map_err(|_| CommerceError::invalid("分页游标无效")).and_then(|offset| {
            if offset < 0 {
                Err(CommerceError::invalid("分页游标无效"))
            } else {
                Ok(offset)
            }
        }),
    }
}

fn parse_order_cursor(cursor: Option<&str>, sellers: usize) -> Result<(usize, i64), CommerceError> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((0, 0));
    };
    if let Some((index, offset)) = cursor.split_once(':') {
        let index = index.parse::<usize>().map_err(|_| CommerceError::invalid("分页游标无效"))?;
        let offset = parse_offset(Some(offset))?;
        if sellers > 0 && index >= sellers {
            return Err(CommerceError::invalid("分页游标无效"));
        }
        return Ok((index, offset));
    }
    Ok((0, parse_offset(Some(cursor))?))
}

async fn list_orders(
    http: &SharedTransport,
    session: &ApiSession,
    account: &Account,
    start: i64,
    end: i64,
) -> Result<Vec<OrderHit>, CallFail> {
    let mut hits = Vec::new();
    for seller in account.order_sellers() {
        let mut offset = 0_i64;
        for page in 0..MAX_PAGES {
            let body = search_orders(http, session, &seller, start, end, offset, &account.site_id).await?;
            let rows = order_hits(&body);
            let page_len = rows.len();
            hits.extend(rows);
            match next_cursor(body.get("paging"), offset, page_len) {
                Some(next) => {
                    if page + 1 == MAX_PAGES {
                        return Err(CallFail::Failed("订单分页未在单次汇总内取完".into()));
                    }
                    offset = next.parse().unwrap_or(offset + PAGE);
                }
                None => break,
            }
        }
    }
    Ok(hits)
}

fn order_hits(body: &Value) -> Vec<OrderHit> {
    body.get("results").and_then(Value::as_array).into_iter().flatten().map(order_hit).collect()
}

fn order_hit(row: &Value) -> OrderHit {
    let tags = row.get("tags").and_then(Value::as_array).map(|tags| {
        tags.iter().filter_map(|tag| tag.as_str().map(str::to_owned)).collect::<Vec<_>>()
    });
    let pending = tags.as_ref().map(|tags| {
        tags.iter().any(|tag| tag == "not_delivered") && tags.iter().all(|tag| tag != "delivered")
    });
    OrderHit {
        status: text(row, "status").unwrap_or_default(),
        amount: row.get("total_amount").and_then(as_f64),
        currency: text(row, "currency_id"),
        buyer: row.get("buyer").and_then(|buyer| identifier(buyer, "id")),
        pending,
    }
}

async fn search_orders(
    http: &SharedTransport,
    session: &ApiSession,
    seller: &str,
    start: i64,
    end: i64,
    offset: i64,
    site: &str,
) -> Result<Value, CallFail> {
    let offset_secs = site_offset_secs(site);
    call(
        http,
        session,
        "GET",
        "/orders/search".into(),
        vec![
            ("seller".into(), seller.to_string()),
            ("order.date_created.from".into(), format_from(start, offset_secs)),
            ("order.date_created.to".into(), format_to(end, offset_secs)),
            ("offset".into(), offset.to_string()),
            ("limit".into(), PAGE.to_string()),
            ("sort".into(), "date_asc".into()),
        ],
        None,
        None,
        false,
    )
    .await
}

async fn live_count(http: &SharedTransport, session: &ApiSession, account: &Account) -> Result<f64, CallFail> {
    let body = call(
        http,
        session,
        "GET",
        format!("/users/{}/items/search", account.seller_id),
        vec![("status".into(), "active".into()), ("limit".into(), "1".into()), ("offset".into(), "0".into())],
        None,
        None,
        false,
    )
    .await?;
    body.get("paging")
        .and_then(|paging| paging.get("total"))
        .and_then(as_f64)
        .ok_or_else(|| CallFail::Failed("在售商品数没有 paging.total".into()))
}

fn preflight(draft: &ListingDraft, target: &ListingTarget, images: &[(String, Vec<u8>)]) -> Option<String> {
    if draft.title.trim().is_empty() {
        return Some("缺少标题".into());
    }
    if target.category_id.trim().is_empty() {
        return Some("缺少类目".into());
    }
    if images.is_empty() || images.iter().any(|(_, bytes)| bytes.is_empty()) {
        return Some("上架至少需要一张图片".into());
    }
    if draft.skus.len() > 1 && draft.skus.iter().any(|sku| sku.name.trim().is_empty()) {
        return Some("多 SKU 缺少规格名".into());
    }
    None
}

async fn prepare(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
) -> Result<Prepared, CallFail> {
    let account = load_account(http, session).await?;
    let category = call(
        http,
        session,
        "GET",
        format!("/categories/{}", target.category_id),
        Vec::new(),
        None,
        None,
        false,
    )
    .await?;
    if category.get("settings").and_then(|settings| settings.get("listing_allowed")).and_then(Value::as_bool) == Some(false)
    {
        return Err(CallFail::Rejected("类目不允许发布".into()));
    }
    if let Some(status) = category.get("settings").and_then(|settings| text(settings, "status")) {
        if status != "enabled" {
            return Err(CallFail::Rejected(format!("类目状态 {status} 不可发布")));
        }
    }
    let currencies = category
        .get("settings")
        .and_then(|settings| settings.get("currencies"))
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect::<Vec<_>>())
        .unwrap_or_default();
    let variation_attribute = if draft.skus.len() > 1 {
        let attributes = call(
            http,
            session,
            "GET",
            format!("/categories/{}/attributes", target.category_id),
            Vec::new(),
            None,
            None,
            false,
        )
        .await?;
        variation_attribute_id(&attributes)
    } else {
        None
    };
    if account.fully_managed() {
        return Ok(Prepared { account, currency: String::new(), variation_attribute, price: None, stock: 0 });
    }
    let currency = choose_currency(draft, &account, &currencies)?;
    let prices = sku_prices(draft);
    if prices.iter().any(Option::is_none) {
        return Err(CallFail::Rejected("缺少价格".into()));
    }
    let stocks = sku_stocks(draft);
    if stocks.iter().any(Option::is_none) {
        return Err(CallFail::Rejected("缺少库存".into()));
    }
    if !account.user_product && prices.len() > 1 {
        let first = prices[0];
        if prices.iter().any(|price| price != &first) {
            return Err(CallFail::Rejected("当前账号仍用一条商品的变体，各 SKU 价格必须相同".into()));
        }
    }
    Ok(Prepared {
        price: prices.first().copied().flatten(),
        stock: stocks.iter().copied().flatten().sum(),
        account,
        currency,
        variation_attribute,
    })
}

fn choose_currency(draft: &ListingDraft, account: &Account, currencies: &[String]) -> Result<String, CallFail> {
    if account.global() {
        if draft.currency.as_deref().is_some_and(|currency| currency != "USD") {
            return Err(CallFail::Rejected("全球销售价格币种只能是 USD".into()));
        }
        return Ok("USD".into());
    }
    if let Some(currency) = draft.currency.clone().filter(|value| !value.trim().is_empty()) {
        if !currencies.is_empty() && !currencies.iter().any(|allowed| allowed == &currency) {
            return Err(CallFail::Rejected(format!("类目不接受币种 {currency}")));
        }
        return Ok(currency);
    }
    match currencies {
        [only] => Ok(only.clone()),
        [] => Err(CallFail::Rejected("类目未返回可用币种".into())),
        _ => Err(CallFail::Rejected("类目有多个币种，请指定币种".into())),
    }
}

fn sku_prices(draft: &ListingDraft) -> Vec<Option<f64>> {
    if draft.skus.is_empty() {
        vec![draft.price]
    } else {
        draft.skus.iter().map(|sku| sku.price.or(draft.price)).collect()
    }
}

fn sku_stocks(draft: &ListingDraft) -> Vec<Option<i64>> {
    if draft.skus.is_empty() {
        vec![draft.stock]
    } else {
        draft.skus.iter().map(|sku| sku.stock.or(draft.stock)).collect()
    }
}

fn variation_attribute_id(body: &Value) -> Option<String> {
    attribute_rows(body)
        .into_iter()
        .find(|row| tag_flag(row.get("tags").unwrap_or(&Value::Null), "allow_variations"))
        .and_then(|row| text(&row, "id"))
}

async fn upload_picture(http: &SharedTransport, session: &ApiSession, name: &str, bytes: &[u8]) -> Result<String, CallFail> {
    let body = call(
        http,
        session,
        "POST",
        "/pictures/items/upload".into(),
        Vec::new(),
        None,
        Some((name, bytes)),
        false,
    )
    .await?;
    text(&body, "id").ok_or_else(|| CallFail::Rejected("Mercado Libre 未返回图片 ID".into()))
}

async fn create_items(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
) -> Result<Created, CallFail> {
    if prepared.account.fully_managed() {
        return create_family(http, session, draft, target, prepared, picture_ids, true).await;
    }
    if prepared.account.user_product {
        return create_family(http, session, draft, target, prepared, picture_ids, false).await;
    }
    let body = if prepared.account.global() {
        global_body(draft, target, prepared, picture_ids)
    } else {
        local_body(draft, target, prepared, picture_ids)
    };
    let path = if prepared.account.global() { "/global/items" } else { "/items" };
    let created = call(http, session, "POST", path.into(), Vec::new(), Some(body), None, true).await?;
    let snap = snap_of(&created);
    let id = text(&created, "item_id")
        .or_else(|| text(&created, "id"))
        .ok_or_else(|| CallFail::Rejected("Mercado Libre 未返回商品 ID".into()))?;
    Ok(Created { ids: vec![id], snaps: vec![snap] })
}

async fn create_family(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
    fully_managed: bool,
) -> Result<Created, CallFail> {
    let units: Vec<Option<&ListingSku>> = if draft.skus.is_empty() {
        vec![None]
    } else {
        draft.skus.iter().map(Some).collect()
    };
    let mut created_ids = Vec::new();
    let mut snaps = Vec::new();
    for sku in units {
        let body = if fully_managed {
            managed_body(draft, target, prepared, picture_ids, sku)
        } else if prepared.account.global() {
            global_family_body(draft, target, prepared, picture_ids, sku)
        } else {
            local_family_body(draft, target, prepared, picture_ids, sku)
        };
        let path = if prepared.account.global() || fully_managed { "/global/items" } else { "/items" };
        let created = match call(http, session, "POST", path.into(), Vec::new(), Some(body), None, true).await {
            Ok(created) => created,
            Err(error) if created_ids.is_empty() => return Err(error),
            Err(error) => return Err(promote_uncertain(error, &created_ids)),
        };
        let snap = snap_of(&created);
        let id = match text(&created, "item_id").or_else(|| text(&created, "id")) {
            Some(id) => id,
            None if created_ids.is_empty() => return Err(CallFail::Rejected("Mercado Libre 未返回商品 ID".into())),
            None => {
                return Err(CallFail::Uncertain(format!("后续 SKU 未返回商品 ID，已创建 {}", created_ids.join(","))));
            }
        };
        snaps.push(snap);
        created_ids.push(id);
    }
    Ok(Created { ids: created_ids, snaps })
}

fn promote_uncertain(error: CallFail, ids: &[String]) -> CallFail {
    CallFail::Uncertain(format!("{} 已创建 {}", error.text(), ids.join(",")))
}

fn local_body(draft: &ListingDraft, target: &ListingTarget, prepared: &Prepared, picture_ids: &[String]) -> Value {
    let mut body = base_sale(draft, target, prepared, picture_ids);
    body.insert("title".into(), json!(draft.title));
    body.insert("listing_type_id".into(), json!("gold_special"));
    body.insert("condition".into(), json!("new"));
    if draft.skus.len() > 1 {
        body.insert("variations".into(), json!(legacy_variations(draft, prepared, picture_ids)));
    }
    Value::Object(body)
}

fn local_family_body(
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
    sku: Option<&ListingSku>,
) -> Value {
    let mut body = base_sale(draft, target, prepared, picture_ids);
    body.insert("family_name".into(), json!(draft.title));
    body.insert("listing_type_id".into(), json!("gold_special"));
    body.insert("condition".into(), json!("new"));
    body.remove("title");
    apply_sku(&mut body, prepared, sku);
    Value::Object(body)
}

fn global_body(draft: &ListingDraft, target: &ListingTarget, prepared: &Prepared, picture_ids: &[String]) -> Value {
    let mut body = global_common(draft, target, prepared, picture_ids);
    body.insert("title".into(), json!(draft.title));
    if draft.skus.len() > 1 {
        body.insert("variations".into(), json!(legacy_variations(draft, prepared, picture_ids)));
        if prepared.account.markets.iter().all(|market| market.net_proceeds) {
            body.remove("price");
        }
    }
    if let Some(description) = plain_description(draft) {
        body.insert("description".into(), json!({"plain_text": description}));
    }
    Value::Object(body)
}

fn global_family_body(
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
    sku: Option<&ListingSku>,
) -> Value {
    let mut body = global_common(draft, target, prepared, picture_ids);
    body.insert("family_name".into(), json!(draft.title));
    body.remove("title");
    apply_sku(&mut body, prepared, sku);
    if prepared.account.markets.iter().all(|market| market.net_proceeds) {
        body.remove("price");
    }
    if let Some(description) = plain_description(draft) {
        body.insert("description".into(), json!({"plain_text": description}));
    }
    Value::Object(body)
}

fn managed_body(
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
    sku: Option<&ListingSku>,
) -> Value {
    let mut attributes = shared_attributes(draft, target, prepared.variation_attribute.as_deref());
    push_sku_attributes(&mut attributes, prepared, sku);
    let mut body = Map::new();
    body.insert("family_name".into(), json!(draft.title));
    body.insert("category_id".into(), json!(target.category_id));
    body.insert("pictures".into(), picture_refs(picture_ids));
    body.insert("attributes".into(), Value::Array(attributes));
    if let Some(description) = plain_description(draft) {
        body.insert("description".into(), json!({"plain_text": description}));
    }
    Value::Object(body)
}

fn global_common(
    draft: &ListingDraft,
    target: &ListingTarget,
    prepared: &Prepared,
    picture_ids: &[String],
) -> Map<String, Value> {
    let mut body = base_sale(draft, target, prepared, picture_ids);
    body.insert("currency_id".into(), json!("USD"));
    body.insert("catalog_listing".into(), json!(false));
    body.insert("sites_to_sell".into(), sites_to_sell(draft, prepared));
    body.remove("buying_mode");
    if prepared.account.markets.iter().all(|market| market.net_proceeds) {
        body.remove("price");
    }
    body
}

fn sites_to_sell(draft: &ListingDraft, prepared: &Prepared) -> Value {
    let price = prepared.price.unwrap_or(0.0);
    Value::Array(
        prepared
            .account
            .markets
            .iter()
            .map(|market| {
                let mut site = json!({
                    "site_id": market.site_id,
                    "logistic_type": market.logistic_type,
                    "title": draft.title,
                });
                if market.logistic_type == "fulfillment" || !market.net_proceeds {
                    site["price"] = json!(price);
                } else {
                    site["net_proceeds"] = json!(price);
                }
                site
            })
            .collect(),
    )
}

fn base_sale(draft: &ListingDraft, target: &ListingTarget, prepared: &Prepared, picture_ids: &[String]) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert("category_id".into(), json!(target.category_id));
    body.insert("buying_mode".into(), json!("buy_it_now"));
    body.insert("currency_id".into(), json!(prepared.currency));
    if let Some(price) = prepared.price {
        body.insert("price".into(), json!(price));
    }
    body.insert("available_quantity".into(), json!(prepared.stock));
    body.insert("pictures".into(), picture_refs(picture_ids));
    let mut attributes = shared_attributes(
        draft,
        target,
        if draft.skus.len() > 1 { prepared.variation_attribute.as_deref() } else { None },
    );
    if draft.skus.len() <= 1 {
        if let Some(code) = draft.skus.first().and_then(|sku| sku.code.clone()).filter(|code| !code.trim().is_empty()) {
            attributes.push(json!({"id": "SELLER_SKU", "value_name": code}));
        }
    }
    if !attributes.is_empty() {
        body.insert("attributes".into(), Value::Array(attributes));
    }
    body
}

fn picture_refs(picture_ids: &[String]) -> Value {
    json!(picture_ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>())
}

fn apply_sku(body: &mut Map<String, Value>, prepared: &Prepared, sku: Option<&ListingSku>) {
    let price = sku.and_then(|sku| sku.price).or(prepared.price);
    let stock = sku.and_then(|sku| sku.stock).or(Some(prepared.stock));
    if let Some(price) = price {
        body.insert("price".into(), json!(price));
    }
    if let Some(stock) = stock {
        body.insert("available_quantity".into(), json!(stock));
    }
    if prepared.account.global() {
        if let Some(price) = price {
            if let Some(Value::Array(sites)) = body.get_mut("sites_to_sell") {
                for site in sites {
                    let fulfillment = site.get("logistic_type").and_then(Value::as_str) == Some("fulfillment");
                    let net = site.get("net_proceeds").is_some() && !fulfillment;
                    if net {
                        site["net_proceeds"] = json!(price);
                        if let Some(map) = site.as_object_mut() {
                            map.remove("price");
                        }
                    } else {
                        site["price"] = json!(price);
                    }
                }
            }
        }
    }
    let mut attributes = body.get("attributes").and_then(Value::as_array).cloned().unwrap_or_default();
    attributes.retain(|attribute| text(attribute, "id").as_deref() != Some("SELLER_SKU"));
    push_sku_attributes(&mut attributes, prepared, sku);
    body.insert("attributes".into(), Value::Array(attributes));
}

fn push_sku_attributes(attributes: &mut Vec<Value>, prepared: &Prepared, sku: Option<&ListingSku>) {
    let Some(sku) = sku else {
        return;
    };
    if !sku.name.trim().is_empty() {
        attributes.push(match prepared.variation_attribute.as_deref() {
            Some(id) => json!({"id": id, "value_name": sku.name}),
            None => json!({"name": "Variante", "value_name": sku.name}),
        });
    }
    if let Some(code) = sku.code.clone().filter(|code| !code.trim().is_empty()) {
        attributes.push(json!({"id": "SELLER_SKU", "value_name": code}));
    }
}

fn legacy_variations(draft: &ListingDraft, prepared: &Prepared, picture_ids: &[String]) -> Vec<Value> {
    let net_only = prepared.account.global() && prepared.account.markets.iter().all(|market| market.net_proceeds);
    draft
        .skus
        .iter()
        .map(|sku| {
            let mut variation = Map::new();
            variation.insert(
                "attribute_combinations".into(),
                json!([match prepared.variation_attribute.as_deref() {
                    Some(id) => json!({"id": id, "value_name": sku.name}),
                    None => json!({"name": "Variante", "value_name": sku.name}),
                }]),
            );
            variation.insert("available_quantity".into(), json!(sku.stock.or(draft.stock).unwrap_or(0)));
            if !net_only {
                variation.insert("price".into(), json!(sku.price.or(draft.price).unwrap_or(0.0)));
            }
            variation.insert("picture_ids".into(), json!(picture_ids));
            if let Some(code) = sku.code.clone().filter(|code| !code.trim().is_empty()) {
                variation.insert("attributes".into(), json!([{"id": "SELLER_SKU", "value_name": code}]));
            }
            Value::Object(variation)
        })
        .collect()
}

fn shared_attributes(draft: &ListingDraft, target: &ListingTarget, variation_attribute: Option<&str>) -> Vec<Value> {
    let mut attributes = vec![json!({"id": "ITEM_CONDITION", "value_id": NEW_CONDITION})];
    if let Some(brand) = draft.brand.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        attributes.push(json!({"id": "BRAND", "value_name": brand}));
    }
    if let Some(dimensions) = &draft.dimensions_cm {
        attributes.push(json!({"id": "PACKAGE_LENGTH", "value_name": measure(dimensions.l, "cm")}));
        attributes.push(json!({"id": "PACKAGE_WIDTH", "value_name": measure(dimensions.w, "cm")}));
        attributes.push(json!({"id": "PACKAGE_HEIGHT", "value_name": measure(dimensions.h, "cm")}));
    }
    if let Some(weight) = draft.weight_kg {
        attributes.push(json!({"id": "PACKAGE_WEIGHT", "value_name": measure(weight, "kg")}));
    }
    for attribute in &target.attributes {
        if attribute.id.trim().is_empty() || attribute.id == "SELLER_SKU" || variation_attribute == Some(attribute.id.as_str())
        {
            continue;
        }
        if let Some(value) = attribute_value(attribute) {
            attributes.push(value);
        }
    }
    attributes
}

fn attribute_value(attribute: &ListingAttribute) -> Option<Value> {
    let mut values = if attribute.values.is_empty() { vec![attribute.value.clone()] } else { attribute.values.clone() };
    values.retain(|value| !value.trim().is_empty());
    if values.is_empty() {
        return None;
    }
    if values.len() == 1 {
        Some(json!({"id": attribute.id, "value_name": values.remove(0)}))
    } else {
        let listed: Vec<Value> = values.into_iter().map(|name| json!({"name": name})).collect();
        Some(json!({"id": attribute.id, "values": listed}))
    }
}

fn measure(value: f64, unit: &str) -> String {
    let text = if (value - value.round()).abs() < f64::EPSILON {
        format!("{}", value.round() as i64)
    } else {
        let raw = format!("{value:.3}");
        raw.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    format!("{text} {unit}")
}

fn plain_description(draft: &ListingDraft) -> Option<String> {
    draft.description.clone().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

async fn post_description(http: &SharedTransport, session: &ApiSession, item_id: &str, text: &str) -> Result<(), CallFail> {
    match call(
        http,
        session,
        "POST",
        format!("/items/{item_id}/description"),
        Vec::new(),
        Some(json!({"plain_text": text})),
        None,
        true,
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(CallFail::Rejected(message)) if message.to_ascii_lowercase().contains("already") => Ok(()),
        Err(error) => Err(error),
    }
}

async fn resolve_site(http: &SharedTransport, session: &ApiSession) -> Result<String, CallFail> {
    if let Some(site) = session.region.as_deref().and_then(normalize_site) {
        return Ok(site);
    }
    Ok(load_account(http, session).await?.site_id)
}

async fn load_account(http: &SharedTransport, session: &ApiSession) -> Result<Account, CallFail> {
    let user = call(http, session, "GET", format!("/users/{}", session.remote_id), Vec::new(), None, None, false).await?;
    let site_id = text(&user, "site_id")
        .and_then(|site| normalize_site(&site))
        .or_else(|| session.region.as_deref().and_then(normalize_site))
        .ok_or_else(|| CallFail::Rejected("无法确定 Mercado Libre 站点".into()))?;
    let user_product = string_list(user.get("tags")).iter().any(|tag| tag == "user_product_seller");
    let markets = if site_id == "CBT" { load_markets(http, session).await? } else { Vec::new() };
    Ok(Account { site_id, user_product, markets, seller_id: session.remote_id.clone() })
}

async fn load_markets(http: &SharedTransport, session: &ApiSession) -> Result<Vec<Market>, CallFail> {
    let body = call(
        http,
        session,
        "GET",
        format!("/marketplace/users/{}", session.remote_id),
        Vec::new(),
        None,
        None,
        false,
    )
    .await?;
    let markets = body
        .get("marketplaces")
        .and_then(Value::as_array)
        .ok_or_else(|| CallFail::Rejected("全球销售账号没有 marketplace 配置".into()))?
        .iter()
        .map(|market| {
            let site_id = text(market, "site_id").ok_or_else(|| CallFail::Rejected("站点未返回 site_id".into()))?;
            let logistic_type =
                text(market, "logistic_type").ok_or_else(|| CallFail::Rejected(format!("站点 {site_id} 未返回物流类型")))?;
            let business = text(market, "business_model").unwrap_or_default();
            Ok(Market {
                user_id: identifier(market, "user_id").unwrap_or_else(|| session.remote_id.clone()),
                site_id,
                net_proceeds: text(market, "pricing_model").as_deref() == Some("net_proceeds") && logistic_type != "fulfillment",
                fully_managed: business == FULLY_MANAGED,
                logistic_type,
            })
        })
        .collect::<Result<Vec<_>, CallFail>>()?;
    if markets.is_empty() {
        return Err(CallFail::Rejected("全球销售账号没有可发布的站点".into()));
    }
    Ok(markets)
}

impl Account {
    fn global(&self) -> bool {
        self.site_id == "CBT"
    }

    fn fully_managed(&self) -> bool {
        self.markets.iter().any(|market| market.fully_managed)
    }

    fn order_sellers(&self) -> Vec<String> {
        if self.global() {
            self.markets.iter().map(|market| market.user_id.clone()).collect()
        } else {
            vec![self.seller_id.clone()]
        }
    }
}

async fn call(
    http: &SharedTransport,
    session: &ApiSession,
    method: &'static str,
    path: String,
    query: Vec<(String, String)>,
    json_body: Option<Value>,
    file: Option<(&str, &[u8])>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let body = match (json_body, file) {
        (_, Some((name, bytes))) => HttpBody::Multipart(MultipartFile {
            field: "file".into(),
            file_name: name.to_string(),
            bytes: bytes.to_vec(),
        }),
        (Some(value), None) => HttpBody::Json(value),
        (None, None) => HttpBody::Empty,
    };
    let request = HttpRequest {
        method,
        url: format!("{HOST}{path}"),
        headers: vec![("Authorization".into(), format!("Bearer {}", session.access_token))],
        query,
        body,
        mutating,
    };
    let response = match http.send(request).await {
        Ok(response) => response,
        Err(TransportFault::BeforeDispatch(message)) => return Err(CallFail::Failed(message)),
        Err(TransportFault::AfterDispatch(message)) if mutating => return Err(CallFail::Uncertain(message)),
        Err(TransportFault::AfterDispatch(message)) => return Err(CallFail::Failed(message)),
    };
    classify(mutating, response.status, response.body)
}

fn classify(mutating: bool, http_status: u16, body: Value) -> Result<Value, CallFail> {
    if body.is_array() {
        if http_status >= 500 {
            let message = format!("HTTP {http_status}");
            return if mutating { Err(CallFail::Uncertain(message)) } else { Err(CallFail::Failed(message)) };
        }
        if http_status >= 400 {
            let message = format!("HTTP {http_status}");
            return Err(if http_status == 401 { CallFail::Auth(message) } else { CallFail::Rejected(message) });
        }
        return Ok(body);
    }
    let status = if http_status >= 400 {
        http_status
    } else {
        body.get("status").and_then(Value::as_u64).map(|value| value as u16).unwrap_or(http_status)
    };
    let error = error_code(&body);
    let message = text(&body, "message").unwrap_or_default();
    let cause = body
        .get("cause")
        .and_then(Value::as_array)
        .map(|causes| {
            causes.iter().filter_map(|cause| text(cause, "message").or_else(|| text(cause, "code"))).collect::<Vec<_>>().join("; ")
        })
        .unwrap_or_default();
    let joined = [error.as_str(), message.as_str(), cause.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(": ");
    let joined = if joined.is_empty() { format!("HTTP {status}") } else { joined };
    if status >= 500 {
        return if mutating { Err(CallFail::Uncertain(joined)) } else { Err(CallFail::Failed(joined)) };
    }
    if status >= 400 || !error.is_empty() {
        if status == 401 || is_token(&joined) {
            return Err(CallFail::Auth(joined));
        }
        return Err(CallFail::Rejected(joined));
    }
    Ok(body)
}

fn error_code(body: &Value) -> String {
    match body.get("error") {
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        Some(Value::Object(map)) => map.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

fn is_token(text: &str) -> bool {
    let blob = text.to_ascii_lowercase();
    blob.contains("invalid_token")
        || blob.contains("invalid access token")
        || blob.contains("expired_token")
        || blob.contains("unauthorized")
        || blob.contains("not_identified_user")
}

fn ensure_platform(session: &ApiSession) -> Result<(), CommerceError> {
    if session.platform == Platform::Mercadolibre {
        Ok(())
    } else {
        Err(CommerceError::unsupported("该平台尚未接入"))
    }
}

fn fail_to_error(error: CallFail) -> CommerceError {
    match error {
        CallFail::Auth(message) | CallFail::Rejected(message) => CommerceError::rejected(message),
        CallFail::Failed(message) => CommerceError::failed(message),
        CallFail::Uncertain(message) => CommerceError::uncertain(message),
    }
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

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).filter(|item| !item.is_empty()).map(str::to_owned)
}

fn identifier(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(identifier_value)
}

fn identifier_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .or_else(|| value.as_u64().map(|item| item.to_string()))
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned).or_else(|| item.as_i64().map(|id| id.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|item| item as f64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|item| item as i64)).or_else(|| value.as_f64().map(|item| item as i64))
}
