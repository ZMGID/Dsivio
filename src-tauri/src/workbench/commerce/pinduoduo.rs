//! 拼多多 POP 商品与订单适配器。
//!
//! 网关：`POST https://gw-api.pinduoduo.com/api/router`（表单字段）。
//! 签名与授权说明：<https://open.pinduoduo.com/application/document/browse?idStr=BD3A776A4D41D5F5>
//! 及开放平台「API 调用」：除 `sign` 外的参数按名 ASCII 升序拼成 `keyvalue`，
//! 首尾各加 `client_secret`，MD5 后转大写。空值不参与签名。
//!
//! - 可发布类目：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.authorization.cats>
//! - 类目发布规则：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.cat.rule.get>
//! - 图片上传：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.image.upload>
//! - 规格：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.spec.get>
//! - 自定义规格：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.spec.id.get>
//! - 新增或编辑草稿：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.edit.goods.commit>
//! - 提交草稿：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.submit.goods.commit>
//! - 草稿状态：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.commit.list.get>
//! - 商品列表：<https://open.pinduoduo.com/application/document/api?id=pdd.goods.list.get>
//! - 订单列表：<https://open.pinduoduo.com/application/document/api?id=pdd.order.list.get>
//!
//! 价格入参单位是分，订单 `pay_amount` 单位是元。草稿必填项只来自标题、价格、库存、SKU 名和下列属性，缺一项就在发请求前拒绝：
//! `multi_price`（拼单价，元）、`market_price`（参考价，元）、`cost_template_id`、`goods_type`、
//! `country_id`、`is_folt`、`is_pre_sale`、`is_refundable`、`second_hand`、
//! `shipment_limit_second`（非预售必填，秒）、`pre_sale_time`（预售必填，Unix 秒）、
//! `parent_spec_id`（类目返回多个父规格时必填）、`spec_name`（没有 SKU 行时必填）。
//! 数字属性 id 视为 `ref_pid`：纯数字值作为选项 `vid`，其余作为自定义 `value`。
//! `pdd.order.list.get` 没有稳定买家 id，也没有退款金额，这两个指标固定为不支持。

use super::transport::{
    Inbound, Outbound, PushOutcome, ResolvedShop, Transport, TransportFault,
};
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceError, ListingAttribute,
    ListingDraft, ListingStatus, ListingTarget, MetricKey, MetricRange, ShopMetrics,
};
use crate::workbench::shops::Platform;
use base64::{engine::general_purpose::STANDARD, Engine};
use md5::{Digest, Md5};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const HOST: &str = "https://gw-api.pinduoduo.com";
const ROUTER: &str = "/api/router";
const FORM: &str = "application/x-www-form-urlencoded;charset=utf-8";
const PAGE_SIZE: i64 = 100;
const MAX_PAGES: usize = 20;
/// 官方成交时间间距不超过 24 小时。取开区间以避免等于 86400 秒被拒。
const MAX_SPAN: i64 = 86_399;
const CN_OFFSET: i64 = 8 * 3600;

