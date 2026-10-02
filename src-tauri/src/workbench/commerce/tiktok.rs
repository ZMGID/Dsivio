//! TikTok Shop seller adapter (not content posting).
//!
//! Signing follows [Sign your API request](https://partner.tiktokshop.com/docv2/page/sign-your-api-request):
//! HMAC-SHA256 of `app_secret + path + sorted{key}{value} + json body + app_secret`,
//! excluding `sign` and `access_token`. Multipart bodies are not signed.
//! `shops::tiktok_sign` is private and only covers `app_key` plus `timestamp`, so shop
//! calls that send `shop_cipher` or a JSON body cannot reuse it. Tokens stay in `shops`.
//!
//! Access token goes in `x-tts-access-token` ([common parameters](https://partner.tiktokshop.com/docv2/page/common-parameters)).
//! `shop_cipher` comes from [Get Authorized Shops](https://partner.tiktokshop.com/docv2/page/get-authorized-shops-202309)
//! by matching the stored shop id. It is not a second credential store.
//!
//! Endpoints, versions current in the partner API reference on 2026-10-02:
//! - categories: GET `/product/202309/categories` — <https://partner.tiktokshop.com/docv2/page/get-categories-202309>
//! - attributes: GET `/product/202309/categories/{category_id}/attributes` — <https://partner.tiktokshop.com/docv2/page/get-attributes-202309>
//! - rules: GET `/product/202309/categories/{category_id}/rules` — <https://partner.tiktokshop.com/docv2/page/get-category-rules-202309>
//! - image: POST `/product/202309/images/upload` multipart field `data` (`x-dsivio-file-field`) — <https://partner.tiktokshop.com/docv2/page/upload-product-image-202309>
//! - create: POST `/product/202309/products` — <https://partner.tiktokshop.com/docv2/page/create-product-202309>
//! - product: GET `/product/202309/products/{product_id}` — <https://partner.tiktokshop.com/docv2/page/get-product-202309>
//! - search products: POST `/product/202502/products/search` — <https://partner.tiktokshop.com/docv2/page/search-products-202502>
//! - warehouses: GET `/logistics/202309/warehouses` — <https://partner.tiktokshop.com/docv2/page/get-warehouse-list-202309>
//! - orders: POST `/order/202309/orders/search` — <https://partner.tiktokshop.com/docv2/page/get-order-list-202309>
//! - performance: GET `/analytics/202609/shop/performance` — <https://partner.tiktokshop.com/docv2/page/get-shop-performance-202609>
//!
//! Gateway errors: <https://partner.tiktokshop.com/docv2/page/common-errors>.

use super::transport::{Outbound, PushOutcome, ResolvedShop, Transport, TransportFault};
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities, CommerceError, CommerceOrder,
    CommerceOrderLine, CommerceProduct, ListingAttribute, ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey,
    MetricRange, OrderPage, ProductPage, ShopMetrics,
};
use crate::workbench::shops::Platform;
use chrono::{DateTime, Days, NaiveDate};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const HOST: &str = "https://open-api.tiktokglobalshop.com";
const SHOPS: &str = "/authorization/202309/shops";
const CATEGORIES: &str = "/product/202309/categories";
const UPLOAD: &str = "/product/202309/images/upload";
const CREATE: &str = "/product/202309/products";
const SEARCH_PRODUCTS: &str = "/product/202502/products/search";
const WAREHOUSES: &str = "/logistics/202309/warehouses";
const SEARCH_ORDERS: &str = "/order/202309/orders/search";
const PERFORMANCE: &str = "/analytics/202609/shop/performance";
const PAGE_SIZE: i64 = 20;

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

pub fn tiktok_capabilities() -> CommerceCapabilities {
    CommerceCapabilities {
        platform: Platform::Tiktok,
        metrics: vec![
            MetricKey::Gmv,
            MetricKey::Orders,
            MetricKey::RefundAmount,
            MetricKey::Buyers,
            MetricKey::ProductsLive,
            MetricKey::PendingShipment,
        ],
        listing: true,
        categories: true,
        products: true,
        orders: true,
        notes: vec![
            "成交额、订单、退款金额和买家来自 Get Shop Performance 202609，日期按店铺地区的固定时区窗口。".into(),
            "在售商品数是 Search Products 202502 里 status=ACTIVATE 的 total_count。".into(),
            "待发货是 Get Order List 202309 里 AWAITING_SHIPMENT 且落在同一窗口的 total_count。".into(),
            "退款订单数不支持：店铺表现只有 cancellations_and_returns，取消和退货合在一起。".into(),
            "商品和订单用 page_token 分页，每页 20 条。订单按指标窗口过滤 create_time_ge 和 create_time_lt。".into(),
            "金额不跨币种相加。接口没有的指标放进 unsupported，不写成 0。".into(),
        ],
    }
}

