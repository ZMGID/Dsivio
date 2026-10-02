//! 微信小店业务适配器。令牌只走店铺会话里的 `access_token`，不另存凭证，也不做网关签名。
//! 类目、发布规则和图片走当前小店 `/shop/ec` 接口；商品、订单、罗盘仍按小店文档使用
//! `/channels/ec` 路径。不调用已废弃的视频号类目接口，也不单独调上架接口：添加/更新商品时 `listing=1`。
//!
//! 获取所有类目：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-category/api_getallcategory>
//! 获取类目信息：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-category/api_getcategorydetail.html>
//! 类目发布规则：<https://developers.weixin.qq.com/doc/store/shop/API/category-rule/api_getcategoryproductrule.html>
//! 上传图片：<https://developers.weixin.qq.com/doc/store/shop/API/apimgnt/api_img_upload.html>
//! 添加商品：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-product/shop/api_addproduct.html>
//! 更新商品：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-product/shop/api_updateproduct.html>
//! 获取商品：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-product/shop/api_getproduct.html>
//! 商品列表：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-product/shop/api_getproductlist.html>
//! 订单列表：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-order/api_getorderlist.html>
//! 订单详情：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-order/api_getorder.html>
//! 罗盘商品列表：<https://developers.weixin.qq.com/doc/store/shop/API/compass/api_getshopproductlist.html>
//! 运费模板：<https://developers.weixin.qq.com/doc/store/shop/API/channels-shop-delivery/merchant/api_getfreighttemplatelist.html>
//!
//! 金额字段单位是分，这里换成元，币种 CNY。接口没有的指标放进 `unsupported`，不补 0。
//! 图片上传使用多部件字段 `media`，查询参数 `upload_type=0`、`resp_type=1`，并带上读到的宽高。

use super::adapter::{self, ApiSession, HttpBody, HttpRequest, MultipartFile, SharedTransport};
use super::shopee::PushOutcome;
use super::types::{
    AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities, CommerceError, CommerceOrder,
    CommerceOrderLine, CommerceProduct, ListingAttribute, ListingDraft, ListingSku, ListingStatus, ListingTarget,
    MetricKey, MetricRange, OrderPage, ProductPage, ShopMetrics,
};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const HOST: &str = "https://api.weixin.qq.com";
const CHINA_OFFSET: i64 = 8 * 3600;
const NO_BRAND: &str = "2100000000";
const ORDER_PAGE: i64 = 50;
const PRODUCT_PAGE: i64 = 10;
const COMPASS_LIMIT: i64 = 10;
const MAX_PAGES: usize = 20;
const MAX_COMPASS_PAGES: usize = 100;
const ORDER_SPAN: i64 = 7 * 86_400 - 1;

const CATEGORY_ALL: &str = "/shop/ec/category/all";
const CATEGORY_DETAIL: &str = "/shop/ec/category/detail";
const CATEGORY_RULE: &str = "/shop/ec/category/getcategoryproductrule";
const IMG_UPLOAD: &str = "/shop/ec/basics/img/upload";
const PRODUCT_ADD: &str = "/channels/ec/product/add";
const PRODUCT_UPDATE: &str = "/channels/ec/product/update";
const PRODUCT_GET: &str = "/channels/ec/product/get";
const PRODUCT_LIST: &str = "/channels/ec/product/list/get";
const ORDER_LIST: &str = "/channels/ec/order/list/get";
const ORDER_GET: &str = "/channels/ec/order/get";
const FREIGHT_LIST: &str = "/channels/ec/merchant/getfreighttemplatelist";
const COMPASS_PRODUCTS: &str = "/channels/ec/compass/shop/product/list/get";

pub fn capabilities() -> CommerceCapabilities {
    CommerceCapabilities {
        platform: Platform::Wechat,
        metrics: MetricKey::ALL.to_vec(),
        listing: true,
        categories: true,
        products: true,
        orders: true,
        notes: capability_notes().iter().map(|note| (*note).to_string()).collect(),
    }
}

pub fn capability_notes() -> &'static [&'static str] {
    &[
        "成交额、订单、买家和待发货按中国时间从订单详情汇总。订单列表单次不超过 7 天，近 30 天拆段。实付金额单位分，换成元，币种 CNY。不含待付款、一起买待成团和未付款取消。",
        "退款金额和退款订单只来自罗盘商品列表的历史日（pay_refund_gmv、refund_cnt）。罗盘没有当天，今天以及含今天的区间不返回退款，也不记为 0。",
        "在售商品是商品列表 status=5 的 total_num。接口没给的指标放在 unsupported，不补 0。",
        "上架使用微信小店商品接口：图片上传、cats_v2、商品参数和 SKU，listing=1 提交审核。店铺需已有运费模板。令牌复用店铺 access_token。",
    ]
}

