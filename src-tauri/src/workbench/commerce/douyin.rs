//! 抖店开放平台适配器。签名串与 `shops/providers.rs` 的 `doudian_sign_pattern` 相同：
//! `app_secret + app_key…method…param_json…timestamp…v2 + app_secret`，HMAC-SHA256 小写十六进制。
//! 商务调用的 timestamp 用文档要求的 GMT+8 `yyyy-MM-dd HH:mm:ss`，不把 token.create 的 unix 秒套到这里。
//!
//! - 调用指南：<https://op.jinritemai.com/docs/guide-docs/10/23>
//! - 错误码：<https://op.jinritemai.com/docs/guide-docs/212/1427>
//! - 类目：<https://op.jinritemai.com/docs/api-docs/13/1820> `shop.getShopCategory`
//! - 类目属性：<https://op.jinritemai.com/docs/api-docs/14/1373> `product.getCatePropertyV2`
//! - 发布：<https://op.jinritemai.com/docs/api-docs/14/249> `product.addV2`
//! - 编辑：<https://op.jinritemai.com/docs/api-docs/14/250> `product.editV2`
//! - 详情：<https://op.jinritemai.com/docs/api-docs/14/56> `product.detail`
//! - 列表：<https://op.jinritemai.com/docs/api-docs/14/633> `product.listV2`
//! - 状态机：<https://op.jinritemai.com/docs/question-docs/92/2070>
//! - 图片：<https://op.jinritemai.com/docs/api-docs/69/1146> `material.uploadImageSync`
//!   与 <https://op.jinritemai.com/docs/api-docs/69/1145> `material.queryMaterialDetail`
//!   （对接说明 <https://op.jinritemai.com/docs/guide-docs/171/1719>）
//! - 订单：<https://op.jinritemai.com/docs/api-docs/15/1342> `order.searchList`
//! - 售后：<https://op.jinritemai.com/docs/api-docs/17/1295> `afterSale.List`
//!
//! 测试用脚本传输重放上述文档里的响应形状，不是真实店铺抓包。

use super::adapter::{self, ApiSession, HttpBody, HttpRequest, SharedTransport};
use super::shopee::PushOutcome;
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities, CommerceError, CommerceOrder,
    CommerceProduct, ListingAttribute, ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange, OrderPage,
    ProductPage, ShopMetrics,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

const HOST: &str = "https://openapi-fxg.jinritemai.com";
const PAGE: i64 = 100;
const MAX_PAGES: i64 = 20;
const OFFSET: i64 = 8 * 3600;
/// 文档：无品牌 id 则传 596120136。
const NO_BRAND_ID: i64 = 596_120_136;
const CONTROL_ATTRS: [&str; 6] =
    ["mobile", "reduce_type", "freight_id", "product_type", "delivery_delay_day", "standard_brand_id"];

#[derive(Clone, Debug)]
enum CallFail {
    Auth(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
    Unsupported(String),
}

impl CallFail {
    fn text(&self) -> &str {
        match self {
            Self::Auth(message)
            | Self::Rejected(message)
            | Self::Failed(message)
            | Self::Uncertain(message)
            | Self::Unsupported(message) => message,
        }
    }
}

struct SkuRow {
    name: String,
    price_fen: i64,
    stock: i64,
    code: Option<String>,
}

struct Prepared {
    name: String,
    urls: Vec<String>,
    mobile: String,
    reduce_type: i64,
    product_type: i64,
    freight_id: i64,
    brand_id: i64,
    delivery_delay_day: Option<i64>,
    weight_kg: Option<f64>,
    skus: Vec<SkuRow>,
    format_new: Option<String>,
    category_leaf_id: i64,
}

/// 签名基串（不含首尾 app_secret）。access_token 与 sign_method 不参与签名。
pub fn sign_base(app_key: &str, method: &str, param_json: &str, timestamp: &str) -> String {
    crate::workbench::shops::doudian_sign_pattern(app_key, method, param_json, timestamp)
}

pub fn sign_param(
    app_secret: &str,
    app_key: &str,
    method: &str,
    param_json: &str,
    timestamp: &str,
) -> Result<String, String> {
    crate::workbench::shops::sign_doudian(app_secret, app_key, method, param_json, timestamp)
}

pub fn gmt8_timestamp(now: i64) -> String {
    chrono::DateTime::from_timestamp(now + OFFSET, 0)
        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "1970-01-01 08:00:00".into())
}

pub fn capabilities() -> CommerceCapabilities {
    CommerceCapabilities {
        platform: Platform::Douyin,
        metrics: MetricKey::ALL.to_vec(),
        listing: true,
        categories: true,
        products: true,
        orders: true,
        notes: vec![
            "指标按 GMT+8。GMV 为 order.searchList 的 pay_amount（分）换成元。退款为 afterSale.List 的 refund_amount。买家为 doudian_open_id。在售商品为 product.listV2 的 status=0 且 check_status=3。图片必须是 HTTPS。mobile 与 reduce_type 通过类目属性传入。freight_id 缺省为 0。无品牌 ID 为 596120136。".into(),
        ],
    }
}