pub fn sign_request(secret: &str, path: &str, query: &[(String, String)], body: Option<&str>) -> Result<String, String> {
    let pairs: Vec<(&str, &str)> = query.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
    crate::workbench::shops::sign_tiktok(secret, path, &pairs, body)
}

fn ensure_tiktok(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform == Platform::Tiktok {
        Ok(())
    } else {
        Err(CommerceError::unsupported("该平台尚未接入"))
    }
}

fn category_version(region: Option<&str>) -> &'static str {
    match region_code(region) {
        "US" | "ID" | "MY" | "PH" | "SG" | "TH" | "VN" | "DE" | "ES" | "FR" | "IE" | "IT" => "v2",
        _ => "v1",
    }
}

fn region_code(region: Option<&str>) -> &str {
    region.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("")
}

fn region_offset_secs(region: Option<&str>) -> i64 {
    match region_code(region).to_ascii_uppercase().as_str() {
        "DE" | "FR" | "ES" | "IT" | "IE" | "NL" | "PL" | "AT" | "BE" => 3600,
        "BR" => -3 * 3600,
        "US" => -5 * 3600,
        "MX" => -6 * 3600,
        "ID" | "TH" | "VN" => 7 * 3600,
        "CN" | "MY" | "PH" | "SG" | "TW" => 8 * 3600,
        "JP" => 9 * 3600,
        _ => 0,
    }
}

fn region_currency(region: Option<&str>) -> Option<&'static str> {
    Some(match region_code(region).to_ascii_uppercase().as_str() {
        "US" => "USD",
        "GB" | "UK" => "GBP",
        "BR" => "BRL",
        "DE" | "FR" | "IE" | "IT" | "ES" | "AT" | "BE" | "NL" | "PT" => "EUR",
        "ID" => "IDR",
        "JP" => "JPY",
        "MX" => "MXN",
        "MY" => "MYR",
        "PH" => "PHP",
        "SG" => "SGD",
        "TH" => "THB",
        "VN" => "VND",
        _ => return None,
    })
}

fn title_bounds(region: Option<&str>) -> (usize, usize) {
    match region_code(region).to_ascii_uppercase().as_str() {
        "BR" | "MX" => (1, 300),
        "DE" | "ES" | "FR" | "IE" | "IT" | "JP" | "UK" | "GB" | "US" => (1, 255),
        _ => (25, 255),
    }
}

fn local_date(now: i64, offset: i64) -> NaiveDate {
    DateTime::from_timestamp(now + offset, 0).map(|time| time.date_naive()).unwrap_or(NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"))
}

fn shift_date(date: NaiveDate, days: i64) -> NaiveDate {
    if days >= 0 {
        date.checked_add_days(Days::new(days as u64)).unwrap_or(date)
    } else {
        date.checked_sub_days(Days::new((-days) as u64)).unwrap_or(date)
    }
}

fn performance_dates(range: MetricRange, now: i64, offset: i64) -> (String, String) {
    let today = local_date(now, offset);
    let (start, end) = match range {
        MetricRange::Today => (today, shift_date(today, 1)),
        MetricRange::Yesterday => (shift_date(today, -1), today),
        MetricRange::Last7 => (shift_date(today, -6), shift_date(today, 1)),
        MetricRange::Last30 => (shift_date(today, -29), shift_date(today, 1)),
    };
    (start.format("%Y-%m-%d").to_string(), end.format("%Y-%m-%d").to_string())
}

fn unix_window(range: MetricRange, now: i64, offset: i64) -> (i64, i64) {
    let today = local_date(now, offset);
    let (start, end) = match range {
        MetricRange::Today => (today, shift_date(today, 1)),
        MetricRange::Yesterday => (shift_date(today, -1), today),
        MetricRange::Last7 => (shift_date(today, -6), shift_date(today, 1)),
        MetricRange::Last30 => (shift_date(today, -29), shift_date(today, 1)),
    };
    let start_utc = start.and_hms_opt(0, 0, 0).and_then(|time| time.and_utc().timestamp().checked_sub(offset));
    let end_utc = end.and_hms_opt(0, 0, 0).and_then(|time| time.and_utc().timestamp().checked_sub(offset));
    (start_utc.unwrap_or(now), end_utc.unwrap_or(now))
}

pub async fn fetch_metrics(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    if shop.platform != Platform::Tiktok {
        return Err(CommerceError::unsupported("该平台尚未接入店铺指标"));
    }
    let cipher = shop_cipher(transport, shop, now).await.map_err(fail_to_error)?;
    let offset = region_offset_secs(shop.region.as_deref());
    let (start_date, end_date) = performance_dates(range, now, offset);
    let (window_start, window_end) = unix_window(range, now, offset);
    let performance = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "GET",
        PERFORMANCE,
        vec![
            ("start_date_ge".into(), start_date),
            ("end_date_lt".into(), end_date),
            ("granularity".into(), "ALL".into()),
            ("currency".into(), "LOCAL".into()),
        ],
        None,
    )
    .await;
    let mut unsupported = vec![MetricKey::RefundOrders];
    let mut values = BTreeMap::new();
    let mut currency = None;
    let mut errors = Vec::new();
    match performance {
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(error) => {
            unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::RefundAmount, MetricKey::Buyers]);
            errors.push(error.text().to_string());
        }
        Ok(body) => fold_performance(&body, &mut values, &mut currency, &mut unsupported),
    }
    match counted(
        transport,
        shop,
        now,
        &cipher,
        "POST",
        SEARCH_PRODUCTS,
        vec![("page_size".into(), "1".into())],
        Some(json!({"status": "ACTIVATE"})),
    )
    .await
    {
        Ok(count) => {
            values.insert(MetricKey::ProductsLive, count);
        }
        Err(error) => {
            unsupported.push(MetricKey::ProductsLive);
            errors.push(error.text().to_string());
        }
    }
    match counted(
        transport,
        shop,
        now,
        &cipher,
        "POST",
        SEARCH_ORDERS,
        vec![("page_size".into(), "1".into())],
        Some(json!({
            "order_status": "AWAITING_SHIPMENT",
            "create_time_ge": window_start,
            "create_time_lt": window_end,
        })),
    )
    .await
    {
        Ok(count) => {
            values.insert(MetricKey::PendingShipment, count);
        }
        Err(error) => {
            unsupported.push(MetricKey::PendingShipment);
            errors.push(error.text().to_string());
        }
    }
    unsupported.sort();
    unsupported.dedup();
    let fetched_at = DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    Ok(ShopMetrics {
        shop_id: shop_id.to_string(),
        range,
        currency,
        values,
        unsupported,
        fetched_at,
        error: if errors.is_empty() { None } else { Some(errors.join("；")) },
    })
}