#[derive(Debug)]
enum CallFail {
    Auth(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
}

struct OrderView {
    status: i64,
    amount_fen: Option<i64>,
    buyer: Option<String>,
}

struct OrderFold {
    orders: f64,
    gmv_fen: Option<i64>,
    buyers: Option<f64>,
    pending: f64,
}

struct RefundFold {
    amount_fen: Option<i64>,
    orders: Option<i64>,
}

struct SkuDraft {
    name: String,
    fen: i64,
    stock: i64,
    code: Option<String>,
    attrs: Vec<(String, String)>,
}

struct Prepared {
    title: String,
    description: Option<String>,
    category_id: i64,
    skus: Vec<SkuDraft>,
    ready_urls: Vec<String>,
    attrs: Vec<(String, String)>,
    brand: String,
    weight_grams: Option<i64>,
}

struct CatNode {
    name: String,
    parent_id: String,
    leaf: Option<bool>,
}

pub async fn metrics(
    transport: &SharedTransport,
    shop: &ApiSession,
    range: MetricRange,
    now: i64,
) -> Result<ShopMetrics, CommerceError> {
    ensure(shop)?;
    let orders = match list_order_views(transport, shop, now, range).await {
        Ok(items) => Ok(items),
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(error) => Err(error_text(&error)),
    };
    let refunds = if range == MetricRange::Yesterday {
        match collect_refunds(transport, shop, now).await {
            Ok(fold) => Ok(Some(fold)),
            Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
            Err(error) => Err(error_text(&error)),
        }
    } else {
        Ok(None)
    };
    let live = match live_count(transport, shop, now).await {
        Ok(count) => Ok(Some(count)),
        Err(CallFail::Auth(message)) => return Err(CommerceError::rejected(message)),
        Err(_) => Ok(None),
    };
    let orders = orders.map(|items| fold_orders(&items));
    Ok(assemble_metrics(&shop.shop_id, range, now, orders, refunds, live?))
}

pub async fn categories(
    transport: &SharedTransport,
    shop: &ApiSession,
    parent_id: Option<&str>,
    now: i64,
) -> Result<Vec<Category>, CommerceError> {
    let _ = now;
    ensure(shop)?;
    let body = call(transport, shop, CATEGORY_ALL, Vec::new(), Some(json!({})), None, false)
        .await
        .map_err(to_error)?;
    let mut categories = categories_from(&body);
    if let Some(parent) = parent_id.map(str::trim).filter(|value| !value.is_empty()) {
        categories.retain(|category| category.parent_id == parent);
    }
    Ok(categories)
}

pub async fn attributes(
    transport: &SharedTransport,
    shop: &ApiSession,
    category_id: &str,
    now: i64,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    let _ = now;
    ensure(shop)?;
    let category_id = parse_category_id(category_id)?;
    let body = call(
        transport,
        shop,
        CATEGORY_RULE,
        Vec::new(),
        Some(json!({"cat_id": category_id, "release_mode": 0})),
        None,
        false,
    )
    .await
    .map_err(to_error)?;
    Ok(attribute_list(&body))
}

pub async fn products(
    transport: &SharedTransport,
    shop: &ApiSession,
    cursor: Option<&str>,
    now: i64,
) -> Result<ProductPage, CommerceError> {
    let _ = now;
    ensure(shop)?;
    let incoming = cursor.unwrap_or("").trim();
    let mut payload = json!({"page_size": PRODUCT_PAGE});
    if !incoming.is_empty() {
        payload["next_key"] = json!(incoming);
    }
    let body = call(transport, shop, PRODUCT_LIST, Vec::new(), Some(payload), None, false)
        .await
        .map_err(to_error)?;
    let ids = id_list(&body, "product_ids");
    let mut items = Vec::with_capacity(ids.len());
    for id in &ids {
        let detail = call(transport, shop, PRODUCT_GET, Vec::new(), Some(json!({"product_id": id, "data_type": 3})), None, false)
            .await
            .map_err(to_error)?;
        items.push(map_remote_product(&detail).map_err(to_error)?);
    }
    let next = text(&body, "next_key").unwrap_or_default();
    let next_cursor = if ids.is_empty() || next.is_empty() || next == incoming {
        None
    } else {
        Some(next)
    };
    Ok(ProductPage { shop_id: shop.shop_id.clone(), items, next_cursor })
}

pub async fn orders(
    transport: &SharedTransport,
    shop: &ApiSession,
    range: MetricRange,
    cursor: Option<&str>,
    now: i64,
) -> Result<OrderPage, CommerceError> {
    ensure(shop)?;
    let windows = order_windows(range, now);
    if windows.is_empty() {
        return Ok(OrderPage { shop_id: shop.shop_id.clone(), range, items: Vec::new(), next_cursor: None });
    }
    let (index, api_cursor) = parse_order_cursor(cursor, windows.len()).map_err(CommerceError::invalid)?;
    let (start, end) = windows[index];
    let body = order_list(transport, shop, start, end, &api_cursor).await.map_err(to_error)?;
    let ids = id_list(&body, "order_id_list");
    let mut items = Vec::with_capacity(ids.len());
    for id in &ids {
        let detail = call(transport, shop, ORDER_GET, Vec::new(), Some(json!({"order_id": id})), None, false)
            .await
            .map_err(to_error)?;
        items.push(map_remote_order(&detail).map_err(to_error)?);
    }
    let has_more = body.get("has_more").and_then(Value::as_bool).unwrap_or(false);
    let next = text(&body, "next_key").unwrap_or_default();
    let next_cursor = if has_more {
        if next.is_empty() || next == api_cursor {
            return Err(CommerceError::failed("微信小店未返回订单下一页标记"));
        }
        Some(format!("{index}:{next}"))
    } else if index + 1 < windows.len() {
        Some(format!("{}:", index + 1))
    } else {
        None
    };
    Ok(OrderPage { shop_id: shop.shop_id.clone(), range, items, next_cursor })
}

pub async fn push_listing(
    transport: &SharedTransport,
    shop: &ApiSession,
    draft: &ListingDraft,
    target: &ListingTarget,
    images: &[(String, Vec<u8>)],
    now: i64,
    existing_remote: Option<&str>,
) -> PushOutcome {
    let _ = now;
    if shop.platform != Platform::Wechat {
        return outcome(None, ListingStatus::Rejected, Some("该平台尚未接入上架".into()));
    }
    let existing = existing_remote.map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned);
    if shop.access_token.trim().is_empty() {
        return outcome(existing.clone(), ListingStatus::Rejected, Some("店铺访问令牌为空，请重新绑定微信小店".into()));
    }
    let mut prepared = match prepare(draft, target, images) {
        Ok(prepared) => prepared,
        Err(message) => return outcome(existing.clone(), ListingStatus::Rejected, Some(message)),
    };
    let categories = match load_categories(transport, shop).await {
        Ok(items) => items,
        Err(error) => return from_fail(existing.clone(), error),
    };
    let chain = match category_chain(&categories, target.category_id.trim()) {
        Ok(chain) => chain,
        Err(error) => return from_fail(existing.clone(), error),
    };
    let (seven_day, limit_brand) = match category_facts(transport, shop, prepared.category_id).await {
        Ok(facts) => facts,
        Err(error) => return from_fail(existing.clone(), error),
    };
    if limit_brand && prepared.brand == NO_BRAND {
        return from_fail(existing.clone(), CallFail::Rejected("该类目必须填写品牌 ID".into()));
    }
    let rules = match category_rules(transport, shop, prepared.category_id).await {
        Ok(rules) => rules,
        Err(error) => return from_fail(existing.clone(), error),
    };
    if let Some(message) = missing_attrs(&rules.0, &prepared.attrs) {
        return from_fail(existing.clone(), CallFail::Rejected(message));
    }
    if let Some(floor) = rules.2 {
        if prepared.skus.iter().any(|sku| sku.fen < floor) {
            return from_fail(existing.clone(), CallFail::Rejected(format!("售价低于类目价格下限 {floor} 分")));
        }
    }
    if let Err(error) = apply_sale_attrs(&mut prepared.skus, &rules.1) {
        return from_fail(existing.clone(), error);
    }
    let template_id = match freight_template(transport, shop).await {
        Ok(id) => id,
        Err(error) => return from_fail(existing.clone(), error),
    };
    let mut urls = Vec::new();
    for (name, bytes) in images {
        match upload_image(transport, shop, name, bytes).await {
            Ok(url) => urls.push(url),
            Err(error) => return from_fail(existing.clone(), error),
        }
    }
    urls.extend(prepared.ready_urls.iter().cloned());
    if urls.len() < 3 || urls.len() > 9 {
        return from_fail(existing.clone(), CallFail::Rejected("微信小店商品头图需要 3 到 9 张".into()));
    }
    if has_duplicate(&urls) {
        return from_fail(existing.clone(), CallFail::Rejected("商品头图不能重复".into()));
    }
    let path = if existing.is_some() { PRODUCT_UPDATE } else { PRODUCT_ADD };
    let payload = product_payload(&prepared, &chain, &template_id, &urls, seven_day, existing.as_deref());
    let created = match call(transport, shop, path, Vec::new(), Some(payload), None, true).await {
        Ok(body) => body,
        Err(error) => return from_fail(existing.clone(), error),
    };
    let remote_id = created.pointer("/data/product_id").and_then(identifier_value).or(existing);
    if remote_id.is_none() {
        return outcome(None, ListingStatus::Uncertain, Some("微信小店已接受提交但未返回商品 ID".into()));
    }
    outcome(remote_id, ListingStatus::Reviewing, None)
}

