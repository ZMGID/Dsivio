//! 快手小店开放平台业务适配器。令牌只走店铺会话，签名与 `shops/providers.rs` 的 `kuaishou_api` 相同：
//! 除 `sign` 外的参数按名升序拼成 `key=value`，用 `&` 连接，末尾加 `&signSecret={appSecret}`，
//! 再用 app secret 做 HMAC-SHA256 并 Base64。业务参数放在 `param` 的 JSON 字符串里一起签名。
//!
//! 调用与签名：<https://open.kwaixiaodian.com/docs/dev?pageSign=e1d9e229332f4f233a04b44833a5dfe71614263940720>
//! 类目：<https://open.kwaixiaodian.com/zone/new/docs/api?name=open.item.category&version=1>
//! 类目属性：<https://open.kwaixiaodian.com/zone/new/docs/api?name=open.item.category.config&version=1>
//! 图片：<https://open.kwaixiaodian.com/docs/api?apiName=open.item.image.upload&version=1>
//! 新增商品：<https://open.kwaixiaodian.com/zone/new/docs/api?name=open.item.new&version=1>
//! 商品详情：<https://open.kwaixiaodian.com/docs/api?categoryId=44&apiName=open.item.get&version=1>
//! 商品列表：<https://open.kwaixiaodian.com/zone/new/docs/api?name=open.item.list&version=1>
//! 运费模板：<https://open.kwaixiaodian.com/zone/new/docs/api?name=open.logistics.express.template.list&version=1>
//! 订单游标：<https://open.kwaixiaodian.com/docs/api?categoryId=43&apiName=open.order.cursor.list&version=1>
//! 售后游标：<https://open.kwaixiaodian.com/docs/api?categoryId=42&apiName=open.seller.order.refund.pcursor.list&version=1>
//!
//! 指标按中国时间（UTC+8）的今天、昨天、近 7 天、近 30 天汇总。订单查询单次不超过 7 天，
//! 售后查询单次不超过 1 天，且都不能早于 90 天前；近 30 天拆段请求。金额字段单位是分，这里换成元，币种 CNY。
//! 活体上传把文件放在 `file_bytes`，并用请求头 `x-dsivio-file-field: imgBytes` 告诉共享传输多部件字段名。

use super::transport::{Inbound, Outbound, PushOutcome, ResolvedShop, Transport, TransportFault};
use super::types::{
    zero_metrics, AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceError, ListingAttribute,
    ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey, MetricRange, ShopMetrics,
};
use crate::workbench::shops::Platform;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const HOST: &str = "https://openapi.kwaixiaodian.com";
const CN_OFFSET: i64 = 8 * 3600;
const ORDER_PAGE: i64 = 50;
const REFUND_PAGE: i64 = 100;
const PRODUCT_PAGE: i64 = 20;
const MAX_PAGES: usize = 20;
const ORDER_SPAN_MS: i64 = 7 * 86_400 * 1000 - 1;
const REFUND_SPAN_MS: i64 = 86_400 * 1000 - 1;