fn fold_performance(body: &Value, values: &mut BTreeMap<MetricKey, f64>, currency: &mut Option<String>, unsupported: &mut Vec<MetricKey>) {
    let intervals = body["data"]["performance"]["intervals"].as_array().cloned().unwrap_or_default();
    match sum_count(&intervals, "orders_count") {
        Some(count) => {
            values.insert(MetricKey::Orders, count);
        }
        None => unsupported.push(MetricKey::Orders),
    }
    match sum_count(&intervals, "customers_count") {
        Some(count) => {
            values.insert(MetricKey::Buyers, count);
        }
        None => unsupported.push(MetricKey::Buyers),
    }
    match sum_money(&intervals, "gmv") {
        Some((amount, code)) => {
            values.insert(MetricKey::Gmv, amount);
            *currency = Some(code);
        }
        None => unsupported.push(MetricKey::Gmv),
    }
    match sum_money(&intervals, "refunds") {
        Some((amount, code)) => {
            if currency.as_ref().is_some_and(|existing| existing != &code) {
                unsupported.push(MetricKey::RefundAmount);
            } else {
                if currency.is_none() {
                    *currency = Some(code);
                }
                values.insert(MetricKey::RefundAmount, amount);
            }
        }
        None => unsupported.push(MetricKey::RefundAmount),
    }
}

fn sum_count(intervals: &[Value], key: &str) -> Option<f64> {
    if intervals.is_empty() {
        return None;
    }
    let mut total = 0i64;
    for interval in intervals {
        total += interval.get("sales")?.get(key).and_then(as_i64)?;
    }
    Some(total as f64)
}

fn sum_money(intervals: &[Value], key: &str) -> Option<(f64, String)> {
    if intervals.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut currency = None;
    for interval in intervals {
        let money = interval.get("sales")?.get(key)?;
        let amount = money.get("amount").and_then(as_f64)?;
        let code = text(money, "currency")?;
        if let Some(existing) = &currency {
            if existing != &code {
                return None;
            }
        } else {
            currency = Some(code);
        }
        total += amount;
    }
    Some((total, currency?))
}

async fn counted(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    cipher: &str,
    method: &'static str,
    path: &str,
    query: Vec<(String, String)>,
    json_body: Option<Value>,
) -> Result<f64, CallFail> {
    let body = read(transport, shop, now, Some(cipher), method, path, query, json_body).await?;
    body["data"].get("total_count").and_then(as_f64).ok_or_else(|| CallFail::Failed("TikTok Shop 未返回 total_count".into()))
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure_tiktok(shop)?;
    let cipher = shop_cipher(transport, shop, now).await.map_err(fail_to_error)?;
    let body = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "GET",
        CATEGORIES,
        vec![
            ("locale".into(), "en-US".into()),
            ("category_version".into(), category_version(shop.region.as_deref()).into()),
        ],
        None,
    )
    .await
    .map_err(fail_to_error)?;
    let mut categories = map_categories(&body);
    if let Some(parent) = parent_id.filter(|value| !value.is_empty()) {
        categories.retain(|category| category.parent_id == parent);
    }
    Ok(categories)
}