pub async fn listing_status(
    transport: &SharedTransport,
    shop: &ApiSession,
    remote_id: &str,
    now: i64,
) -> Result<(ListingStatus, Option<String>), CommerceError> {
    let _ = now;
    ensure(shop)?;
    if remote_id.trim().is_empty() {
        return Err(CommerceError::invalid("缺少商品 ID"));
    }
    let body = call(
        transport,
        shop,
        PRODUCT_GET,
        Vec::new(),
        Some(json!({"product_id": remote_id.trim(), "data_type": 3})),
        None,
        false,
    )
    .await
    .map_err(to_error)?;
    let (online, edit, audit) = read_state(&body);
    map_product_status(online, edit, audit.as_deref()).ok_or_else(|| CommerceError::failed("微信小店未返回商品状态"))
}

fn assemble_metrics(
    shop_id: &str,
    range: MetricRange,
    now: i64,
    orders: Result<Option<OrderFold>, String>,
    refunds: Result<Option<RefundFold>, String>,
    live: Option<f64>,
) -> ShopMetrics {
    let mut values = BTreeMap::new();
    let mut unsupported = Vec::new();
    let mut error = None;
    match orders {
        Err(message) => {
            unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
            error = Some(message);
        }
        Ok(None) => {
            unsupported.extend([MetricKey::Gmv, MetricKey::Orders, MetricKey::Buyers, MetricKey::PendingShipment]);
            error = Some("订单状态无法归类".into());
        }
        Ok(Some(fold)) => {
            values.insert(MetricKey::Orders, fold.orders);
            values.insert(MetricKey::PendingShipment, fold.pending);
            match fold.gmv_fen {
                Some(fen) => {
                    values.insert(MetricKey::Gmv, yuan(fen));
                }
                None => unsupported.push(MetricKey::Gmv),
            }
            match fold.buyers {
                Some(count) => {
                    values.insert(MetricKey::Buyers, count);
                }
                None => unsupported.push(MetricKey::Buyers),
            }
        }
    }
    match refunds {
        Ok(None) => unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]),
        Err(message) => {
            unsupported.extend([MetricKey::RefundAmount, MetricKey::RefundOrders]);
            if error.is_none() {
                error = Some(message);
            }
        }
        Ok(Some(fold)) => {
            match fold.amount_fen {
                Some(fen) => {
                    values.insert(MetricKey::RefundAmount, yuan(fen));
                }
                None => unsupported.push(MetricKey::RefundAmount),
            }
            match fold.orders {
                Some(count) => {
                    values.insert(MetricKey::RefundOrders, count as f64);
                }
                None => unsupported.push(MetricKey::RefundOrders),
            }
        }
    }
    match live {
        Some(count) => {
            values.insert(MetricKey::ProductsLive, count);
        }
        None => unsupported.push(MetricKey::ProductsLive),
    }
    for key in &unsupported {
        values.remove(key);
    }
    unsupported.sort();
    unsupported.dedup();
    let currency = if values.contains_key(&MetricKey::Gmv) || values.contains_key(&MetricKey::RefundAmount) {
        Some("CNY".into())
    } else {
        None
    };
    ShopMetrics {
        shop_id: shop_id.to_string(),
        range,
        currency,
        values,
        unsupported,
        fetched_at: chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default(),
        error,
    }
}

fn fold_orders(orders: &[OrderView]) -> Option<OrderFold> {
    let mut counted = Vec::new();
    for order in orders {
        match classify_status(order.status) {
            Some(true) => counted.push(order),
            Some(false) => {}
            None => return None,
        }
    }
    if counted.is_empty() {
        return Some(OrderFold { orders: 0.0, gmv_fen: Some(0), buyers: Some(0.0), pending: 0.0 });
    }
    let mut gmv = Some(0i64);
    let mut buyers = BTreeSet::new();
    let mut buyer_gap = false;
    let mut pending = 0.0;
    for order in &counted {
        match order.amount_fen {
            Some(fen) => gmv = gmv.and_then(|sum| sum.checked_add(fen)),
            None => gmv = None,
        }
        match &order.buyer {
            Some(buyer) => {
                buyers.insert(buyer.clone());
            }
            None => buyer_gap = true,
        }
        if order.status == 20 {
            pending += 1.0;
        }
    }
    Some(OrderFold {
        orders: counted.len() as f64,
        gmv_fen: gmv,
        buyers: if buyer_gap { None } else { Some(buyers.len() as f64) },
        pending,
    })
}

fn classify_status(status: i64) -> Option<bool> {
    Some(match status {
        10 | 13 | 250 => false,
        12 | 17 | 20 | 21 | 30 | 100 | 200 => true,
        _ => return None,
    })
}

async fn list_order_views(
    transport: &SharedTransport,
    shop: &ApiSession,
    now: i64,
    range: MetricRange,
) -> Result<Vec<OrderView>, CallFail> {
    let mut views = Vec::new();
    for (start, end) in order_windows(range, now) {
        let mut api_cursor = String::new();
        for page in 0..MAX_PAGES {
            let body = order_list(transport, shop, start, end, &api_cursor).await?;
            for id in id_list(&body, "order_id_list") {
                let detail = call(transport, shop, ORDER_GET, Vec::new(), Some(json!({"order_id": id})), None, false).await?;
                views.push(view_from_order(&detail)?);
            }
            let has_more = body.get("has_more").and_then(Value::as_bool).unwrap_or(false);
            let next = text(&body, "next_key").unwrap_or_default();
            if !has_more || next.is_empty() || next == api_cursor {
                if has_more {
                    return Err(CallFail::Failed("微信小店未返回订单下一页标记".into()));
                }
                break;
            }
            if page + 1 == MAX_PAGES {
                return Err(CallFail::Failed("订单分页未完成".into()));
            }
            api_cursor = next;
        }
    }
    Ok(views)
}

async fn order_list(
    transport: &SharedTransport,
    shop: &ApiSession,
    start: i64,
    end: i64,
    api_cursor: &str,
) -> Result<Value, CallFail> {
    let mut payload = json!({
        "create_time_range": {"start_time": start, "end_time": end},
        "page_size": ORDER_PAGE,
    });
    if !api_cursor.is_empty() {
        payload["next_key"] = json!(api_cursor);
    }
    call(transport, shop, ORDER_LIST, Vec::new(), Some(payload), None, false).await
}

fn view_from_order(body: &Value) -> Result<OrderView, CallFail> {
    let order = body.get("order").ok_or_else(|| CallFail::Failed("微信小店未返回订单".into()))?;
    let status = order
        .get("status")
        .and_then(as_i64)
        .ok_or_else(|| CallFail::Failed("微信小店未返回订单状态".into()))?;
    Ok(OrderView {
        status,
        amount_fen: order.pointer("/order_detail/price_info/order_price").and_then(as_i64),
        buyer: text(order, "openid"),
    })
}