const RESERVED_ATTRS: &[&str] = &[
    "multi_price",
    "market_price",
    "cost_template_id",
    "shipment_limit_second",
    "goods_type",
    "country_id",
    "is_folt",
    "is_pre_sale",
    "is_refundable",
    "second_hand",
    "two_pieces_discount",
    "parent_spec_id",
    "limit_quantity",
    "out_goods_id",
    "pre_sale_time",
    "spec_name",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProduct {
    pub id: String,
    pub title: String,
    pub status: String,
    pub price: Option<f64>,
    pub stock: Option<i64>,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteOrder {
    pub id: String,
    pub status: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub buyer: Option<String>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePage<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: Option<i64>,
    pub has_next: bool,
}

#[derive(Clone, Debug)]
enum CallFail {
    Auth(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
}

impl CallFail {
    fn text(&self) -> &str {
        match self {
            Self::Auth(message) | Self::Rejected(message) | Self::Failed(message) | Self::Uncertain(message) => {
                message
            }
        }
    }
}

struct OrderView {
    status: Option<i64>,
    amount: Option<f64>,
    refund: Option<i64>,
}

struct Prepared {
    cat_id: i64,
    goods_name: String,
    goods_desc: Option<String>,
    market_price: i64,
    multi_price: i64,
    cost_template_id: i64,
    country_id: i64,
    goods_type: i64,
    is_folt: bool,
    is_pre_sale: bool,
    is_refundable: bool,
    second_hand: bool,
    shipment_limit_second: Option<i64>,
    pre_sale_time: Option<i64>,
    two_pieces_discount: Option<i64>,
    limit_quantity: Option<i64>,
    out_goods_id: Option<String>,
    parent_spec_id: Option<i64>,
    weight_g: Option<i64>,
    skus: Vec<SkuPlan>,
    properties: Vec<Value>,
    goods_id: Option<i64>,
}

struct SkuPlan {
    name: String,
    price: i64,
    stock: i64,
    code: Option<String>,
}

struct CommitIds {
    goods_id: String,
    commit_id: String,
}

struct CommitRow {
    status: i64,
    submit_time: i64,
    reject_comment: Option<String>,
}

pub async fn fetch_metrics(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    ensure_shop(shop)?;
    let (start, end) = metric_window(range, now, region_offset_secs(shop.region.as_deref()));
    let mut orders = Vec::new();
    let mut order_error = None;
    for (chunk_start, chunk_end) in time_chunks(start, end) {
        match list_orders(transport, shop, now, chunk_start, chunk_end).await {
            Ok(mut page) => orders.append(&mut page),
            Err(CallFail::Auth(message) | CallFail::Rejected(message)) => {
                return Err(CommerceError::rejected(message));
            }
            Err(other) => {
                order_error = Some(other.text().to_string());
                break;
            }
        }
    }
    let live = match list_live_count(transport, shop, now).await {
        Ok(count) => Some(count),
        Err(CallFail::Auth(message)) if order_error.is_none() && orders.is_empty() => {
            return Err(CommerceError::rejected(message));
        }
        Err(_) => None,
    };
    Ok(fold_metrics(shop_id, range, now, &orders, order_error, live))
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure_shop(shop)?;
    let parent = parent_id.filter(|value| !value.is_empty()).unwrap_or("0");
    let parent_cat_id = parent.parse::<i64>().map_err(|_| CommerceError::invalid("类目 id 无效"))?;
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.authorization.cats",
        vec![("parent_cat_id".into(), json!(parent_cat_id))],
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_categories(&body, parent))
}

pub fn map_categories(body: &Value, parent_id: &str) -> Vec<Category> {
    body["goods_auth_cats_get_response"]
        .get("goods_cats_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "cat_id")?;
            let name = text(item, "cat_name").unwrap_or_else(|| id.clone());
            let leaf = item.get("leaf").and_then(Value::as_bool).unwrap_or(false);
            Some(Category { id, name, parent_id: parent_id.to_string(), leaf })
        })
        .collect()
}

pub async fn fetch_attributes(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure_shop(shop)?;
    let cat_id = category_id.parse::<i64>().map_err(|_| CommerceError::invalid("类目 id 无效"))?;
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.cat.rule.get",
        vec![("cat_id".into(), json!(cat_id))],
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    body["cat_rule_get_response"]["goods_properties_rule"]
        .get("properties")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(map_property)
        .collect()
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    page: i64,
) -> Result<RemotePage<RemoteProduct>, CommerceError> {
    ensure_shop(shop)?;
    if page < 1 {
        return Err(CommerceError::invalid("页码从 1 开始"));
    }
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.list.get",
        vec![("page".into(), json!(page)), ("page_size".into(), json!(PAGE_SIZE))],
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_products(&body, page))
}

pub fn map_products(body: &Value, page: i64) -> RemotePage<RemoteProduct> {
    let response = &body["goods_list_get_response"];
    let items = response
        .get("goods_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = identifier(item, "goods_id")?;
            let title = text(item, "goods_name").unwrap_or_else(|| id.clone());
            let status = item
                .get("is_onsale")
                .and_then(Value::as_i64)
                .map(|value| if value == 1 { "onsale" } else { "offsale" }.to_string())
                .unwrap_or_default();
            let stock = item.get("goods_quantity").and_then(as_i64);
            let image_url = text(item, "image_url").or_else(|| text(item, "thumb_url"));
            Some(RemoteProduct { id, title, status, price: None, stock, image_url })
        })
        .collect::<Vec<_>>();
    page_from(response, "total_count", page, items)
}