pub async fn metrics(
    http: &SharedTransport,
    session: &ApiSession,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    ensure_douyin(session)?;
    let (start, end) = metric_window(range, now);
    let orders = match list_orders(http, session, now, start, end).await {
        Ok(items) => Ok(items),
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(other) => Err(other.text().to_string()),
    };
    let refunds = match list_refunds(http, session, now, start, end).await {
        Ok(items) => Ok(items),
        Err(_) => Err(()),
    };
    let live = match live_count(http, session, now).await {
        Ok(count) => Some(count),
        Err(CallFail::Auth(message)) if orders.as_ref().ok().is_some_and(|items| items.is_empty()) => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => None,
    };
    Ok(fold_metrics(&session.shop_id, range, now, orders, refunds, live))
}

pub async fn categories(
    http: &SharedTransport,
    session: &ApiSession,
    parent_id: Option<&str>,
    now: i64,
) -> Result<Vec<Category>, CommerceError> {
    ensure_douyin(session)?;
    let parent = parent_id.unwrap_or("").trim();
    let cid = if parent.is_empty() {
        0
    } else {
        match parent.parse::<i64>() {
            Ok(cid) => cid,
            Err(_) => return Err(CommerceError::invalid("类目 ID 无效")),
        }
    };
    let body = call(http, session, now, "shop.getShopCategory", json!({"cid": cid}), false).await.map_err(fail_to_error)?;
    Ok(map_categories(&body))
}

pub fn map_categories(body: &Value) -> Vec<Category> {
    rows(body, "data")
        .iter()
        .filter(|item| item.get("enable").and_then(Value::as_bool).unwrap_or(true))
        .filter_map(|item| {
            let id = identifier(item, "id")?;
            let name = text(item, "name").unwrap_or_else(|| id.clone());
            let parent_id = identifier(item, "parent_id").unwrap_or_else(|| "0".into());
            let leaf = item.get("is_leaf").and_then(Value::as_bool).unwrap_or(false);
            Some(Category { id, name, parent_id, leaf })
        })
        .collect()
}

