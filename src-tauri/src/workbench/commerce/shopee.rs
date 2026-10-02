//! Shopee Open Platform v2 adapter. Tests replace [`Transport`] and replay recorded JSON.
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceError, CommerceOrder, CommerceOrderLine,
    CommerceProduct, DimensionsCm, ListingAttribute, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange,
    OrderPage, ProductPage, ShopMetrics,
};
use crate::workbench::shops::{sign_shopee, Platform};
use serde_json::{json, Value};
use std::collections::HashMap;

pub use super::transport::{Inbound, Outbound, PushOutcome, ResolvedShop, ScriptTransport, Step, Transport, TransportFault};

const HOST: &str = "https://partner.shopeemobile.com";
const ORDER_PAGE: i64 = 50;
const RETURN_PAGE: i64 = 20;
const MAX_PAGES: usize = 20;
const CHUNK_SECS: i64 = 15 * 86_400;

#[derive(Debug)]
pub enum CallFail {
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

struct OrderView {
    status: String,
    amount: Option<f64>,
    currency: Option<String>,
    buyer: Option<String>,
}

struct ReturnView {
    status: String,
    amount: Option<f64>,
}

pub fn region_offset_secs(region: Option<&str>) -> i64 {
    match region.unwrap_or("").trim().to_ascii_uppercase().as_str() {
        "SG" | "MY" | "PH" | "TW" | "CN" => 8 * 3600,
        "TH" | "VN" | "ID" | "KH" => 7 * 3600,
        "BR" => -3 * 3600,
        "MX" => -6 * 3600,
        "CO" | "CL" | "PE" => -5 * 3600,
        "PL" | "ES" | "FR" => 3600,
        _ => 0,
    }
}

pub fn metric_window(range: MetricRange, now: i64, offset: i64) -> (i64, i64) {
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

fn time_chunks(start: i64, end: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut cursor = start;
    if end < start {
        return vec![(start, start)];
    }
    while cursor <= end {
        let chunk_end = (cursor + CHUNK_SECS - 1).min(end);
        out.push((cursor, chunk_end));
        if chunk_end >= end {
            break;
        }
        cursor = chunk_end + 1;
    }
    out
}

pub async fn fetch_metrics(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    if shop.platform != Platform::Shopee {
        return Err(CommerceError::unsupported("该平台尚未接入店铺指标"));
    }
    let (start, end) = metric_window(range, now, region_offset_secs(shop.region.as_deref()));
    let mut orders = Vec::new();
    let mut order_error = None;
    for (chunk_start, chunk_end) in time_chunks(start, end) {
        match list_orders(transport, shop, now, chunk_start, chunk_end).await {
            Ok(mut page) => orders.append(&mut page),
            Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
            Err(other) => {
                order_error = Some(other.text().to_string());
                break;
            }
        }
    }
    let returns = match list_returns(transport, shop, now, start, end).await {
        Ok(items) => Ok(items),
        Err(_) => Err(()),
    };
    let live = match list_live_count(transport, shop, now).await {
        Ok(count) => Some(count),
        Err(CallFail::Auth(message)) if order_error.is_none() && orders.is_empty() => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => None,
    };
    Ok(fold_metrics(shop_id, range, now, &orders, returns, live, order_error))
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: &[OrderView],
    returns: Result<Vec<ReturnView>, ()>,
    live: Option<f64>,
    order_error: Option<String>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0)
        .map(|time| time.to_rfc3339())
        .unwrap_or_default();
    let mut metrics = super::types::zero_metrics(shop_id.to_string(), range, fetched_at);
    let mut unsupported = Vec::new();
    if let Some(message) = order_error {
        unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
        metrics.error = Some(message);
    } else {
        let counted: Vec<&OrderView> = orders.iter().filter(|order| counts_order(&order.status)).collect();
        metrics.values.insert(MetricKey::Orders, counted.len() as f64);
        metrics.values.insert(
            MetricKey::PendingShipment,
            counted.iter().filter(|order| order.status == "READY_TO_SHIP").count() as f64,
        );
        if counted.is_empty() {
            metrics.values.insert(MetricKey::Gmv, 0.0);
            metrics.values.insert(MetricKey::Buyers, 0.0);
        } else if counted.iter().any(|order| order.amount.is_none()) {
            unsupported.push(MetricKey::Gmv);
        } else {
            metrics.values.insert(MetricKey::Gmv, counted.iter().filter_map(|order| order.amount).sum());
        }
        if !counted.is_empty() && counted.iter().any(|order| order.buyer.is_none()) {
            unsupported.push(MetricKey::Buyers);
        } else {
            let mut buyers = std::collections::BTreeSet::new();
            for order in &counted {
                if let Some(buyer) = &order.buyer {
                    buyers.insert(buyer.clone());
                }
            }
            metrics.values.insert(MetricKey::Buyers, buyers.len() as f64);
        }
        metrics.currency = counted.iter().find_map(|order| order.currency.clone());
    }
    match returns {
        Err(()) => unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]),
        Ok(items) => {
            let counted: Vec<&ReturnView> = items.iter().filter(|item| counts_return(&item.status)).collect();
            metrics.values.insert(MetricKey::RefundOrders, counted.len() as f64);
            if counted.iter().any(|item| item.amount.is_none()) {
                unsupported.push(MetricKey::RefundAmount);
            } else {
                metrics.values.insert(MetricKey::RefundAmount, counted.iter().filter_map(|item| item.amount).sum());
            }
        }
    }
    match live {
        Some(count) => { metrics.values.insert(MetricKey::ProductsLive, count); }
        None => unsupported.push(MetricKey::ProductsLive),
    };
    for key in unsupported {
        metrics.values.insert(key, 0.0);
        if !metrics.unsupported.contains(&key) {
            metrics.unsupported.push(key);
        }
    }
    metrics.unsupported.sort();
    metrics
}