fn map_remote_order(body: &Value) -> Result<CommerceOrder, CallFail> {
    let order = body.get("order").ok_or_else(|| CallFail::Failed("微信小店未返回订单".into()))?;
    let id = identifier(order, "order_id").ok_or_else(|| CallFail::Failed("微信小店未返回订单号".into()))?;
    let status = order
        .get("status")
        .and_then(as_i64)
        .ok_or_else(|| CallFail::Failed("微信小店未返回订单状态".into()))?;
    let created = order
        .get("create_time")
        .and_then(as_i64)
        .ok_or_else(|| CallFail::Failed("微信小店未返回下单时间".into()))?;
    let created_at = chrono::DateTime::from_timestamp(created, 0)
        .map(|time| time.to_rfc3339())
        .ok_or_else(|| CallFail::Failed("微信小店下单时间无效".into()))?;
    let amount_fen = order.pointer("/order_detail/price_info/order_price").and_then(as_i64);
    Ok(CommerceOrder {
        id,
        status: status.to_string(),
        amount: amount_fen.map(yuan),
        currency: amount_fen.map(|_| "CNY".into()),
        buyer: text(order, "openid"),
        created_at: Some(created_at),
        lines: order_lines(order),
    })
}

fn order_lines(order: &Value) -> Vec<CommerceOrderLine> {
    order
        .pointer("/order_detail/product_infos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let title = text(item, "title").or_else(|| identifier(item, "product_id"))?;
            let quantity = item.get("sku_cnt").and_then(as_i64)?;
            let sku = text(item, "sku_code").or_else(|| identifier(item, "sku_id"));
            Some(CommerceOrderLine { title, quantity, sku })
        })
        .collect()
}

async fn collect_refunds(transport: &SharedTransport, shop: &ApiSession, now: i64) -> Result<RefundFold, CallFail> {
    let ds = yesterday_ds(now).map_err(CallFail::Failed)?;
    let mut fold = RefundFold { amount_fen: Some(0), orders: Some(0) };
    let mut offset = 0i64;
    for _ in 0..MAX_COMPASS_PAGES {
        let body = call(
            transport,
            shop,
            COMPASS_PRODUCTS,
            Vec::new(),
            Some(json!({"ds": ds, "limit": COMPASS_LIMIT, "offset": offset})),
            None,
            false,
        )
        .await?;
        let list = body.get("product_list").and_then(Value::as_array).cloned().unwrap_or_default();
        for item in &list {
            absorb_refund(&mut fold, item.get("data").unwrap_or(&Value::Null));
        }
        let total = body.get("total_count").and_then(as_i64);
        offset += list.len() as i64;
        if list.is_empty() {
            if total.is_some_and(|total| offset < total) {
                return Err(CallFail::Failed("罗盘分页中断".into()));
            }
            return Ok(fold);
        }
        if total.is_none() && list.len() as i64 == COMPASS_LIMIT {
            return Err(CallFail::Failed("罗盘未返回 total_count，无法确认退款是否完整".into()));
        }
        if total.is_some_and(|total| offset >= total) || total.is_none() {
            return Ok(fold);
        }
    }
    Err(CallFail::Failed("罗盘分页未完成".into()))
}

fn absorb_refund(fold: &mut RefundFold, data: &Value) {
    add_metric(&mut fold.amount_fen, data, "pay_refund_gmv");
    add_metric(&mut fold.orders, data, "refund_cnt");
}

fn add_metric(slot: &mut Option<i64>, data: &Value, key: &str) {
    let Some(sum) = *slot else {
        return;
    };
    match data.get(key).and_then(as_i64) {
        Some(value) => *slot = sum.checked_add(value),
        None => *slot = None,
    }
}

async fn live_count(transport: &SharedTransport, shop: &ApiSession, now: i64) -> Result<f64, CallFail> {
    let _ = now;
    let body = call(
        transport,
        shop,
        PRODUCT_LIST,
        Vec::new(),
        Some(json!({"status": 5, "page_size": 1})),
        None,
        false,
    )
    .await?;
    body.get("total_num")
        .and_then(as_i64)
        .filter(|count| *count >= 0)
        .map(|count| count as f64)
        .ok_or_else(|| CallFail::Failed("微信小店未返回在售商品数".into()))
}

fn categories_from(body: &Value) -> Vec<Category> {
    let mut nodes: HashMap<String, CatNode> = HashMap::new();
    let mut order = Vec::new();
    for row in category_rows(body) {
        let Some(entries) = row.get("cat_and_qua").and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let Some(cat) = entry.get("cat") else {
                continue;
            };
            let Some(id) = identifier(cat, "cat_id") else {
                continue;
            };
            let name = text(cat, "name").unwrap_or_else(|| id.clone());
            let parent_id = identifier(cat, "f_cat_id").unwrap_or_else(|| "0".into());
            let explicit = cat.get("leaf").and_then(Value::as_bool);
            if let Some(node) = nodes.get_mut(&id) {
                if explicit.is_some() {
                    node.leaf = explicit;
                }
                if node.name.is_empty() {
                    node.name = name;
                }
            } else {
                order.push(id.clone());
                nodes.insert(id, CatNode { name, parent_id, leaf: explicit });
            }
        }
    }
    let parents: HashSet<String> = nodes.values().map(|node| node.parent_id.clone()).collect();
    order
        .into_iter()
        .filter_map(|id| {
            let node = nodes.remove(&id)?;
            let leaf = node.leaf.unwrap_or(!parents.contains(&id));
            Some(Category { id, name: node.name, parent_id: node.parent_id, leaf })
        })
        .collect()
}

fn category_rows(body: &Value) -> &[Value] {
    if let Some(rows) = body.get("cats_v2").and_then(Value::as_array) {
        if !rows.is_empty() {
            return rows;
        }
    }
    body.get("cats").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn category_chain(categories: &[Category], leaf_id: &str) -> Result<Vec<i64>, CallFail> {
    let by_id: HashMap<&str, &Category> = categories.iter().map(|category| (category.id.as_str(), category)).collect();
    let leaf = by_id
        .get(leaf_id)
        .copied()
        .ok_or_else(|| CallFail::Rejected(format!("类目不存在：{leaf_id}")))?;
    if !leaf.leaf {
        return Err(CallFail::Rejected("请选择叶子类目".into()));
    }
    let mut current = leaf_id.to_string();
    let mut ids = Vec::new();
    for _ in 0..16 {
        let category = by_id
            .get(current.as_str())
            .copied()
            .ok_or_else(|| CallFail::Rejected("类目父级缺失".into()))?;
        let id = category
            .id
            .parse::<i64>()
            .map_err(|_| CallFail::Rejected("类目 ID 不是数字".into()))?;
        ids.push(id);
        if category.parent_id == "0" || category.parent_id.is_empty() {
            ids.reverse();
            return Ok(ids);
        }
        current = category.parent_id.clone();
    }
    Err(CallFail::Rejected("类目层级异常".into()))
}

fn attribute_list(body: &Value) -> Vec<CategoryAttribute> {
    let mut all = map_attr_list(body.get("product_attr_list"));
    for attr in map_attr_list(body.get("sale_attr_list")) {
        if let Some(existing) = all.iter_mut().find(|item| item.id == attr.id) {
            existing.required |= attr.required;
            if existing.options.is_empty() {
                existing.options = attr.options;
            }
        } else {
            all.push(attr);
        }
    }
    all
}

fn map_attr_list(value: Option<&Value>) -> Vec<CategoryAttribute> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(map_attr).collect()
}

