//! One commerce implementation for the workbench, the `commerce` agent tool, and `dsivio commerce`.
pub mod cli;
mod adapter;
mod douyin;
mod kuaishou;
mod mercadolibre;
mod pinduoduo;
mod service;
mod shein;
mod shopee;
mod store;
mod taobao;
mod tiktok;
mod transport;
mod types;
mod wechat;

#[cfg(test)]
mod mercadolibre_tests;

#[cfg(test)]
mod tests;

pub use cli::{handle, run};
pub use types::{
    capabilities_for, AttributeInput, AttributeOption, Category, CategoryAttribute, CommerceCapabilities,
    CommerceError, CommerceOrder, CommerceOrderLine, CommerceProduct, DimensionsCm, ListingAttribute, ListingDraft,
    ListingFilter, ListingRecord, ListingSku, ListingStatus, ListingTarget, MetricKey, MetricRange, OrderPage,
    ProductPage, ShopMetrics,
};

pub const SUBCOMMAND: &str = cli::SUBCOMMAND;

pub fn action_requires_approval(arguments: &serde_json::Value) -> bool {
    matches!(arguments.get("action").and_then(|value| value.as_str()), Some("submit" | "resubmit"))
}

pub async fn execute(op: &serde_json::Value) -> Result<serde_json::Value, CommerceError> {
    service::execute(op).await
}

#[tauri::command]
pub async fn commerce_shops() -> Result<Vec<crate::workbench::shops::Shop>, String> {
    crate::workbench::shops::shop_list().await
}

#[tauri::command]
pub async fn commerce_capabilities(shop_id: String) -> Result<CommerceCapabilities, String> {
    let value = execute(&serde_json::json!({"action": "capabilities", "shopId": shop_id})).await.map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_metrics(shop_id: String, range: MetricRange) -> Result<ShopMetrics, String> {
    let value = execute(&serde_json::json!({"action": "metrics", "shopId": shop_id, "range": range}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_categories(shop_id: String, parent_id: Option<String>) -> Result<Vec<Category>, String> {
    let value = execute(&serde_json::json!({"action": "categories", "shopId": shop_id, "parentId": parent_id}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_attributes(shop_id: String, category_id: String) -> Result<Vec<CategoryAttribute>, String> {
    let value = execute(&serde_json::json!({"action": "attributes", "shopId": shop_id, "categoryId": category_id}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_products(shop_id: String, cursor: Option<String>) -> Result<ProductPage, String> {
    let value = execute(&serde_json::json!({"action": "products", "shopId": shop_id, "cursor": cursor}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_orders(shop_id: String, range: MetricRange, cursor: Option<String>) -> Result<OrderPage, String> {
    let value = execute(&serde_json::json!({"action": "orders", "shopId": shop_id, "range": range, "cursor": cursor}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_listings(filter: ListingFilter) -> Result<Vec<ListingRecord>, String> {
    let value = execute(&serde_json::json!({"action": "listings", "filter": filter})).await.map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_submit(
    draft: ListingDraft,
    targets: Vec<ListingTarget>,
    group_id: Option<String>,
) -> Result<Vec<ListingRecord>, String> {
    let value = execute(&serde_json::json!({"action": "submit", "draft": draft, "targets": targets, "groupId": group_id}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_resubmit(
    id: String,
    draft: Option<ListingDraft>,
    target: Option<ListingTarget>,
) -> Result<ListingRecord, String> {
    let value = execute(&serde_json::json!({"action": "resubmit", "id": id, "draft": draft, "target": target}))
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_refresh(id: String) -> Result<ListingRecord, String> {
    let value = execute(&serde_json::json!({"action": "refresh", "id": id})).await.map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn commerce_status(id: String) -> Result<ListingRecord, String> {
    let value = execute(&serde_json::json!({"action": "status", "id": id})).await.map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}