const ORDER_LIST: &str = "open.order.cursor.list";
const REFUND_LIST: &str = "open.seller.order.refund.pcursor.list";
const ITEM_LIST: &str = "open.item.list";
const ITEM_GET: &str = "open.item.get";
const ITEM_NEW: &str = "open.item.new";
const ITEM_CATEGORY: &str = "open.item.category";
const CATEGORY_CONFIG: &str = "open.item.category.config";
const IMAGE_UPLOAD: &str = "open.item.image.upload";
const TEMPLATE_LIST: &str = "open.logistics.express.template.list";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProduct {
    pub remote_id: String,
    pub title: String,
    pub status: ListingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPage {
    pub items: Vec<RemoteProduct>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteOrder {
    pub remote_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderPage {
    pub items: Vec<RemoteOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

pub fn supported_metrics() -> &'static [MetricKey] {
    &MetricKey::ALL
}

pub fn capability_notes() -> &'static [&'static str] {
    &[
        "指标支持今天、昨天、近 7 天、近 30 天（中国时间）。成交额和退款金额把接口的分换成元，币种 CNY。",
        "订单走 open.order.cursor.list，单次时间窗不超过 7 天；退款走 open.seller.order.refund.pcursor.list，单次不超过 1 天。",
        "待发货只计订单状态 30；退款只计售后状态 60。在售商品只计列表里 onOfflineStatus=1；没有上下架状态则该指标为不支持。",
        "上架需要店铺已有运费模板。图片用 open.item.image.upload，商品和 SKU 一次 open.item.new 提交。创建成功后只能用 open.item.get 查状态。",
    ]
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

struct OrderView {
    status: i64,
    amount_fen: Option<i64>,
    buyer: Option<String>,
    created_ms: Option<i64>,
    oid: String,
}

struct RefundView {
    status: i64,
    amount_fen: Option<i64>,
}

pub fn metric_window(range: MetricRange, now: i64) -> (i64, i64) {
    let local = now + CN_OFFSET;
    let day = local - local.rem_euclid(86_400);
    let (start_local, end_local) = match range {
        MetricRange::Today => (day, local),
        MetricRange::Yesterday => (day - 86_400, day - 1),
        MetricRange::Last7 => (day - 6 * 86_400, local),
        MetricRange::Last30 => (day - 29 * 86_400, local),
    };
    (start_local - CN_OFFSET, end_local - CN_OFFSET)
}

fn chunks(start_ms: i64, end_ms: i64, span_ms: i64) -> Vec<(i64, i64)> {
    if end_ms < start_ms {
        return vec![(start_ms, start_ms)];
    }
    let mut out = Vec::new();
    let mut cursor = start_ms;
    while cursor <= end_ms {
        let chunk_end = cursor.saturating_add(span_ms).min(end_ms);
        out.push((cursor, chunk_end));
        if chunk_end >= end_ms {
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
    ensure(shop)?;
    let (start, end) = metric_window(range, now);
    let start_ms = start.saturating_mul(1000);
    let end_ms = end.saturating_mul(1000);
    let mut orders = Vec::new();
    let mut order_error = None;
    for (chunk_start, chunk_end) in chunks(start_ms, end_ms, ORDER_SPAN_MS) {
        match list_orders(transport, shop, now, chunk_start, chunk_end).await {
            Ok(mut page) => orders.append(&mut page),
            Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
            Err(other) => {
                order_error = Some(other.text().to_string());
                break;
            }
        }
    }
    let refunds = match list_refunds(transport, shop, now, start_ms, end_ms).await {
        Ok(items) => Ok(items),
        Err(CallFail::Auth(message)) if order_error.is_none() && orders.is_empty() => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => Err(()),
    };
    let live = match list_live_count(transport, shop, now).await {
        Ok(count) => Ok(count),
        Err(CallFail::Auth(message)) if order_error.is_none() && orders.is_empty() => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => Err(()),
    };
    Ok(fold_metrics(shop_id, range, now, &orders, refunds, live, order_error))
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: &[OrderView],
    refunds: Result<Vec<RefundView>, ()>,
    live: Result<f64, ()>,
    order_error: Option<String>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    let mut metrics = zero_metrics(shop_id.to_string(), range, fetched_at);
    let mut unsupported = Vec::new();
    if let Some(message) = order_error {
        unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
        metrics.error = Some(message);
    } else {
        metrics.currency = Some("CNY".into());
        let counted: Vec<&OrderView> = orders.iter().filter(|order| counts_order(order.status)).collect();
        metrics.values.insert(MetricKey::Orders, counted.len() as f64);
        metrics.values.insert(
            MetricKey::PendingShipment,
            counted.iter().filter(|order| order.status == 30).count() as f64,
        );
        if counted.is_empty() {
            metrics.values.insert(MetricKey::Gmv, 0.0);
            metrics.values.insert(MetricKey::Buyers, 0.0);
        } else if counted.iter().any(|order| order.amount_fen.is_none()) {
            unsupported.push(MetricKey::Gmv);
        } else {
            let fen: i64 = counted.iter().filter_map(|order| order.amount_fen).sum();
            metrics.values.insert(MetricKey::Gmv, fen as f64 / 100.0);
        }
        if !counted.is_empty() && counted.iter().any(|order| order.buyer.is_none()) {
            unsupported.push(MetricKey::Buyers);
        } else {
            let mut buyers = BTreeSet::new();
            for order in &counted {
                if let Some(buyer) = &order.buyer {
                    buyers.insert(buyer.clone());
                }
            }
            metrics.values.insert(MetricKey::Buyers, buyers.len() as f64);
        }
    }
    match refunds {
        Err(()) => unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]),
        Ok(items) => {
            let counted: Vec<&RefundView> = items.iter().filter(|item| item.status == 60).collect();
            metrics.values.insert(MetricKey::RefundOrders, counted.len() as f64);
            if counted.iter().any(|item| item.amount_fen.is_none()) {
                unsupported.push(MetricKey::RefundAmount);
            } else {
                let fen: i64 = counted.iter().filter_map(|item| item.amount_fen).sum();
                metrics.values.insert(MetricKey::RefundAmount, fen as f64 / 100.0);
            }
        }
    }
    match live {
        Ok(count) => {
            metrics.values.insert(MetricKey::ProductsLive, count);
        }
        Err(()) => unsupported.push(MetricKey::ProductsLive),
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

fn counts_order(status: i64) -> bool {
    matches!(status, 30 | 40 | 50 | 70)
}

async fn list_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<OrderView>, CallFail> {
    let mut cursor = String::new();
    let mut orders = Vec::new();
    for page in 0..MAX_PAGES {
        let body = call(
            transport,
            shop,
            now,
            ORDER_LIST,
            Some(json!({
                "orderViewStatus": 1,
                "pageSize": ORDER_PAGE,
                "sort": 1,
                "queryType": 1,
                "cpsType": 0,
                "beginTime": start_ms,
                "endTime": end_ms,
                "cursor": cursor,
            })),
            None,
            false,
        )
        .await?;
        let data = &body["data"];
        if let Some(list) = data.get("orderList").and_then(Value::as_array) {
            orders.extend(list.iter().filter_map(map_order));
        }
        let next = text(data, "cursor").unwrap_or_default();
        let more = !next.is_empty() && next != "nomore" && next != cursor;
        if !more {
            return Ok(orders);
        }
        if page + 1 == MAX_PAGES {
            return Err(CallFail::Failed("快手订单分页未完成，不能当作完整指标".into()));
        }
        cursor = next;
    }
    Ok(orders)
}

fn map_order(item: &Value) -> Option<OrderView> {
    let base = item.get("orderBaseInfo").unwrap_or(item);
    let oid = identifier(base, "oid")?;
    Some(OrderView {
        status: base.get("status").and_then(Value::as_i64).unwrap_or(0),
        amount_fen: base.get("totalFee").and_then(as_i64),
        buyer: text(base, "buyerOpenId"),
        created_ms: base.get("createTime").and_then(as_i64),
        oid,
    })
}

async fn list_refunds(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<RefundView>, CallFail> {
    let mut refunds = Vec::new();
    for (chunk_start, chunk_end) in chunks(start_ms, end_ms, REFUND_SPAN_MS) {
        let mut cursor = String::new();
        for page in 0..MAX_PAGES {
            let body = call(
                transport,
                shop,
                now,
                REFUND_LIST,
                Some(json!({
                    "beginTime": chunk_start,
                    "endTime": chunk_end,
                    "type": 9,
                    "pageSize": REFUND_PAGE,
                    "currentPage": 1,
                    "queryType": 1,
                    "pcursor": cursor,
                })),
                None,
                false,
            )
            .await?;
            let data = &body["data"];
            if let Some(list) = data.get("refundOrderInfoList").and_then(Value::as_array) {
                refunds.extend(list.iter().map(|item| RefundView {
                    status: item.get("status").and_then(Value::as_i64).unwrap_or(0),
                    amount_fen: item.get("refundFee").and_then(as_i64),
                }));
            }
            let next = text(data, "pcursor").unwrap_or_default();
            let more = !next.is_empty() && next != "nomore" && next != cursor;
            if !more {
                break;
            }
            if page + 1 == MAX_PAGES {
                return Err(CallFail::Failed("快手售后分页未完成，不能当作完整指标".into()));
            }
            cursor = next;
        }
    }
    Ok(refunds)
}

async fn list_live_count(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<f64, CallFail> {
    let mut page_number = 1_i64;
    let mut live = 0_i64;
    let mut seen = false;
    for _ in 0..MAX_PAGES {
        let body = call(
            transport,
            shop,
            now,
            ITEM_LIST,
            Some(json!({
                "pageNumber": page_number,
                "pageSize": 100,
                "itemStatus": 1,
            })),
            None,
            false,
        )
        .await?;
        let data = &body["data"];
        let items = data.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        if items.is_empty() {
            return Ok(live as f64);
        }
        seen = true;
        for item in &items {
            match item.get("onOfflineStatus").and_then(Value::as_i64) {
                Some(1) => live += 1,
                Some(_) => {}
                None => return Err(CallFail::Failed("商品列表没有 onOfflineStatus，不能把未删除商品数当在售".into())),
            }
        }
        let total_page = data.get("totalPage").and_then(Value::as_i64).unwrap_or(page_number);
        if page_number >= total_page {
            return Ok(live as f64);
        }
        page_number += 1;
    }
    if seen {
        return Err(CallFail::Failed("商品列表分页未完成，不能当作完整在售数".into()));
    }
    Ok(0.0)
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure(shop)?;
    let body = call(transport, shop, now, ITEM_CATEGORY, None, None, false).await.map_err(fail_to_error)?;
    let mut categories = map_categories(&body);
    if let Some(parent) = parent_id.filter(|value| !value.is_empty()) {
        categories.retain(|category| category.parent_id == parent);
    }
    Ok(categories)
}

pub fn map_categories(body: &Value) -> Vec<Category> {
    let nodes: Vec<&Value> = body.get("data").and_then(Value::as_array).into_iter().flatten().collect();
    let mut categories: Vec<Category> = nodes
        .iter()
        .filter_map(|item| {
            let id = identifier(item, "categoryId")?;
            let parent_id = identifier(item, "categoryPid").unwrap_or_else(|| "0".into());
            let name = text(item, "categoryName").unwrap_or_else(|| id.clone());
            Some(Category { id, name, parent_id, leaf: true })
        })
        .collect();
    let parents: BTreeSet<String> = categories.iter().map(|category| category.parent_id.clone()).collect();
    for category in &mut categories {
        category.leaf = !parents.contains(&category.id);
    }
    categories
}

pub async fn fetch_attributes(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure(shop)?;
    let category_id = parse_id(category_id).map_err(CommerceError::invalid)?;
    let body = call(
        transport,
        shop,
        now,
        CATEGORY_CONFIG,
        Some(json!({"categoryId": category_id})),
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    body["data"]
        .get("propConfigs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_one_attribute)
        .collect()
}

fn map_one_attribute(node: &Value) -> Option<CategoryAttribute> {
    let id = identifier(node, "propId")?;
    let name = text(node, "propName").unwrap_or_else(|| id.clone());
    let required = node.get("required").and_then(Value::as_bool).unwrap_or(false);
    let input_type = text(node, "propInputType").unwrap_or_default();
    let options = node
        .get("prePropValues")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let name = text(value, "propValue")?;
                    let id = identifier(value, "propValueId").unwrap_or_default();
                    Some(AttributeOption { id, name })
                })
                .collect()
        })
        .unwrap_or_default();
    let unit = node
        .get("unitProp")
        .and_then(Value::as_array)
        .and_then(|units| units.first())
        .and_then(|unit| text(unit, "unitPropValueName"));
    Some(CategoryAttribute {
        id,
        name,
        required,
        input: map_input(&input_type, node),
        options,
        unit,
    })
}

fn map_input(input_type: &str, node: &Value) -> AttributeInput {
    match input_type.to_ascii_uppercase().as_str() {
        "RADIO" => AttributeInput::Select,
        "CHECKBOX" => AttributeInput::MultiSelect,
        "TEXT" if numeric_text(node) => AttributeInput::Number,
        _ => AttributeInput::Text,
    }
}

fn numeric_text(node: &Value) -> bool {
    node["propInputConfig"]["inputFormatConfig"]
        .get("patternList")
        .and_then(Value::as_array)
        .is_some_and(|patterns| {
            patterns.iter().any(|pattern| {
                pattern.as_str().is_some_and(|value| value.contains("[0-9]") || value.contains("0-9"))
            })
        })
}

pub fn map_item_status(item: &Value) -> (ListingStatus, Option<String>) {
    let duplication = item.get("duplicationStatus").and_then(Value::as_i64);
    if matches!(duplication, Some(0)) {
        return (ListingStatus::Reviewing, None);
    }
    if matches!(duplication, Some(1 | 3)) {
        let reason = text(item, "duplicationReason").unwrap_or_else(|| "副本审核未通过".into());
        return (ListingStatus::Rejected, Some(reason));
    }
    let audit = item.get("auditStatus").and_then(Value::as_i64);
    let online = item.get("onOfflineStatus").and_then(Value::as_i64);
    match audit {
        Some(0) => (ListingStatus::Reviewing, None),
        Some(1) => (ListingStatus::Rejected, Some(text(item, "auditReason").unwrap_or_else(|| "审核待修改".into()))),
        Some(3) => (ListingStatus::Rejected, Some(text(item, "auditReason").unwrap_or_else(|| "审核拒绝".into()))),
        Some(2) => match online {
            Some(1) => (ListingStatus::Live, None),
            Some(0) => (ListingStatus::Live, Some("已下架".into())),
            _ => (ListingStatus::Reviewing, Some("审核通过但未返回上下架状态".into())),
        },
        Some(other) => (ListingStatus::Reviewing, Some(format!("未识别的审核状态 {other}"))),
        None => (ListingStatus::Reviewing, Some("列表未返回审核状态".into())),
    }
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    cursor: Option<&str>,
) -> Result<ProductPage, CommerceError> {
    ensure(shop)?;
    let page_number = match cursor {
        None | Some("") => 1,
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|page| *page > 0)
            .ok_or_else(|| CommerceError::invalid("商品分页游标无效"))?,
    };
    let body = call(
        transport,
        shop,
        now,
        ITEM_LIST,
        Some(json!({
            "pageNumber": page_number,
            "pageSize": PRODUCT_PAGE,
            "itemStatus": 1,
        })),
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let data = &body["data"];
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_product)
        .collect::<Vec<_>>();
    let total_page = data.get("totalPage").and_then(Value::as_i64).unwrap_or(page_number);
    let cursor = if items.is_empty() || page_number >= total_page {
        None
    } else {
        Some((page_number + 1).to_string())
    };
    Ok(ProductPage { items, cursor })
}

fn map_product(item: &Value) -> Option<RemoteProduct> {
    let remote_id = identifier(item, "kwaiItemId").or_else(|| identifier(item, "itemId"))?;
    let (status, reason) = map_item_status(item);
    let price = item.get("price").and_then(as_i64).map(|fen| fen as f64 / 100.0);
    Some(RemoteProduct {
        remote_id,
        title: text(item, "title").unwrap_or_default(),
        status,
        reason,
        currency: price.map(|_| "CNY".into()),
        price,
        stock: item.get("stock").and_then(as_i64),
    })
}

pub async fn fetch_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    range: MetricRange,
    cursor: Option<&str>,
) -> Result<OrderPage, CommerceError> {
    ensure(shop)?;
    let (start, end) = metric_window(range, now);
    let windows = chunks(start.saturating_mul(1000), end.saturating_mul(1000), ORDER_SPAN_MS);
    if windows.is_empty() {
        return Ok(OrderPage { items: Vec::new(), cursor: None });
    }
    let (index, api_cursor) = parse_order_cursor(cursor, windows.len()).map_err(CommerceError::invalid)?;
    let (start_ms, end_ms) = windows[index];
    let body = call(
        transport,
        shop,
        now,
        ORDER_LIST,
        Some(json!({
            "orderViewStatus": 1,
            "pageSize": ORDER_PAGE,
            "sort": 1,
            "queryType": 1,
            "cpsType": 0,
            "beginTime": start_ms,
            "endTime": end_ms,
            "cursor": api_cursor,
        })),
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let data = &body["data"];
    let items = data
        .get("orderList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_order)
        .map(|order| RemoteOrder {
            remote_id: order.oid,
            status: order.status.to_string(),
            currency: order.amount_fen.map(|_| "CNY".into()),
            amount: order.amount_fen.map(|fen| fen as f64 / 100.0),
            buyer: order.buyer,
            created_at: order
                .created_ms
                .and_then(|millis| chrono::DateTime::from_timestamp_millis(millis))
                .map(|time| time.to_rfc3339())
                .unwrap_or_default(),
        })
        .collect();
    let next = text(data, "cursor").unwrap_or_default();
    let cursor = if !next.is_empty() && next != "nomore" && next != api_cursor {
        Some(format!("{index}:{next}"))
    } else if index + 1 < windows.len() {
        Some(format!("{}:", index + 1))
    } else {
        None
    };
    Ok(OrderPage { items, cursor })
}

fn parse_order_cursor(cursor: Option<&str>, windows: usize) -> Result<(usize, String), String> {
    let Some(cursor) = cursor.filter(|value| !value.is_empty()) else {
        return Ok((0, String::new()));
    };
    let (index, api) = cursor.split_once(':').unwrap_or((cursor, ""));
    let index = index.parse::<usize>().map_err(|_| "订单分页游标无效".to_string())?;
    if index >= windows {
        return Err("订单分页游标无效".into());
    }
    Ok((index, api.to_string()))
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
    if shop.platform != Platform::Kuaishou {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    if let Some(remote_id) = existing_remote.filter(|value| !value.is_empty()) {
        return continue_existing(transport, shop, now, remote_id).await;
    }
    if let Err(message) = validate_draft(draft, target, images) {
        return outcome(None, ListingStatus::Rejected, Some(message));
    }
    let category_id = match parse_id(&target.category_id) {
        Ok(id) => id,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    let template_id = match express_template(transport, shop, now).await {
        Ok(id) => id,
        Err(error) => return from_fail(None, error),
    };
    let mut image_urls = Vec::new();
    for (name, bytes) in images {
        match upload_image(transport, shop, now, name, bytes).await {
            Ok(url) => image_urls.push(url),
            Err(error) => return from_fail(None, error),
        }
    }
    let config = match category_config(transport, shop, now, category_id).await {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    let refund_rule = config["data"]["categoryConfig"]
        .get("refundRuleList")
        .and_then(Value::as_array)
        .and_then(|rules| rules.first())
        .and_then(as_i64)
        .unwrap_or(1)
        .to_string();
    let attributes = match attribute_payload(&config, &target.attributes) {
        Ok(values) => values,
        Err(error) => return from_fail(None, error),
    };
    let mut payload = json!({
        "title": draft.title.trim(),
        "relItemId": rel_id(&draft.title, now),
        "categoryId": category_id,
        "imageUrls": image_urls,
        "details": draft.description.clone().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| draft.title.trim().to_string()),
        "skuList": sku_payload(draft, &image_urls),
        "serviceRule": {
            "refundRule": refund_rule,
            "immediatelyOnOfflineFlag": 0,
            "servicePromise": { "brokenRefund": true }
        },
        "expressTemplateId": template_id,
    });
    if !attributes.is_empty() {
        payload["itemPropValues"] = Value::Array(attributes);
    }
    let created = match call(transport, shop, now, ITEM_NEW, Some(payload), None, true).await {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    let remote_id = created.get("data").and_then(|data| identifier(data, "kwaiItemId"));
    if remote_id.is_none() {
        return outcome(None, ListingStatus::Uncertain, Some("快手小店已接受创建但未返回商品 ID".into()));
    }
    outcome(remote_id, ListingStatus::Reviewing, None)
}

fn validate_draft(draft: &ListingDraft, target: &ListingTarget, images: &[(String, Vec<u8>)]) -> Result<(), String> {
    if draft.title.trim().is_empty() {
        return Err("商品标题不能为空".into());
    }
    parse_id(&target.category_id)?;
    if images.is_empty() || images.iter().any(|(_, bytes)| bytes.is_empty()) {
        return Err("快手小店上架需要商品图片".into());
    }
    let skus: Vec<&ListingSku> = if draft.skus.is_empty() { Vec::new() } else { draft.skus.iter().collect() };
    if skus.is_empty() {
        require_price(draft.price)?;
        require_stock(draft.stock)?;
    } else {
        for sku in skus {
            require_price(sku.price.or(draft.price))?;
            require_stock(sku.stock.or(draft.stock))?;
        }
    }
    Ok(())
}

fn require_price(price: Option<f64>) -> Result<(), String> {
    match price {
        Some(value) if value > 0.0 => Ok(()),
        _ => Err("快手小店上架需要大于 0 的价格".into()),
    }
}

fn require_stock(stock: Option<i64>) -> Result<(), String> {
    match stock {
        Some(value) if value < 0 => Err("库存不能为负数".into()),
        _ => Ok(()),
    }
}

fn sku_payload(draft: &ListingDraft, image_urls: &[String]) -> Vec<Value> {
    let rows: Vec<(String, f64, i64, Option<String>)> = if draft.skus.is_empty() {
        vec![(
            "默认".into(),
            draft.price.unwrap_or(0.0),
            draft.stock.unwrap_or(0),
            None,
        )]
    } else {
        draft
            .skus
            .iter()
            .map(|sku| {
                let name = if sku.name.trim().is_empty() { "默认".into() } else { sku.name.trim().to_string() };
                (
                    name,
                    sku.price.or(draft.price).unwrap_or(0.0),
                    sku.stock.or(draft.stock).unwrap_or(0),
                    sku.code.clone(),
                )
            })
            .collect()
    };
    rows.into_iter()
        .enumerate()
        .map(|(index, (name, price, stock, code))| {
            let mut sku = json!({
                "relSkuId": index as i64 + 1,
                "skuStock": stock,
                "skuSalePrice": fen(price),
                "skuProps": [{
                    "propName": "规格",
                    "propValueName": name,
                    "isMainProp": 1,
                    "propVersion": 1,
                }],
            });
            if let Some(code) = code.filter(|value| !value.trim().is_empty()) {
                sku["skuNick"] = json!(code);
            }
            if let Some(url) = image_urls.first() {
                if let Some(props) = sku.get_mut("skuProps").and_then(Value::as_array_mut).and_then(|props| props.get_mut(0)) {
                    props["imageUrl"] = json!(url);
                }
            }
            sku
        })
        .collect()
}

fn attribute_payload(config: &Value, attributes: &[ListingAttribute]) -> Result<Vec<Value>, CallFail> {
    let mut inputs = BTreeMap::new();
    if let Some(list) = config["data"].get("propConfigs").and_then(Value::as_array) {
        for prop in list {
            if let Some(id) = identifier(prop, "propId") {
                inputs.insert(id, text(prop, "propInputType").unwrap_or_default());
            }
        }
    }
    let mut out = Vec::new();
    for attribute in attributes {
        let prop_id = attribute.id.parse::<i64>().map_err(|_| CallFail::Rejected(format!("属性 ID 无效：{}", attribute.id)))?;
        let input = inputs.get(&attribute.id).map(String::as_str).unwrap_or("RADIO");
        if input.eq_ignore_ascii_case("IMAGE") && (!attribute.value.trim().is_empty() || !attribute.values.is_empty()) {
            return Err(CallFail::Rejected(format!("属性 {}是图片，当前上架只提交文本和选项", attribute.id)));
        }
        if input.eq_ignore_ascii_case("CHECKBOX") {
            let values = if attribute.values.is_empty() { vec![attribute.value.clone()] } else { attribute.values.clone() };
            let list: Vec<Value> = values
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .map(|value| json!({"propValueId": 0, "propValue": value}))
                .collect();
            if list.is_empty() {
                continue;
            }
            out.push(json!({"propId": prop_id, "checkBoxPropValuesList": list}));
            continue;
        }
        let value = attribute.value.trim();
        if value.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("TEXT") {
            out.push(json!({"propId": prop_id, "textPropValue": value}));
        } else {
            out.push(json!({
                "propId": prop_id,
                "radioPropValue": {"propValueId": 0, "propValue": value},
            }));
        }
    }
    Ok(out)
}

async fn continue_existing(transport: &Transport, shop: &ResolvedShop, now: i64, remote_id: &str) -> PushOutcome {
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
    ensure(shop)?;
    if remote_id.trim().is_empty() {
        return Err(CommerceError::invalid("缺少快手商品 ID"));
    }
    item_status(transport, shop, now, remote_id).await.map_err(fail_to_error)
}

async fn item_status(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let kwai_item_id = remote_id.parse::<i64>().map_err(|_| CallFail::Rejected("快手商品 ID 无效".into()))?;
    let body = call(transport, shop, now, ITEM_GET, Some(json!({"kwaiItemId": kwai_item_id})), None, false).await?;
    let item = body.get("data").filter(|data| data.is_object()).ok_or_else(|| CallFail::Failed("快手小店未返回商品".into()))?;
    if item.get("auditStatus").is_none() && item.get("onOfflineStatus").is_none() {
        return Err(CallFail::Failed("快手小店未返回商品状态".into()));
    }
    Ok(map_item_status(item))
}

async fn express_template(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<i64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        TEMPLATE_LIST,
        Some(json!({"offset": 0, "limit": 10, "searchUsed": false})),
        None,
        false,
    )
    .await?;
    body["data"]
        .get("expressTemplateDetailVOS")
        .and_then(Value::as_array)
        .and_then(|templates| {
            let usable = |template: &&Value| {
                template.get("deleteTime").and_then(Value::as_i64).unwrap_or(0) == 0
                    && identifier(template, "id").is_some()
            };
            templates
                .iter()
                .filter(usable)
                .find(|template| template.get("status").and_then(Value::as_i64) == Some(1))
                .or_else(|| templates.iter().find(usable))
        })
        .and_then(|template| identifier(template, "id"))
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| CallFail::Rejected("没有可用的快手运费模板".into()))
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
        IMAGE_UPLOAD,
        Some(json!({"uploadType": 1})),
        Some((name.to_string(), bytes.to_vec())),
        false,
    )
    .await?;
    text(&body["data"], "kwaiImgUrl").ok_or_else(|| CallFail::Rejected("快手小店未返回图片地址".into()))
}

async fn category_config(transport: &Transport, shop: &ResolvedShop, now: i64, category_id: i64) -> Result<Value, CallFail> {
    call(transport, shop, now, CATEGORY_CONFIG, Some(json!({"categoryId": category_id})), None, false).await
}

fn rel_id(title: &str, now: i64) -> i64 {
    let mut hash = now.max(1);
    for byte in title.bytes() {
        hash = hash.wrapping_mul(131).wrapping_add(i64::from(byte));
    }
    let positive = hash & 0x7fff_ffff_ffff_ffff;
    if positive == 0 { 1 } else { positive }
}

fn fen(yuan: f64) -> i64 {
    (yuan * 100.0).round() as i64
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

fn ensure(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform == Platform::Kuaishou {
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

fn parse_id(value: &str) -> Result<i64, String> {
    value.trim().parse::<i64>().map_err(|_| "类目 ID 无效".to_string())
}

pub fn sign_form(secret: &str, pairs: &[(String, String)]) -> Result<String, String> {
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
    crate::workbench::shops::sign_kuaishou(secret, &refs)
}

async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    method_name: &'static str,
    param: Option<Value>,
    file: Option<(String, Vec<u8>)>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let timestamp = now.saturating_mul(1000).to_string();
    let mut pairs = vec![
        ("method".into(), method_name.to_string()),
        ("appkey".into(), shop.partner_id.clone()),
        ("access_token".into(), shop.access_token.clone()),
        ("version".into(), "1".into()),
        ("signMethod".into(), "HMAC-SHA256".into()),
        ("timestamp".into(), timestamp),
    ];
    if let Some(param) = &param {
        let encoded = serde_json::to_string(param).map_err(|_| CallFail::Failed("快手请求参数编码失败".into()))?;
        pairs.push(("param".into(), encoded));
    }
    let sign = sign_form(&shop.partner_key, &pairs).map_err(CallFail::Failed)?;
    pairs.push(("sign".into(), sign));
    let headers = if file.is_some() {
        vec![("x-dsivio-file-field".into(), "imgBytes".into())]
    } else {
        vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())]
    };
    let (file_name, file_bytes) = match file {
        Some((name, bytes)) => (Some(name), Some(bytes)),
        None => (None, None),
    };
    let request = Outbound {
        method: "POST",
        path: format!("/{}", method_name.replace('.', "/")),
        query: pairs,
        json: None,
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
    classify(mutating, inbound)
}

fn classify(mutating: bool, inbound: Inbound) -> Result<Value, CallFail> {
    let error = inbound.body.get("error").and_then(Value::as_str).unwrap_or("");
    let message = inbound
        .body
        .get("error_msg")
        .and_then(Value::as_str)
        .or_else(|| inbound.body.get("error_description").and_then(Value::as_str))
        .or_else(|| inbound.body.get("msg").and_then(Value::as_str))
        .unwrap_or("");
    let result = inbound.body.get("result").and_then(as_i64);
    let text = match (error.is_empty(), message.is_empty()) {
        (true, true) => format!("HTTP {}", inbound.status),
        (false, true) => error.to_string(),
        (true, false) => message.to_string(),
        (false, false) => format!("{error}: {message}"),
    };
    if inbound.status >= 500 || inbound.status == 408 || inbound.status == 429 {
        return if mutating { Err(CallFail::Uncertain(text)) } else { Err(CallFail::Failed(text)) };
    }
    if !error.is_empty() || result.is_some_and(|code| code != 1) || inbound.status >= 400 {
        if is_token_auth(error, message) {
            return Err(CallFail::Auth(text));
        }
        return Err(CallFail::Rejected(text));
    }
    if result != Some(1) {
        return if mutating {
            Err(CallFail::Uncertain("快手小店未返回结果".into()))
        } else {
            Err(CallFail::Failed("快手小店未返回结果".into()))
        };
    }
    Ok(inbound.body)
}

fn is_token_auth(error: &str, message: &str) -> bool {
    let blob = format!("{error} {message}").to_ascii_lowercase();
    blob.contains("access_token")
        || blob.contains("invalid_token")
        || blob.contains("invalid_grant")
        || blob.contains("token过期")
        || blob.contains("token失效")
        || message.contains("授权")
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

fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|item| i64::try_from(item).ok()))
        .or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

#[cfg(test)]
#[path = "kuaishou_tests.rs"]
mod kuaishou_tests;
