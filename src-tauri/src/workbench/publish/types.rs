//! Unified content-platform types. Platform adapters map these onto TikTok and YouTube.
use crate::app_cli::exit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ContentPlatform {
    Tiktok,
    Youtube,
}
impl ContentPlatform {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Tiktok => "tiktok",
            Self::Youtube => "youtube",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "tiktok" => Ok(Self::Tiktok),
            "youtube" => Ok(Self::Youtube),
            _ => Err("不支持的内容平台".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Connected,
    NeedsReauthorization,
    Error,
}
impl AccountStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::NeedsReauthorization => "needs_reauthorization",
            Self::Error => "error",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "connected" => Ok(Self::Connected),
            "needs_reauthorization" => Ok(Self::NeedsReauthorization),
            "error" => Ok(Self::Error),
            _ => Err("账号状态无效".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Privacy {
    Public,
    Private,
    Unlisted,
    Friends,
}
impl Privacy {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Unlisted => "unlisted",
            Self::Friends => "friends",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            "unlisted" => Ok(Self::Unlisted),
            "friends" => Ok(Self::Friends),
            _ => Err("可见范围无效".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PublishStatus {
    Uploading,
    Processing,
    Published,
    Rejected,
    Failed,
    Uncertain,
}
impl PublishStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "uploading",
            Self::Processing => "processing",
            Self::Published => "published",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "uploading" => Ok(Self::Uploading),
            "processing" => Ok(Self::Processing),
            "published" => Ok(Self::Published),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            "uncertain" => Ok(Self::Uncertain),
            _ => Err("发布状态无效".into()),
        }
    }
    /// Published, processing, in-flight, and uncertain rows must never be uploaded again.
    pub(crate) fn blocks_upload(self) -> bool {
        matches!(
            self,
            Self::Uploading | Self::Processing | Self::Published | Self::Uncertain
        )
    }
    pub(crate) fn can_retry(self) -> bool {
        matches!(self, Self::Rejected | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StatKey {
    Views,
    Likes,
    Comments,
    Shares,
}
impl StatKey {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Likes => "likes",
            Self::Comments => "comments",
            Self::Shares => "shares",
        }
    }
    pub(crate) const ALL: [StatKey; 4] = [Self::Views, Self::Likes, Self::Comments, Self::Shares];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishAccount {
    pub id: String,
    pub platform: ContentPlatform,
    pub remote_id: String,
    pub name: String,
    pub bound_at: String,
    pub checked_at: String,
    pub status: AccountStatus,
    pub detail: Option<String>,
    pub fans: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishAppConfig {
    pub platform: ContentPlatform,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishBeginResult {
    pub request_id: String,
    pub url: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishRequest {
    pub account_ids: Vec<String>,
    pub video_path: String,
    pub title: String,
    pub description: String,
    pub privacy: Privacy,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    #[ts(optional)]
    pub media_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishRecord {
    pub id: String,
    pub group_id: String,
    pub account_id: String,
    pub platform: ContentPlatform,
    pub video_path: String,
    pub title: String,
    pub description: String,
    pub privacy: Privacy,
    #[serde(default)]
    pub tags: Vec<String>,
    pub status: PublishStatus,
    pub remote_id: Option<String>,
    pub url: Option<String>,
    pub reason: Option<String>,
    pub attempts: u32,
    pub created_at: String,
    pub updated_at: String,
    pub origin: String,
    pub media_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishBlock {
    pub account_id: String,
    pub status: PublishStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PublishSubmitResult {
    pub records: Vec<PublishRecord>,
    pub blocked: Vec<PublishBlock>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishRecordFilter {
    #[serde(default)]
    #[ts(optional)]
    pub account_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<PublishStatus>,
}

/// Values the platform actually returned. Missing keys are listed in `unsupported`, never as 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VideoStats {
    pub record_id: String,
    #[ts(type = "Partial<Record<StatKey, number>>")]
    pub values: BTreeMap<StatKey, f64>,
    pub unsupported: Vec<StatKey>,
    pub fetched_at: String,
    pub error: Option<String>,
}
impl VideoStats {
    pub(crate) fn from_values(
        record_id: String,
        values: BTreeMap<StatKey, f64>,
        error: Option<String>,
    ) -> Self {
        let unsupported = StatKey::ALL
            .into_iter()
            .filter(|key| !values.contains_key(key))
            .collect();
        Self {
            record_id,
            values,
            unsupported,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            error,
        }
    }

    pub(crate) fn unsupported_all(record_id: String, error: Option<String>) -> Self {
        Self::from_values(record_id, BTreeMap::new(), error)
    }
}

/// Failures from [`super::handle`]. Display is the message MediaLocal puts on `CliFailure`.
///
/// A publish that reached the platform is `Ok`: `rejected` / `failed` / `uncertain` stay on
/// `status` (or the worst of `records[].status`). Do not turn that payload into this error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    Invalid(String),
    Unsupported(String),
    NotFound(String),
    Rejected(String),
    Failed(String),
    Uncertain(String),
    Internal(String),
    Timeout(String),
}

impl PublishError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
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
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout(message.into())
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Invalid(_) | Self::Unsupported(_) | Self::NotFound(_) => exit::INVALID,
            Self::Rejected(_) => exit::REJECTED,
            Self::Failed(_) => exit::FAILED,
            Self::Uncertain(_) => exit::UNCERTAIN,
            Self::Internal(_) => exit::INTERNAL,
            Self::Timeout(_) => exit::TIMEOUT,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Invalid(message)
            | Self::Unsupported(message)
            | Self::NotFound(message)
            | Self::Rejected(message)
            | Self::Failed(message)
            | Self::Uncertain(message)
            | Self::Internal(message)
            | Self::Timeout(message) => message,
        }
    }
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Credential {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub scope: String,
}
