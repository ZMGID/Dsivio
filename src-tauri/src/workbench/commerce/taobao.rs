//! 淘宝 TOP 适配器。令牌只读 [`ResolvedShop`]（`partner_id` = AppKey，`partner_key` = AppSecret，`access_token` = session），不另存一份。
//!
//! 签名与平台错误码：<https://developer.alibaba.com/docs/doc.htm?articleId=101617&docType=1&treeId=1>
//! 正式 HTTPS 网关以各 API 页的 HTTPS 地址为准：<https://eco.taobao.com/router/rest>
//! （协议文档同时列出 <https://gw.api.taobao.com/router/rest>）。
//! 普通调用设置 `content-type: application/x-www-form-urlencoded`，共用传输把 query 写成表单正文，schema XML 不进 URL。
//! 图片上传是 multipart，文件域名请求头 `x-dsivio-file-field: img`（byte[] 不参与签名）；其余参数留在 query。
//!
//! - 类目 <https://developer.alibaba.com/docs/api.htm?apiId=122> `taobao.itemcats.get`
//! - 属性 <https://developer.alibaba.com/docs/api.htm?apiId=121> `taobao.itemprops.get`（集市 `type=1`）
//! - 图片 <https://developer.alibaba.com/docs/api.htm?apiId=140> `taobao.picture.upload`
//! - 发布规则 <https://developer.alibaba.com/docs/api.htm?apiId=53943> `alibaba.item.publish.schema.get`
//! - 发布提交 <https://developer.alibaba.com/docs/api.htm?apiId=53962> `alibaba.item.publish.submit`（`market=taobao`）
//! - 状态 <https://developer.alibaba.com/docs/api.htm?apiId=24625> `taobao.item.seller.get` 的 `approve_status`
//! - 出售中 <https://developer.alibaba.com/docs/api.htm?apiId=18> `taobao.items.onsale.get`
//! - 仓库 <https://developer.alibaba.com/docs/api.htm?apiId=162> `taobao.items.inventory.get`（`banner=for_shelved`）
//! - 订单 <https://developer.alibaba.com/docs/api.htm?apiId=46> `taobao.trades.sold.get`（仅三个月，金额单位元）
//! - 退款 <https://developer.alibaba.com/docs/api.htm?apiId=52> `taobao.refunds.receive.get`
//!
//! 应用没有接口包权限时返回权限错误（协议错误码 11/12/26/27 与 `isv.permission-*`），不把空列表当成功。
//! 生意参谋没有对应的开放接口，指标由交易和退款按北京时间窗口汇总。

use super::transport::{
    Inbound, Outbound, PushOutcome, ResolvedShop, Transport, TransportFault,
};
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities,
    CommerceError, CommerceOrder, CommerceOrderLine, CommerceProduct, ListingAttribute,
    ListingDraft, ListingSku, ListingStatus, ListingTarget, MetricKey, MetricRange, OrderPage,
    ProductPage, ShopMetrics,
};
use crate::workbench::shops::Platform;
use md5::{Digest, Md5};
use serde_json::Value;
use std::collections::BTreeSet;

const HOST: &str = "https://eco.taobao.com";
const PATH: &str = "/router/rest";
const OFFSET_SECS: i32 = 8 * 3600;
const OFFSET: i64 = OFFSET_SECS as i64;
const MAX_PAGES: usize = 20;
const TRADE_PAGE: i64 = 40;
const REFUND_PAGE: i64 = 40;
const PAGE_SIZE: i64 = 40;
/// 协议文档列出的全部交易类型。不传 `type` 时接口只返回默认的几种。
const TRADE_TYPES: &str = "fixed,auction,guarantee_trade,step,independent_simple_trade,independent_shop_trade,auto_delivery,ec,cod,game_equipment,shopex_trade,netcn_trade,external_trade,instant_trade,b2c_cod,hotel_trade,super_market_trade,super_market_cod_trade,taohua,waimai,o2o_offlinetrade,nopaid,eticket,tmall_i18n,insurance_plus,finance,pre_auth_type,lazada";

