use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct SourcingConfig {
    pub alibaba_ak: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProductSort {
    Relevance,
    PriceAsc,
    PriceDesc,
    SalesDesc,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookalikeRequest {
    pub image: String,
    pub name: String,
    pub sort: ProductSort,
    pub limit: u32,
    pub purchase_amount: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourcingProduct {
    pub id: String,
    pub title: String,
    pub url: String,
    pub image_url: Option<String>,
    pub price: Option<String>,
    pub supplier: Option<String>,
    pub sku_id: Option<String>,
    pub sku_title: Option<String>,
    pub minimum_order: Option<String>,
    pub sold_count: Option<String>,
    pub stock: Option<String>,
    pub relevance: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourcingSearch {
    pub id: String,
    pub name: String,
    pub fetched_at: String,
    pub sort: ProductSort,
    pub purchase_amount: u32,
    pub products: Vec<SourcingProduct>,
    /// A completed API search remains usable if the local history write fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub history_warning: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourcingSearchSummary {
    pub id: String,
    pub name: String,
    pub fetched_at: String,
    pub count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PickSource {
    Manual,
    Alibaba1688,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PickStage {
    New,
    Evaluating,
    Selected,
    Rejected,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickDraft {
    pub source: PickSource,
    pub source_id: Option<String>,
    pub title: String,
    pub url: Option<String>,
    pub image_url: Option<String>,
    /// Decimal display value; never infer missing price or sales metrics.
    pub price: Option<String>,
    pub currency: Option<String>,
    pub supplier: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: String,
    pub stage: PickStage,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PickItem {
    pub id: String,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
    pub product: PickDraft,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePickRequest {
    /// None creates/collects; updates require id and expectedRevision together.
    pub id: Option<String>,
    pub expected_revision: Option<u32>,
    pub product: PickDraft,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickFilter {
    #[serde(default)]
    pub keyword: String,
    pub stage: Option<PickStage>,
    pub source: Option<PickSource>,
    pub page: u32,
    pub page_size: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PickPage {
    pub items: Vec<PickItem>,
    pub total: u32,
}

impl std::fmt::Debug for SourcingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcingConfig")
            .field("alibaba_ak", &"[REDACTED]")
            .finish()
    }
}