pub async fn attributes(
    http: &SharedTransport,
    session: &ApiSession,
    category_id: &str,
    now: i64,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure_douyin(session)?;
    if category_id.trim().is_empty() || category_id.parse::<i64>().is_err() {
        return Err(CommerceError::invalid("类目 ID 无效"));
    }
    let body = call(
        http,
        session,
        now,
        "product.getCatePropertyV2",
        json!({"category_leaf_id": category_id.parse::<i64>().unwrap_or(0)}),
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    rows(body, "data").iter().filter_map(map_one_attribute).collect()
}

pub async fn push_listing(
    http: &SharedTransport,
    session: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    now: i64,
    existing_remote: Option<&str>,
) -> PushOutcome {
    if session.platform != Platform::Douyin {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    let prepared = match prepare(draft, target, images) {
        Ok(prepared) => prepared,
        Err(message) => return outcome(existing_remote.map(str::to_owned), ListingStatus::Rejected, Some(message)),
    };
    if let Some(remote) = existing_remote {
        if remote.parse::<i64>().is_err() {
            return outcome(Some(remote.to_string()), ListingStatus::Rejected, Some("商品 ID 无效".into()));
        }
    }
    let mut uploaded = Vec::new();
    for (index, url) in prepared.urls.iter().enumerate() {
        match upload_image(http, session, now, index, url).await {
            Ok(byte_url) => uploaded.push(byte_url),
            Err(error) => return from_fail(existing_remote.map(str::to_owned), error),
        }
    }
    let method = if existing_remote.is_some() { "product.editV2" } else { "product.addV2" };
    let param = listing_param(&prepared, &uploaded, existing_remote);
    match call(http, session, now, method, param, true).await {
        Ok(body) => {
            let remote_id = body["data"].get("product_id").and_then(identifier_value).or_else(|| existing_remote.map(str::to_owned));
            if remote_id.is_none() {
                return outcome(None, ListingStatus::Rejected, Some("抖店未返回商品 ID".into()));
            }
            outcome(remote_id, ListingStatus::Reviewing, None)
        }
        Err(error) => from_fail(existing_remote.map(str::to_owned), error),
    }
}

pub async fn listing_status(
    http: &SharedTransport,
    session: &ApiSession,
    remote_id: &str,
    now: i64,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_douyin(session)?;
    product_status(http, session, now, remote_id).await.map_err(fail_to_error)
}

/// 游标是 product.listV2 的页码，从 1 开始；空游标是第一页。每页 100 条，page*size 不超过 10000。
pub async fn products(
    http: &SharedTransport,
    session: &ApiSession,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    ensure_douyin(session)?;
    let page = page_cursor(cursor, 1)?;
    if page < 1 || page.saturating_mul(PAGE) > 10_000 {
        return Err(CommerceError::invalid("商品游标需为从 1 开始的页码，且不超过 10000 条"));
    }
    let body = call(http, session, now, "product.listV2", json!({"page": page, "size": PAGE}), false)
        .await
        .map_err(fail_to_error)?;
    let data = &body["data"];
    let rows = data.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
    let items = rows.iter().filter_map(map_listed_product).collect::<Vec<_>>();
    let total = as_i64(&data["total"]);
    Ok(ProductPage {
        shop_id: session.shop_id.clone(),
        items,
        next_cursor: next_page(page.saturating_mul(PAGE), rows.len() as i64, total, page + 1, true),
    })
}

/// 游标是 order.searchList 的页码，从 0 开始；空游标是第一页。时间窗按 GMT+8 的指标区间。
pub async fn orders(
    http: &SharedTransport,
    session: &ApiSession,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    ensure_douyin(session)?;
    let page = page_cursor(cursor, 0)?;
    if page < 0 {
        return Err(CommerceError::invalid("订单游标需为从 0 开始的页码"));
    }
    let (start, end) = metric_window(range, now);
    let body = call(
        http,
        session,
        now,
        "order.searchList",
        json!({"create_time_end": end, "create_time_start": start, "page": page, "size": PAGE}),
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let data = &body["data"];
    let rows = data.get("shop_order_list").and_then(Value::as_array).cloned().unwrap_or_default();
    let items = rows.iter().filter_map(map_listed_order).collect::<Vec<_>>();
    let total = as_i64(&data["total"]);
    Ok(OrderPage {
        shop_id: session.shop_id.clone(),
        range,
        items,
        next_cursor: next_page((page + 1).saturating_mul(PAGE), rows.len() as i64, total, page + 1, false),
    })
}

fn page_cursor(cursor: Option<&str>, origin: i64) -> Result<i64, CommerceError> {
    match cursor.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(origin),
        Some(value) => value.parse::<i64>().map_err(|_| CommerceError::invalid("分页游标无效")),
    }
}

fn next_page(covered: i64, count: i64, total: Option<i64>, next: i64, product: bool) -> Option<String> {
    if count < PAGE {
        return None;
    }
    if total.is_some_and(|total| covered >= total) {
        return None;
    }
    if product && next.saturating_mul(PAGE) > 10_000 {
        return None;
    }
    Some(next.to_string())
}

/// 商品状态机：https://op.jinritemai.com/docs/question-docs/92/2070
pub fn map_product_status(status: i64, check_status: i64, draft_status: i64) -> (ListingStatus, Option<String>) {
    if check_status == 5 {
        return (ListingStatus::Banned, Some("封禁".into()));
    }
    if status == -2 {
        return (ListingStatus::Rejected, Some("彻底删除".into()));
    }
    if status == 2 {
        return (ListingStatus::Rejected, Some("删除".into()));
    }
    if status == -1 {
        return (ListingStatus::Failed, Some("系统异常，废弃商品".into()));
    }
    if check_status == 4 {
        return (ListingStatus::Rejected, Some("审核未通过".into()));
    }
    if status == 0 && check_status == 3 {
        let reason = (draft_status == 2).then(|| "新版本审核中".to_string());
        return (ListingStatus::Live, reason);
    }
    if check_status == 2 || draft_status == 2 {
        return (ListingStatus::Reviewing, Some("待审核".into()));
    }
    if check_status == 7 {
        return (ListingStatus::Reviewing, Some("审核通过待上架".into()));
    }
    if status == 1 && check_status == 3 {
        return (ListingStatus::Live, Some("下线".into()));
    }
    if status == 1 && check_status == 1 {
        return (ListingStatus::Rejected, Some("已下架".into()));
    }
    if check_status == 1 {
        return (ListingStatus::Reviewing, Some("未提审".into()));
    }
    (
        ListingStatus::Reviewing,
        Some(format!("未识别的商品状态 status={status} check_status={check_status} draft_status={draft_status}")),
    )
}

fn metric_window(range: MetricRange, now: i64) -> (i64, i64) {
    let local = now + OFFSET;
    let day = local - local.rem_euclid(86_400);
    let (start_local, end_local) = match range {
        MetricRange::Today => (day, local),
        MetricRange::Yesterday => (day - 86_400, day - 1),
        MetricRange::Last7 => (day - 6 * 86_400, local),
        MetricRange::Last30 => (day - 29 * 86_400, local),
    };
    (start_local - OFFSET, end_local - OFFSET)
}

struct OrderView {
    status: i64,
    amount_fen: Option<i64>,
    buyer: Option<String>,
}

struct RefundView {
    amount_fen: Option<i64>,
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: Result<Vec<OrderView>, String>,
    refunds: Result<Vec<RefundView>, ()>,
    live: Option<f64>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    let mut metrics = super::types::zero_metrics(shop_id.to_string(), range, fetched_at);
    let mut unsupported = Vec::new();
    match orders {
        Err(message) => {
            unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
            metrics.error = Some(message);
        }
        Ok(items) => {
            let counted: Vec<&OrderView> = items.iter().filter(|order| counts_order(order.status)).collect();
            metrics.values.insert(MetricKey::Orders, counted.len() as f64);
            metrics.values.insert(
                MetricKey::PendingShipment,
                counted.iter().filter(|order| order.status == 2).count() as f64,
            );
            if counted.is_empty() {
                metrics.values.insert(MetricKey::Gmv, 0.0);
                metrics.values.insert(MetricKey::Buyers, 0.0);
            } else if counted.iter().any(|order| order.amount_fen.is_none()) {
                unsupported.push(MetricKey::Gmv);
            } else {
                let fen: i64 = counted.iter().filter_map(|order| order.amount_fen).sum();
                metrics.values.insert(MetricKey::Gmv, fen as f64 / 100.0);
                metrics.currency = Some("CNY".into());
            }
            if !counted.is_empty() && counted.iter().any(|order| order.buyer.is_none()) {
                unsupported.push(MetricKey::Buyers);
            } else if !counted.is_empty() {
                let buyers: BTreeSet<&str> = counted.iter().filter_map(|order| order.buyer.as_deref()).collect();
                metrics.values.insert(MetricKey::Buyers, buyers.len() as f64);
            }
        }
    }
    match refunds {
        Err(()) => unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]),
        Ok(items) => {
            metrics.values.insert(MetricKey::RefundOrders, items.len() as f64);
            if items.iter().any(|item| item.amount_fen.is_none()) {
                unsupported.push(MetricKey::RefundAmount);
            } else {
                let fen: i64 = items.iter().filter_map(|item| item.amount_fen).sum();
                metrics.values.insert(MetricKey::RefundAmount, fen as f64 / 100.0);
            }
        }
    }
    match live {
        Some(count) => {
            metrics.values.insert(MetricKey::ProductsLive, count);
        }
        None => unsupported.push(MetricKey::ProductsLive),
    }
    unsupported.sort();
    unsupported.dedup();
    metrics.unsupported = unsupported;
    metrics
}

