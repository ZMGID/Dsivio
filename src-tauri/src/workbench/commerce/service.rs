//! Listing lifecycle: one record per shop in a group, uncertain is query-only.
use super::adapter::{self, SharedTransport};
use super::shopee::{PushOutcome, ResolvedShop, Transport};
use super::store;
use super::types::{
    capabilities_for, title_key, Category, CategoryAttribute, CommerceError, ListingDraft,
    ListingFilter, ListingRecord, ListingStatus, ListingTarget, MetricRange, ShopMetrics,
};
use crate::workbench::shops::{self, Platform};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct Runtime {
    pub db: PathBuf,
    pub transport: Transport,
    pub http: SharedTransport,
    pub shops: ShopSource,
    pub now: i64,
}

pub enum ShopSource {
    Live,
    Fixed(HashMap<String, ResolvedShop>),
}

impl Runtime {
    pub fn production() -> Result<Self, CommerceError> {
        Ok(Self {
            db: store::default_path()?,
            transport: Transport::live()?,
            http: SharedTransport::live()?,
            shops: ShopSource::Live,
            now: chrono::Utc::now().timestamp(),
        })
    }
}

impl ShopSource {
    pub(crate) async fn resolve(&self, id: &str) -> Result<ResolvedShop, CommerceError> {
        match self {
            Self::Fixed(shops) => shops
                .get(id)
                .cloned()
                .ok_or_else(|| CommerceError::not_found(format!("店铺不存在：{id}"))),
            Self::Live => {
                let session = shops::open_shop_session(id).await.map_err(CommerceError::failed)?;
                let (partner_id, partner_key) = if session.shop.platform == Platform::Shein {
                    let secret = if session.seller_secret.is_empty() {
                        session.partner_key.clone()
                    } else {
                        session.seller_secret.clone()
                    };
                    (session.open_key, secret)
                } else {
                    (session.partner_id, session.partner_key)
                };
                Ok(ResolvedShop {
                    platform: session.shop.platform,
                    partner_id,
                    partner_key,
                    access_token: session.access_token,
                    remote_id: session.shop.remote_id,
                    region: session.shop.region,
                    name: session.shop.name,
                })
            }
        }
    }
}

fn gate() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub async fn execute(op: &Value) -> Result<Value, CommerceError> {
    let runtime = Runtime::production()?;
    dispatch(&runtime, op).await
}

pub async fn dispatch(runtime: &Runtime, op: &Value) -> Result<Value, CommerceError> {
    let action = op.get("action").and_then(Value::as_str).unwrap_or("");
    match action {
        "shops" => {
            let shops = shops::shop_list().await.map_err(CommerceError::failed)?;
            Ok(json!(shops))
        }
        "capabilities" => {
            let shop_id = required_str(op, "shopId")?;
            let platform = shop_platform(&shop_id).await?;
            Ok(json!(capabilities_for(platform)))
        }
        "metrics" => {
            let shop_id = required_str(op, "shopId")?;
            let range = metric_range(op)?;
            Ok(json!(metrics(runtime, &shop_id, range).await?))
        }
        "categories" => {
            let shop_id = required_str(op, "shopId")?;
            let parent = op.get("parentId").and_then(Value::as_str);
            Ok(json!(categories(runtime, &shop_id, parent).await?))
        }
        "attributes" => {
            let shop_id = required_str(op, "shopId")?;
            let category_id = required_str(op, "categoryId")?;
            Ok(json!(attributes(runtime, &shop_id, &category_id).await?))
        }
        "products" => {
            let shop_id = required_str(op, "shopId")?;
            let cursor = op.get("cursor").and_then(Value::as_str).filter(|value| !value.is_empty());
            Ok(json!(adapter::products(runtime, &shop_id, cursor).await?))
        }
        "orders" => {
            let shop_id = required_str(op, "shopId")?;
            let range = metric_range(op)?;
            let cursor = op.get("cursor").and_then(Value::as_str).filter(|value| !value.is_empty());
            Ok(json!(adapter::orders(runtime, &shop_id, range, cursor).await?))
        }
        "listings" => {
            let filter = match op.get("filter") {
                Some(value) => serde_json::from_value(value.clone())
                    .map_err(|error| CommerceError::invalid(format!("筛选条件无效：{error}")))?,
                None => ListingFilter::default(),
            };
            Ok(json!(list_records(runtime, &filter)?))
        }
        "submit" => {
            let draft = parse_draft(op.get("draft"))?;
            let targets: Vec<ListingTarget> = serde_json::from_value(op.get("targets").cloned().unwrap_or(Value::Null))
                .map_err(|error| CommerceError::invalid(format!("上架目标无效：{error}")))?;
            let group_id = op.get("groupId").and_then(Value::as_str).map(str::to_owned);
            Ok(json!(submit(runtime, draft, targets, group_id).await?))
        }
        "resubmit" => {
            let id = required_str(op, "id")?;
            let draft = match op.get("draft") {
                None | Some(Value::Null) => None,
                Some(value) => Some(parse_draft(Some(value))?),
            };
            let target = match op.get("target") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    serde_json::from_value(value.clone())
                        .map_err(|error| CommerceError::invalid(format!("上架目标无效：{error}")))?,
                ),
            };
            Ok(json!(resubmit(runtime, &id, draft, target).await?))
        }
        "refresh" => {
            let id = required_str(op, "id")?;
            Ok(json!(refresh(runtime, &id).await?))
        }
        "status" => {
            let id = required_str(op, "id")?;
            Ok(json!(record(runtime, &id)?))
        }
        "" => Err(CommerceError::invalid("缺少 action")),
        other => Err(CommerceError::invalid(format!("未知操作 {other}"))),
    }
}