pub async fn fetch_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
    page: i64,
) -> Result<RemotePage<RemoteOrder>, CommerceError> {
    ensure_shop(shop)?;
    validate_order_window(start, end, page)?;
    let body = order_page(transport, shop, now, start, end, page).await.map_err(fail_to_error)?;
    Ok(map_order_page(&body, page))
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
    if shop.platform != Platform::Pinduoduo {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    if let Err(message) = credentials(shop) {
        return outcome(None, ListingStatus::Rejected, Some(message));
    }
    let goods_id = match existing_goods_id(existing_remote) {
        Ok(id) => id,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    let prepared = match prepare(draft, target, goods_id) {
        Ok(prepared) => prepared,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    if images.is_empty() {
        return outcome(None, ListingStatus::Rejected, Some("缺少商品图片".into()));
    }
    if images.len() > 20 {
        return outcome(None, ListingStatus::Rejected, Some("轮播图和详情图最多各 20 张，请先减少图片".into()));
    }
    let mut urls = Vec::new();
    for (name, bytes) in images {
        match upload_image(transport, shop, now, name, bytes).await {
            Ok(url) => urls.push(url),
            Err(error) => return from_fail(None, error),
        }
    }
    let parent_spec = match parent_spec_id(transport, shop, now, &prepared).await {
        Ok(id) => id,
        Err(error) => return from_fail(None, error),
    };
    let mut skus = Vec::new();
    for sku in &prepared.skus {
        match spec_id(transport, shop, now, parent_spec, &sku.name).await {
            Ok(id) => skus.push(sku_body(sku, id, &prepared, urls.first().map(String::as_str))),
            Err(error) => return from_fail(None, error),
        }
    }
    let created = match edit_commit(transport, shop, now, &prepared, &urls, &skus).await {
        Ok(ids) => ids,
        Err(error) => return from_fail(None, error),
    };
    match submit_commit(transport, shop, now, &created).await {
        Ok(()) => outcome(Some(created.goods_id), ListingStatus::Reviewing, None),
        Err(error) => from_fail(Some(created.goods_id), error),
    }
}

pub async fn refresh_remote(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure_shop(shop)?;
    let goods_id = remote_id.parse::<i64>().map_err(|_| CommerceError::invalid("商品 ID 无效"))?;
    item_status(transport, shop, now, goods_id).await.map_err(fail_to_error)
}

pub fn map_commit_status(status: i64, reject_comment: Option<String>) -> (ListingStatus, Option<String>) {
    match status {
        0 => (ListingStatus::Reviewing, Some("编辑中".into())),
        1 => (ListingStatus::Reviewing, None),
        2 => (ListingStatus::Live, None),
        3 => (ListingStatus::Rejected, Some(reject_comment.filter(|value| !value.is_empty()).unwrap_or_else(|| "审核驳回".into()))),
        other => (ListingStatus::Reviewing, Some(format!("未识别的草稿状态 {other}"))),
    }
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: &[OrderView],
    order_error: Option<String>,
    live: Option<f64>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default();
    let mut metrics = super::types::zero_metrics(shop_id.to_string(), range, fetched_at);
    let mut unsupported = vec![MetricKey::Buyers, MetricKey::RefundAmount];
    if let Some(message) = order_error {
        unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::PendingShipment, MetricKey::RefundOrders]);
        metrics.error = Some(message);
    } else {
        metrics.values.insert(MetricKey::Orders, orders.len() as f64);
        if orders.iter().any(|order| order.amount.is_none()) {
            unsupported.push(MetricKey::Gmv);
        } else {
            metrics.values.insert(MetricKey::Gmv, orders.iter().filter_map(|order| order.amount).sum());
            metrics.currency = Some("CNY".into());
        }
        if orders.iter().any(|order| order.status.is_none()) {
            unsupported.push(MetricKey::PendingShipment);
        } else {
            let pending = orders.iter().filter(|order| order.status == Some(1)).count() as f64;
            metrics.values.insert(MetricKey::PendingShipment, pending);
        }
        if orders.iter().any(|order| order.refund.is_none()) {
            unsupported.push(MetricKey::RefundOrders);
        } else {
            let refunds = orders.iter().filter(|order| order.refund == Some(4)).count() as f64;
            metrics.values.insert(MetricKey::RefundOrders, refunds);
        }
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

fn map_property(node: &Value) -> Option<CategoryAttribute> {
    let id = identifier(node, "ref_pid")?;
    let name = text(node, "name").unwrap_or_else(|| id.clone());
    let required_flag = node.get("required").and_then(Value::as_bool).unwrap_or(false);
    let conditional = node.get("required_rule_type").and_then(Value::as_i64) == Some(1);
    let required = required_flag && !conditional;
    let choose_max = node.get("choose_max_num").and_then(Value::as_i64).unwrap_or(1);
    let value_type = node.get("property_value_type").and_then(Value::as_i64).unwrap_or(0);
    let options = node
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let id = identifier(value, "vid").unwrap_or_default();
                    let name = text(value, "value")?;
                    Some(AttributeOption { id, name })
                })
                .collect::<Vec<AttributeOption>>()
        })
        .unwrap_or_default();
    let input = if !options.is_empty() {
        if choose_max == 1 { AttributeInput::Select } else { AttributeInput::MultiSelect }
    } else if matches!(value_type, 1 | 2 | 3 | 4) {
        AttributeInput::Number
    } else {
        AttributeInput::Text
    };
    let unit = node
        .get("value_unit")
        .and_then(Value::as_array)
        .and_then(|units| units.iter().find_map(|unit| unit.as_str().filter(|value| !value.is_empty())))
        .map(str::to_owned);
    Some(CategoryAttribute { id, name, required, input, options, unit })
}

async fn list_orders(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<OrderView>, CallFail> {
    let mut page = 1;
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let body = order_page(transport, shop, now, start, end, page).await?;
        let response = &body["order_list_get_response"];
        let list = response.get("order_list").and_then(Value::as_array).cloned().unwrap_or_default();
        let has_next = response.get("has_next").and_then(Value::as_bool).unwrap_or(list.len() as i64 == PAGE_SIZE);
        for item in list {
            items.push(OrderView {
                status: item.get("order_status").and_then(as_i64),
                amount: item.get("pay_amount").and_then(as_f64),
                refund: item.get("refund_status").and_then(as_i64),
            });
        }
        if !has_next {
            return Ok(items);
        }
        page += 1;
    }
    Err(CallFail::Failed("订单分页未取完，不把截断结果当成完整指标".into()))
}