fn counts_order(status: i64) -> bool {
    matches!(status, 2 | 3 | 5 | 101 | 105)
}

async fn list_orders(
    http: &SharedTransport,
    session: &ApiSession,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<OrderView>, CallFail> {
    let mut page = 0;
    let mut items = Vec::new();
    let mut truncated = false;
    for _ in 0..MAX_PAGES {
        let body = call(
            http,
            session,
            now,
            "order.searchList",
            json!({
                "create_time_end": end,
                "create_time_start": start,
                "page": page,
                "size": PAGE,
            }),
            false,
        )
        .await?;
        let list = body["data"].get("shop_order_list").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = as_i64(&body["data"]["total"]);
        items.extend(list.iter().map(|item| OrderView {
            status: as_i64(&item["order_status"]).unwrap_or(0),
            amount_fen: as_i64(&item["pay_amount"]),
            buyer: text(item, "doudian_open_id"),
        }));
        page += 1;
        if list.is_empty() || total.is_some_and(|total| items.len() as i64 >= total) {
            truncated = false;
            break;
        }
        truncated = true;
    }
    if truncated {
        return Err(CallFail::Failed("订单分页未取完，不以部分结果冒充指标".into()));
    }
    Ok(items)
}

async fn list_refunds(
    http: &SharedTransport,
    session: &ApiSession,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<RefundView>, CallFail> {
    let mut page = 0;
    let mut items = Vec::new();
    let mut truncated = false;
    for _ in 0..MAX_PAGES {
        let body = call(
            http,
            session,
            now,
            "afterSale.List",
            json!({
                "end_time": end.saturating_add(1),
                "page": page,
                "size": PAGE,
                "start_time": start,
            }),
            false,
        )
        .await?;
        let list = body["data"].get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let has_more = body["data"].get("has_more").and_then(Value::as_bool);
        for item in &list {
            let info = &item["aftersale_info"];
            if !counts_refund(as_i64(&info["aftersale_type"]).unwrap_or(-1), as_i64(&info["aftersale_status"]).unwrap_or(0), as_i64(&info["refund_status"]).unwrap_or(0))
            {
                continue;
            }
            items.push(RefundView { amount_fen: as_i64(&info["refund_amount"]) });
        }
        page += 1;
        if list.is_empty() || has_more == Some(false) || (has_more.is_none() && (list.len() as i64) < PAGE) {
            truncated = false;
            break;
        }
        truncated = true;
    }
    if truncated {
        return Err(CallFail::Failed("售后分页未取完".into()));
    }
    Ok(items)
}

fn counts_refund(aftersale_type: i64, aftersale_status: i64, refund_status: i64) -> bool {
    matches!(aftersale_type, 0 | 1 | 2 | 6) && (aftersale_status == 12 || refund_status == 3)
}

async fn live_count(http: &SharedTransport, session: &ApiSession, now: i64) -> Result<f64, CallFail> {
    let body = call(
        http,
        session,
        now,
        "product.listV2",
        json!({"check_status": 3, "page": 1, "size": 1, "status": 0}),
        false,
    )
    .await?;
    as_i64(&body["data"]["total"])
        .map(|count| count as f64)
        .ok_or_else(|| CallFail::Failed("抖店未返回在售商品总数".into()))
}

fn map_one_attribute(node: &Value) -> Option<CategoryAttribute> {
    if as_i64(&node["status"]) == Some(1) {
        return None;
    }
    let id = identifier(node, "property_id")?;
    let name = text(node, "property_name").unwrap_or_else(|| id.clone());
    let required = as_i64(&node["required"]) == Some(1);
    let options = node
        .get("options")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let id = identifier(value, "value_id").or_else(|| text(value, "value"))?;
                    let name = text(value, "name").unwrap_or_else(|| id.clone());
                    Some(AttributeOption { id, name })
                })
                .collect::<Vec<AttributeOption>>()
        })
        .unwrap_or_default();
    let kind = text(node, "type").unwrap_or_default();
    let input = match kind.as_str() {
        "multi_select" => AttributeInput::MultiSelect,
        "select" if !options.is_empty() => AttributeInput::Select,
        "text" if numeric_measure(node) => AttributeInput::Number,
        _ => AttributeInput::Text,
    };
    let unit = node
        .get("measure_templates")
        .and_then(Value::as_array)
        .and_then(|templates| templates.first())
        .and_then(|template| template.get("value_modules"))
        .and_then(Value::as_array)
        .and_then(|modules| modules.first())
        .and_then(|module| module.get("units"))
        .and_then(Value::as_array)
        .and_then(|units| units.first())
        .and_then(|unit| text(unit, "unit_name"));
    Some(CategoryAttribute { id, name, required, input, options, unit })
}