fn map_attr(node: &Value) -> Option<CategoryAttribute> {
    let name = text(node, "name")?;
    let kind = text(node, "type_v2").or_else(|| text(node, "type")).unwrap_or_else(|| "string".into());
    let input = match kind.as_str() {
        "select_one" => AttributeInput::Select,
        "select_many" => AttributeInput::MultiSelect,
        "integer" | "decimal4" | "integer_unit" | "decimal4_unit" => AttributeInput::Number,
        _ => AttributeInput::Text,
    };
    let options = if matches!(input, AttributeInput::Select | AttributeInput::MultiSelect) || kind.ends_with("_unit") {
        split_options(&text(node, "value").unwrap_or_default())
    } else {
        Vec::new()
    };
    let unit = if kind.ends_with("_unit") && options.len() == 1 {
        Some(options[0].name.clone())
    } else {
        None
    };
    Some(CategoryAttribute { id: name.clone(), name, required: attr_required(node), input, options, unit })
}

fn attr_required(node: &Value) -> bool {
    if let Some(rule) = node.pointer("/required_rule/rule_type").and_then(as_i64) {
        return rule == 1;
    }
    node.get("is_required").and_then(Value::as_bool).unwrap_or(false)
}

fn split_options(value: &str) -> Vec<AttributeOption> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| AttributeOption { id: item.to_string(), name: item.to_string() })
        .collect()
}

fn map_remote_product(body: &Value) -> Result<CommerceProduct, CallFail> {
    let node = display_node(body).ok_or_else(|| CallFail::Failed("微信小店未返回商品".into()))?;
    let id = identifier(node, "product_id").ok_or_else(|| CallFail::Failed("微信小店未返回商品 ID".into()))?;
    let (online, edit, _) = read_state(body);
    if online.is_none() && edit.is_none() {
        return Err(CallFail::Failed("微信小店未返回商品状态".into()));
    }
    let price = price_of(node);
    Ok(CommerceProduct {
        id,
        title: text(node, "title").unwrap_or_default(),
        status: platform_status(online, edit),
        price,
        currency: price.map(|_| "CNY".into()),
        stock: stock_of(node),
        sku: sku_code_of(node),
    })
}

fn platform_status(online: Option<i64>, edit: Option<i64>) -> String {
    match (online, edit) {
        (Some(online), Some(edit)) => format!("{online}/{edit}"),
        (Some(online), None) => online.to_string(),
        (None, Some(edit)) => format!("edit:{edit}"),
        (None, None) => String::new(),
    }
}

fn sku_code_of(node: &Value) -> Option<String> {
    let skus = node.get("skus").and_then(Value::as_array)?;
    skus.iter().find_map(|sku| text(sku, "sku_code").or_else(|| text(sku, "out_sku_id")))
}

fn display_node(body: &Value) -> Option<&Value> {
    body.get("product")
        .filter(|value| value.is_object())
        .or_else(|| body.get("edit_product").filter(|value| value.is_object()))
}

fn read_state(body: &Value) -> (Option<i64>, Option<i64>, Option<String>) {
    let product = body.get("product");
    let edit_product = body.get("edit_product");
    let online = product.and_then(|item| item.get("status")).and_then(as_i64);
    let edit = edit_product
        .and_then(|item| item.get("edit_status"))
        .and_then(as_i64)
        .or_else(|| product.and_then(|item| item.get("edit_status")).and_then(as_i64));
    let audit = body.pointer("/info_score/sub_score_list").and_then(Value::as_array).and_then(|items| {
        items.iter().find_map(|item| text(item, "audit_remark"))
    });
    (online, edit, audit)
}

fn map_product_status(
    online: Option<i64>,
    edit: Option<i64>,
    audit: Option<&str>,
) -> Option<(ListingStatus, Option<String>)> {
    if online.is_none() && edit.is_none() {
        return None;
    }
    if matches!(online, Some(20 | 13 | 14 | 15)) {
        let reason = match online {
            Some(20) => "封禁下架",
            Some(13) => "违规下架",
            Some(14) => "保证金违规下架",
            _ => "品牌到期下架",
        };
        return Some((ListingStatus::Banned, Some(reason.into())));
    }
    match edit {
        Some(2) => return Some((ListingStatus::Reviewing, None)),
        Some(7) => return Some((ListingStatus::Submitting, Some("商品异步提交上传中".into()))),
        Some(70) => return Some((ListingStatus::Submitting, Some("商品异步提审中".into()))),
        Some(3) => {
            let reason = audit.filter(|value| !value.is_empty()).map(str::to_owned).unwrap_or_else(|| "审核失败".into());
            return Some((ListingStatus::Rejected, Some(reason)));
        }
        Some(8) => return Some((ListingStatus::Failed, Some("商品异步提交上传失败".into()))),
        Some(72) => return Some((ListingStatus::Failed, Some("商品异步提审失败，当日额度不足".into()))),
        Some(73) => return Some((ListingStatus::Failed, Some("商品异步提审失败，已触发限频".into()))),
        Some(1) if !matches!(online, Some(5 | 11 | 12)) => {
            return Some((ListingStatus::Failed, Some("编辑中".into())));
        }
        _ => {}
    }
    match online {
        Some(5) => Some((ListingStatus::Live, None)),
        Some(11) => Some((ListingStatus::Live, Some("自主下架".into()))),
        Some(12) => Some((ListingStatus::Live, Some("售罄下架".into()))),
        Some(6) => Some((ListingStatus::Rejected, Some("回收站".into()))),
        Some(0) => Some((ListingStatus::Reviewing, Some("初始值".into()))),
        Some(other) => Some((ListingStatus::Uncertain, Some(format!("未识别的商品状态 {other}")))),
        None if matches!(edit, Some(4)) => Some((ListingStatus::Reviewing, Some("审核成功但未返回线上状态".into()))),
        None if matches!(edit, Some(0)) => Some((ListingStatus::Reviewing, Some("初始值".into()))),
        None => None,
    }
}

fn price_of(node: &Value) -> Option<f64> {
    if let Some(fen) = node.get("min_price").and_then(as_i64) {
        return Some(yuan(fen));
    }
    node.get("skus")
        .and_then(Value::as_array)
        .and_then(|skus| skus.iter().filter_map(|sku| sku.get("sale_price").and_then(as_i64)).min())
        .map(yuan)
}

fn stock_of(node: &Value) -> Option<i64> {
    let skus = node.get("skus").and_then(Value::as_array)?;
    if skus.is_empty() {
        return None;
    }
    let mut sum = 0i64;
    for sku in skus {
        sum = sum.checked_add(sku.get("stock_num").and_then(as_i64)?)?;
    }
    Some(sum)
}