async fn order_page(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
    page: i64,
) -> Result<Value, CallFail> {
    call(
        transport,
        shop,
        now,
        "pdd.order.list.get",
        vec![
            ("start_confirm_at".into(), json!(start)),
            ("end_confirm_at".into(), json!(end)),
            ("order_status".into(), json!(5)),
            ("refund_status".into(), json!(5)),
            ("page".into(), json!(page)),
            ("page_size".into(), json!(PAGE_SIZE)),
            ("use_has_next".into(), json!(true)),
        ],
        false,
    )
    .await
}

fn map_order_page(body: &Value, page: i64) -> RemotePage<RemoteOrder> {
    let response = &body["order_list_get_response"];
    let items = response
        .get("order_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            let id = text(item, "order_sn").unwrap_or_default();
            let status = item.get("order_status").and_then(as_i64).map(|value| value.to_string()).unwrap_or_default();
            let amount = item.get("pay_amount").and_then(as_f64);
            let created_at = item.get("confirm_time").and_then(as_i64);
            RemoteOrder { id, status, amount, currency: amount.map(|_| "CNY".to_string()), buyer: None, created_at }
        })
        .collect::<Vec<_>>();
    let has_next = response.get("has_next").and_then(Value::as_bool).unwrap_or(items.len() as i64 == PAGE_SIZE);
    RemotePage { items, page, page_size: PAGE_SIZE, total: response.get("total_count").and_then(as_i64), has_next }
}

async fn list_live_count(transport: &Transport, shop: &ResolvedShop, now: i64) -> Result<f64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.list.get",
        vec![
            ("is_onsale".into(), json!(1)),
            ("page".into(), json!(1)),
            ("page_size".into(), json!(1)),
        ],
        false,
    )
    .await?;
    body["goods_list_get_response"]
        .get("total_count")
        .and_then(as_f64)
        .ok_or_else(|| CallFail::Failed("拼多多未返回在售商品数".into()))
}

async fn upload_image(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    _name: &str,
    bytes: &[u8],
) -> Result<String, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.image.upload",
        vec![("image".into(), json!(STANDARD.encode(bytes)))],
        true,
    )
    .await?;
    text(&body["goods_image_upload_response"], "image_url").ok_or_else(|| CallFail::Rejected("拼多多未返回图片 URL".into()))
}

async fn parent_spec_id(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    prepared: &Prepared,
) -> Result<i64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.spec.get",
        vec![("cat_id".into(), json!(prepared.cat_id))],
        false,
    )
    .await?;
    let specs = body["goods_spec_get_response"]
        .get("goods_spec_list")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ids = specs.iter().filter_map(|item| item.get("parent_spec_id").and_then(as_i64)).collect::<Vec<_>>();
    if ids.is_empty() {
        return Err(CallFail::Rejected("类目没有可用的父规格".into()));
    }
    if let Some(requested) = prepared.parent_spec_id {
        if ids.contains(&requested) {
            return Ok(requested);
        }
        return Err(CallFail::Rejected(format!("parent_spec_id 不在类目规格中：{ids:?}")));
    }
    if ids.len() == 1 {
        return Ok(ids[0]);
    }
    Err(CallFail::Rejected(format!("类目有多个父规格，请在属性 parent_spec_id 中指定其一：{ids:?}")))
}

async fn spec_id(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_spec_id: i64,
    spec_name: &str,
) -> Result<i64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.spec.id.get",
        vec![("parent_spec_id".into(), json!(parent_spec_id)), ("spec_name".into(), json!(spec_name))],
        true,
    )
    .await?;
    body["goods_spec_id_get_response"]
        .get("spec_id")
        .and_then(as_i64)
        .ok_or_else(|| CallFail::Rejected("拼多多未返回规格 ID".into()))
}

fn sku_body(sku: &SkuPlan, spec_id: i64, prepared: &Prepared, thumb: Option<&str>) -> Value {
    let mut map = Map::new();
    map.insert("is_onsale".into(), json!(1));
    map.insert("multi_price".into(), json!(prepared.multi_price));
    map.insert("price".into(), json!(sku.price));
    map.insert("quantity".into(), json!(sku.stock));
    map.insert("spec_id_list".into(), json!(format!("[{spec_id}]")));
    if let Some(code) = &sku.code {
        map.insert("out_sku_sn".into(), json!(code));
    }
    if let Some(url) = thumb {
        map.insert("thumb_url".into(), json!(url));
    }
    if let Some(weight) = prepared.weight_g {
        map.insert("weight".into(), json!(weight));
    }
    if let Some(limit) = prepared.limit_quantity {
        map.insert("limit_quantity".into(), json!(limit));
    }
    Value::Object(map)
}