fn numeric_measure(node: &Value) -> bool {
    node.get("measure_templates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|template| template.get("value_modules").and_then(Value::as_array).into_iter().flatten())
        .any(|module| {
            module
                .pointer("/validate_rule/data_type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "integer" || kind == "float")
        })
}

fn prepare(draft: &ListingDraft, target: &ListingTarget, images: &[(String, Vec<u8>)]) -> Result<Prepared, String> {
    let name = draft.title.trim();
    if name.is_empty() {
        return Err("商品标题不能为空".into());
    }
    let units = title_units(name);
    if !(8..=60).contains(&units) {
        return Err("抖店标题需为 8 到 60 个字符（汉字计 2 个）".into());
    }
    if has_emoji(name) {
        return Err("抖店标题不能含表情".into());
    }
    if let Some(currency) = draft.currency.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        if !matches!(currency.to_ascii_uppercase().as_str(), "CNY" | "RMB") {
            return Err("抖店售价单位是人民币，不接受其他币种".into());
        }
    }
    let category_leaf_id = target.category_id.trim().parse::<i64>().map_err(|_| "类目 ID 无效".to_string())?;
    let mobile = attr_value(target, "mobile").ok_or("缺少客服电话，请在类目属性中填写 mobile")?;
    let reduce_type = attr_value(target, "reduce_type")
        .ok_or("缺少减库存类型，请在类目属性 reduce_type 填写 1（拍下减）或 2（付款减）")?
        .parse::<i64>()
        .map_err(|_| "reduce_type 只能是 1 或 2".to_string())?;
    if !matches!(reduce_type, 1 | 2) {
        return Err("reduce_type 只能是 1 或 2".into());
    }
    let product_type = match attr_value(target, "product_type") {
        None => 0,
        Some(value) => {
            let parsed = value.parse::<i64>().map_err(|_| "product_type 无效".to_string())?;
            if !matches!(parsed, 0 | 3 | 6 | 7) {
                return Err("product_type 只能是 0、3、6 或 7".into());
            }
            parsed
        }
    };
    let freight_id = match attr_value(target, "freight_id") {
        None => 0,
        Some(value) => value.parse::<i64>().map_err(|_| "freight_id 无效".to_string())?,
    };
    if freight_id < 0 {
        return Err("freight_id 无效".into());
    }
    let delivery_delay_day = match attr_value(target, "delivery_delay_day") {
        None => None,
        Some(value) => Some(value.parse::<i64>().map_err(|_| "delivery_delay_day 无效".to_string())?),
    };
    let brand_id = match attr_value(target, "standard_brand_id") {
        Some(value) => value.parse::<i64>().map_err(|_| "standard_brand_id 需为品牌 ID".to_string())?,
        None => match draft.brand.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            None => NO_BRAND_ID,
            Some(value) => value.parse::<i64>().map_err(|_| {
                "品牌需填写品牌 ID；无品牌请留空，将使用文档中的无品牌 ID 596120136".to_string()
            })?,
        },
    };
    let urls = public_urls(draft, images)?;
    let skus = sku_rows(draft)?;
    let format_new = product_format(target)?;
    Ok(Prepared {
        name: name.to_string(),
        urls,
        mobile,
        reduce_type,
        product_type,
        freight_id,
        brand_id,
        delivery_delay_day,
        weight_kg: draft.weight_kg,
        skus,
        format_new,
        category_leaf_id,
    })
}

fn public_urls(draft: &ListingDraft, images: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
    let mut urls: Vec<String> = draft
        .images
        .iter()
        .map(|image| image.trim())
        .filter(|image| image.starts_with("https://"))
        .map(str::to_owned)
        .collect();
    if urls.is_empty() {
        urls = images
            .iter()
            .map(|(name, _)| name.trim())
            .filter(|name| name.starts_with("https://"))
            .map(str::to_owned)
            .collect();
    }
    if urls.is_empty() {
        return Err("抖店图片需要公网 HTTPS 地址（material.uploadImageSync 的 url）。接口不接收本地二进制".into());
    }
    if urls.len() > 5 {
        return Err("抖店轮播图最多 5 张".into());
    }
    Ok(urls)
}