fn counts_order(status: &str) -> bool {
    !matches!(status, "" | "UNPAID" | "CANCELLED" | "IN_CANCEL")
}

fn counts_return(status: &str) -> bool {
    !matches!(status, "" | "CANCELLED" | "CLOSED")
}

async fn list_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<OrderView>, CallFail> {
    let mut cursor = String::new();
    let mut listed = Vec::new();
    for _ in 0..MAX_PAGES {
        let mut extra = vec![
            ("time_range_field".into(), "create_time".into()),
            ("time_from".into(), start.to_string()),
            ("time_to".into(), end.to_string()),
            ("page_size".into(), ORDER_PAGE.to_string()),
        ];
        if !cursor.is_empty() {
            extra.push(("cursor".into(), cursor.clone()));
        }
        let body = call(transport, shop, now, "GET", "/api/v2/order/get_order_list", extra, None, None, false).await?;
        let response = &body["response"];
        if let Some(list) = response.get("order_list").and_then(Value::as_array) {
            listed.extend(list.clone());
        }
        let more = response.get("more").and_then(Value::as_bool).unwrap_or(false);
        cursor = response.get("next_cursor").and_then(Value::as_str).unwrap_or("").to_string();
        if !more || cursor.is_empty() {
            break;
        }
    }
    if listed.is_empty() {
        return Ok(Vec::new());
    }
    let mut details = HashMap::new();
    for batch in listed.chunks(ORDER_PAGE as usize) {
        let sns = batch.iter().filter_map(|item| text(item, "order_sn")).collect::<Vec<_>>().join(",");
        let body = call(
            transport,
            shop,
            now,
            "GET",
            "/api/v2/order/get_order_detail",
            vec![
                ("order_sn_list".into(), sns),
                ("response_optional_fields".into(), "buyer_user_id,total_amount,item_list".into()),
            ],
            None,
            None,
            false,
        )
        .await?;
        if let Some(list) = body["response"].get("order_list").and_then(Value::as_array) {
            for item in list {
                if let Some(sn) = text(item, "order_sn") {
                    details.insert(sn, item.clone());
                }
            }
        }
    }
    Ok(listed
        .iter()
        .map(|item| {
            let sn = text(item, "order_sn").unwrap_or_default();
            let detail = details.get(&sn);
            let source = detail.unwrap_or(item);
            OrderView {
                status: text(source, "order_status").or_else(|| text(item, "order_status")).unwrap_or_default(),
                amount: source.get("total_amount").and_then(as_f64).or_else(|| item.get("total_amount").and_then(as_f64)),
                currency: text(source, "currency").or_else(|| text(item, "currency")),
                buyer: source.get("buyer_user_id").and_then(identifier_value).or_else(|| item.get("buyer_user_id").and_then(identifier_value)),
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
    let mut page_no = 1;
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let body = call(
            transport,
            shop,
            now,
            "GET",
            "/api/v2/returns/get_return_list",
            vec![
                ("page_no".into(), page_no.to_string()),
                ("page_size".into(), RETURN_PAGE.to_string()),
                ("create_time_from".into(), start.to_string()),
                ("create_time_to".into(), end.to_string()),
            ],
            None,
            None,
            false,
        )
        .await?;
        let response = &body["response"];
        let list = response
            .get("return")
            .or_else(|| response.get("return_list"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let more = response.get("more").and_then(Value::as_bool).unwrap_or(false);
        items.extend(list.iter().map(|item| ReturnView {
            status: text(item, "status").unwrap_or_default(),
            amount: item.get("refund_amount").and_then(as_f64),
        }));
        if !more || list.is_empty() {
            break;
        }
        page_no += 1;
    }
    Ok(items)
}

async fn list_live_count(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<f64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "GET",
        "/api/v2/product/get_item_list",
        vec![
            ("item_status".into(), "NORMAL".into()),
            ("offset".into(), "0".into()),
            ("page_size".into(), "1".into()),
        ],
        None,
        None,
        false,
    )
    .await?;
    let response = &body["response"];
    if let Some(count) = response.get("total_count").and_then(as_f64) {
        return Ok(count);
    }
    Ok(response.get("item").and_then(Value::as_array).map(|items| items.len() as f64).unwrap_or(0.0))
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    ensure_shopee(shop)?;
    let offset: i64 = cursor.unwrap_or("0").parse().unwrap_or(0);
    if offset < 0 {
        return Err(CommerceError::invalid("cursor 无效"));
    }
    let body = call(
        transport,
        shop,
        now,
        "GET",
        "/api/v2/product/get_item_list",
        vec![
            ("offset".into(), offset.to_string()),
            ("page_size".into(), "20".into()),
            ("item_status".into(), "NORMAL".into()),
        ],
        None,
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let listed = body["response"].get("item").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids = listed.iter().filter_map(|item| identifier(item, "item_id")).collect::<Vec<_>>();
    let mut details = HashMap::new();
    if !ids.is_empty() {
        let info = call(
            transport,
            shop,
            now,
            "GET",
            "/api/v2/product/get_item_base_info",
            vec![("item_id_list".into(), ids.join(","))],
            None,
            None,
            false,
        )
        .await
        .map_err(fail_to_error)?;
        if let Some(items) = info["response"].get("item_list").and_then(Value::as_array) {
            for item in items {
                if let Some(id) = identifier(item, "item_id") {
                    details.insert(id, item.clone());
                }
            }
        }
    }
    let items = ids
        .iter()
        .map(|id| {
            let item = details.get(id);
            let price_info = item.and_then(|item| item.get("price_info")).and_then(Value::as_array).and_then(|list| list.first());
            CommerceProduct {
                id: id.clone(),
                title: item.and_then(|item| text(item, "item_name")).unwrap_or_default(),
                status: item.and_then(|item| text(item, "item_status")).unwrap_or_else(|| "NORMAL".into()),
                price: price_info.and_then(|info| info.get("current_price")).and_then(as_f64).or_else(|| item.and_then(|item| item.get("original_price")).and_then(as_f64)),
                currency: price_info.and_then(|info| text(info, "currency")),
                stock: item.and_then(product_stock),
                sku: item.and_then(|item| text(item, "item_sku")),
            }
        })
        .collect();
    let has_next = body["response"].get("has_next_page").and_then(Value::as_bool).unwrap_or(false);
    Ok(ProductPage {
        shop_id: shop_id.to_string(),
        items,
        next_cursor: if has_next { Some((offset + ids.len() as i64).to_string()) } else { None },
    })
}

pub async fn fetch_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    ensure_shopee(shop)?;
    let (range_start, range_end) = metric_window(range, now, region_offset_secs(shop.region.as_deref()));
    let (window_start, page_cursor) = split_order_cursor(cursor, range_start);
    if window_start > range_end {
        return Ok(OrderPage { shop_id: shop_id.to_string(), range, items: Vec::new(), next_cursor: None });
    }
    let window_end = (window_start + CHUNK_SECS - 1).min(range_end);
    let mut extra = vec![
        ("time_range_field".into(), "create_time".into()),
        ("time_from".into(), window_start.to_string()),
        ("time_to".into(), window_end.to_string()),
        ("page_size".into(), ORDER_PAGE.to_string()),
    ];
    if !page_cursor.is_empty() {
        extra.push(("cursor".into(), page_cursor));
    }
    let body = call(transport, shop, now, "GET", "/api/v2/order/get_order_list", extra, None, None, false)
        .await
        .map_err(fail_to_error)?;
    let listed = body["response"].get("order_list").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut details = HashMap::new();
    if !listed.is_empty() {
        let sns = listed.iter().filter_map(|item| text(item, "order_sn")).collect::<Vec<_>>().join(",");
        let info = call(
            transport,
            shop,
            now,
            "GET",
            "/api/v2/order/get_order_detail",
            vec![
                ("order_sn_list".into(), sns),
                ("response_optional_fields".into(), "buyer_user_id,total_amount,item_list".into()),
            ],
            None,
            None,
            false,
        )
        .await
        .map_err(fail_to_error)?;
        if let Some(items) = info["response"].get("order_list").and_then(Value::as_array) {
            for item in items {
                if let Some(sn) = text(item, "order_sn") {
                    details.insert(sn, item.clone());
                }
            }
        }
    }
    let items = listed
        .iter()
        .map(|item| {
            let sn = text(item, "order_sn").unwrap_or_default();
            let source = details.get(&sn).unwrap_or(item);
            CommerceOrder {
                id: sn,
                status: text(source, "order_status").or_else(|| text(item, "order_status")).unwrap_or_default(),
                amount: source.get("total_amount").and_then(as_f64),
                currency: text(source, "currency"),
                buyer: source.get("buyer_user_id").and_then(identifier_value),
                created_at: source.get("create_time").and_then(Value::as_i64).and_then(|time| chrono::DateTime::from_timestamp(time, 0)).map(|time| time.to_rfc3339()),
                lines: source.get("item_list").and_then(Value::as_array).map(|items| order_lines(items)).unwrap_or_default(),
            }
        })
        .collect();
    let more = body["response"].get("more").and_then(Value::as_bool).unwrap_or(false);
    let next = body["response"].get("next_cursor").and_then(Value::as_str).unwrap_or("");
    let next_cursor = if more && !next.is_empty() {
        Some(format!("{window_start}:{next}"))
    } else if window_end < range_end {
        Some(format!("{}:", window_end + 1))
    } else {
        None
    };
    Ok(OrderPage { shop_id: shop_id.to_string(), range, items, next_cursor })
}

fn product_stock(item: &Value) -> Option<i64> {
    item.get("stock_info_v2")
        .and_then(|info| info.get("summary_info"))
        .and_then(|summary| summary.get("total_available_stock"))
        .and_then(Value::as_i64)
        .or_else(|| item.get("seller_stock").and_then(Value::as_array).and_then(|list| list.first()).and_then(|row| row.get("stock")).and_then(Value::as_i64))
}

fn order_lines(items: &[Value]) -> Vec<CommerceOrderLine> {
    items
        .iter()
        .map(|item| CommerceOrderLine {
            title: text(item, "item_name").unwrap_or_default(),
            quantity: item.get("model_quantity_purchased").and_then(Value::as_i64).unwrap_or(0),
            sku: text(item, "model_sku").or_else(|| text(item, "item_sku")),
        })
        .collect()
}

fn split_order_cursor(cursor: Option<&str>, range_start: i64) -> (i64, String) {
    let Some(cursor) = cursor.filter(|value| !value.is_empty()) else {
        return (range_start, String::new());
    };
    match cursor.split_once(':') {
        Some((start, page)) => (start.parse().unwrap_or(range_start), page.to_string()),
        None => (range_start, cursor.to_string()),
    }
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure_shopee(shop)?;
    let body = call(
        transport,
        shop,
        now,
        "GET",
        "/api/v2/product/get_category",
        vec![("language".into(), "en".into())],
        None,
        None,
        false,
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
    body["response"]
        .get("category_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "category_id")?;
            let parent_id = identifier(item, "parent_category_id").unwrap_or_else(|| "0".into());
            let name = text(item, "display_category_name")
                .or_else(|| text(item, "original_category_name"))
                .unwrap_or_else(|| id.clone());
            let leaf = !item.get("has_children").and_then(Value::as_bool).unwrap_or(false);
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
    ensure_shopee(shop)?;
    let body = call(
        transport,
        shop,
        now,
        "GET",
        "/api/v2/product/get_attribute_tree",
        vec![("category_id_list".into(), category_id.to_string()), ("language".into(), "en".into())],
        None,
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    let mut out = Vec::new();
    if let Some(list) = body["response"].get("list").and_then(Value::as_array) {
        for entry in list {
            if let Some(tree) = entry.get("attribute_tree").and_then(Value::as_array) {
                walk_attributes(tree, &mut out);
            }
        }
    }
    if out.is_empty() {
        if let Some(list) = body["response"].get("attribute_list").and_then(Value::as_array) {
            walk_attributes(list, &mut out);
        }
    }
    out
}

fn walk_attributes(nodes: &[Value], out: &mut Vec<CategoryAttribute>) {
    for node in nodes {
        if let Some(attribute) = map_one_attribute(node) {
            out.push(attribute);
        }
        if let Some(children) = node.get("child_attribute_list").and_then(Value::as_array) {
            walk_attributes(children, out);
        }
    }
}

fn map_one_attribute(node: &Value) -> Option<CategoryAttribute> {
    let id = identifier(node, "attribute_id")?;
    let name = text(node, "name")
        .or_else(|| text(node, "display_attribute_name"))
        .or_else(|| text(node, "original_attribute_name"))
        .unwrap_or_else(|| id.clone());
    let required = node.get("mandatory").and_then(Value::as_bool).unwrap_or(false)
        || node.get("is_mandatory").and_then(Value::as_bool).unwrap_or(false);
    let info = node.get("attribute_info").cloned().unwrap_or(Value::Null);
    let input_type = node.get("input_type").unwrap_or_else(|| info.get("input_type").unwrap_or(&Value::Null));
    let validation = node
        .get("input_validation_type")
        .unwrap_or_else(|| info.get("input_validation_type").unwrap_or(&Value::Null));
    let options = node
        .get("attribute_value_list")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let id = identifier(value, "value_id").unwrap_or_default();
                    let name = text(value, "name")
                        .or_else(|| text(value, "display_value_name"))
                        .or_else(|| text(value, "original_value_name"))?;
                    Some(AttributeOption { id, name })
                })
                .collect()
        })
        .unwrap_or_default();
    let unit = info
        .get("attribute_unit_list")
        .and_then(Value::as_array)
        .and_then(|units| units.first())
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| text(&info, "attribute_unit"));
    Some(CategoryAttribute { id, name, required, input: map_input(input_type, validation), options, unit })
}

fn map_input(input_type: &Value, validation: &Value) -> AttributeInput {
    let token = input_type
        .as_i64()
        .map(|value| value.to_string())
        .or_else(|| input_type.as_str().map(|value| value.to_string()))
        .unwrap_or_default()
        .to_ascii_uppercase();
    let numeric = matches!(validation.as_i64(), Some(1 | 2))
        || validation.as_str().is_some_and(|value| {
            value.eq_ignore_ascii_case("INT_TYPE") || value.eq_ignore_ascii_case("FLOAT_TYPE")
        });
    match token.as_str() {
        "1" | "DROP_DOWN" | "2" | "COMBO_BOX" => AttributeInput::Select,
        "4" | "MULTIPLE_SELECT" | "5" | "MULTIPLE_SELECT_COMBO_BOX" => AttributeInput::MultiSelect,
        _ if numeric => AttributeInput::Number,
        _ => AttributeInput::Text,
    }
}

pub fn map_item_status(status: &str) -> (ListingStatus, Option<String>) {
    match status {
        "NORMAL" => (ListingStatus::Live, None),
        "REVIEWING" => (ListingStatus::Reviewing, None),
        "UNLIST" => (ListingStatus::Live, Some("item unlisted (UNLIST)".into())),
        "BANNED" => (ListingStatus::Banned, Some("BANNED".into())),
        "SELLER_DELETE" | "SHOPEE_DELETE" | "DELETED" => (ListingStatus::Rejected, Some(status.into())),
        other => (ListingStatus::Reviewing, Some(format!("未识别的商品状态 {other}"))),
    }
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
    if shop.platform != Platform::Shopee {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    if let Some(remote_id) = existing_remote {
        return continue_existing(transport, shop, draft, now, remote_id).await;
    }
    let channel = match logistics_channel(transport, shop, now).await {
        Ok(id) => id,
        Err(error) => return from_fail(None, error),
    };
    let mut image_ids = Vec::new();
    for (name, bytes) in images {
        match upload_image(transport, shop, now, name, bytes).await {
            Ok(id) => image_ids.push(id),
            Err(error) => return from_fail(None, error),
        }
    }
    let payload = add_item_body(draft, target, &image_ids, channel);
    let created = match call(
        transport,
        shop,
        now,
        "POST",
        "/api/v2/product/add_item",
        Vec::new(),
        Some(payload),
        None,
        true,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    let remote_id = created["response"].get("item_id").and_then(identifier_value);
    if remote_id.is_none() {
        return outcome(None, ListingStatus::Rejected, Some("Shopee 未返回商品 ID".into()));
    }
    if draft.skus.len() > 1 {
        if let Err(error) = init_tiers(transport, shop, draft, now, remote_id.as_deref().unwrap_or_default()).await {
            return from_fail(remote_id, error);
        }
    }
    outcome(remote_id, ListingStatus::Reviewing, None)
}

async fn continue_existing(
    transport: &Transport,
    shop: &ResolvedShop,
    draft: &ListingDraft,
    now: i64,
    remote_id: &str,
) -> PushOutcome {
    if draft.skus.len() > 1 {
        if let Err(error) = init_tiers(transport, shop, draft, now, remote_id).await {
            return from_fail(Some(remote_id.to_string()), error);
        }
    }
    match item_status(transport, shop, now, remote_id).await {
        Ok((status, reason)) => outcome(Some(remote_id.to_string()), status, reason),
        Err(error) => from_fail(Some(remote_id.to_string()), error),
    }
}

pub async fn refresh_remote(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_shopee(shop)?;
    item_status(transport, shop, now, remote_id).await.map_err(fail_to_error)
}

async fn item_status(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "GET",
        "/api/v2/product/get_item_base_info",
        vec![("item_id_list".into(), remote_id.to_string())],
        None,
        None,
        false,
    )
    .await?;
    let status = body["response"]["item_list"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| text(item, "item_status"))
        .ok_or_else(|| CallFail::Failed("Shopee 未返回商品状态".into()))?;
    Ok(map_item_status(&status))
}

async fn logistics_channel(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<i64, CallFail> {
    let body = call(transport, shop, now, "GET", "/api/v2/logistics/get_channel_list", Vec::new(), None, None, false).await?;
    body["response"]["logistics_channel_list"]
        .as_array()
        .and_then(|channels| {
            channels.iter().find(|channel| channel.get("enabled").and_then(Value::as_bool).unwrap_or(false))
        })
        .and_then(|channel| identifier(channel, "logistics_channel_id"))
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| CallFail::Rejected("没有可用的 Shopee 物流渠道".into()))
}

async fn upload_image(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    name: &str,
    bytes: &[u8],
) -> Result<String, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "POST",
        "/api/v2/media_space/upload_image",
        vec![("scene".into(), "normal".into())],
        None,
        Some((name.to_string(), bytes.to_vec())),
        false,
    )
    .await?;
    text(&body["response"]["image_info"], "image_id").ok_or_else(|| CallFail::Rejected("Shopee 未返回图片 ID".into()))
}

async fn init_tiers(
    transport: &Transport,
    shop: &ResolvedShop,
    draft: &ListingDraft,
    now: i64,
    remote_id: &str,
) -> Result<(), CallFail> {
    let item_id = remote_id.parse::<i64>().map_err(|_| CallFail::Failed("商品 ID 无效".into()))?;
    let options = draft
        .skus
        .iter()
        .map(|sku| json!({ "option": if sku.name.trim().is_empty() { "选项" } else { sku.name.as_str() } }))
        .collect::<Vec<_>>();
    let models = draft
        .skus
        .iter()
        .enumerate()
        .map(|(index, sku)| {
            json!({
                "tier_index": [index],
                "original_price": sku.price.or(draft.price).unwrap_or(0.0),
                "model_sku": sku.code.clone().unwrap_or_default(),
                "seller_stock": [{ "stock": sku.stock.or(draft.stock).unwrap_or(0) }],
            })
        })
        .collect::<Vec<_>>();
    let result = call(
        transport,
        shop,
        now,
        "POST",
        "/api/v2/product/init_tier_variation",
        Vec::new(),
        Some(json!({
            "item_id": item_id,
            "tier_variation": [{ "name": "规格", "option_list": options }],
            "model": models,
        })),
        None,
        true,
    )
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(CallFail::Rejected(message)) if message.to_ascii_lowercase().contains("already") => Ok(()),
        Err(error) => Err(error),
    }
}