async fn load_categories(transport: &SharedTransport, shop: &ApiSession) -> Result<Vec<Category>, CallFail> {
    let body = call(transport, shop, CATEGORY_ALL, Vec::new(), Some(json!({})), None, false).await?;
    Ok(categories_from(&body))
}

async fn category_facts(transport: &SharedTransport, shop: &ApiSession, category_id: i64) -> Result<(i64, bool), CallFail> {
    let body = call(transport, shop, CATEGORY_DETAIL, Vec::new(), Some(json!({"cat_id": category_id})), None, false).await?;
    let attr = body.get("attr").ok_or_else(|| CallFail::Rejected("类目未返回属性".into()))?;
    let seven = attr
        .get("seven_day_return")
        .and_then(Value::as_bool)
        .ok_or_else(|| CallFail::Rejected("类目未返回七天无理由规则".into()))?;
    let limit_brand = attr.get("is_limit_brand").and_then(Value::as_bool).unwrap_or(false);
    Ok((if seven { 1 } else { 0 }, limit_brand))
}

async fn category_rules(
    transport: &SharedTransport,
    shop: &ApiSession,
    category_id: i64,
) -> Result<(Vec<CategoryAttribute>, Vec<CategoryAttribute>, Option<i64>), CallFail> {
    let body = call(
        transport,
        shop,
        CATEGORY_RULE,
        Vec::new(),
        Some(json!({"cat_id": category_id, "release_mode": 0})),
        None,
        false,
    )
    .await?;
    let floor = body.get("floor_price").and_then(as_i64);
    Ok((map_attr_list(body.get("product_attr_list")), map_attr_list(body.get("sale_attr_list")), floor))
}

async fn freight_template(transport: &SharedTransport, shop: &ApiSession) -> Result<String, CallFail> {
    let body = call(
        transport,
        shop,
        FREIGHT_LIST,
        Vec::new(),
        Some(json!({"offset": 0, "limit": 10})),
        None,
        false,
    )
    .await?;
    body.get("template_id_list")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find_map(identifier_value))
        .ok_or_else(|| CallFail::Rejected("店铺没有运费模板，无法快递发货".into()))
}

async fn upload_image(transport: &SharedTransport, shop: &ApiSession, name: &str, bytes: &[u8]) -> Result<String, CallFail> {
    let (width, height) = image_size(bytes).map_err(CallFail::Rejected)?;
    let file_name = if name.trim().is_empty() { "image.png".into() } else { name.trim().to_string() };
    let body = call(
        transport,
        shop,
        IMG_UPLOAD,
        vec![
            ("upload_type".into(), "0".into()),
            ("resp_type".into(), "1".into()),
            ("height".into(), height.to_string()),
            ("width".into(), width.to_string()),
        ],
        None,
        Some((file_name, bytes.to_vec())),
        false,
    )
    .await?;
    text(&body["pic_file"], "img_url").ok_or_else(|| CallFail::Rejected("微信小店未返回图片链接".into()))
}

fn missing_attrs(rules: &[CategoryAttribute], provided: &[(String, String)]) -> Option<String> {
    let missing: Vec<&str> = rules
        .iter()
        .filter(|attr| attr.required && !provided.iter().any(|(id, value)| id == &attr.id && !value.is_empty()))
        .map(|attr| attr.name.as_str())
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(format!("缺少必填商品参数：{}", missing.join("、")))
    }
}

fn apply_sale_attrs(skus: &mut [SkuDraft], sale: &[CategoryAttribute]) -> Result<(), CallFail> {
    let required: Vec<&CategoryAttribute> = sale.iter().filter(|attr| attr.required).collect();
    if required.len() > 1 {
        let names = required.iter().map(|attr| attr.name.as_str()).collect::<Vec<_>>().join("、");
        return Err(CallFail::Rejected(format!("类目要求多个销售属性（{names}），当前每个 SKU 只有一个名称")));
    }
    for sku in skus.iter_mut() {
        if let Some(attr) = required.first() {
            if sku.name.trim().is_empty() {
                return Err(CallFail::Rejected(format!("SKU 缺少销售属性 {}", attr.name)));
            }
            if sku.name.chars().count() > 40 {
                return Err(CallFail::Rejected(format!("SKU 规格超过 40 个字符：{}", sku.name)));
            }
            sku.attrs = vec![(attr.name.clone(), sku.name.clone())];
        } else if !sku.name.trim().is_empty() {
            sku.attrs = vec![("规格".into(), sku.name.clone())];
        }
    }
    Ok(())
}

fn product_payload(
    prepared: &Prepared,
    chain: &[i64],
    template_id: &str,
    image_urls: &[String],
    seven_day: i64,
    remote_id: Option<&str>,
) -> Value {
    let skus: Vec<Value> = prepared
        .skus
        .iter()
        .map(|sku| {
            let mut body = json!({
                "sale_price": sku.fen,
                "stock_num": sku.stock,
                "sku_deliver_info": {"stock_type": 0},
            });
            if let Some(url) = image_urls.first() {
                body["thumb_img"] = json!(url);
            }
            if let Some(code) = &sku.code {
                body["sku_code"] = json!(code);
                body["out_sku_id"] = json!(code);
            }
            if !sku.attrs.is_empty() {
                body["sku_attrs"] = json!(sku.attrs.iter().map(|(key, value)| json!({"attr_key": key, "attr_value": value})).collect::<Vec<_>>());
            }
            body
        })
        .collect();
    let mut desc_info = json!({"imgs": image_urls});
    if let Some(desc) = &prepared.description {
        desc_info["desc"] = json!(desc);
    }
    let mut express = json!({"template_id": template_id});
    if let Some(grams) = prepared.weight_grams.filter(|grams| *grams > 0) {
        express["weight"] = json!(grams);
    }
    let mut body = json!({
        "title": prepared.title,
        "head_imgs": image_urls,
        "desc_info": desc_info,
        "cats": [],
        "cats_v2": chain.iter().map(|id| json!({"cat_id": id})).collect::<Vec<_>>(),
        "deliver_method": 0,
        "extra_service": {"seven_day_return": seven_day, "freight_insurance": 0},
        "skus": skus,
        "listing": 1,
        "brand_id": prepared.brand,
        "express_info": express,
    });
    if !prepared.attrs.is_empty() {
        body["attrs"] = json!(prepared.attrs.iter().map(|(key, value)| json!({"attr_key": key, "attr_value": value})).collect::<Vec<_>>());
    }
    if let Some(remote_id) = remote_id {
        body["product_id"] = json!(remote_id);
    }
    body
}

fn prepare(draft: &ListingDraft, target: &ListingTarget, images: &[(String, Vec<u8>)]) -> Result<Prepared, String> {
    let title = prepare_title(&draft.title)?;
    currency_ok(draft.currency.as_deref())?;
    let category_id = parse_category_id(&target.category_id).map_err(|error| error.message().to_string())?;
    let skus = prepare_skus(draft)?;
    let ready_urls = prepare_images(draft, images)?;
    Ok(Prepared {
        title,
        description: draft.description.as_ref().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        category_id,
        skus,
        ready_urls,
        attrs: prepare_attrs(&target.attributes),
        brand: brand_id(draft.brand.as_deref())?,
        weight_grams: prepare_weight(draft.weight_kg)?,
    })
}