fn sku_rows(draft: &ListingDraft) -> Result<Vec<SkuRow>, String> {
    let source = if draft.skus.is_empty() {
        vec![SkuRow {
            name: "默认".into(),
            price_fen: yuan_to_fen(draft.price).ok_or("缺少售价")?,
            stock: draft.stock.unwrap_or(0),
            code: None,
        }]
    } else {
        draft
            .skus
            .iter()
            .map(|sku| {
                let name = if sku.name.trim().is_empty() { "默认".into() } else { sku.name.trim().to_string() };
                Ok(SkuRow {
                    name,
                    price_fen: yuan_to_fen(sku.price.or(draft.price)).ok_or("缺少售价")?,
                    stock: sku.stock.or(draft.stock).unwrap_or(0),
                    code: sku.code.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned),
                })
            })
            .collect::<Result<Vec<_>, &str>>()?
    };
    if source.len() > 1 && source.iter().any(|sku| sku.name == "默认" && draft.skus.iter().any(|item| item.name.trim().is_empty())) {
        return Err("多规格时每个 SKU 都需要名称".into());
    }
    if source.iter().any(|sku| sku.name.chars().any(|ch| matches!(ch, ',' | '|' | '^'))) {
        return Err("SKU 名称不能包含逗号、竖线或 ^，specs 用这些字符分隔".into());
    }
    let mut seen = BTreeSet::new();
    for sku in &source {
        if !seen.insert(sku.name.clone()) {
            return Err("SKU 名称不能重复".into());
        }
        if sku.stock < 0 {
            return Err("库存不能为负".into());
        }
    }
    Ok(source)
}

fn yuan_to_fen(yuan: Option<f64>) -> Option<i64> {
    let yuan = yuan?;
    if !yuan.is_finite() || yuan < 0.0 {
        return None;
    }
    Some((yuan * 100.0).round() as i64)
}

fn product_format(target: &ListingTarget) -> Result<Option<String>, String> {
    let mut object = Map::new();
    for attribute in &target.attributes {
        if CONTROL_ATTRS.contains(&attribute.id.as_str()) {
            continue;
        }
        if attribute.id.trim().is_empty() {
            continue;
        }
        let values = property_values(attribute);
        if values.is_empty() {
            continue;
        }
        object.insert(attribute.id.clone(), Value::Array(values));
    }
    if object.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&Value::Object(object)).map(Some).map_err(|_| "类目属性编码失败".to_string())
}

fn property_values(attribute: &ListingAttribute) -> Vec<Value> {
    let id_text = attribute.value.trim();
    if attribute.values.len() == 1 && id_text.parse::<i64>().ok().is_some_and(|id| id > 0) {
        return vec![json!({"diy_type": 0, "name": attribute.values[0].trim(), "value": id_text.parse::<i64>().unwrap_or(0)})];
    }
    let mut texts: Vec<String> = attribute.values.iter().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()).collect();
    if texts.is_empty() && !id_text.is_empty() {
        texts.push(id_text.to_string());
    }
    texts
        .into_iter()
        .map(|text| match text.parse::<i64>() {
            Ok(id) if id > 0 => json!({"diy_type": 0, "name": text, "value": id}),
            _ => json!({"diy_type": 1, "name": text, "value": 0}),
        })
        .collect()
}

fn listing_param(prepared: &Prepared, urls: &[String], existing_remote: Option<&str>) -> Value {
    let joined = urls.join("|");
    let names = prepared.skus.iter().map(|sku| sku.name.as_str()).collect::<Vec<_>>().join(",");
    let prices: Vec<Value> = prepared
        .skus
        .iter()
        .map(|sku| {
            let mut row = Map::new();
            if let Some(code) = &sku.code {
                row.insert("code".into(), json!(code));
            }
            row.insert("price".into(), json!(sku.price_fen));
            row.insert("spec_detail_name1".into(), json!(sku.name));
            row.insert("stock_num".into(), json!(sku.stock));
            Value::Object(row)
        })
        .collect();
    let mut param = Map::new();
    param.insert("category_leaf_id".into(), json!(prepared.category_leaf_id));
    param.insert("commit".into(), json!(true));
    param.insert("description".into(), json!(joined));
    if let Some(day) = prepared.delivery_delay_day {
        param.insert("delivery_delay_day".into(), json!(day));
    }
    param.insert("freight_id".into(), json!(prepared.freight_id));
    param.insert("mobile".into(), json!(prepared.mobile));
    param.insert("name".into(), json!(prepared.name));
    param.insert("pic".into(), json!(joined));
    if let Some(format_new) = &prepared.format_new {
        param.insert("product_format_new".into(), json!(format_new));
    }
    param.insert("product_type".into(), json!(prepared.product_type));
    param.insert("reduce_type".into(), json!(prepared.reduce_type));
    param.insert("spec_name".into(), json!("规格"));
    param.insert("spec_prices".into(), json!(serde_json::to_string(&Value::Array(prices)).unwrap_or_else(|_| "[]".into())));
    param.insert("specs".into(), json!(format!("规格|{names}")));
    param.insert("standard_brand_id".into(), json!(prepared.brand_id));
    if let Some(weight) = prepared.weight_kg {
        param.insert("weight".into(), json!(weight));
        param.insert("weight_unit".into(), json!(0));
    }
    if let Some(remote) = existing_remote {
        if let Ok(product_id) = remote.parse::<i64>() {
            param.insert("product_id".into(), json!(product_id));
        }
    }
    Value::Object(param)
}