async fn edit_commit(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    prepared: &Prepared,
    urls: &[String],
    skus: &[Value],
) -> Result<CommitIds, CallFail> {
    let mut params = vec![
        ("goods_name".into(), json!(prepared.goods_name)),
        ("cat_id".into(), json!(prepared.cat_id)),
        ("carousel_gallery".into(), json!(urls)),
        ("detail_gallery".into(), json!(urls)),
        ("cost_template_id".into(), json!(prepared.cost_template_id)),
        ("country_id".into(), json!(prepared.country_id)),
        ("goods_type".into(), json!(prepared.goods_type)),
        ("is_folt".into(), json!(prepared.is_folt)),
        ("is_pre_sale".into(), json!(prepared.is_pre_sale)),
        ("is_refundable".into(), json!(prepared.is_refundable)),
        ("second_hand".into(), json!(prepared.second_hand)),
        ("market_price".into(), json!(prepared.market_price)),
        ("sku_list".into(), Value::Array(skus.to_vec())),
    ];
    if let Some(url) = urls.first() {
        params.push(("image_url".into(), json!(url)));
    }
    if let Some(desc) = &prepared.goods_desc {
        params.push(("goods_desc".into(), json!(desc)));
    }
    if let Some(seconds) = prepared.shipment_limit_second {
        params.push(("shipment_limit_second".into(), json!(seconds)));
    }
    if let Some(time) = prepared.pre_sale_time {
        params.push(("pre_sale_time".into(), json!(time)));
    }
    if let Some(discount) = prepared.two_pieces_discount {
        params.push(("two_pieces_discount".into(), json!(discount)));
    }
    if let Some(out_goods_id) = &prepared.out_goods_id {
        params.push(("out_goods_id".into(), json!(out_goods_id)));
    }
    if let Some(goods_id) = prepared.goods_id {
        params.push(("goods_id".into(), json!(goods_id)));
    }
    if !prepared.properties.is_empty() {
        params.push(("goods_properties".into(), Value::Array(prepared.properties.clone())));
    }
    let body = call(transport, shop, now, "pdd.goods.edit.goods.commit", params, true).await?;
    commit_ids(&body)
}

async fn submit_commit(transport: &Transport, shop: &ResolvedShop, now: i64, ids: &CommitIds) -> Result<(), CallFail> {
    let goods_id = ids.goods_id.parse::<i64>().map_err(|_| CallFail::Failed("商品 ID 无效".into()))?;
    let goods_commit_id = ids.commit_id.parse::<i64>().map_err(|_| CallFail::Failed("草稿 ID 无效".into()))?;
    let body = call(
        transport,
        shop,
        now,
        "pdd.goods.submit.goods.commit",
        vec![
            ("goods_id".into(), json!(goods_id)),
            ("goods_commit_id".into(), json!(goods_commit_id)),
            ("operate_type".into(), json!(0)),
        ],
        true,
    )
    .await?;
    let submitted = commit_ids(&body)?;
    if submitted.goods_id != ids.goods_id {
        return Err(CallFail::Uncertain(format!("提交返回的商品 ID {} 与草稿 {} 不一致", submitted.goods_id, ids.goods_id)));
    }
    Ok(())
}

fn commit_ids(body: &Value) -> Result<CommitIds, CallFail> {
    let response = &body["goods_update_response"];
    let goods_id = identifier(response, "goods_id").ok_or_else(|| CallFail::Rejected("拼多多未返回商品 ID".into()))?;
    let commit_id = identifier(response, "goods_commit_id").ok_or_else(|| CallFail::Rejected("拼多多未返回草稿 ID".into()))?;
    Ok(CommitIds { goods_id, commit_id })
}

async fn item_status(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    goods_id: i64,
) -> Result<(ListingStatus, Option<String>), CallFail> {
    let mut best: Option<CommitRow> = None;
    for check_status in [0_i64, 1, 2, 3] {
        let body = call(
            transport,
            shop,
            now,
            "pdd.goods.commit.list.get",
            vec![
                ("check_status".into(), json!(check_status)),
                ("goods_id".into(), json!(goods_id)),
                ("page".into(), json!(1)),
                ("page_size".into(), json!(PAGE_SIZE)),
            ],
            false,
        )
        .await?;
        let list = body["goods_commit_list_get_response"].get("list").and_then(Value::as_array).cloned().unwrap_or_default();
        for item in list {
            if identifier(&item, "goods_id").as_deref() != Some(&goods_id.to_string()) {
                continue;
            }
            let row = CommitRow {
                status: item.get("check_status").and_then(as_i64).unwrap_or(check_status),
                submit_time: item.get("submit_time").and_then(as_i64).unwrap_or(0),
                reject_comment: text(&item, "reject_comment"),
            };
            let replace = match &best {
                None => true,
                Some(current) => {
                    (row.submit_time, status_rank(row.status)) >= (current.submit_time, status_rank(current.status))
                }
            };
            if replace {
                best = Some(row);
            }
        }
    }
    let row = best.ok_or_else(|| CallFail::Failed("拼多多未返回该商品的草稿状态".into()))?;
    Ok(map_commit_status(row.status, row.reject_comment))
}

