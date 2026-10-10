use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ImPlatform {
    Feishu,
    Wecom,
    WecomCallback,
}
impl ImPlatform {
    pub fn key(self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::Wecom => "wecom",
            Self::WecomCallback => "wecom_callback",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Feishu => "飞书",
            Self::Wecom => "企业微信",
            Self::WecomCallback => "企业微信自建应用",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DmPolicy {
    #[default]
    Pairing,
    Allowlist,
    Open,
    Disabled,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupPolicy {
    #[default]
    Allowlist,
    Open,
    Disabled,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ImAccessConfig {
    pub dm_policy: DmPolicy,
    pub group_policy: GroupPolicy,
    pub allowed_users: Vec<String>,
    pub allowed_groups: Vec<String>,
    pub group_users: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FeishuDomain {
    #[default]
    Feishu,
    Lark,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FeishuConnectionMode {
    #[default]
    Websocket,
    Webhook,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ImWebhookConfig {
    pub host: String,
    pub port: u16,
    pub path: String,
}
impl Default for ImWebhookConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8765,
            path: "/feishu/webhook".into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct FeishuConfig {
    pub enabled: bool,
    pub app_id: String,
    pub domain: FeishuDomain,
    pub connection_mode: FeishuConnectionMode,
    pub webhook: ImWebhookConfig,
    pub access: ImAccessConfig,
    pub require_mention: bool,
    pub home_channel: String,
}
impl Default for FeishuConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_id: String::new(),
            domain: FeishuDomain::default(),
            connection_mode: FeishuConnectionMode::default(),
            webhook: ImWebhookConfig::default(),
            access: ImAccessConfig::default(),
            require_mention: true,
            home_channel: String::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WecomConfig {
    pub enabled: bool,
    pub bot_id: String,
    pub websocket_url: String,
    pub access: ImAccessConfig,
    pub home_channel: String,
}
impl Default for WecomConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_id: String::new(),
            websocket_url: "wss://openws.work.weixin.qq.com".into(),
            access: ImAccessConfig::default(),
            home_channel: String::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WecomCallbackConfig {
    pub enabled: bool,
    pub corp_id: String,
    pub agent_id: String,
    pub webhook: ImWebhookConfig,
    pub access: ImAccessConfig,
    pub home_channel: String,
}
impl Default for WecomCallbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            corp_id: String::new(),
            agent_id: String::new(),
            webhook: ImWebhookConfig {
                host: "127.0.0.1".into(),
                port: 8645,
                path: "/wecom/callback".into(),
            },
            access: ImAccessConfig::default(),
            home_channel: String::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ImAgentConfig {
    pub assistant_id: String,
    pub provider_id: String,
    pub model: String,
    pub working_directory: String,
    pub group_sessions_per_user: bool,
    pub streaming: bool,
}
impl Default for ImAgentConfig {
    fn default() -> Self {
        Self {
            assistant_id: String::new(),
            provider_id: String::new(),
            model: String::new(),
            working_directory: String::new(),
            group_sessions_per_user: true,
            streaming: true,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ImConfig {
    pub feishu: FeishuConfig,
    pub wecom: WecomConfig,
    pub wecom_callback: WecomCallbackConfig,
    pub agent: ImAgentConfig,
}
impl ImConfig {
    pub fn access(&self, platform: ImPlatform) -> &ImAccessConfig {
        match platform {
            ImPlatform::Feishu => &self.feishu.access,
            ImPlatform::Wecom => &self.wecom.access,
            ImPlatform::WecomCallback => &self.wecom_callback.access,
        }
    }
    pub fn enabled(&self, platform: ImPlatform) -> bool {
        match platform {
            ImPlatform::Feishu => self.feishu.enabled,
            ImPlatform::Wecom => self.wecom.enabled,
            ImPlatform::WecomCallback => self.wecom_callback.enabled,
        }
    }
    pub fn identity(&self, platform: ImPlatform) -> &str {
        match platform {
            ImPlatform::Feishu => &self.feishu.app_id,
            ImPlatform::Wecom => &self.wecom.bot_id,
            ImPlatform::WecomCallback => &self.wecom_callback.corp_id,
        }
    }
}

// Secrets are passed only on credential writes and backend transport startup. Never settings/status.
#[derive(Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct CredentialInput {
    pub secret: String,
    pub encrypt_key: String,
    pub verification_token: String,
    pub token: String,
    pub encoding_aes_key: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disabled,
    Connecting,
    Connected,
    Retrying,
    Error,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImStatus {
    pub platform: ImPlatform,
    pub state: ConnectionState,
    pub message: String,
    pub credentials_configured: bool,
    pub webhook_url: String,
    pub last_message_at: Option<i64>,
}
#[derive(Debug, Clone)]
pub struct StatusUpdate {
    pub platform: ImPlatform,
    pub state: ConnectionState,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImPairingRequest {
    pub platform: ImPlatform,
    pub code: String,
    pub user_id: String,
    pub user_name: String,
    pub created_at: i64,
    pub expires_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImApprovedUser {
    pub platform: ImPlatform,
    pub identity: String,
    pub user_id: String,
    pub user_name: String,
    pub approved_at: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ImSetupState {
    Pending,
    Authorized,
    Completed,
    Denied,
    Expired,
    Cancelled,
    Error,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImSetupIdentity {
    pub app_id: String,
    pub bot_id: String,
    pub domain: FeishuDomain,
    pub owner_id: String,
    pub bot_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImSetupSession {
    pub id: String,
    pub platform: ImPlatform,
    pub url: String,
    pub status: ImSetupState,
    pub expires_at: i64,
    pub message: String,
    pub identity: Option<ImSetupIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    pub platform: ImPlatform,
    pub message_id: String,
    pub chat_id: String,
    pub user_id: String,
    pub user_name: String,
    pub is_group: bool,
    pub thread_id: Option<String>,
    pub reply_token: Option<String>,
    pub text: String,
    pub attachments: Vec<String>,
    /// User-visible attachment failures. Never copied into `text`.
    pub attachment_failures: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub is_group: bool,
    pub thread_id: Option<String>,
    pub reply_token: Option<String>,
    pub reply_to: Option<String>,
    pub text: String,
    pub attachments: Vec<String>,
    pub stream_id: Option<String>,
    pub finished: bool,
}