pub async fn metrics(runtime: &Runtime, shop_id: &str, range: MetricRange) -> Result<ShopMetrics, CommerceError> {
    adapter::metrics(runtime, shop_id, range).await
}

pub async fn categories(
    runtime: &Runtime,
    shop_id: &str,
    parent_id: Option<&str>,
) -> Result<Vec<Category>, CommerceError> {
    adapter::categories(runtime, shop_id, parent_id).await
}

pub async fn attributes(
    runtime: &Runtime,
    shop_id: &str,
    category_id: &str,
) -> Result<Vec<CategoryAttribute>, CommerceError> {
    adapter::attributes(runtime, shop_id, category_id).await
}

pub fn list_records(runtime: &Runtime, filter: &ListingFilter) -> Result<Vec<ListingRecord>, CommerceError> {
    store::with_store(&runtime.db, |store| store.list(filter))
}

pub fn record(runtime: &Runtime, id: &str) -> Result<ListingRecord, CommerceError> {
    store::with_store(&runtime.db, |store| store.get(id))
}

pub async fn submit(
    runtime: &Runtime,
    draft: ListingDraft,
    targets: Vec<ListingTarget>,
    group_id: Option<String>,
) -> Result<Vec<ListingRecord>, CommerceError> {
    let _guard = gate().lock().await;
    validate_draft(&draft)?;
    validate_targets(&targets)?;
    let mut resolved = Vec::new();
    for target in &targets {
        let shop = runtime.shops.resolve(&target.shop_id).await?;
        adapter::require_listing(shop.platform)?;
        resolved.push(shop);
    }
    let group_id = group_id.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = store::with_store(&runtime.db, |store| store.list(&ListingFilter::default()))?;
    for target in &targets {
        reject_duplicate(&existing, &group_id, &target.shop_id, &draft.title)?;
    }
    let images = read_images(&draft.images)?;
    let stamp = stamp(runtime.now);
    let mut created = Vec::new();
    for target in &targets {
        let record = ListingRecord {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: group_id.clone(),
            shop_id: target.shop_id.clone(),
            platform: Platform::Shopee,
            status: ListingStatus::Submitting,
            reason: None,
            remote_id: None,
            title: draft.title.clone(),
            currency: draft.currency.clone(),
            draft: draft.clone(),
            target: target.clone(),
            attempts: 1,
            created_at: stamp.clone(),
            updated_at: stamp.clone(),
        };
        store::with_store(&runtime.db, |store| store.insert(&record))?;
        created.push(record);
    }
    let mut finished = Vec::new();
    for (record, shop) in created.into_iter().zip(resolved) {
        finished.push(run_push(runtime, record, &shop, &images, None).await?);
    }
    Ok(finished)
}