fn status_rank(status: i64) -> i64 {
    match status {
        3 => 4,
        1 => 3,
        0 => 2,
        2 => 1,
        _ => 0,
    }
}

fn prepare(draft: &ListingDraft, target: &ListingTarget, goods_id: Option<i64>) -> Result<Prepared, String> {
    if let Some(currency) = draft.currency.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        if !matches!(currency.to_ascii_uppercase().as_str(), "CNY" | "RMB") {
            return Err("拼多多只支持人民币价格".into());
        }
    }
    let goods_name = draft.title.trim();
    if goods_name.is_empty() {
        return Err("缺少商品标题".into());
    }
    let goods_desc = match draft.description.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        None => None,
        Some(desc) if (20..=500).contains(&desc.chars().count()) => Some(desc.to_string()),
        Some(_) => return Err("商品描述需在 20 到 500 字".into()),
    };
    let cat_id = target.category_id.parse::<i64>().map_err(|_| "类目 id 无效".to_string())?;
    if cat_id <= 0 {
        return Err("类目 id 无效".into());
    }
    let multi_price = yuan_attr(target, "multi_price", "拼单价 multi_price")?;
    let market_price = yuan_attr(target, "market_price", "参考价 market_price")?;
    let cost_template_id = int_attr(target, "cost_template_id", "运费模板 cost_template_id")?;
    let country_id = int_attr(target, "country_id", "国家 country_id")?;
    let goods_type = int_attr(target, "goods_type", "商品类型 goods_type")?;
    let is_folt = bool_attr(target, "is_folt", "假一赔十 is_folt")?;
    let is_pre_sale = bool_attr(target, "is_pre_sale", "预售 is_pre_sale")?;
    let is_refundable = bool_attr(target, "is_refundable", "七天退换 is_refundable")?;
    let second_hand = bool_attr(target, "second_hand", "二手 second_hand")?;
    let shipment_limit_second = if is_pre_sale {
        optional_int(target, "shipment_limit_second")?
    } else {
        Some(int_attr(target, "shipment_limit_second", "承诺发货时间 shipment_limit_second")?)
    };
    if let Some(seconds) = shipment_limit_second {
        if seconds <= 0 {
            return Err("承诺发货时间必须大于 0".into());
        }
    }
    let pre_sale_time = if is_pre_sale {
        Some(int_attr(target, "pre_sale_time", "预售时间 pre_sale_time")?)
    } else {
        optional_int(target, "pre_sale_time")?
    };
    let two_pieces_discount = optional_int(target, "two_pieces_discount")?;
    if let Some(discount) = two_pieces_discount {
        if !(0..=100).contains(&discount) {
            return Err("满两件折扣必须在 0 到 100".into());
        }
    }
    let limit_quantity = optional_int(target, "limit_quantity")?;
    if let Some(limit) = limit_quantity {
        if limit != 999 {
            return Err("拼多多限购 limit_quantity 只接受 999".into());
        }
    }
    let out_goods_id = attr(target, "out_goods_id").map(str::to_owned);
    let parent_spec_id = optional_int(target, "parent_spec_id")?;
    let weight_g = match draft.weight_kg {
        None => None,
        Some(kg) => Some(grams(kg)?),
    };
    let skus = sku_plans(draft, target, multi_price, market_price)?;
    let properties = goods_properties(&target.attributes);
    Ok(Prepared {
        cat_id,
        goods_name: goods_name.to_string(),
        goods_desc,
        market_price,
        multi_price,
        cost_template_id,
        country_id,
        goods_type,
        is_folt,
        is_pre_sale,
        is_refundable,
        second_hand,
        shipment_limit_second,
        pre_sale_time,
        two_pieces_discount,
        limit_quantity,
        out_goods_id,
        parent_spec_id,
        weight_g,
        skus,
        properties,
        goods_id,
    })
}