async fn upload_image(http: &SharedTransport, session: &ApiSession, now: i64, index: usize, url: &str) -> Result<String, CallFail> {
    let uploaded = call(
        http,
        session,
        now,
        "material.uploadImageSync",
        json!({
            "folder_id": "0",
            "material_name": format!("image-{}.jpg", index + 1),
            "url": url,
        }),
        false,
    )
    .await?;
    let data = &uploaded["data"];
    if as_i64(&data["audit_status"]) == Some(3) {
        if let Some(byte_url) = text(data, "byte_url") {
            return Ok(byte_url);
        }
    }
    let material_id = text(data, "material_id").ok_or_else(|| CallFail::Rejected("抖店未返回素材 ID".into()))?;
    let detail = call(http, session, now, "material.queryMaterialDetail", json!({"material_id": material_id}), false).await?;
    let info = &detail["data"]["material_info"];
    match as_i64(&info["audit_status"]) {
        Some(3) => text(info, "byte_url").ok_or_else(|| CallFail::Failed("素材已通过审核但没有图片地址，未提交商品".into())),
        Some(4) => Err(CallFail::Rejected(text(info, "audit_reject_desc").unwrap_or_else(|| "素材审核拒绝".into()))),
        _ => Err(CallFail::Failed("素材仍在审核，未提交商品".into())),
    }
}

async fn product_status(
    http: &SharedTransport,
    session: &ApiSession,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let product_id = remote_id.parse::<i64>().map_err(|_| CallFail::Rejected("商品 ID 无效".into()))?;
    let body = call(http, session, now, "product.detail", json!({"product_id": product_id}), false).await?;
    let data = &body["data"];
    let status = as_i64(&data["status"]).ok_or_else(|| CallFail::Failed("抖店未返回商品状态".into()))?;
    let check_status = as_i64(&data["check_status"]).unwrap_or(0);
    let draft_status = as_i64(&data["draft_status"]).unwrap_or(0);
    Ok(map_product_status(status, check_status, draft_status))
}

fn map_listed_product(item: &Value) -> Option<CommerceProduct> {
    let id = identifier(item, "product_id")?;
    let price = as_i64(&item["discount_price"]).map(|fen| fen as f64 / 100.0);
    let status = as_i64(&item["status"]).unwrap_or(0);
    let check_status = as_i64(&item["check_status"]).unwrap_or(0);
    Some(CommerceProduct {
        id,
        title: text(item, "name").unwrap_or_default(),
        status: format!("{status}/{check_status}"),
        price,
        currency: price.map(|_| "CNY".into()),
        stock: None,
        sku: None,
    })
}

fn map_listed_order(item: &Value) -> Option<CommerceOrder> {
    let id = identifier(item, "order_id")?;
    let status = as_i64(&item["order_status"]).map(|code| code.to_string()).or_else(|| text(item, "order_status_desc"))?;
    let amount = as_i64(&item["pay_amount"]).map(|fen| fen as f64 / 100.0);
    Some(CommerceOrder {
        id,
        status,
        amount,
        currency: amount.map(|_| "CNY".into()),
        buyer: text(item, "doudian_open_id"),
        created_at: as_i64(&item["create_time"]).and_then(|time| chrono::DateTime::from_timestamp(time, 0)).map(|time| time.to_rfc3339()),
        lines: Vec::new(),
    })
}

fn rows<'a>(body: &'a Value, key: &str) -> &'a Vec<Value> {
    body.get("data")
        .and_then(|data| data.get(key))
        .and_then(Value::as_array)
        .or_else(|| body.get("data").and_then(Value::as_array).filter(|_| key == "data"))
        .unwrap_or(&EMPTY)
}

static EMPTY: Vec<Value> = Vec::new();

async fn call(
    http: &SharedTransport,
    session: &ApiSession,
    now: i64,
    method: &'static str,
    param: Value,
    mutating: bool,
) -> Result<Value, CallFail> {
    let param = sort_json(param);
    let raw = serde_json::to_string(&param).map_err(|_| CallFail::Failed("抖店请求参数编码失败".into()))?;
    let param_json = escape_param(&raw);
    let timestamp = gmt8_timestamp(now);
    let sign = sign_param(&session.app_secret, &session.app_id, method, &param_json, &timestamp).map_err(CallFail::Failed)?;
    let mut query = vec![
        ("method".into(), method.to_string()),
        ("app_key".into(), session.app_id.clone()),
        ("access_token".into(), session.access_token.clone()),
        ("timestamp".into(), timestamp),
        ("v".into(), "2".into()),
        ("sign".into(), sign),
        ("sign_method".into(), "hmac-sha256".into()),
    ];
    let body = if param_json == raw {
        HttpBody::Json(param)
    } else {
        query.push(("param_json".into(), param_json));
        HttpBody::Empty
    };
    let path = format!("/{}", method.replace('.', "/"));
    let response = match http
        .send(HttpRequest {
            method: "POST",
            url: format!("{HOST}{path}"),
            headers: vec![("Content-Type".into(), "application/json".into())],
            query,
            body,
            mutating,
        })
        .await
    {
        Ok(response) => response,
        Err(fault) => return Err(fault_to_call(mutating, fault)),
    };
    if code_of(&response.body).is_none() {
        if let Err(error) = adapter::classify_http(mutating, response.status, format!("HTTP {}", response.status)) {
            return Err(error_to_call(error));
        }
    }
    classify(mutating, response.status, &response.body)
}