pub async fn resubmit(
    runtime: &Runtime,
    id: &str,
    draft: Option<ListingDraft>,
    target: Option<ListingTarget>,
) -> Result<ListingRecord, CommerceError> {
    let _guard = gate().lock().await;
    let mut record = store::with_store(&runtime.db, |store| store.get(id))?;
    if record.status == ListingStatus::Uncertain {
        return Err(CommerceError::uncertain("提交结果不确定，只能查询，不能重新提交"));
    }
    if !record.status.can_resubmit() {
        return Err(CommerceError::duplicate("只能从已拒绝或失败的记录重新提交"));
    }
    if let Some(draft) = draft {
        record.title = draft.title.clone();
        record.currency = draft.currency.clone();
        record.draft = draft;
    }
    if let Some(target) = target {
        if target.shop_id != record.shop_id {
            return Err(CommerceError::invalid("重新提交不能更换店铺"));
        }
        record.target = target;
    }
    validate_draft(&record.draft)?;
    validate_targets(std::slice::from_ref(&record.target))?;
    let shop = runtime.shops.resolve(&record.shop_id).await?;
    adapter::require_listing(shop.platform)?;
    record.attempts = record.attempts.saturating_add(1);
    record.status = ListingStatus::Submitting;
    record.reason = None;
    record.updated_at = stamp(runtime.now);
    store::with_store(&runtime.db, |store| store.update(&record))?;
    let images = read_images(&record.draft.images)?;
    let remote = record.remote_id.clone();
    run_push(runtime, record, &shop, &images, remote.as_deref()).await
}

pub async fn refresh(runtime: &Runtime, id: &str) -> Result<ListingRecord, CommerceError> {
    let mut record = store::with_store(&runtime.db, |store| store.get(id))?;
    let Some(remote_id) = record.remote_id.clone() else {
        return Ok(record);
    };
    let shop = runtime.shops.resolve(&record.shop_id).await?;
    let (status, reason) = adapter::listing_status(runtime, &shop, &record.shop_id, &remote_id).await?;
    record.status = status;
    record.reason = reason;
    if record.status == ListingStatus::Rejected
        && record.reason.as_deref().is_some_and(|reason| reason.contains("DELETE"))
    {
        record.remote_id = None;
    }
    record.updated_at = stamp(runtime.now);
    store::with_store(&runtime.db, |store| store.update(&record))?;
    Ok(record)
}

async fn run_push(
    runtime: &Runtime,
    mut record: ListingRecord,
    shop: &ResolvedShop,
    images: &[(String, Vec<u8>)],
    remote: Option<&str>,
) -> Result<ListingRecord, CommerceError> {
    let outcome = adapter::push_listing(runtime, shop, &record.draft, &record.target, images, remote).await;
    apply_outcome(&mut record, outcome, runtime.now);
    store::with_store(&runtime.db, |store| store.update(&record))?;
    Ok(record)
}

fn apply_outcome(record: &mut ListingRecord, outcome: PushOutcome, now: i64) {
    record.status = outcome.status;
    record.reason = outcome.reason;
    if outcome.remote_id.is_some() {
        record.remote_id = outcome.remote_id;
    }
    record.updated_at = stamp(now);
}

fn reject_duplicate(
    existing: &[ListingRecord],
    group_id: &str,
    shop_id: &str,
    title: &str,
) -> Result<(), CommerceError> {
    if let Some(record) = existing.iter().find(|record| record.group_id == group_id && record.shop_id == shop_id) {
        if record.status.can_resubmit() {
            return Err(CommerceError::duplicate(format!(
                "该店铺在此批次已有被拒绝或失败的记录 {}，请重新提交而不是新建",
                record.id
            )));
        }
        return Err(CommerceError::duplicate(format!(
            "该店铺在此批次已有上架记录 {}（{}）",
            record.id,
            record.status.as_str()
        )));
    }
    let key = title_key(title);
    if let Some(record) = existing.iter().find(|record| {
        record.shop_id == shop_id && title_key(&record.title) == key && record.status.blocks_new_listing()
    }) {
        return Err(CommerceError::duplicate(format!(
            "该店铺已有同名商品 {}（{}），不能重复上架",
            record.id,
            record.status.as_str()
        )));
    }
    Ok(())
}

fn validate_targets(targets: &[ListingTarget]) -> Result<(), CommerceError> {
    if targets.is_empty() {
        return Err(CommerceError::invalid("请选择至少一个店铺"));
    }
    let mut seen = HashSet::new();
    for target in targets {
        if target.shop_id.trim().is_empty() {
            return Err(CommerceError::invalid("缺少店铺"));
        }
        if !seen.insert(target.shop_id.clone()) {
            return Err(CommerceError::invalid("同一批次不能重复选择同一店铺"));
        }
        if target.category_id.parse::<u64>().is_err() {
            return Err(CommerceError::invalid("类目 ID 无效"));
        }
    }
    Ok(())
}