fn add_item_body(draft: &ListingDraft, target: &ListingTarget, image_ids: &[String], channel: i64) -> Value {
    let (price, stock, sku) = if draft.skus.len() == 1 {
        let item = &draft.skus[0];
        (item.price.or(draft.price).unwrap_or(0.0), item.stock.or(draft.stock).unwrap_or(0), item.code.clone())
    } else {
        (draft.price.unwrap_or(0.0), draft.stock.unwrap_or(0), None)
    };
    let dimensions = draft.dimensions_cm.clone().unwrap_or(DimensionsCm { l: 1.0, w: 1.0, h: 1.0 });
    let brand = draft.brand.as_deref().map(str::trim).filter(|value| !value.is_empty()).unwrap_or("NoBrand");
    let mut body = json!({
        "original_price": price,
        "description": draft.description.clone().unwrap_or_default(),
        "weight": draft.weight_kg.unwrap_or(0.1),
        "item_name": draft.title,
        "item_status": "NORMAL",
        "dimension": {
            "package_length": centimeters(dimensions.l),
            "package_width": centimeters(dimensions.w),
            "package_height": centimeters(dimensions.h),
        },
        "category_id": target.category_id.parse::<i64>().unwrap_or(0),
        "image": { "image_id_list": image_ids },
        "logistic_info": [{ "logistic_id": channel, "enabled": true }],
        "seller_stock": [{ "stock": stock }],
        "brand": { "brand_id": 0, "original_brand_name": brand },
    });
    if let Some(sku) = sku.filter(|value| !value.is_empty()) {
        body["item_sku"] = json!(sku);
    }
    let attributes = attribute_payload(&target.attributes);
    if !attributes.is_empty() {
        body["attribute_list"] = Value::Array(attributes);
    }
    body
}