pub fn map_categories(body: &Value) -> Vec<Category> {
    body["data"]
        .get("categories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "id")?;
            let parent_id = identifier(item, "parent_id").unwrap_or_else(|| "0".into());
            let name = text(item, "local_name").unwrap_or_else(|| id.clone());
            let leaf = item.get("is_leaf").and_then(Value::as_bool).unwrap_or(false);
            Some(Category { id, name, parent_id, leaf })
        })
        .collect()
}

pub async fn fetch_attributes(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure_tiktok(shop)?;
    if category_id.trim().is_empty() {
        return Err(CommerceError::invalid("缺少类目"));
    }
    let cipher = shop_cipher(transport, shop, now).await.map_err(fail_to_error)?;
    let body = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "GET",
        &format!("{CATEGORIES}/{category_id}/attributes"),
        vec![
            ("locale".into(), "en-US".into()),
            ("category_version".into(), category_version(shop.region.as_deref()).into()),
        ],
        None,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    body["data"]
        .get("attributes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_one_attribute)
        .collect()
}

fn map_one_attribute(node: &Value) -> Option<CategoryAttribute> {
    let id = identifier(node, "id")?;
    let name = text(node, "name").unwrap_or_else(|| id.clone());
    let required = node.get("is_requried").and_then(Value::as_bool).unwrap_or(false);
    let multiple = node.get("is_multiple_selection").and_then(Value::as_bool).unwrap_or(false);
    let options = node
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let name = text(value, "name")?;
                    Some(AttributeOption { id: identifier(value, "id").unwrap_or_default(), name })
                })
                .collect::<Vec<AttributeOption>>()
        })
        .unwrap_or_default();
    let numeric = text(node, "value_data_format").is_some_and(|format| format == "POSITIVE_INT_OR_DECIMAL");
    let input = if multiple {
        AttributeInput::MultiSelect
    } else if !options.is_empty() {
        AttributeInput::Select
    } else if numeric {
        AttributeInput::Number
    } else {
        AttributeInput::Text
    };
    Some(CategoryAttribute { id, name, required, input, options, unit: None })
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    ensure_tiktok(shop)?;
    let cipher = shop_cipher(transport, shop, now).await.map_err(fail_to_error)?;
    let mut query = vec![("page_size".into(), PAGE_SIZE.to_string())];
    if let Some(token) = cursor.filter(|value| !value.is_empty()) {
        query.push(("page_token".into(), token.to_string()));
    }
    let body = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "POST",
        SEARCH_PRODUCTS,
        query,
        Some(json!({"status": "ALL"})),
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_product_page(shop_id, &body))
}

fn map_product_page(shop_id: &str, body: &Value) -> ProductPage {
    let data = &body["data"];
    let items = data
        .get("products")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "id")?;
            let skus = item.get("skus").and_then(Value::as_array);
            let first = skus.and_then(|rows| rows.first());
            let price_node = first.and_then(|sku| sku.get("price"));
            Some(CommerceProduct {
                id,
                title: text(item, "title").unwrap_or_default(),
                status: text(item, "status").unwrap_or_default(),
                price: price_node.and_then(|price| {
                    price.get("sale_price").and_then(as_f64).or_else(|| price.get("tax_exclusive_price").and_then(as_f64))
                }),
                currency: price_node.and_then(|price| text(price, "currency")),
                stock: skus.and_then(|rows| sum_sku_stock(rows)),
                sku: first.and_then(|sku| text(sku, "seller_sku")),
            })
        })
        .collect();
    ProductPage { shop_id: shop_id.to_string(), items, next_cursor: text(data, "next_page_token").filter(|token| !token.is_empty()) }
}

fn sum_sku_stock(skus: &[Value]) -> Option<i64> {
    let quantities: Vec<i64> = skus
        .iter()
        .filter_map(|sku| sku.get("inventory").and_then(Value::as_array))
        .flatten()
        .filter_map(|row| row.get("quantity").and_then(as_i64))
        .collect();
    if quantities.is_empty() { None } else { Some(quantities.into_iter().sum()) }
}

pub async fn fetch_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    ensure_tiktok(shop)?;
    let cipher = shop_cipher(transport, shop, now).await.map_err(fail_to_error)?;
    let (window_start, window_end) = unix_window(range, now, region_offset_secs(shop.region.as_deref()));
    let mut query = vec![("page_size".into(), PAGE_SIZE.to_string())];
    if let Some(token) = cursor.filter(|value| !value.is_empty()) {
        query.push(("page_token".into(), token.to_string()));
    }
    let body = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "POST",
        SEARCH_ORDERS,
        query,
        Some(json!({"create_time_ge": window_start, "create_time_lt": window_end})),
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_order_page(shop_id, range, &body))
}