fn prepare_title(title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("商品标题不能为空".into());
    }
    if title.chars().count() > 60 {
        return Err("商品标题超过 60 个字符".into());
    }
    if title.chars().all(|ch| ch.is_ascii()) {
        return Err("商品标题不能仅为数字或英文".into());
    }
    Ok(title.to_string())
}

fn prepare_skus(draft: &ListingDraft) -> Result<Vec<SkuDraft>, String> {
    let source: Vec<ListingSku> = if draft.skus.is_empty() {
        vec![ListingSku { name: "默认".into(), price: draft.price, stock: draft.stock, code: None }]
    } else {
        draft.skus.clone()
    };
    if source.is_empty() || source.len() > 500 {
        return Err("SKU 数量必须在 1 到 500 之间".into());
    }
    let mut skus = Vec::new();
    for (index, sku) in source.iter().enumerate() {
        let name = if sku.name.trim().is_empty() {
            if source.len() == 1 {
                "默认".into()
            } else {
                return Err(format!("第 {} 个 SKU 缺少规格名", index + 1));
            }
        } else {
            sku.name.trim().to_string()
        };
        if name.chars().count() > 40 {
            return Err(format!("SKU 规格超过 40 个字符：{name}"));
        }
        let price = sku.price.or(draft.price).ok_or_else(|| format!("SKU {name} 缺少售价"))?;
        let stock = sku.stock.or(draft.stock).ok_or_else(|| format!("SKU {name} 缺少库存"))?;
        if stock < 0 {
            return Err(format!("SKU {name} 库存不能为负"));
        }
        let code = sku.code.as_ref().map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        if code.as_ref().is_some_and(|value| value.chars().count() > 100) {
            return Err("SKU 编码超过 100 个字符".into());
        }
        skus.push(SkuDraft { name, fen: yuan_to_fen(price)?, stock, code, attrs: Vec::new() });
    }
    Ok(skus)
}

fn prepare_images(draft: &ListingDraft, images: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
    for (_, bytes) in images {
        if bytes.is_empty() {
            return Err("商品图片为空".into());
        }
        image_size(bytes)?;
    }
    let mut ready = Vec::new();
    for image in &draft.images {
        let image = image.trim();
        if is_store_image(image) {
            ready.push(image.to_string());
        }
    }
    let total = images.len() + ready.len();
    if !(3..=9).contains(&total) {
        return Err("微信小店商品头图需要 3 到 9 张，且必须先上传或使用 mmecimage.cn/p/ 链接".into());
    }
    if has_duplicate(&ready) {
        return Err("商品头图不能重复".into());
    }
    Ok(ready)
}

fn prepare_attrs(attributes: &[ListingAttribute]) -> Vec<(String, String)> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let key = attribute.id.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), attribute_value(attribute)?))
        })
        .collect()
}

fn attribute_value(attribute: &ListingAttribute) -> Option<String> {
    let values = if attribute.values.is_empty() {
        vec![attribute.value.clone()]
    } else {
        attribute.values.clone()
    };
    let joined = values
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join(";");
    (!joined.is_empty()).then_some(joined)
}

fn currency_ok(currency: Option<&str>) -> Result<(), String> {
    match currency.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(()),
        Some(value) if value.eq_ignore_ascii_case("CNY") || value.eq_ignore_ascii_case("RMB") || value == "人民币" => {
            Ok(())
        }
        Some(_) => Err("微信小店只支持人民币".into()),
    }
}

fn brand_id(brand: Option<&str>) -> Result<String, String> {
    match brand.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(NO_BRAND.into()),
        Some(value) if value.chars().all(|ch| ch.is_ascii_digit()) => Ok(value.to_string()),
        Some(_) => Err("品牌需填写微信小店品牌 ID，无品牌请留空".into()),
    }
}

fn prepare_weight(weight_kg: Option<f64>) -> Result<Option<i64>, String> {
    let Some(kg) = weight_kg else {
        return Ok(None);
    };
    if !kg.is_finite() || kg < 0.0 {
        return Err("商品重量无效".into());
    }
    if kg == 0.0 {
        return Ok(None);
    }
    let grams = (kg * 1000.0).round();
    if grams > i64::MAX as f64 {
        return Err("商品重量无效".into());
    }
    Ok(Some(grams as i64))
}

fn yuan_to_fen(yuan: f64) -> Result<i64, String> {
    if !yuan.is_finite() || yuan <= 0.0 {
        return Err("售价必须大于 0".into());
    }
    let fen = (yuan * 100.0).round();
    if fen < 1.0 || fen > 1_000_000_000.0 {
        return Err("售价超出微信小店允许范围".into());
    }
    Ok(fen as i64)
}

fn is_store_image(url: &str) -> bool {
    url.starts_with("https://mmecimage.cn/p/") || url.starts_with("http://mmecimage.cn/p/")
}

fn has_duplicate(urls: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    urls.iter().any(|url| !seen.insert(url))
}

fn image_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    png_size(bytes)
        .or_else(|| jpeg_size(bytes))
        .or_else(|| webp_size(bytes))
        .or_else(|| bmp_size(bytes))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .ok_or_else(|| "微信小店图片仅支持 bmp、jpg、png、webp，且需要能读取宽高".into())
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || &bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut index = 2usize;
    while index + 3 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        while index < bytes.len() && bytes[index] == 0xFF {
            index += 1;
        }
        if index >= bytes.len() {
            return None;
        }
        let marker = bytes[index];
        index += 1;
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        if index + 1 >= bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes([bytes[index], bytes[index + 1]]) as usize;
        if len < 2 || index + len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF
        ) && len >= 7
        {
            let height = u16::from_be_bytes([bytes[index + 3], bytes[index + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as u32;
            return Some((width, height));
        }
        if marker == 0xDA {
            return None;
        }
        index += len;
    }
    None
}

fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    match &bytes[12..16] {
        b"VP8X" => {
            let width = 1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]);
            let height = 1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]);
            Some((width, height))
        }
        b"VP8L" if bytes.len() >= 25 && bytes[20] == 0x2F => {
            let b1 = bytes[21] as u32;
            let b2 = bytes[22] as u32;
            let b3 = bytes[23] as u32;
            let b4 = bytes[24] as u32;
            let width = 1 + (b1 | ((b2 & 0x3F) << 8));
            let height = 1 + ((b2 >> 6) | (b3 << 2) | ((b4 & 0x0F) << 10));
            Some((width, height))
        }
        b"VP8 " if bytes.len() >= 30 && bytes[23] == 0x9D && bytes[24] == 0x01 && bytes[25] == 0x2A => {
            let width = u16::from_le_bytes([bytes[26], bytes[27]]) as u32 & 0x3FFF;
            let height = u16::from_le_bytes([bytes[28], bytes[29]]) as u32 & 0x3FFF;
            Some((width, height))
        }
        _ => None,
    }
}

