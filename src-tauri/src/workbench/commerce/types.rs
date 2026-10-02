//! Commerce contract types shared by the Tauri commands, agent tool, and CLI.
use crate::app_cli::exit;
use crate::workbench::shops::Platform;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum MetricKey {
    Gmv,
    Orders,
    RefundAmount,
    RefundOrders,
    Buyers,
    ProductsLive,
    PendingShipment,
}

impl MetricKey {
    pub const ALL: [MetricKey; 7] = [
        Self::Gmv,
        Self::Orders,
        Self::RefundAmount,
        Self::RefundOrders,
        Self::Buyers,
        Self::ProductsLive,
        Self::PendingShipment,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum MetricRange {
    Today,
    Yesterday,
    #[serde(rename = "last7")]
    #[ts(rename = "last7")]
    Last7,
    #[serde(rename = "last30")]
    #[ts(rename = "last30")]
    Last30,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommerceCapabilities {
    pub platform: Platform,
    pub metrics: Vec<MetricKey>,
    pub listing: bool,
    pub categories: bool,
    pub products: bool,
    pub orders: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShopMetrics {
    pub shop_id: String,
    pub range: MetricRange,
    pub currency: Option<String>,
    #[ts(type = "Partial<Record<MetricKey, number>>")]
    pub values: BTreeMap<MetricKey, f64>,
    pub unsupported: Vec<MetricKey>,
    pub fetched_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommerceProduct {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stock: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sku: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProductPage {
    pub shop_id: String,
    pub items: Vec<CommerceProduct>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommerceOrderLine {
    pub title: String,
    pub quantity: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sku: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommerceOrder {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub buyer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub lines: Vec<CommerceOrderLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OrderPage {
    pub shop_id: String,
    pub range: MetricRange,
    pub items: Vec<CommerceOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub leaf: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum AttributeInput {
    Text,
    Number,
    Select,
    MultiSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttributeOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategoryAttribute {
    pub id: String,
    pub name: String,
    pub required: bool,
    pub input: AttributeInput,
    pub options: Vec<AttributeOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingSku {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub price: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub stock: Option<i64>,
    #[serde(default)]
    #[ts(optional)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionsCm {
    pub l: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingDraft {
    pub title: String,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub price: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub currency: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub stock: Option<i64>,
    #[serde(default)]
    pub skus: Vec<ListingSku>,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    #[ts(optional)]
    pub weight_kg: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub dimensions_cm: Option<DimensionsCm>,
    #[serde(default)]
    #[ts(optional)]
    pub brand: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingAttribute {
    pub id: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingTarget {
    pub shop_id: String,
    pub category_id: String,
    #[serde(default)]
    pub attributes: Vec<ListingAttribute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ListingStatus {
    Submitting,
    Reviewing,
    Live,
    Rejected,
    Failed,
    Uncertain,
    Banned,
}

impl ListingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submitting => "submitting",
            Self::Reviewing => "reviewing",
            Self::Live => "live",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
            Self::Banned => "banned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "submitting" => Self::Submitting,
            "reviewing" => Self::Reviewing,
            "live" => Self::Live,
            "rejected" => Self::Rejected,
            "failed" => Self::Failed,
            "uncertain" => Self::Uncertain,
            "banned" => Self::Banned,
            _ => return None,
        })
    }

    /// Statuses that already occupy the shop slot. Rejected and failed stay on the
    /// same record and must go through resubmit.
    pub fn blocks_new_listing(self) -> bool {
        matches!(
            self,
            Self::Submitting | Self::Reviewing | Self::Live | Self::Uncertain | Self::Banned
        )
    }

    pub fn can_resubmit(self) -> bool {
        matches!(self, Self::Rejected | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ListingRecord {
    pub id: String,
    pub group_id: String,
    pub shop_id: String,
    pub platform: Platform,
    pub status: ListingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remote_id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub currency: Option<String>,
    pub draft: ListingDraft,
    pub target: ListingTarget,
    pub attempts: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListingFilter {
    #[serde(default)]
    #[ts(optional)]
    pub shop_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub group_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<ListingStatus>,
}

/// Failures the CLI maps onto exit codes. Display is the message MediaLocal puts on `CliFailure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommerceError {
    Invalid(String),
    Unsupported(String),
    Duplicate(String),
    NotFound(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
    Internal(String),
}

impl CommerceError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }
    pub fn duplicate(message: impl Into<String>) -> Self {
        Self::Duplicate(message.into())
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }
    pub fn rejected(message: impl Into<String>) -> Self {
        Self::Rejected(message.into())
    }
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
    pub fn uncertain(message: impl Into<String>) -> Self {
        Self::Uncertain(message.into())
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Invalid(_) | Self::Unsupported(_) | Self::Duplicate(_) | Self::NotFound(_) => {
                exit::INVALID
            }
            Self::Rejected(_) => exit::REJECTED,
            Self::Failed(_) => exit::FAILED,
            Self::Uncertain(_) => exit::UNCERTAIN,
            Self::Internal(_) => exit::INTERNAL,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Invalid(message)
            | Self::Unsupported(message)
            | Self::Duplicate(message)
            | Self::NotFound(message)
            | Self::Rejected(message)
            | Self::Failed(message)
            | Self::Uncertain(message)
            | Self::Internal(message) => message,
        }
    }
}

impl fmt::Display for CommerceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

pub fn capabilities_for(platform: Platform) -> CommerceCapabilities {
    match platform {
        Platform::Shopee => CommerceCapabilities {
            platform,
            metrics: MetricKey::ALL.to_vec(),
            listing: true,
            categories: true,
            products: true,
            orders: true,
            notes: vec![
                "GMV、订单、买家和待发货按店铺时区从订单汇总；退款来自退货单。接口没有的指标记为不支持。"
                    .into(),
            ],
        },
        Platform::Tiktok => super::tiktok::tiktok_capabilities(),
        Platform::Shein => CommerceCapabilities {
            platform,
            metrics: MetricKey::ALL.to_vec(),
            listing: true,
            categories: true,
            products: true,
            orders: true,
            notes: vec![
                "买家 ID 不在订单列表里，buyers 记为不支持。订单分页单次不超过 48 小时，今天和昨天可以翻页；更长窗口只在指标汇总里拆开。".into(),
            ],
        },
        Platform::Kuaishou => CommerceCapabilities {
            platform,
            metrics: super::kuaishou::supported_metrics().to_vec(),
            listing: true,
            categories: true,
            products: true,
            orders: true,
            notes: super::kuaishou::capability_notes().iter().map(|note| (*note).to_string()).collect(),
        },
        Platform::Pinduoduo => CommerceCapabilities {
            platform,
            metrics: vec![
                MetricKey::Gmv,
                MetricKey::Orders,
                MetricKey::RefundOrders,
                MetricKey::ProductsLive,
                MetricKey::PendingShipment,
            ],
            listing: true,
            categories: true,
            products: true,
            orders: true,
            notes: vec!["买家和退款金额没有对应字段，记为不支持。订单成交时间单次不超过 24 小时，分页游标按天切开。金额记 CNY。".into()],
        },
        Platform::Douyin => super::douyin::capabilities(),
        Platform::Mercadolibre => super::mercadolibre::capabilities(),
        Platform::Wechat => super::wechat::capabilities(),
        Platform::Taobao => super::taobao::capabilities(),
    }
}

pub fn zero_metrics(shop_id: String, range: MetricRange, fetched_at: String) -> ShopMetrics {
    ShopMetrics {
        shop_id,
        range,
        currency: None,
        values: MetricKey::ALL.into_iter().map(|key| (key, 0.0)).collect(),
        unsupported: Vec::new(),
        fetched_at,
        error: None,
    }
}

pub fn title_key(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}