fn map_order_page(shop_id: &str, range: MetricRange, body: &Value) -> OrderPage {
    let data = &body["data"];
    let items = data
        .get("orders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "id")?;
            let payment = item.get("payment");
            Some(CommerceOrder {
                id,
                status: text(item, "status").unwrap_or_default(),
                amount: payment.and_then(|value| value.get("total_amount").and_then(as_f64)),
                currency: payment.and_then(|value| text(value, "currency")),
                buyer: identifier(item, "user_id"),
                created_at: item.get("create_time").and_then(as_i64).and_then(|time| DateTime::from_timestamp(time, 0)).map(|time| time.to_rfc3339()),
                lines: order_lines(item),
            })
        })
        .collect();
    OrderPage {
        shop_id: shop_id.to_string(),
        range,
        items,
        next_cursor: text(data, "next_page_token").filter(|token| !token.is_empty()),
    }
}

/// Order list line items are one unit each when `quantity` is absent.
fn order_lines(item: &Value) -> Vec<CommerceOrderLine> {
    item.get("line_items")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let title = text(row, "product_name").or_else(|| text(row, "sku_name"))?;
                    Some(CommerceOrderLine {
                        title,
                        quantity: row.get("quantity").and_then(as_i64).unwrap_or(1),
                        sku: text(row, "seller_sku"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn map_product_status(status: &str, audit: Option<&str>, reason: Option<String>) -> (ListingStatus, Option<String>) {
    if matches!(status, "FAILED") || matches!(audit, Some("FAILED")) {
        return (ListingStatus::Rejected, reason.or_else(|| Some(status.to_string())));
    }
    match status {
        "ACTIVATE" => (ListingStatus::Live, None),
        "SELLER_DEACTIVATED" => (ListingStatus::Live, Some("SELLER_DEACTIVATED".into())),
        "PLATFORM_DEACTIVATED" | "FREEZE" => (ListingStatus::Banned, Some(status.into())),
        "DELETED" => (ListingStatus::Rejected, Some("DELETED".into())),
        "DRAFT" | "PENDING" | "SCHEDULED" => (ListingStatus::Reviewing, None),
        other => (ListingStatus::Reviewing, Some(format!("未识别的商品状态 {other}"))),
    }
}

pub async fn refresh_remote(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_tiktok(shop)?;
    product_status(transport, shop, now, remote_id).await.map_err(fail_to_error)
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
    if shop.platform != Platform::Tiktok {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    if let Some(remote_id) = existing_remote.filter(|value| !value.is_empty()) {
        return match product_status(transport, shop, now, remote_id).await {
            Ok((status, reason)) => outcome(Some(remote_id.to_string()), status, reason),
            Err(error) => from_fail(Some(remote_id.to_string()), error),
        };
    }
    if let Some(message) = validate_draft(shop, draft, target, images) {
        return outcome(None, ListingStatus::Rejected, Some(message));
    }
    let cipher = match shop_cipher(transport, shop, now).await {
        Ok(cipher) => cipher,
        Err(error) => return from_fail(None, error),
    };
    let category_id = target.category_id.trim();
    if let Err(error) = require_package(transport, shop, now, &cipher, category_id, draft).await {
        return from_fail(None, error);
    }
    let attributes = match read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "GET",
        &format!("{CATEGORIES}/{category_id}/attributes"),
        vec![
            ("locale".into(), "en-US".into()),
            ("category_version".into(), category_version(shop.region.as_deref()).into()),
        ],
        None,
    )
    .await
    {
        Ok(body) => map_attributes(&body),
        Err(error) => return from_fail(None, error),
    };
    let product_attributes = match attribute_payload(&attributes, &target.attributes) {
        Ok(values) => values,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    let warehouse_id = match warehouse(transport, shop, now, &cipher).await {
        Ok(id) => id,
        Err(error) => return from_fail(None, error),
    };
    let mut uris = Vec::new();
    for (name, bytes) in images {
        match upload_image(transport, shop, now, name, bytes).await {
            Ok(uri) => uris.push(uri),
            Err(error) => {
                return outcome(None, ListingStatus::Rejected, Some(format!("图片上传未完成，商品尚未创建：{}", error.text())));
            }
        }
    }
    let payload = match create_body(shop, draft, category_id, &uris, &warehouse_id, product_attributes) {
        Ok(payload) => payload,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    let created = match call(transport, shop, now, Some(cipher.as_str()), "POST", CREATE, Vec::new(), Some(payload), None, true).await {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    match identifier(&created["data"], "product_id") {
        Some(remote_id) => outcome(Some(remote_id), ListingStatus::Reviewing, None),
        None => outcome(None, ListingStatus::Uncertain, Some("TikTok Shop 创建接口没有返回 product_id".into())),
    }
}

fn validate_draft(shop: &ResolvedShop, draft: &ListingDraft, target: &ListingTarget, images: &[(String, Vec<u8>)]) -> Option<String> {
    let (min_title, max_title) = title_bounds(shop.region.as_deref());
    let title = draft.title.trim();
    if title.chars().count() < min_title || title.chars().count() > max_title {
        return Some(format!("标题长度必须在 {min_title} 到 {max_title} 之间"));
    }
    let description = draft.description.as_deref().map(str::trim).unwrap_or("");
    if description.is_empty() {
        return Some("缺少商品描述".into());
    }
    if description.chars().count() > 10_000 {
        return Some("商品描述超过 10000 个字符".into());
    }
    if target.category_id.trim().is_empty() {
        return Some("缺少类目".into());
    }
    if images.is_empty() {
        return Some("缺少商品图片".into());
    }
    if images.len() > 9 {
        return Some("商品图片最多 9 张".into());
    }
    if images.iter().any(|(_, bytes)| bytes.is_empty()) {
        return Some("商品图片不能是空文件".into());
    }
    if draft
        .price
        .or_else(|| draft.skus.iter().find_map(|sku| sku.price))
        .map(|price| price <= 0.0)
        .unwrap_or(true)
    {
        return Some("缺少大于 0 的价格".into());
    }
    let currency = draft.currency.as_deref().map(str::trim).filter(|value| !value.is_empty());
    if currency.is_none() && region_currency(shop.region.as_deref()).is_none() {
        return Some("缺少币种".into());
    }
    if draft
        .stock
        .or_else(|| draft.skus.iter().find_map(|sku| sku.stock))
        .map(|stock| stock < 1)
        .unwrap_or(true)
    {
        return Some("库存至少为 1".into());
    }
    if draft.weight_kg.map(|weight| weight <= 0.0).unwrap_or(true) {
        return Some("缺少大于 0 的包裹重量".into());
    }
    if draft.skus.len() > 1 {
        let mut names = std::collections::BTreeSet::new();
        for sku in &draft.skus {
            let name = sku.name.trim();
            if name.is_empty() || name.chars().count() > 50 || !names.insert(name.to_string()) {
                return Some("多规格 SKU 需要互不重复的名称，且不超过 50 个字符".into());
            }
        }
    }
    for sku in &draft.skus {
        if sku.price.or(draft.price).map(|price| price <= 0.0).unwrap_or(true) {
            return Some("缺少大于 0 的价格".into());
        }
        if sku.stock.or(draft.stock).map(|stock| stock < 1).unwrap_or(true) {
            return Some("库存至少为 1".into());
        }
        if let Some(code) = sku.code.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            if code.chars().any(char::is_whitespace) || code.chars().count() > 50 {
                return Some("seller_sku 不能包含空格，且不超过 50 个字符".into());
            }
        }
    }
    None
}

async fn require_package(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    cipher: &str,
    category_id: &str,
    draft: &ListingDraft,
) -> Result<(), CallFail> {
    let body = read(
        transport,
        shop,
        now,
        Some(cipher),
        "GET",
        &format!("{CATEGORIES}/{category_id}/rules"),
        vec![("category_version".into(), category_version(shop.region.as_deref()).into())],
        None,
    )
    .await?;
    let required = body["data"]["package_dimension"].get("is_required").and_then(Value::as_bool).unwrap_or(false);
    let dimensions = draft.dimensions_cm.as_ref();
    if let Some(size) = dimensions {
        if size.l.round() < 1.0 || size.w.round() < 1.0 || size.h.round() < 1.0 {
            return Err(CallFail::Rejected("包裹长宽高需要至少 1 厘米".into()));
        }
    } else if required {
        return Err(CallFail::Rejected("该类目要求包裹长宽高".into()));
    }
    Ok(())
}

fn attribute_payload(definitions: &[CategoryAttribute], provided: &[ListingAttribute]) -> Result<Vec<Value>, String> {
    let mut payload = Vec::new();
    for definition in definitions.iter().filter(|item| item.required) {
        let given = provided.iter().find(|item| item.id == definition.id);
        let mut raw = Vec::new();
        if let Some(item) = given {
            if !item.value.trim().is_empty() {
                raw.push(item.value.trim().to_string());
            }
            raw.extend(item.values.iter().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()));
        }
        if raw.is_empty() {
            return Err(format!("缺少必填属性 {}", definition.name));
        }
        if definition.input != AttributeInput::MultiSelect && raw.len() > 1 {
            return Err(format!("属性 {} 只能填一个值", definition.name));
        }
        let mut values = Vec::new();
        for value in raw {
            if let Some(option) = definition.options.iter().find(|option| option.id == value || option.name == value) {
                values.push(json!({"id": option.id}));
            } else if definition.options.is_empty() || definition.input == AttributeInput::Text || definition.input == AttributeInput::Number {
                values.push(json!({"name": value}));
            } else {
                return Err(format!("属性 {} 不接受自定义值", definition.name));
            }
        }
        payload.push(json!({"id": definition.id, "values": values}));
    }
    Ok(payload)
}

fn create_body(
    shop: &ResolvedShop,
    draft: &ListingDraft,
    category_id: &str,
    uris: &[String],
    warehouse_id: &str,
    product_attributes: Vec<Value>,
) -> Result<Value, String> {
    let currency = draft
        .currency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .or_else(|| region_currency(shop.region.as_deref()).map(str::to_string))
        .ok_or_else(|| "缺少币种".to_string())?;
    let weight = draft.weight_kg.ok_or_else(|| "缺少包裹重量".to_string())?;
    let skus = sku_payload(draft, &currency, warehouse_id)?;
    let description = html_description(draft.description.as_deref().unwrap_or(""));
    let mut body = json!({
        "save_mode": "LISTING",
        "title": draft.title.trim(),
        "description": description,
        "category_id": category_id,
        "category_version": category_version(shop.region.as_deref()),
        "main_images": uris.iter().map(|uri| json!({"uri": uri})).collect::<Vec<_>>(),
        "skus": skus,
        "package_weight": {"value": decimal(weight, 3), "unit": "KILOGRAM"},
    });
    if let Some(size) = &draft.dimensions_cm {
        body["package_dimensions"] = json!({
            "length": whole(size.l),
            "width": whole(size.w),
            "height": whole(size.h),
            "unit": "CENTIMETER",
        });
    }
    if !product_attributes.is_empty() {
        body["product_attributes"] = Value::Array(product_attributes);
    }
    Ok(body)
}

fn sku_payload(draft: &ListingDraft, currency: &str, warehouse_id: &str) -> Result<Vec<Value>, String> {
    let items: Vec<&ListingSku> = if draft.skus.is_empty() { Vec::new() } else { draft.skus.iter().collect() };
    if items.is_empty() {
        let price = draft.price.ok_or_else(|| "缺少价格".to_string())?;
        let stock = draft.stock.ok_or_else(|| "缺少库存".to_string())?;
        return Ok(vec![sku_json(None, None, price, stock, currency, warehouse_id)]);
    }
    let multiple = items.len() > 1;
    let mut skus = Vec::new();
    for sku in items {
        let price = sku.price.or(draft.price).ok_or_else(|| "缺少价格".to_string())?;
        let stock = sku.stock.or(draft.stock).ok_or_else(|| "缺少库存".to_string())?;
        if stock < 1 {
            return Err("库存至少为 1".into());
        }
        let name = if multiple { Some(sku.name.trim().to_string()) } else { None };
        skus.push(sku_json(sku.code.as_deref(), name.as_deref(), price, stock, currency, warehouse_id));
    }
    Ok(skus)
}

fn sku_json(code: Option<&str>, variant: Option<&str>, price: f64, stock: i64, currency: &str, warehouse_id: &str) -> Value {
    let mut sku = json!({
        "price": {"amount": money(price), "currency": currency},
        "inventory": [{"warehouse_id": warehouse_id, "quantity": stock}],
    });
    if let Some(code) = code.map(str::trim).filter(|value| !value.is_empty()) {
        sku["seller_sku"] = json!(code);
    }
    if let Some(name) = variant {
        sku["sales_attributes"] = json!([{"name": "Specification", "value_name": name}]);
    }
    sku
}

fn html_description(description: &str) -> String {
    if description.contains('<') {
        description.to_string()
    } else {
        format!("<p>{}</p>", escape_html(description))
    }
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn money(value: f64) -> String {
    decimal(value, 2)
}

fn decimal(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() { "0".into() } else { trimmed.to_string() }
}

fn whole(value: f64) -> String {
    value.round().max(1.0).to_string().trim_end_matches(".0").to_string()
}

async fn warehouse(transport: &Transport, shop: &ResolvedShop, now: i64, cipher: &str) -> Result<String, CallFail> {
    let body = read(transport, shop, now, Some(cipher), "GET", WAREHOUSES, Vec::new(), None).await?;
    let warehouses = body["data"]["warehouses"].as_array().cloned().unwrap_or_default();
    let enabled: Vec<&Value> = warehouses
        .iter()
        .filter(|item| text(item, "effect_status").is_some_and(|status| status == "ENABLED"))
        .collect();
    enabled
        .iter()
        .find(|item| item.get("is_default").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| enabled.first())
        .and_then(|item| identifier(item, "id"))
        .ok_or_else(|| CallFail::Rejected("没有可用的 TikTok Shop 仓库".into()))
}

async fn upload_image(transport: &Transport, shop: &ResolvedShop, now: i64, name: &str, bytes: &[u8]) -> Result<String, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        None,
        "POST",
        UPLOAD,
        Vec::new(),
        Some(json!({"use_case": "MAIN_IMAGE"})),
        Some((name.to_string(), bytes.to_vec())),
        false,
    )
    .await?;
    text(&body["data"], "uri").ok_or_else(|| CallFail::Rejected("TikTok Shop 未返回图片 uri".into()))
}

async fn product_status(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let cipher = shop_cipher(transport, shop, now).await?;
    let body = read(
        transport,
        shop,
        now,
        Some(cipher.as_str()),
        "GET",
        &format!("{CREATE}/{remote_id}"),
        Vec::new(),
        None,
    )
    .await?;
    let data = &body["data"];
    let status = text(data, "status").ok_or_else(|| CallFail::Failed("TikTok Shop 未返回商品状态".into()))?;
    let audit = text(&data["audit"], "status");
    let reason = audit_reason(data);
    Ok(map_product_status(&status, audit.as_deref(), reason))
}

fn audit_reason(data: &Value) -> Option<String> {
    let reasons = data
        .get("audit_failed_reasons")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|item| item.get("reasons").and_then(Value::as_array))
        .flatten()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    if reasons.is_empty() { None } else { Some(reasons.join("；")) }
}

async fn shop_cipher(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<String, CallFail> {
    let body = read(transport, shop, now, None, "GET", SHOPS, Vec::new(), None).await?;
    body["data"]["shops"]
        .as_array()
        .and_then(|shops| {
            shops.iter().find(|item| identifier(item, "id").as_deref() == Some(shop.remote_id.as_str()))
        })
        .and_then(|item| text(item, "cipher"))
        .ok_or_else(|| CallFail::Rejected("TikTok Shop 授权列表没有这家店的 shop_cipher".into()))
}

async fn read(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    cipher: Option<&str>,
    method: &'static str,
    path: &str,
    query: Vec<(String, String)>,
    json_body: Option<Value>,
) -> Result<Value, CallFail> {
    call(transport, shop, now, cipher, method, path, query, json_body, None, false).await
}

#[allow(clippy::too_many_arguments)]
async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    cipher: Option<&str>,
    method: &'static str,
    path: &str,
    extra: Vec<(String, String)>,
    json_body: Option<Value>,
    file: Option<(String, Vec<u8>)>,
    create: bool,
) -> Result<Value, CallFail> {
    let mut query = vec![("app_key".into(), shop.partner_id.clone()), ("timestamp".into(), now.to_string())];
    if let Some(cipher) = cipher {
        query.push(("shop_cipher".into(), cipher.to_string()));
    }
    query.extend(extra);
    let body = match (&json_body, file.is_some()) {
        (Some(value), false) => {
            Some(serde_json::to_string(value).map_err(|error| CallFail::Rejected(format!("请求体无法编码：{error}")))?)
        }
        _ => None,
    };
    let sign = sign_request(&shop.partner_key, path, &query, body.as_deref()).map_err(CallFail::Rejected)?;
    query.push(("sign".into(), sign));
    let multipart = file.is_some();
    let (file_name, file_bytes) = match file {
        Some((name, bytes)) => (Some(name), Some(bytes)),
        None => (None, None),
    };
    let request = Outbound {
        method,
        path: path.to_string(),
        query,
        json: json_body,
        file_name,
        file_bytes,
        mutating: create || multipart,
        host: HOST.to_string(),
        headers: {
            let mut headers = vec![
                ("x-tts-access-token".into(), shop.access_token.clone()),
                ("content-type".into(), if multipart { "multipart/form-data" } else { "application/json" }.into()),
            ];
            if multipart {
                headers.push(("x-dsivio-file-field".into(), "data".into()));
            }
            headers
        },
    };
    let inbound = match transport.execute(request).await {
        Ok(inbound) => inbound,
        Err(TransportFault::BeforeDispatch(message)) => return Err(CallFail::Rejected(message)),
        Err(TransportFault::AfterDispatch(message)) if create => return Err(CallFail::Uncertain(message)),
        Err(TransportFault::AfterDispatch(message)) => return Err(CallFail::Failed(message)),
    };
    classify(create, inbound.status, &inbound.body)
}

fn classify(create: bool, status: u16, body: &Value) -> Result<Value, CallFail> {
    let code = body.get("code").and_then(Value::as_i64);
    let message = body.get("message").and_then(Value::as_str).unwrap_or("");
    let Some(code) = code else {
        let text = if message.is_empty() { format!("HTTP {status}") } else { message.to_string() };
        return Err(if create { CallFail::Uncertain(text) } else { CallFail::Failed(text) });
    };
    if code == 0 && status < 400 {
        return Ok(body.clone());
    }
    let text = if message.is_empty() { format!("{code}") } else { format!("{code}: {message}") };
    if is_auth(code, message) {
        return Err(CallFail::Auth(text));
    }
    if create && (status >= 500 || matches!(code, 12001000 | 36009003 | 36009007)) {
        return Err(CallFail::Uncertain(text));
    }
    if status >= 500 {
        return Err(CallFail::Failed(text));
    }
    Err(CallFail::Rejected(text))
}

fn is_auth(code: i64, message: &str) -> bool {
    if matches!(code, 105002 | 101000 | 105005) {
        return true;
    }
    if code == 36009004 {
        let lower = message.to_ascii_lowercase();
        return lower.contains("access_token") || lower.contains("x-tts-access-token") || lower.contains("signature");
    }
    false
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

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::trim).filter(|item| !item.is_empty()).map(str::to_owned)
}

fn identifier(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(identifier_value)
}

fn identifier_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .or_else(|| value.as_u64().map(|item| item.to_string()))
}

fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|item| item as f64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|item| item as i64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

#[cfg(test)]
#[path = "tiktok_tests.rs"]
mod tiktok_tests;