fn bmp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 26 || &bytes[0..2] != b"BM" {
        return None;
    }
    let width = i32::from_le_bytes(bytes[18..22].try_into().ok()?);
    let height = i32::from_le_bytes(bytes[22..26].try_into().ok()?);
    if width <= 0 || height == 0 {
        return None;
    }
    Some((width as u32, height.unsigned_abs()))
}

fn order_windows(range: MetricRange, now: i64) -> Vec<(i64, i64)> {
    let (start, end) = metric_window(range, now, CHINA_OFFSET);
    chunks(start, end, ORDER_SPAN)
}

fn chunks(start: i64, end: i64, span: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    if end < start {
        return out;
    }
    let mut cursor = start;
    while cursor <= end {
        let chunk_end = cursor.saturating_add(span).min(end);
        out.push((cursor, chunk_end));
        if chunk_end >= end {
            break;
        }
        cursor = chunk_end + 1;
    }
    out
}

fn parse_order_cursor(cursor: Option<&str>, windows: usize) -> Result<(usize, String), String> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((0, String::new()));
    };
    let (index, api) = cursor.split_once(':').unwrap_or((cursor, ""));
    let index = index.parse::<usize>().map_err(|_| "订单分页游标无效".to_string())?;
    if index >= windows {
        return Err("订单分页游标无效".into());
    }
    Ok((index, api.to_string()))
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

fn compass_dates(range: MetricRange, now: i64) -> Result<(Vec<String>, bool), String> {
    let (start, end) = metric_window(range, now, CHINA_OFFSET);
    let today = china_midnight(now);
    let mut day = china_midnight(start);
    let end_day = china_midnight(end);
    let mut dates = Vec::new();
    let mut includes_today = false;
    while day <= end_day {
        if day == today {
            includes_today = true;
        } else {
            dates.push(format_ds(day).ok_or_else(|| "无法格式化指标日期".to_string())?);
        }
        day += 86_400;
    }
    Ok((dates, includes_today))
}

fn yesterday_ds(now: i64) -> Result<String, String> {
    let (dates, includes_today) = compass_dates(MetricRange::Yesterday, now)?;
    if includes_today || dates.len() != 1 {
        return Err("无法确定昨天的罗盘日期".into());
    }
    dates.into_iter().next().ok_or_else(|| "无法确定昨天的罗盘日期".into())
}

fn china_midnight(unix: i64) -> i64 {
    let local = unix + CHINA_OFFSET;
    local - local.rem_euclid(86_400)
}

fn format_ds(china_midnight: i64) -> Option<String> {
    let text = chrono::DateTime::from_timestamp(china_midnight, 0)?.format("%Y%m%d").to_string();
    (text.len() == 8).then_some(text)
}

fn parse_category_id(category_id: &str) -> Result<i64, CommerceError> {
    let id = category_id.trim();
    let parsed = id.parse::<i64>().map_err(|_| CommerceError::invalid("类目 ID 必须是数字"))?;
    if parsed <= 0 {
        return Err(CommerceError::invalid("类目 ID 必须是数字"));
    }
    Ok(parsed)
}

fn ensure(shop: &ApiSession) -> Result<(), CommerceError> {
    if shop.platform != Platform::Wechat {
        return Err(CommerceError::unsupported("该平台尚未接入"));
    }
    if shop.access_token.trim().is_empty() {
        return Err(CommerceError::rejected("店铺访问令牌为空，请重新绑定微信小店"));
    }
    Ok(())
}

fn yuan(fen: i64) -> f64 {
    fen as f64 / 100.0
}

fn id_list(body: &Value, key: &str) -> Vec<String> {
    body.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(identifier_value).collect()
}

#[allow(clippy::too_many_arguments)]
async fn call(
    transport: &SharedTransport,
    shop: &ApiSession,
    path: &'static str,
    extra_query: Vec<(String, String)>,
    json_body: Option<Value>,
    file: Option<(String, Vec<u8>)>,
    mutating: bool,
) -> Result<Value, CallFail> {
    let mut query = vec![("access_token".into(), shop.access_token.clone())];
    query.extend(extra_query);
    let body = match (file, json_body) {
        (Some((file_name, bytes)), _) => HttpBody::Multipart(MultipartFile { field: "media".into(), file_name, bytes }),
        (None, Some(json_body)) => HttpBody::Json(json_body),
        (None, None) => HttpBody::Empty,
    };
    let request = HttpRequest {
        method: "POST",
        url: format!("{HOST}{path}"),
        headers: Vec::new(),
        query,
        body,
        mutating,
    };
    let response = match transport.send(request).await {
        Ok(response) => response,
        Err(fault) => return Err(fault_to_call(mutating, fault)),
    };
    classify(mutating, response.status, response.body)
}

fn fault_to_call(mutating: bool, fault: super::adapter::TransportFault) -> CallFail {
    match adapter::classify_fault(mutating, fault) {
        CommerceError::Uncertain(message) => CallFail::Uncertain(message),
        CommerceError::Failed(message) => CallFail::Failed(message),
        other => CallFail::Failed(other.message().to_string()),
    }
}

fn classify(mutating: bool, status: u16, body: Value) -> Result<Value, CallFail> {
    if let Some(code) = errcode(&body) {
        if code == 0 && status < 500 {
            return Ok(body);
        }
        let message = if code == 0 {
            format!("HTTP {status}")
        } else {
            format!("微信小店：{}（{code}）", errmsg(&body))
        };
        if code != 0 && is_auth(code, &message) {
            return Err(CallFail::Auth(message));
        }
        if status >= 500 {
            return Err(if mutating { CallFail::Uncertain(message) } else { CallFail::Failed(message) });
        }
        if code != 0 {
            return Err(CallFail::Rejected(message));
        }
    }
    let message = if status >= 400 {
        format!("HTTP {status}")
    } else {
        format!("微信小店未返回 errcode（HTTP {status}）")
    };
    if status >= 500 {
        return Err(if mutating { CallFail::Uncertain(message) } else { CallFail::Failed(message) });
    }
    if status >= 400 {
        return Err(CallFail::Rejected(message));
    }
    Err(if mutating { CallFail::Uncertain(message) } else { CallFail::Failed(message) })
}

fn is_auth(code: i64, message: &str) -> bool {
    matches!(code, 40001 | 40014 | 41001 | 42001)
        || message.to_ascii_lowercase().contains("access_token")
        || message.to_ascii_lowercase().contains("credential")
}

fn errcode(body: &Value) -> Option<i64> {
    body.get("errcode").and_then(as_i64)
}

fn errmsg(body: &Value) -> String {
    text(body, "errmsg").unwrap_or_else(|| "平台接口返回失败".into())
}

fn to_error(error: CallFail) -> CommerceError {
    let message = error_text(&error);
    if message.contains("（10020052）") || message.contains("（100002）") {
        return CommerceError::not_found(message);
    }
    fail_to_error(error)
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

fn error_text(error: &CallFail) -> String {
    match error {
        CallFail::Auth(message) | CallFail::Rejected(message) | CallFail::Failed(message) | CallFail::Uncertain(message) => {
            message.clone()
        }
    }
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
#[path = "wechat_tests.rs"]
mod wechat_tests;