fn sku_plans(draft: &ListingDraft, target: &ListingTarget, multi_price: i64, market_price: i64) -> Result<Vec<SkuPlan>, String> {
    let plans = if draft.skus.is_empty() {
        let name = attr(target, "spec_name").ok_or("缺少 SKU 规格名")?.to_string();
        vec![sku_plan(&name, draft.price, draft.stock, None, multi_price, market_price)?]
    } else {
        draft
            .skus
            .iter()
            .map(|sku| {
                let name = sku.name.trim();
                if name.is_empty() {
                    return Err("SKU 规格名不能为空".into());
                }
                sku_plan(name, sku.price.or(draft.price), sku.stock.or(draft.stock), sku.code.as_deref(), multi_price, market_price)
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if plans.is_empty() {
        return Err("缺少 SKU".into());
    }
    Ok(plans)
}

fn sku_plan(
    name: &str,
    price: Option<f64>,
    stock: Option<i64>,
    code: Option<&str>,
    multi_price: i64,
    market_price: i64,
) -> Result<SkuPlan, String> {
    let price = yuan_to_fen(price.ok_or_else(|| format!("SKU {name} 缺少单买价"))?, &format!("SKU {name} 单买价"))?;
    if price <= multi_price {
        return Err(format!("SKU {name} 的单买价必须高于拼单价"));
    }
    if market_price <= price {
        return Err(format!("参考价必须高于 SKU {name} 的单买价"));
    }
    let stock = stock.ok_or_else(|| format!("SKU {name} 缺少库存"))?;
    if stock < 0 {
        return Err(format!("SKU {name} 库存不能为负"));
    }
    let code = code.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
    Ok(SkuPlan { name: name.to_string(), price, stock, code })
}

fn goods_properties(attributes: &[ListingAttribute]) -> Vec<Value> {
    let mut out = Vec::new();
    for attribute in attributes {
        if RESERVED_ATTRS.contains(&attribute.id.as_str()) || attribute.id.parse::<i64>().is_err() {
            continue;
        }
        let ref_pid = attribute.id.parse::<i64>().unwrap_or(0);
        let mut values = attribute.values.clone();
        if values.is_empty() && !attribute.value.trim().is_empty() {
            values.push(attribute.value.clone());
        }
        for value in values {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let mut map = Map::new();
            map.insert("ref_pid".into(), json!(ref_pid));
            if let Some(vid) = integer_vid(value) {
                map.insert("vid".into(), json!(vid));
            } else {
                map.insert("value".into(), json!(value));
            }
            out.push(Value::Object(map));
        }
    }
    out
}

fn integer_vid(value: &str) -> Option<i64> {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn page_from<T>(response: &Value, total_key: &str, page: i64, items: Vec<T>) -> RemotePage<T> {
    let total = response.get(total_key).and_then(as_i64);
    let has_next = total.map(|count| page * PAGE_SIZE < count).unwrap_or(items.len() as i64 == PAGE_SIZE);
    RemotePage { items, page, page_size: PAGE_SIZE, total, has_next }
}

fn validate_order_window(start: i64, end: i64, page: i64) -> Result<(), CommerceError> {
    if page < 1 {
        return Err(CommerceError::invalid("页码从 1 开始"));
    }
    if end < start {
        return Err(CommerceError::invalid("订单结束时间早于开始时间"));
    }
    if end - start > MAX_SPAN {
        return Err(CommerceError::invalid("拼多多订单成交时间间隔不能超过 24 小时"));
    }
    Ok(())
}

fn existing_goods_id(remote: Option<&str>) -> Result<Option<i64>, String> {
    let Some(remote) = remote.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let id = remote.parse::<i64>().map_err(|_| "商品 ID 无效".to_string())?;
    if id <= 0 {
        return Err("商品 ID 无效".into());
    }
    Ok(Some(id))
}

fn ensure_shop(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform != Platform::Pinduoduo {
        return Err(CommerceError::unsupported("该平台尚未接入"));
    }
    credentials(shop).map_err(CommerceError::invalid)
}

fn credentials(shop: &ResolvedShop) -> Result<(), String> {
    if shop.partner_id.trim().is_empty() || shop.partner_key.trim().is_empty() || shop.access_token.trim().is_empty() {
        return Err("拼多多应用凭证或 access_token 为空".into());
    }
    Ok(())
}

fn attr<'a>(target: &'a ListingTarget, id: &str) -> Option<&'a str> {
    target
        .attributes
        .iter()
        .find(|attribute| attribute.id == id)
        .map(|attribute| attribute.value.trim())
        .filter(|value| !value.is_empty())
}

fn int_attr(target: &ListingTarget, id: &str, label: &str) -> Result<i64, String> {
    let value = attr(target, id).ok_or_else(|| format!("缺少{label}"))?;
    value.parse::<i64>().map_err(|_| format!("{label} 不是整数"))
}

fn optional_int(target: &ListingTarget, id: &str) -> Result<Option<i64>, String> {
    match attr(target, id) {
        None => Ok(None),
        Some(value) => value.parse::<i64>().map(Some).map_err(|_| format!("{id} 不是整数")),
    }
}

fn bool_attr(target: &ListingTarget, id: &str, label: &str) -> Result<bool, String> {
    match attr(target, id).map(|value| value.to_ascii_lowercase()) {
        Some(value) if value == "true" || value == "1" => Ok(true),
        Some(value) if value == "false" || value == "0" => Ok(false),
        Some(_) => Err(format!("{label} 只能是 true 或 false")),
        None => Err(format!("缺少{label}")),
    }
}

fn yuan_attr(target: &ListingTarget, id: &str, label: &str) -> Result<i64, String> {
    let value = attr(target, id).ok_or_else(|| format!("缺少{label}"))?;
    let yuan = value.parse::<f64>().map_err(|_| format!("{label} 不是金额"))?;
    yuan_to_fen(yuan, label)
}

fn yuan_to_fen(yuan: f64, label: &str) -> Result<i64, String> {
    if !yuan.is_finite() || yuan <= 0.0 {
        return Err(format!("{label}必须大于 0"));
    }
    let scaled = yuan * 100.0;
    if (scaled - scaled.round()).abs() > 1e-6 {
        return Err(format!("{label}最多两位小数"));
    }
    Ok(scaled.round() as i64)
}

fn grams(kg: f64) -> Result<i64, String> {
    if !kg.is_finite() || kg <= 0.0 {
        return Err("重量必须大于 0".into());
    }
    let grams = kg * 1000.0;
    if (grams - grams.round()).abs() > 1e-6 {
        return Err("重量换算成克后必须是整数".into());
    }
    Ok(grams.round() as i64)
}

fn region_offset_secs(region: Option<&str>) -> i64 {
    match region.unwrap_or("CN").trim().to_ascii_uppercase().as_str() {
        "SG" | "MY" | "PH" | "TW" | "CN" | "" => CN_OFFSET,
        "TH" | "VN" | "ID" | "KH" => 7 * 3600,
        "JP" | "KR" => 9 * 3600,
        _ => CN_OFFSET,
    }
}

fn metric_window(range: MetricRange, now: i64, offset: i64) -> (i64, i64) {
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
    if end < start {
        return vec![(start, start)];
    }
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let chunk_end = (cursor + MAX_SPAN).min(end);
        out.push((cursor, chunk_end));
        if chunk_end >= end {
            break;
        }
        cursor = chunk_end + 1;
    }
    out
}

fn sign_pinduoduo(secret: &str, params: &BTreeMap<String, String>) -> String {
    let mut raw = String::from(secret);
    for (key, value) in params {
        if key == "sign" || value.is_empty() {
            continue;
        }
        raw.push_str(key);
        raw.push_str(value);
    }
    raw.push_str(secret);
    hex_upper(Md5::digest(raw.as_bytes()))
}

fn hex_upper(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|byte| format!("{byte:02X}")).collect()
}