fn attribute_payload(attributes: &[ListingAttribute]) -> Vec<Value> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let values = if attribute.values.is_empty() {
                vec![attribute.value.clone()]
            } else {
                attribute.values.clone()
            };
            let values: Vec<Value> = values
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .map(|name| json!({ "value_id": 0, "original_value_name": name }))
                .collect();
            if values.is_empty() {
                return None;
            }
            Some(json!({
                "attribute_id": attribute.id.parse::<i64>().unwrap_or(0),
                "attribute_value_list": values,
            }))
        })
        .collect()
}

fn centimeters(value: f64) -> i64 {
    let rounded = value.round() as i64;
    if rounded < 1 { 1 } else { rounded }
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

fn ensure_shopee(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform == Platform::Shopee {
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

#[allow(clippy::too_many_arguments)]
async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    method: &'static str,
    path: &str,
    extra: Vec<(String, String)>,
    json_body: Option<Value>,
    file: Option<(String, Vec<u8>)>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let sign = sign_shopee(
        &shop.partner_id,
        &shop.partner_key,
        path,
        now,
        Some(&shop.access_token),
        Some(&shop.remote_id),
    )
    .map_err(CallFail::Failed)?;
    let mut query = vec![
        ("partner_id".into(), shop.partner_id.clone()),
        ("timestamp".into(), now.to_string()),
        ("access_token".into(), shop.access_token.clone()),
        ("shop_id".into(), shop.remote_id.clone()),
        ("sign".into(), sign),
    ];
    query.extend(extra);
    let (file_name, file_bytes) = match file {
        Some((name, bytes)) => (Some(name), Some(bytes)),
        None => (None, None),
    };
    let request = Outbound {
        method,
        host: HOST.to_string(),
        path: path.to_string(),
        headers: Vec::new(),
        query,
        json: json_body,
        file_name,
        file_bytes,
        mutating,
    };
    let inbound = match transport.execute(request).await {
        Ok(inbound) => inbound,
        Err(TransportFault::BeforeDispatch(message)) => return Err(CallFail::Failed(message)),
        Err(TransportFault::AfterDispatch(message)) if mutating => return Err(CallFail::Uncertain(message)),
        Err(TransportFault::AfterDispatch(message)) => return Err(CallFail::Failed(message)),
    };
    classify(mutating, inbound)
}

fn classify(mutating: bool, inbound: Inbound) -> Result<Value, CallFail> {
    let error = inbound.body.get("error").and_then(Value::as_str).unwrap_or("");
    let message = inbound.body.get("message").and_then(Value::as_str).unwrap_or("");
    let text = match (error.is_empty(), message.is_empty()) {
        (true, true) => format!("HTTP {}", inbound.status),
        (false, true) => error.to_string(),
        (true, false) => message.to_string(),
        (false, false) => format!("{error}: {message}"),
    };
    if inbound.status >= 500 {
        return if mutating { Err(CallFail::Uncertain(text)) } else { Err(CallFail::Failed(text)) };
    }
    if inbound.status >= 400 || !error.is_empty() {
        if is_token_auth(error, message) {
            return Err(CallFail::Auth(text));
        }
        return Err(CallFail::Rejected(text));
    }
    Ok(inbound.body)
}

fn is_token_auth(error: &str, message: &str) -> bool {
    let blob = format!("{error} {message}").to_ascii_lowercase();
    blob.contains("error_auth") || blob.contains("invalid access_token") || blob.contains("invalid_access_token")
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

fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|item| item as f64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}