fn validate_draft(draft: &ListingDraft) -> Result<(), CommerceError> {
    let title = draft.title.trim();
    if title.is_empty() || draft.title.chars().count() > 255 {
        return Err(CommerceError::invalid("商品标题需为 1 到 255 个字符"));
    }
    let description = draft.description.as_deref().unwrap_or("");
    if description.trim().is_empty() || description.chars().count() > 3000 {
        return Err(CommerceError::invalid("请填写 3000 字以内的商品描述"));
    }
    let price_ok = if draft.skus.is_empty() {
        draft.price.unwrap_or(0.0) > 0.0
    } else {
        draft.skus.iter().all(|sku| sku.price.or(draft.price).unwrap_or(0.0) > 0.0)
    };
    if !price_ok {
        return Err(CommerceError::invalid("请填写大于 0 的价格"));
    }
    let stock_ok = if draft.skus.is_empty() {
        matches!(draft.stock, Some(stock) if stock >= 0)
    } else {
        draft.skus.iter().all(|sku| sku.stock.or(draft.stock).unwrap_or(-1) >= 0)
    };
    if !stock_ok {
        return Err(CommerceError::invalid("请填写不小于 0 的库存"));
    }
    if draft.images.is_empty() || draft.images.len() > 9 {
        return Err(CommerceError::invalid("请提供 1 到 9 张图片的绝对路径"));
    }
    for image in &draft.images {
        let path = Path::new(image);
        if !path.is_absolute() {
            return Err(CommerceError::invalid(format!("图片必须是绝对路径：{image}")));
        }
    }
    if draft.weight_kg.unwrap_or(0.0) <= 0.0 {
        return Err(CommerceError::invalid("请填写大于 0 的重量（千克）"));
    }
    let Some(dimensions) = &draft.dimensions_cm else {
        return Err(CommerceError::invalid("请填写包装长宽高（厘米）"));
    };
    if dimensions.l <= 0.0 || dimensions.w <= 0.0 || dimensions.h <= 0.0 {
        return Err(CommerceError::invalid("包装长宽高必须大于 0"));
    }
    Ok(())
}

fn read_images(paths: &[String]) -> Result<Vec<(String, Vec<u8>)>, CommerceError> {
    let mut images = Vec::new();
    for path in paths {
        let file = Path::new(path);
        if !file.is_file() {
            return Err(CommerceError::invalid(format!("找不到图片：{path}")));
        }
        let bytes = std::fs::read(file).map_err(|error| CommerceError::failed(format!("读取图片失败：{error}")))?;
        if bytes.is_empty() || bytes.len() > 2_000_000 {
            return Err(CommerceError::invalid(format!("图片需小于 2MB：{path}")));
        }
        let name = file.file_name().and_then(|name| name.to_str()).unwrap_or("image.jpg").to_string();
        images.push((name, bytes));
    }
    Ok(images)
}

async fn shop_platform(id: &str) -> Result<Platform, CommerceError> {
    let shops = shops::shop_list().await.map_err(CommerceError::failed)?;
    shops
        .into_iter()
        .find(|shop| shop.id == id)
        .map(|shop| shop.platform)
        .ok_or_else(|| CommerceError::not_found(format!("店铺不存在：{id}")))
}

fn parse_draft(value: Option<&Value>) -> Result<ListingDraft, CommerceError> {
    serde_json::from_value(value.cloned().unwrap_or(Value::Null))
        .map_err(|error| CommerceError::invalid(format!("草稿无效：{error}")))
}

fn required_str(op: &Value, key: &str) -> Result<String, CommerceError> {
    op.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CommerceError::invalid(format!("缺少 {key}")))
}

fn metric_range(op: &Value) -> Result<MetricRange, CommerceError> {
    let value = op.get("range").cloned().unwrap_or(json!("today"));
    serde_json::from_value(value).map_err(|_| CommerceError::invalid("指标范围无效，应为 today、yesterday、last7 或 last30"))
}

fn stamp(now: i64) -> String {
    chrono::DateTime::from_timestamp(now, 0).map(|time| time.to_rfc3339()).unwrap_or_default()
}