fn param_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::Null => return None,
        Value::String(value) if value.is_empty() => return None,
        Value::String(value) => value.clone(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(value) => value.to_string(),
        other => serde_json::to_string(other).ok()?, 
    };
    if text.is_empty() { None } else { Some(text) }
}

async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    api_type: &'static str,
    biz: Vec<(String, Value)>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let mut params = BTreeMap::new();
    params.insert("type".into(), api_type.to_string());
    params.insert("client_id".into(), shop.partner_id.clone());
    params.insert("access_token".into(), shop.access_token.clone());
    params.insert("timestamp".into(), now.to_string());
    params.insert("data_type".into(), "JSON".into());
    for (key, value) in biz {
        if let Some(text) = param_string(&value) {
            params.insert(key, text);
        }
    }
    let sign = sign_pinduoduo(&shop.partner_key, &params);
    params.insert("sign".into(), sign);
    let query = params.into_iter().collect::<Vec<_>>();
    let request = Outbound {
        method: "POST",
        path: ROUTER.to_string(),
        query,
        json: None,
        file_name: None,
        file_bytes: None,
        mutating,
        host: HOST.to_string(),
        headers: vec![("Content-Type".into(), FORM.into())],
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
    if let Some(error) = inbound.body.get("error_response") {
        let text = error_text(error);
        if is_auth(error) {
            return Err(CallFail::Auth(text));
        }
        if mutating && error_code(error) == Some(50000) {
            return Err(CallFail::Uncertain(text));
        }
        return Err(CallFail::Rejected(text));
    }
    if inbound.status >= 500 {
        let text = format!("HTTP {}", inbound.status);
        return if mutating { Err(CallFail::Uncertain(text)) } else { Err(CallFail::Failed(text)) };
    }
    if inbound.status >= 400 {
        return Err(CallFail::Rejected(format!("HTTP {}", inbound.status)));
    }
    Ok(inbound.body)
}

fn error_text(error: &Value) -> String {
    let message = text(error, "error_msg")
        .or_else(|| text(error, "sub_msg"))
        .unwrap_or_else(|| "拼多多接口返回失败".into());
    match error_code(error) {
        Some(code) => format!("{code}: {message}"),
        None => message,
    }
}

fn is_auth(error: &Value) -> bool {
    if matches!(error_code(error), Some(10019 | 10035 | 20032)) {
        return true;
    }
    let blob = format!("{} {}", text(error, "error_msg").unwrap_or_default(), text(error, "sub_msg").unwrap_or_default());
    let lower = blob.to_ascii_lowercase();
    lower.contains("access_token")
        && (blob.contains("过期") || blob.contains("失效") || lower.contains("expired") || lower.contains("invalid"))
}

fn error_code(error: &Value) -> Option<i64> {
    error.get("error_code").and_then(as_i64)
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

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().and_then(|item| i64::try_from(item).ok())).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|item| item as f64)).or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

#[cfg(test)]
#[path = "pinduoduo_tests.rs"]
mod pinduoduo_tests;