fn fault_to_call(mutating: bool, fault: adapter::TransportFault) -> CallFail {
    error_to_call(adapter::classify_fault(mutating, fault))
}

fn error_to_call(error: CommerceError) -> CallFail {
    match error {
        CommerceError::Uncertain(message) => CallFail::Uncertain(message),
        CommerceError::Rejected(message) => CallFail::Rejected(message),
        CommerceError::Unsupported(message) => CallFail::Unsupported(message),
        other => CallFail::Failed(other.message().to_string()),
    }
}

fn classify(mutating: bool, status: u16, body: &Value) -> Result<Value, CallFail> {
    let code = code_of(body);
    if code == Some(10_000) && status < 500 {
        return Ok(body.clone());
    }
    let sub_code = text(body, "sub_code").unwrap_or_default();
    let sub_msg = text(body, "sub_msg").unwrap_or_default();
    let msg = text(body, "msg").unwrap_or_default();
    let message = if !sub_msg.is_empty() {
        sub_msg.clone()
    } else if !msg.is_empty() {
        msg.clone()
    } else if !sub_code.is_empty() {
        sub_code.clone()
    } else {
        format!("HTTP {status}")
    };
    if is_auth(&sub_code, &sub_msg) || is_auth(&sub_code, &msg) {
        return Err(CallFail::Auth(message));
    }
    if code == Some(70_000) || sub_code.contains("api-service-off") {
        return Err(CallFail::Unsupported(message));
    }
    let system = matches!(code, Some(20_000 | 20_001 | 50_001 | 90_000)) || status >= 500;
    if mutating && system {
        return Err(CallFail::Uncertain(message));
    }
    if system {
        return Err(CallFail::Failed(message));
    }
    if matches!(code, Some(40_002 | 40_003 | 40_004 | 50_002 | 60_000 | 30_001 | 30_002)) || status >= 400 {
        return Err(CallFail::Rejected(message));
    }
    Err(CallFail::Failed(message))
}

fn is_auth(sub_code: &str, message: &str) -> bool {
    let blob = format!("{sub_code} {message}").to_ascii_lowercase();
    blob.contains("access-token")
        || blob.contains("access_token")
        || blob.contains("authorization-no-existed")
        || blob.contains("authorization-closed")
        || blob.contains("token-expired")
        || blob.contains("token-invalid")
        || blob.contains("no-authorization")
        || message.contains("授权已失效")
        || message.contains("授权已被关闭")
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(child) = map.get(&key) {
                    sorted.insert(key, sort_json(child.clone()));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json).collect()),
        other => other,
    }
}

fn escape_param(param_json: &str) -> String {
    param_json.replace('&', "\\u0026").replace('<', "\\u003c").replace('>', "\\u003e").replace('\u{0008}', "\\u0008")
}

fn ensure_douyin(session: &ApiSession) -> Result<(), CommerceError> {
    if session.platform == Platform::Douyin {
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
        CallFail::Unsupported(message) => CommerceError::unsupported(message),
    }
}

fn from_fail(remote_id: Option<String>, error: CallFail) -> PushOutcome {
    let (status, reason) = match error {
        CallFail::Uncertain(message) => (ListingStatus::Uncertain, message),
        CallFail::Failed(message) => (ListingStatus::Failed, message),
        CallFail::Unsupported(message) | CallFail::Auth(message) | CallFail::Rejected(message) => {
            (ListingStatus::Rejected, message)
        }
    };
    outcome(remote_id, status, Some(reason))
}

fn outcome(remote_id: Option<String>, status: ListingStatus, reason: Option<String>) -> PushOutcome {
    PushOutcome { remote_id, status, reason }
}

fn attr_value(target: &ListingTarget, id: &str) -> Option<String> {
    target
        .attributes
        .iter()
        .find(|attribute| attribute.id == id)
        .map(|attribute| attribute.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn title_units(value: &str) -> usize {
    value.chars().map(|ch| if ('\u{4e00}'..='\u{9fff}').contains(&ch) { 2 } else { 1 }).sum()
}

fn has_emoji(value: &str) -> bool {
    value.chars().any(|ch| {
        ('\u{1F300}'..='\u{1FAFF}').contains(&ch) || ('\u{2600}'..='\u{27BF}').contains(&ch)
    })
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

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().and_then(|item| i64::try_from(item).ok())).or_else(|| {
        value.as_str().and_then(|item| item.parse().ok())
    })
}

fn code_of(body: &Value) -> Option<i64> {
    body.get("code").and_then(as_i64)
}

#[cfg(test)]
#[path = "douyin_tests.rs"]
mod douyin_tests;