pub fn capabilities() -> CommerceCapabilities {
    CommerceCapabilities {
        platform: Platform::Taobao,
        metrics: MetricKey::ALL.to_vec(),
        listing: true,
        categories: true,
        products: true,
        orders: true,
        notes: vec![
            "类目 taobao.itemcats.get，属性 taobao.itemprops.get（集市 type=1）。上架为 market=taobao 的 alibaba.item.publish.schema.get、taobao.picture.upload、alibaba.item.publish.submit，状态读 taobao.item.seller.get 的 approve_status。".into(),
            "商品游标先翻 taobao.items.onsale.get，结束后为 for_shelved:页码，对应 taobao.items.inventory.get 的 banner=for_shelved（不含 sold_out 与 violation_off_shelf）。列表接口不返回 SKU；SKU 写入发布 schema。".into(),
            "指标窗口为北京时间今天、昨天、近 7 天、近 30 天。成交额、订单、买家、待发货来自 taobao.trades.sold.get（仅三个月内，金额单位元，记 CNY），退款来自 taobao.refunds.receive.get 的默认交易类型。缺字段记为不支持。应用无权限时返回权限错误。".into(),
        ],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProductList {
    OnSale,
    Inventory,
}

struct RemotePage<T> {
    items: Vec<T>,
    has_next: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteSku {
    pub remote_id: Option<String>,
    pub properties: Option<String>,
    pub name: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub stock: Option<i64>,
    pub outer_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct RemoteOrder {
    pub remote_id: String,
    pub status: String,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub buyer: Option<String>,
    pub created_at: Option<String>,
    pub pending_shipment: bool,
    pub skus: Vec<RemoteSku>,
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
            Self::Auth(message)
            | Self::Rejected(message)
            | Self::Failed(message)
            | Self::Uncertain(message) => message,
        }
    }
}

struct SchemaField {
    id: String,
    name: String,
    kind: String,
    required: bool,
    options: Vec<(String, String)>,
    children: Vec<SchemaField>,
}

/// 北京时间的天窗，返回 UTC 秒。淘宝时间戳固定为 GMT+8。
pub fn metric_window(range: MetricRange, now: i64) -> (i64, i64) {
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

pub fn top_timestamp(now: i64) -> String {
    format_top(now)
}

fn format_top(now: i64) -> String {
    chrono::DateTime::from_timestamp(now, 0)
        .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH)
        .with_timezone(&chrono::FixedOffset::east_opt(OFFSET_SECS).expect("offset"))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn rfc3339_from_top(value: &str) -> Option<String> {
    let naive = chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    let offset = chrono::FixedOffset::east_opt(OFFSET_SECS)?;
    Some(naive.and_local_timezone(offset).single()?.to_rfc3339())
}

/// MD5 签名（`sign_method=md5`）。排除空值与 `sign`，密钥拼在排序后的键值串两端，结果为大写十六进制。
pub fn sign_top(secret: &str, params: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(&str, &str)> = params
        .iter()
        .copied()
        .filter(|(key, value)| *key != "sign" && !key.is_empty() && !value.is_empty())
        .collect();
    pairs.sort_by(|left, right| left.0.cmp(right.0));
    let mut raw = String::from(secret);
    for (key, value) in pairs {
        raw.push_str(key);
        raw.push_str(value);
    }
    raw.push_str(secret);
    hex_upper(&Md5::digest(raw.as_bytes()))
}

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
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
    let trades = match list_trades(
        transport,
        shop,
        now,
        Some((start, end)),
        1,
        TRADE_PAGE,
        true,
    )
    .await
    {
        Ok(page) => page,
        Err(error) => return Err(fail_to_error(error)),
    };
    let refunds = match list_refunds(transport, shop, now, start, end).await {
        Ok(page) => page,
        Err(error) => return Err(fail_to_error(error)),
    };
    let live = match live_count(transport, shop, now).await {
        Ok(count) => Ok(count),
        Err(error @ CallFail::Auth(_)) => return Err(fail_to_error(error)),
        Err(error) => Err(error.text().to_string()),
    };
    Ok(fold_metrics(
        shop_id,
        range,
        now,
        &trades.items,
        &refunds,
        live,
    ))
}

fn fold_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    trades: &[RemoteOrder],
    refunds: &[(String, Option<f64>)],
    live: Result<f64, String>,
) -> ShopMetrics {
    let fetched_at = chrono::DateTime::from_timestamp(now, 0)
        .map(|time| time.to_rfc3339())
        .unwrap_or_default();
    let mut metrics = ShopMetrics {
        shop_id: shop_id.to_string(),
        range,
        currency: None,
        values: std::collections::BTreeMap::new(),
        unsupported: Vec::new(),
        fetched_at,
        error: None,
    };
    let counted: Vec<&RemoteOrder> = trades
        .iter()
        .filter(|order| counts_order(&order.status))
        .collect();
    metrics
        .values
        .insert(MetricKey::Orders, counted.len() as f64);
    metrics.values.insert(
        MetricKey::PendingShipment,
        counted
            .iter()
            .filter(|order| order.pending_shipment)
            .count() as f64,
    );
    if counted.is_empty() {
        metrics.values.insert(MetricKey::Gmv, 0.0);
        metrics.values.insert(MetricKey::Buyers, 0.0);
    } else if counted.iter().any(|order| order.amount.is_none()) {
        mark_unsupported(&mut metrics, MetricKey::Gmv);
    } else {
        metrics.values.insert(
            MetricKey::Gmv,
            counted.iter().filter_map(|order| order.amount).sum(),
        );
        metrics.currency = Some("CNY".into());
    }
    if !counted.is_empty() && counted.iter().any(|order| order.buyer.is_none()) {
        mark_unsupported(&mut metrics, MetricKey::Buyers);
    } else {
        let mut buyers = BTreeSet::new();
        for order in &counted {
            if let Some(buyer) = &order.buyer {
                buyers.insert(buyer.clone());
            }
        }
        metrics
            .values
            .insert(MetricKey::Buyers, buyers.len() as f64);
    }
    let refunded: Vec<&(String, Option<f64>)> = refunds
        .iter()
        .filter(|(status, _)| status == "SUCCESS")
        .collect();
    metrics
        .values
        .insert(MetricKey::RefundOrders, refunded.len() as f64);
    if refunded.iter().any(|(_, amount)| amount.is_none()) {
        mark_unsupported(&mut metrics, MetricKey::RefundAmount);
    } else {
        metrics.values.insert(
            MetricKey::RefundAmount,
            refunded.iter().filter_map(|(_, amount)| *amount).sum(),
        );
    }
    match live {
        Ok(count) => {
            metrics.values.insert(MetricKey::ProductsLive, count);
        }
        Err(message) => {
            mark_unsupported(&mut metrics, MetricKey::ProductsLive);
            metrics.error = Some(message);
        }
    }
    metrics.unsupported.sort();
    metrics
}

fn mark_unsupported(metrics: &mut ShopMetrics, key: MetricKey) {
    metrics.values.insert(key, 0.0);
    if !metrics.unsupported.contains(&key) {
        metrics.unsupported.push(key);
    }
}

fn counts_order(status: &str) -> bool {
    matches!(
        status,
        "WAIT_SELLER_SEND_GOODS"
            | "SELLER_CONSIGNED_PART"
            | "WAIT_BUYER_CONFIRM_GOODS"
            | "TRADE_BUYER_SIGNED"
            | "TRADE_FINISHED"
            | "TRADE_CLOSED"
            | "PAID_FORBID_CONSIGN"
    )
}

fn pending_shipment(status: &str) -> bool {
    matches!(
        status,
        "WAIT_SELLER_SEND_GOODS" | "SELLER_CONSIGNED_PART" | "PAID_FORBID_CONSIGN"
    )
}

pub async fn fetch_categories(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    ensure(shop)?;
    let parent = parent_id.filter(|value| !value.is_empty()).unwrap_or("0");
    let body = call(
        transport,
        shop,
        now,
        "taobao.itemcats.get",
        vec![
            ("parent_cid".into(), parent.to_string()),
            (
                "fields".into(),
                "cid,parent_cid,name,is_parent,status".into(),
            ),
        ],
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_categories(&body))
}

pub fn map_categories(body: &Value) -> Vec<Category> {
    let body = payload_ref(body);
    rows(body, "item_cats", "item_cat")
        .into_iter()
        .filter_map(|item| {
            if text(item, "status").is_some_and(|status| status == "deleted") {
                return None;
            }
            let id = identifier(item, "cid")?;
            let parent_id = identifier(item, "parent_cid").unwrap_or_else(|| "0".into());
            let name = text(item, "name").unwrap_or_else(|| id.clone());
            let leaf = !item
                .get("is_parent")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(Category {
                id,
                name,
                parent_id,
                leaf,
            })
        })
        .collect()
}

pub async fn fetch_attributes(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    ensure(shop)?;
    if category_id.parse::<u64>().is_err() {
        return Err(CommerceError::invalid("类目 ID 无效"));
    }
    let body = call(
        transport,
        shop,
        now,
        "taobao.itemprops.get",
        vec![
            ("cid".into(), category_id.to_string()),
            ("type".into(), "1".into()),
            (
                "fields".into(),
                "pid,name,must,multi,prop_values,is_enum_prop,is_input_prop,is_sale_prop,status,is_taosir".into(),
            ),
        ],
        None,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    Ok(map_attributes(&body))
}

pub fn map_attributes(body: &Value) -> Vec<CategoryAttribute> {
    let body = payload_ref(body);
    rows(body, "item_props", "item_prop")
        .into_iter()
        .filter_map(|item| {
            if text(item, "status").is_some_and(|status| status == "deleted") {
                return None;
            }
            let id = identifier(item, "pid")?;
            let name = text(item, "name").unwrap_or_else(|| id.clone());
            let required = item.get("must").and_then(Value::as_bool).unwrap_or(false);
            let multi = item.get("multi").and_then(Value::as_bool).unwrap_or(false);
            let enum_prop = item
                .get("is_enum_prop")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let taosir = item
                .get("is_taosir")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let input = if multi {
                AttributeInput::MultiSelect
            } else if enum_prop {
                AttributeInput::Select
            } else if taosir {
                AttributeInput::Number
            } else {
                AttributeInput::Text
            };
            let options = rows(item, "prop_values", "prop_value")
                .into_iter()
                .filter(|value| {
                    text(value, "status")
                        .map(|status| status != "deleted")
                        .unwrap_or(true)
                })
                .filter_map(|value| {
                    let name = text(value, "name").or_else(|| text(value, "name_alias"))?;
                    let id = identifier(value, "vid").unwrap_or_default();
                    Some(AttributeOption { id, name })
                })
                .collect();
            Some(CategoryAttribute {
                id,
                name,
                required,
                input,
                options,
                unit: None,
            })
        })
        .collect()
}

pub fn map_item_status(approve_status: &str, violation: bool) -> (ListingStatus, Option<String>) {
    if violation {
        return (ListingStatus::Banned, Some("violation".into()));
    }
    match approve_status {
        "onsale" => (ListingStatus::Live, None),
        "instock" => (
            ListingStatus::Reviewing,
            Some("approve_status=instock".into()),
        ),
        other => (
            ListingStatus::Reviewing,
            Some(format!("未识别的商品状态 {other}")),
        ),
    }
}

pub fn map_seller_skus(item: &Value) -> Vec<RemoteSku> {
    let root = payload_ref(item);
    let item = root.get("item").unwrap_or(root);
    rows(item, "skus", "sku")
        .into_iter()
        .map(|sku| RemoteSku {
            remote_id: identifier(sku, "sku_id"),
            properties: text(sku, "properties"),
            name: text(sku, "properties_name")
                .map(|value| sku_display_name(&value))
                .or_else(|| text(sku, "properties")),
            price: sku.get("price").and_then(parse_money),
            currency: sku.get("price").and_then(parse_money).map(|_| "CNY".into()),
            stock: sku.get("quantity").and_then(as_i64),
            outer_id: text(sku, "outer_id"),
        })
        .collect()
}

fn sku_display_name(properties_name: &str) -> String {
    let parts: Vec<&str> = properties_name
        .split(';')
        .filter(|part| !part.is_empty())
        .collect();
    let named: Vec<String> = parts
        .iter()
        .filter_map(|part| {
            let bits: Vec<&str> = part.split(':').collect();
            if bits.len() >= 4 {
                Some(format!("{}:{}", bits[2], bits[3..].join(":")))
            } else {
                None
            }
        })
        .collect();
    if named.len() == parts.len() && !named.is_empty() {
        named.join(";")
    } else {
        properties_name.to_string()
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
    if let Err(error) = preflight(shop) {
        return from_fail(None, error);
    }
    if let Some(remote_id) = existing_remote {
        return match item_status(transport, shop, now, remote_id).await {
            Ok((status, reason)) => outcome(Some(remote_id.to_string()), status, reason),
            Err(error) if is_delete(&error) => deleted_outcome(remote_id, &error),
            Err(error) => from_fail(Some(remote_id.to_string()), error),
        };
    }
    if let Err(message) = validate_listing(draft, target, images) {
        return outcome(None, ListingStatus::Rejected, Some(message));
    }
    let schema_body = match call(
        transport,
        shop,
        now,
        "alibaba.item.publish.schema.get",
        vec![
            ("market".into(), "taobao".into()),
            ("cat_id".into(), target.category_id.clone()),
            ("item_type".into(), "b".into()),
        ],
        None,
        false,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    let schema_xml = match schema_body.get("result").and_then(Value::as_str) {
        Some(xml) if xml.contains("<itemSchema") => xml.to_string(),
        _ => {
            return outcome(
                None,
                ListingStatus::Rejected,
                Some("淘宝未返回发布规则".into()),
            )
        }
    };
    let fields = parse_fields(&schema_xml);
    let needs_images = fields.iter().any(|field| is_image_field(&field.id));
    if needs_images
        && images.is_empty()
        && fields
            .iter()
            .any(|field| is_image_field(&field.id) && field.required)
    {
        return outcome(
            None,
            ListingStatus::Rejected,
            Some("发布规则要求主图".into()),
        );
    }
    let mut urls = Vec::new();
    if needs_images {
        for (name, bytes) in images {
            match upload_picture(transport, shop, now, name, bytes).await {
                Ok(url) => urls.push(url),
                Err(error) => return from_fail(None, error),
            }
        }
    }
    let schema = match fill_schema(&schema_xml, draft, target, &urls) {
        Ok(xml) => xml,
        Err(message) => return outcome(None, ListingStatus::Rejected, Some(message)),
    };
    let created = match call(
        transport,
        shop,
        now,
        "alibaba.item.publish.submit",
        vec![
            ("market".into(), "taobao".into()),
            ("cat_id".into(), target.category_id.clone()),
            ("schema".into(), schema),
        ],
        None,
        true,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => return from_fail(None, error),
    };
    let remote_id = created.get("item_id").and_then(identifier_value);
    let Some(remote_id) = remote_id else {
        return outcome(
            None,
            ListingStatus::Rejected,
            Some("淘宝未返回商品 ID".into()),
        );
    };
    match item_status(transport, shop, now, &remote_id).await {
        Ok((status, reason)) => outcome(Some(remote_id), status, reason),
        Err(error) => from_fail(Some(remote_id), error.into_uncertain()),
    }
}

fn validate_listing(
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
) -> Result<(), String> {
    if draft.title.trim().is_empty() {
        return Err("商品标题不能为空".into());
    }
    if target.category_id.parse::<u64>().is_err() {
        return Err("类目 ID 无效".into());
    }
    if images.len() > 5 {
        return Err("淘宝主图最多 5 张".into());
    }
    Ok(())
}

async fn upload_picture(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    name: &str,
    bytes: &[u8],
) -> Result<String, CallFail> {
    let title = if name.trim().is_empty() {
        "image.jpg".to_string()
    } else {
        name.to_string()
    };
    let body = call(
        transport,
        shop,
        now,
        "taobao.picture.upload",
        vec![
            ("picture_category_id".into(), "0".into()),
            ("image_input_title".into(), title),
            ("client_type".into(), "client:computer".into()),
            ("is_https".into(), "true".into()),
        ],
        Some((name.to_string(), bytes.to_vec())),
        false,
    )
    .await?;
    text(&body["picture"], "picture_path")
        .filter(|path| !path.is_empty())
        .ok_or_else(|| CallFail::Rejected("淘宝未返回图片地址".into()))
}

pub async fn refresh_remote(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    remote_id: &str,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    ensure(shop)?;
    match item_status(transport, shop, now, remote_id).await {
        Ok(status) => Ok(status),
        Err(error) if is_delete(&error) => Ok((
            ListingStatus::Rejected,
            Some(format!("DELETE {}", error.text())),
        )),
        Err(error) => Err(fail_to_error(error)),
    }
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
        "taobao.item.seller.get",
        vec![
            (
                "fields".into(),
                "num_iid,approve_status,violation,sku".into(),
            ),
            ("num_iid".into(), remote_id.to_string()),
        ],
        None,
        false,
    )
    .await?;
    let item = body
        .get("item")
        .ok_or_else(|| CallFail::Failed("淘宝未返回商品".into()))?;
    let status = text(item, "approve_status")
        .ok_or_else(|| CallFail::Failed("淘宝未返回商品状态".into()))?;
    let violation = item
        .get("violation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(map_item_status(&status, violation))
}

pub async fn fetch_products(
    transport: &Transport,
    shop: &ResolvedShop,
    shop_id: &str,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    ensure(shop)?;
    let (list, page) = product_cursor(cursor)?;
    let api = match list {
        ProductList::OnSale => "taobao.items.onsale.get",
        ProductList::Inventory => "taobao.items.inventory.get",
    };
    let mut business = vec![
        (
            "fields".into(),
            "num_iid,title,approve_status,price,num,outer_id,list_time".into(),
        ),
        ("page_no".into(), page.to_string()),
        ("page_size".into(), PAGE_SIZE.to_string()),
    ];
    if list == ProductList::Inventory {
        business.push(("banner".into(), "for_shelved".into()));
    }
    let body = call(transport, shop, now, api, business, None, false)
        .await
        .map_err(fail_to_error)?;
    let items = rows(&body, "items", "item")
        .into_iter()
        .filter_map(|item| {
            let id = identifier(item, "num_iid")?;
            let price = item.get("price").and_then(parse_money);
            Some(CommerceProduct {
                id,
                title: text(item, "title").unwrap_or_default(),
                status: text(item, "approve_status").unwrap_or_else(|| match list {
                    ProductList::OnSale => "onsale".into(),
                    ProductList::Inventory => "instock".into(),
                }),
                price,
                currency: price.map(|_| "CNY".into()),
                stock: item.get("num").and_then(as_i64),
                sku: text(item, "outer_id"),
            })
        })
        .collect::<Vec<_>>();
    let has_next = page_has_next(&body, page, PAGE_SIZE);
    Ok(ProductPage {
        shop_id: shop_id.to_string(),
        items,
        next_cursor: product_next(list, page, has_next),
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
    ensure(shop)?;
    let page = order_cursor(cursor)?;
    let (start, end) = metric_window(range, now);
    let listed = list_trades(
        transport,
        shop,
        now,
        Some((start, end)),
        page,
        PAGE_SIZE,
        false,
    )
    .await
    .map_err(fail_to_error)?;
    let items = listed.items.into_iter().map(order_from_remote).collect();
    let next_cursor = listed.has_next.then(|| (page + 1).to_string());
    Ok(OrderPage {
        shop_id: shop_id.to_string(),
        range,
        items,
        next_cursor,
    })
}

fn order_from_remote(order: RemoteOrder) -> CommerceOrder {
    let lines = order
        .skus
        .into_iter()
        .map(|sku| CommerceOrderLine {
            title: sku.name.clone().unwrap_or_default(),
            quantity: sku.stock.unwrap_or(0),
            sku: sku.outer_id.or(sku.remote_id),
        })
        .collect();
    CommerceOrder {
        id: order.remote_id,
        status: order.status,
        amount: order.amount,
        currency: order.currency,
        buyer: order.buyer,
        created_at: order.created_at,
        lines,
    }
}

async fn list_trades(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    window: Option<(i64, i64)>,
    page: i64,
    page_size: i64,
    all_pages: bool,
) -> Result<RemotePage<RemoteOrder>, CallFail> {
    let mut page_no = page;
    let mut items = Vec::new();
    let mut has_next;
    let mut pages = 0usize;
    loop {
        pages += 1;
        if pages > MAX_PAGES {
            return Err(CallFail::Failed("交易分页未取完，不返回不完整合计".into()));
        }
        let mut business = vec![
            (
                "fields".into(),
                "tid,status,payment,created,buyer_open_uid,buyer_nick,orders.title,orders.price,orders.num,orders.sku_id,orders.sku_properties_name,orders.outer_sku_id".into(),
            ),
            ("type".into(), TRADE_TYPES.into()),
            ("page_no".into(), page_no.to_string()),
            ("page_size".into(), page_size.to_string()),
            ("use_has_next".into(), "true".into()),
        ];
        if let Some((start, end)) = window {
            business.push(("start_created".into(), format_top(start)));
            business.push(("end_created".into(), format_top(end)));
        }
        let body = call(
            transport,
            shop,
            now,
            "taobao.trades.sold.get",
            business,
            None,
            false,
        )
        .await?;
        items.extend(
            rows(&body, "trades", "trade")
                .into_iter()
                .filter_map(map_trade),
        );
        has_next = body
            .get("has_next")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !all_pages || !has_next {
            break;
        }
        page_no += 1;
    }
    Ok(RemotePage { items, has_next })
}

fn map_trade(trade: &Value) -> Option<RemoteOrder> {
    let remote_id = identifier(trade, "tid")?;
    let status = text(trade, "status").unwrap_or_default();
    let amount = trade.get("payment").and_then(parse_money);
    Some(RemoteOrder {
        remote_id,
        pending_shipment: pending_shipment(&status),
        status,
        currency: amount.map(|_| "CNY".into()),
        amount,
        buyer: text(trade, "buyer_open_uid").or_else(|| text(trade, "buyer_nick")),
        created_at: text(trade, "created").as_deref().and_then(rfc3339_from_top),
        skus: map_order_skus(trade),
    })
}

fn map_order_skus(trade: &Value) -> Vec<RemoteSku> {
    rows(trade, "orders", "order")
        .into_iter()
        .map(|order| {
            let price = order.get("price").and_then(parse_money);
            RemoteSku {
                remote_id: identifier(order, "sku_id"),
                properties: text(order, "sku_properties_name"),
                name: text(order, "sku_properties_name"),
                price,
                currency: price.map(|_| "CNY".into()),
                stock: order.get("num").and_then(as_i64),
                outer_id: text(order, "outer_sku_id"),
            }
        })
        .collect()
}

async fn list_refunds(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    start: i64,
    end: i64,
) -> Result<Vec<(String, Option<f64>)>, CallFail> {
    let mut page_no = 1i64;
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let body = call(
            transport,
            shop,
            now,
            "taobao.refunds.receive.get",
            vec![
                (
                    "fields".into(),
                    "refund_id,status,refund_fee,created,dispute_type".into(),
                ),
                ("start_modified".into(), format_top(start)),
                ("end_modified".into(), format_top(end)),
                ("page_no".into(), page_no.to_string()),
                ("page_size".into(), REFUND_PAGE.to_string()),
                ("use_has_next".into(), "true".into()),
            ],
            None,
            false,
        )
        .await?;
        for refund in rows(&body, "refunds", "refund") {
            let dispute = text(refund, "dispute_type");
            if dispute
                .as_deref()
                .is_some_and(|value| !value.is_empty() && value != "REFUND")
            {
                continue;
            }
            let status = text(refund, "status").unwrap_or_default();
            let amount = refund.get("refund_fee").and_then(parse_money);
            items.push((status, amount));
        }
        if !body
            .get("has_next")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(items);
        }
        page_no += 1;
    }
    Err(CallFail::Failed("退款分页未取完，不返回不完整合计".into()))
}

async fn live_count(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
) -> Result<f64, CallFail> {
    let body = call(
        transport,
        shop,
        now,
        "taobao.items.onsale.get",
        vec![
            ("fields".into(), "num_iid".into()),
            ("page_no".into(), "1".into()),
            ("page_size".into(), "1".into()),
        ],
        None,
        false,
    )
    .await?;
    body.get("total_results")
        .and_then(as_f64)
        .ok_or_else(|| CallFail::Failed("淘宝未返回出售中商品总数".into()))
}

fn page_has_next(body: &Value, page: i64, page_size: i64) -> bool {
    let total = body.get("total_results").and_then(as_i64);
    body.get("has_next")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| total.is_some_and(|value| page * page_size < value))
}

fn product_cursor(cursor: Option<&str>) -> Result<(ProductList, i64), CommerceError> {
    let cursor = cursor.unwrap_or("").trim();
    if cursor.is_empty() {
        return Ok((ProductList::OnSale, 1));
    }
    if let Some(page) = cursor.strip_prefix("for_shelved:") {
        return Ok((ProductList::Inventory, page_number(page)?));
    }
    Ok((ProductList::OnSale, page_number(cursor)?))
}

fn product_next(list: ProductList, page: i64, has_next: bool) -> Option<String> {
    if has_next {
        return Some(match list {
            ProductList::OnSale => (page + 1).to_string(),
            ProductList::Inventory => format!("for_shelved:{}", page + 1),
        });
    }
    match list {
        ProductList::OnSale => Some("for_shelved:1".into()),
        ProductList::Inventory => None,
    }
}

fn order_cursor(cursor: Option<&str>) -> Result<i64, CommerceError> {
    let cursor = cursor.unwrap_or("").trim();
    if cursor.is_empty() {
        return Ok(1);
    }
    page_number(cursor)
}

fn page_number(value: &str) -> Result<i64, CommerceError> {
    let page = value
        .parse::<i64>()
        .map_err(|_| CommerceError::invalid("页码游标无效"))?;
    if page < 1 {
        return Err(CommerceError::invalid("页码必须从 1 开始"));
    }
    Ok(page)
}

fn is_delete(error: &CallFail) -> bool {
    let message = error.text();
    message.contains("item-is-delete") || message.contains("已经被删除")
}

fn deleted_outcome(remote_id: &str, error: &CallFail) -> PushOutcome {
    outcome(
        Some(remote_id.to_string()),
        ListingStatus::Rejected,
        Some(format!("DELETE {}", error.text())),
    )
}

pub fn fill_schema(
    xml: &str,
    draft: &ListingDraft,
    target: &ListingTarget,
    image_urls: &[String],
) -> Result<String, String> {
    let fields = parse_fields(xml);
    if fields.is_empty() {
        return Err("发布规则没有 field".into());
    }
    if !draft.skus.is_empty() && !fields.iter().any(|field| field.id == "sku") {
        return Err("发布规则没有 SKU 字段，无法映射规格".into());
    }
    if !target.attributes.is_empty() && !fields.iter().any(|field| field.id == "catProp") {
        return Err("发布规则没有类目属性字段 catProp，无法映射属性".into());
    }
    let mut body = String::from("<itemSchema>");
    for field in &fields {
        let piece = if field.id == "sku" {
            fill_sku(field, draft)?
        } else if field.id == "catProp" {
            fill_cat_prop(field, &target.attributes)?
        } else if is_image_field(&field.id) {
            fill_images(field, image_urls)?
        } else if field.kind == "complex" || field.kind == "multiComplex" {
            if field.required {
                return Err(format!("发布规则必填字段 {} 无法从商品草稿填写", field.id));
            }
            String::new()
        } else {
            match item_leaf(&field.id, draft) {
                Some(value) => render_leaf(field, &value),
                None if field.required => {
                    return Err(format!("发布规则必填字段 {} 无法从商品草稿填写", field.id));
                }
                None => String::new(),
            }
        };
        body.push_str(&piece);
    }
    body.push_str("</itemSchema>");
    Ok(body)
}

fn is_image_field(id: &str) -> bool {
    matches!(id, "images" | "item_images" | "itemImages")
}

fn item_leaf(id: &str, draft: &ListingDraft) -> Option<String> {
    match id {
        "title" => {
            let title = draft.title.trim();
            if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            }
        }
        "price" => item_price(draft).map(money),
        "quantity" | "num" => item_stock(draft).map(|value| value.to_string()),
        "outerId" | "outer_id" => item_outer(draft),
        "desc" | "description" => draft
            .description
            .clone()
            .filter(|value| !value.trim().is_empty()),
        _ => None,
    }
}

fn item_price(draft: &ListingDraft) -> Option<f64> {
    if draft.skus.len() == 1 {
        draft.skus[0].price.or(draft.price)
    } else if draft.skus.is_empty() {
        draft.price
    } else {
        draft
            .price
            .or_else(|| draft.skus.iter().find_map(|sku| sku.price))
    }
}

fn item_stock(draft: &ListingDraft) -> Option<i64> {
    if draft.skus.is_empty() {
        return draft.stock;
    }
    let mut sum = 0i64;
    for sku in &draft.skus {
        sum += sku.stock.or(draft.stock)?;
    }
    Some(sum)
}

fn item_outer(draft: &ListingDraft) -> Option<String> {
    draft
        .skus
        .iter()
        .find_map(|sku| sku.code.clone().filter(|value| !value.trim().is_empty()))
}

fn fill_images(field: &SchemaField, urls: &[String]) -> Result<String, String> {
    if field.kind == "input" {
        let Some(url) = urls.first() else {
            return if field.required {
                Err("发布规则要求主图".into())
            } else {
                Ok(String::new())
            };
        };
        return Ok(render_leaf(field, url));
    }
    let mut slots: Vec<&SchemaField> = field
        .children
        .iter()
        .filter(|child| child.id.starts_with("images_") || child.kind == "input")
        .collect();
    slots.sort_by(|left, right| left.id.cmp(&right.id));
    let mut inner = String::new();
    for (index, slot) in slots.iter().enumerate() {
        match urls.get(index) {
            Some(url) => inner.push_str(&render_leaf(slot, url)),
            None if slot.required => return Err(format!("发布规则必填图片 {}", slot.id)),
            None => {}
        }
    }
    if inner.is_empty() {
        return if field.required {
            Err("发布规则要求主图".into())
        } else {
            Ok(String::new())
        };
    }
    Ok(wrap(field, "complex-value", &inner))
}

fn fill_cat_prop(field: &SchemaField, attributes: &[ListingAttribute]) -> Result<String, String> {
    let mut inner = String::new();
    for child in &field.children {
        let Some(pid) = prop_key(&child.id) else {
            if child.required {
                return Err(format!("发布规则必填字段 {} 无法从商品草稿填写", child.id));
            }
            continue;
        };
        match attribute_raw(attributes, &child.id, &pid) {
            Some(raw) => {
                let value = resolve_option(child, &raw)?;
                inner.push_str(&render_leaf(child, &value));
            }
            None if child.required => {
                return Err(format!("类目属性 {} 缺少取值", child.name));
            }
            None => {}
        }
    }
    if inner.is_empty() {
        return if field.required {
            Err("发布规则要求类目属性".into())
        } else {
            Ok(String::new())
        };
    }
    Ok(wrap(field, "complex-value", &inner))
}

fn fill_sku(field: &SchemaField, draft: &ListingDraft) -> Result<String, String> {
    if draft.skus.is_empty() {
        return if field.required {
            Err("发布规则要求 SKU".into())
        } else {
            Ok(String::new())
        };
    }
    let mut rows = String::new();
    for sku in &draft.skus {
        let inner = fill_sku_row(&field.children, sku, draft)?;
        rows.push_str("<complex-values>");
        rows.push_str(&inner);
        rows.push_str("</complex-values>");
    }
    Ok(format!(
        "<field id=\"sku\" name=\"{}\" type=\"multiComplex\">{rows}</field>",
        xml_escape(&field.name)
    ))
}

fn fill_sku_row(
    children: &[SchemaField],
    sku: &ListingSku,
    draft: &ListingDraft,
) -> Result<String, String> {
    let mut xml = String::new();
    for child in children {
        if child.id == "props"
            || (child.kind == "complex"
                && child
                    .children
                    .iter()
                    .any(|item| prop_key(&item.id).is_some()))
        {
            let inner = fill_sale_props(&child.children, sku)?;
            if inner.is_empty() {
                if child.required {
                    return Err(format!("SKU {} 缺少销售属性", sku.name));
                }
            } else {
                xml.push_str(&wrap(child, "complex-value", &inner));
            }
            continue;
        }
        if prop_key(&child.id).is_some() {
            match match_sale(child, sku)? {
                Some(value) => xml.push_str(&render_leaf(child, &value)),
                None if child.required => return Err(format!("SKU 缺少销售属性 {}", child.name)),
                None => {}
            }
            continue;
        }
        match sku_leaf(&child.id, sku, draft) {
            Some(value) => xml.push_str(&render_leaf(child, &value)),
            None if child.required => {
                return Err(format!("发布规则必填字段 {} 无法映射 SKU", child.id))
            }
            None => {}
        }
    }
    Ok(xml)
}

fn fill_sale_props(children: &[SchemaField], sku: &ListingSku) -> Result<String, String> {
    let mut xml = String::new();
    for child in children {
        if prop_key(&child.id).is_none() {
            if child.required {
                return Err(format!("发布规则必填字段 {} 无法映射 SKU", child.id));
            }
            continue;
        }
        match match_sale(child, sku)? {
            Some(value) => xml.push_str(&render_leaf(child, &value)),
            None if child.required => return Err(format!("SKU 缺少销售属性 {}", child.name)),
            None => {}
        }
    }
    Ok(xml)
}

fn sku_leaf(id: &str, sku: &ListingSku, draft: &ListingDraft) -> Option<String> {
    match id {
        "skuPrice" | "price" => sku.price.or(draft.price).map(money),
        "skuStock" | "skuQuantity" | "quantity" | "stock" | "num" => {
            sku.stock.or(draft.stock).map(|value| value.to_string())
        }
        "skuOuterId" | "outerId" | "outer_id" => {
            sku.code.clone().filter(|value| !value.trim().is_empty())
        }
        _ => None,
    }
}

fn match_sale(field: &SchemaField, sku: &ListingSku) -> Result<Option<String>, String> {
    let pairs = sale_pairs(&sku.name);
    let pid = prop_key(&field.id);
    let matched = pairs.iter().find(|(key, _)| {
        key == &field.id
            || key == &field.name
            || pid.as_ref().is_some_and(|value| {
                key == value || key == &format!("p-{value}") || key == &format!("prop_{value}")
            })
    });
    let raw = if let Some((_, value)) = matched {
        value.clone()
    } else if pairs.is_empty() && !sku.name.trim().is_empty() && prop_key(&field.id).is_some() {
        sku.name.trim().to_string()
    } else {
        return Ok(None);
    };
    Ok(Some(resolve_option(field, &raw)?))
}

fn sale_pairs(name: &str) -> Vec<(String, String)> {
    name.split(';')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let part = part
                .rsplit_once('|')
                .map(|(_, right)| right)
                .unwrap_or(part);
            let (key, value) = part.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn attribute_raw(attributes: &[ListingAttribute], field_id: &str, pid: &str) -> Option<String> {
    attributes
        .iter()
        .find(|attribute| {
            attribute.id == field_id
                || attribute.id == pid
                || attribute.id == format!("p-{pid}")
                || attribute.id == format!("prop_{pid}")
        })
        .and_then(|attribute| {
            if !attribute.value.trim().is_empty() {
                Some(attribute.value.trim().to_string())
            } else {
                attribute
                    .values
                    .iter()
                    .find(|value| !value.trim().is_empty())
                    .map(|value| value.trim().to_string())
            }
        })
}

fn resolve_option(field: &SchemaField, raw: &str) -> Result<String, String> {
    if field.options.is_empty() {
        return Ok(raw.to_string());
    }
    if let Some((value, _)) = field
        .options
        .iter()
        .find(|(value, name)| value == raw || name == raw)
    {
        return Ok(value.clone());
    }
    Err(format!(
        "属性 {} 的值「{raw}」不在发布规则选项中",
        field.name
    ))
}

fn prop_key(id: &str) -> Option<String> {
    id.strip_prefix("p-")
        .or_else(|| id.strip_prefix("prop_"))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn render_leaf(field: &SchemaField, value: &str) -> String {
    format!(
        "<field id=\"{}\" name=\"{}\" type=\"{}\"><value>{}</value></field>",
        xml_escape(&field.id),
        xml_escape(&field.name),
        xml_escape(&field.kind),
        xml_escape(value)
    )
}

fn wrap(field: &SchemaField, tag: &str, inner: &str) -> String {
    format!(
        "<field id=\"{}\" name=\"{}\" type=\"{}\"><{tag}>{inner}</{tag}></field>",
        xml_escape(&field.id),
        xml_escape(&field.name),
        xml_escape(&field.kind)
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn money(value: f64) -> String {
    format!("{value:.2}")
}

fn parse_fields(xml: &str) -> Vec<SchemaField> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < xml.len() {
        let Some(start) = find_field_open(&xml[index..]).map(|relative| index + relative) else {
            break;
        };
        if xml[start..].starts_with("</field") {
            index = start + "<".len();
            continue;
        }
        let Some(tag_end) = xml[start..].find('>') else {
            break;
        };
        let tag = &xml[start..start + tag_end + 1];
        if tag.ends_with("/>") {
            out.push(field_from(tag, ""));
            index = start + tag_end + 1;
            continue;
        }
        let inner_at = start + tag_end + 1;
        let Some(close) = matching_close(xml, inner_at) else {
            break;
        };
        out.push(field_from(tag, &xml[inner_at..close]));
        index = close + "</field>".len();
    }
    out
}

fn matching_close(xml: &str, from: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut index = from;
    while index < xml.len() {
        let rest = &xml[index..];
        if rest.starts_with("</field>") {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
            index += "</field>".len();
            continue;
        }
        if opens_field(rest) {
            let relative = rest.find('>')?;
            let tag = &rest[..=relative];
            if !tag.ends_with("/>") {
                depth += 1;
            }
            index += relative + 1;
            continue;
        }
        index += rest.chars().next().map(char::len_utf8).unwrap_or(1);
    }
    None
}

fn find_field_open(xml: &str) -> Option<usize> {
    let mut offset = 0;
    let mut rest = xml;
    while let Some(relative) = rest.find("<field") {
        let at = offset + relative;
        if opens_field(&xml[at..]) {
            return Some(at);
        }
        offset = at + "<field".len();
        rest = &xml[offset..];
    }
    None
}

fn opens_field(rest: &str) -> bool {
    rest.strip_prefix("<field")
        .is_some_and(|after| after.starts_with([' ', '>', '/', '\n', '\t', '\r']))
}

fn field_from(tag: &str, inner: &str) -> SchemaField {
    let head = inner.split("<field").next().unwrap_or("");
    SchemaField {
        id: attr(tag, "id").unwrap_or("").to_string(),
        name: attr(tag, "name").unwrap_or("").to_string(),
        kind: attr(tag, "type").unwrap_or("input").to_string(),
        required: head.contains("requiredRule")
            && (head.contains("value=\"true\"") || head.contains("value='true'")),
        options: parse_options(head),
        children: parse_fields(inner),
    }
}

fn parse_options(head: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = head;
    while let Some(relative) = rest.find("<option") {
        let Some(end) = rest[relative..].find('>') else {
            break;
        };
        let tag = &rest[relative..relative + end + 1];
        let value = attr(tag, "value").unwrap_or("").to_string();
        let name = attr(tag, "displayName").unwrap_or("").to_string();
        if !value.is_empty() || !name.is_empty() {
            out.push((value, name));
        }
        rest = &rest[relative + end + 1..];
    }
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn ensure(shop: &ResolvedShop) -> Result<(), CommerceError> {
    if shop.platform != Platform::Taobao {
        return Err(CommerceError::unsupported("该平台尚未接入"));
    }
    preflight(shop).map_err(fail_to_error)
}

fn preflight(shop: &ResolvedShop) -> Result<(), CallFail> {
    if shop.platform != Platform::Taobao {
        return Err(CallFail::Rejected("该平台尚未接入".into()));
    }
    if shop.partner_id.trim().is_empty() || shop.partner_key.trim().is_empty() {
        return Err(CallFail::Rejected("淘宝应用密钥为空".into()));
    }
    if shop.access_token.trim().is_empty() {
        return Err(CallFail::Auth("淘宝 session 为空，需要重新授权".into()));
    }
    Ok(())
}

fn from_fail(remote_id: Option<String>, error: CallFail) -> PushOutcome {
    let (status, reason) = match error {
        CallFail::Uncertain(message) => (ListingStatus::Uncertain, message),
        CallFail::Failed(message) => (ListingStatus::Failed, message),
        CallFail::Auth(message) | CallFail::Rejected(message) => (ListingStatus::Rejected, message),
    };
    outcome(remote_id, status, Some(reason))
}

fn outcome(
    remote_id: Option<String>,
    status: ListingStatus,
    reason: Option<String>,
) -> PushOutcome {
    PushOutcome {
        remote_id,
        status,
        reason,
    }
}

fn fail_to_error(error: CallFail) -> CommerceError {
    match error {
        CallFail::Auth(message) | CallFail::Rejected(message) => CommerceError::rejected(message),
        CallFail::Failed(message) => CommerceError::failed(message),
        CallFail::Uncertain(message) => CommerceError::uncertain(message),
    }
}

impl CallFail {
    fn into_uncertain(self) -> Self {
        match self {
            Self::Uncertain(message) => Self::Uncertain(message),
            Self::Auth(message) | Self::Rejected(message) | Self::Failed(message) => {
                Self::Uncertain(message)
            }
        }
    }
}

async fn call(
    transport: &Transport,
    shop: &ResolvedShop,
    now: i64,
    api: &'static str,
    business: Vec<(String, String)>,
    file: Option<(String, Vec<u8>)>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let mut pairs = vec![
        ("method".into(), api.to_string()),
        ("app_key".into(), shop.partner_id.clone()),
        ("session".into(), shop.access_token.clone()),
        ("timestamp".into(), format_top(now)),
        ("format".into(), "json".into()),
        ("v".into(), "2.0".into()),
        ("sign_method".into(), "md5".into()),
    ];
    pairs.extend(business);
    let sign = {
        let sign_input: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        sign_top(&shop.partner_key, &sign_input)
    };
    pairs.push(("sign".into(), sign));
    let (file_name, file_bytes, headers) = match file {
        Some((name, bytes)) => (
            Some(if name.trim().is_empty() {
                "image.jpg".into()
            } else {
                name
            }),
            Some(bytes),
            vec![("x-dsivio-file-field".into(), "img".into())],
        ),
        None => (
            None,
            None,
            vec![(
                "content-type".into(),
                "application/x-www-form-urlencoded;charset=utf-8".into(),
            )],
        ),
    };
    let request = Outbound {
        method: "POST",
        host: HOST.to_string(),
        path: PATH.to_string(),
        headers,
        query: pairs,
        json: None,
        file_name,
        file_bytes,
        mutating,
    };
    let response = match transport.execute(request).await {
        Ok(response) => response,
        Err(fault) => return Err(fault_to_fail(mutating, fault)),
    };
    classify(mutating, response)
}

fn fault_to_fail(mutating: bool, fault: TransportFault) -> CallFail {
    match fault {
        TransportFault::BeforeDispatch(message) => CallFail::Failed(message),
        TransportFault::AfterDispatch(message) if mutating => CallFail::Uncertain(message),
        TransportFault::AfterDispatch(message) => CallFail::Failed(message),
    }
}

fn classify(mutating: bool, response: Inbound) -> Result<Value, CallFail> {
    if let Some(error) = response.body.get("error_response") {
        let message = top_error_text(error);
        if is_auth(error) {
            return Err(CallFail::Auth(message));
        }
        if response.status >= 500 && mutating {
            return Err(CallFail::Uncertain(message));
        }
        return Err(CallFail::Rejected(message));
    }
    if response.status >= 500 {
        let message = format!("HTTP {}", response.status);
        return if mutating {
            Err(CallFail::Uncertain(message))
        } else {
            Err(CallFail::Failed(message))
        };
    }
    if response.status >= 400 {
        return Err(CallFail::Rejected(format!("HTTP {}", response.status)));
    }
    Ok(payload(&response.body))
}

fn top_error_text(error: &Value) -> String {
    let code = error.get("code").map(value_text).unwrap_or_default();
    let sub_code = text(error, "sub_code").unwrap_or_default();
    let sub_msg = text(error, "sub_msg").unwrap_or_default();
    let msg = text(error, "msg").unwrap_or_default();
    match (sub_code.is_empty(), sub_msg.is_empty(), msg.is_empty()) {
        (false, false, _) => format!("{code} {sub_code}: {sub_msg}"),
        (false, true, _) => format!("{code} {sub_code}"),
        (true, _, false) => format!("{code}: {msg}"),
        _ => {
            if code.is_empty() {
                "淘宝返回错误".into()
            } else {
                code
            }
        }
    }
}

fn is_auth(error: &Value) -> bool {
    let code = error.get("code").and_then(as_i64);
    let sub = text(error, "sub_code").unwrap_or_default();
    let msg = text(error, "msg").unwrap_or_default().to_ascii_lowercase();
    matches!(code, Some(11 | 12 | 26 | 27))
        || sub.contains("permission")
        || sub.contains("session")
        || msg.contains("invalid session")
        || (msg.contains("insufficient") && msg.contains("permission"))
}

fn payload(body: &Value) -> Value {
    payload_ref(body).clone()
}

fn payload_ref(body: &Value) -> &Value {
    body.as_object()
        .and_then(|map| {
            map.iter()
                .find(|(key, _)| key.ends_with("_response") && key.as_str() != "error_response")
                .map(|(_, value)| value)
        })
        .unwrap_or(body)
}

fn rows<'a>(value: &'a Value, wrapper: &str, item: &str) -> Vec<&'a Value> {
    let node = value.get(wrapper).unwrap_or(value);
    match node.get(item) {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(other) if other.is_object() => vec![other],
        _ => Vec::new(),
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
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

fn as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|item| item as f64))
        .or_else(|| value.as_str().and_then(|item| item.parse().ok()))
}

fn parse_money(value: &Value) -> Option<f64> {
    let parsed = as_f64(value)?;
    parsed.is_finite().then_some(parsed)
}

fn value_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "taobao_tests.rs"]
mod taobao_tests;
