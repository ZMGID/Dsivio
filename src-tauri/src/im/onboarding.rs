//! Scan-to-create protocols adapted from Hermes Agent (Nous Research, MIT).
//! Credentials are committed to the OS store and never returned to the webview.
use super::{common::http_client, credentials, types::*};
use parking_lot::Mutex;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct SetupFlows {
    flows: Mutex<HashMap<String, Arc<SetupFlow>>>,
}
impl Default for SetupFlows {
    fn default() -> Self {
        Self {
            flows: Mutex::new(HashMap::new()),
        }
    }
}
struct SetupFlow {
    platform: ImPlatform,
    expires_at: i64,
    inner: tokio::sync::Mutex<Flow>,
    cancelled: Mutex<bool>,
}
struct Flow {
    view: ImSetupSession,
    code: String,
    domain: FeishuDomain,
    interval: Duration,
    last_poll: Option<Instant>,
}
fn accounts(domain: FeishuDomain) -> &'static str {
    match domain {
        FeishuDomain::Feishu => "https://accounts.feishu.cn",
        FeishuDomain::Lark => "https://accounts.larksuite.com",
    }
}
async fn registration(domain: FeishuDomain, params: &[(&str, &str)]) -> Result<Value, String> {
    let response = http_client()?
        .post(format!("{}/oauth/v1/app/registration", accounts(domain)))
        .timeout(Duration::from_secs(10))
        .form(params)
        .send()
        .await
        .map_err(|_| "无法连接飞书注册服务，请使用手动配置")?;
    response
        .json()
        .await
        .map_err(|_| "飞书注册服务返回无效数据，请使用手动配置".into())
}
fn str_field(value: &Value, field: &str) -> String {
    value[field].as_str().unwrap_or_default().to_owned()
}
fn valid_setup_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.host_str().is_some_and(|h| {
                h == "work.weixin.qq.com"
                    || h.ends_with(".feishu.cn")
                    || h.ends_with(".larksuite.com")
            })
    })
}
impl SetupFlows {
    pub async fn begin(
        &self,
        platform: ImPlatform,
        domain: FeishuDomain,
    ) -> Result<ImSetupSession, String> {
        if platform == ImPlatform::WecomCallback {
            return Err("自建应用请在企业微信管理后台创建，再填写回调配置".into());
        }
        let (code, url, interval, ttl) = match platform {
            ImPlatform::Feishu => {
                let init = registration(domain, &[("action", "init")]).await?;
                if !init["supported_auth_methods"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "client_secret"))
                {
                    return Err("飞书注册服务暂不支持机器人密钥，请手动配置".into());
                }
                let result = registration(
                    domain,
                    &[
                        ("action", "begin"),
                        ("archetype", "PersonalAgent"),
                        ("auth_method", "client_secret"),
                        ("request_user_info", "open_id"),
                    ],
                )
                .await?;
                let code = str_field(&result, "device_code");
                let url = str_field(&result, "verification_uri_complete");
                if code.is_empty() || !valid_setup_url(&url) {
                    return Err("飞书未返回有效授权链接，请手动配置".into());
                }
                (
                    code,
                    url,
                    result["interval"].as_u64().unwrap_or(5).clamp(3, 60),
                    result["expire_in"].as_i64().unwrap_or(600).clamp(30, 900),
                )
            }
            ImPlatform::Wecom => {
                // These management-console endpoints are not a stable public API.
                // Keep Hermes' supported source tag; callers always have manual setup.
                let result: Value = http_client()?
                    .get("https://work.weixin.qq.com/ai/qc/generate")
                    .query(&[("source", "hermes")])
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await
                    .map_err(|_| "企业微信扫码服务不可用，请手动配置")?
                    .json()
                    .await
                    .map_err(|_| "企业微信扫码服务返回无效数据")?;
                let code = str_field(&result["data"], "scode");
                let url = str_field(&result["data"], "auth_url");
                if code.is_empty() || !valid_setup_url(&url) {
                    return Err("企业微信未返回有效授权链接，请手动配置".into());
                }
                (code, url, 3, 300)
            }
            ImPlatform::WecomCallback => unreachable!(),
        };
        let view = ImSetupSession {
            id: uuid::Uuid::new_v4().to_string(),
            platform,
            url,
            status: ImSetupState::Pending,
            expires_at: chrono::Utc::now().timestamp() + ttl,
            message: "请使用对应客户端扫码创建机器人".into(),
            identity: None,
        };
        let mut flows = self.flows.lock();
        // A single flow per platform; replacing it cannot later publish credentials.
        flows.retain(|_, f| {
            if f.platform == platform || f.expires_at <= chrono::Utc::now().timestamp() {
                *f.cancelled.lock() = true;
                return false;
            }
            true
        });
        if flows.len() >= 8 {
            return Err("扫码配置过多，请取消已有配置后重试".into());
        }
        flows.insert(
            view.id.clone(),
            Arc::new(SetupFlow {
                platform,
                expires_at: view.expires_at,
                inner: tokio::sync::Mutex::new(Flow {
                    view: view.clone(),
                    code,
                    domain,
                    interval: Duration::from_secs(interval),
                    last_poll: None,
                }),
                cancelled: Mutex::new(false),
            }),
        );
        Ok(view)
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        if let Some(flow) = self.flows.lock().get(id).cloned() {
            *flow.cancelled.lock() = true;
        }
        Ok(())
    }
    pub async fn poll(&self, id: &str) -> Result<ImSetupSession, String> {
        let flow = self
            .flows
            .lock()
            .get(id)
            .cloned()
            .ok_or("扫码配置不存在或已被替换")?;
        let mut state = flow.inner.lock().await;
        if *flow.cancelled.lock() {
            state.view.status = ImSetupState::Cancelled;
            return Ok(state.view.clone());
        }
        if state.view.status != ImSetupState::Pending {
            return Ok(state.view.clone());
        }
        if chrono::Utc::now().timestamp() >= state.view.expires_at {
            state.view.status = ImSetupState::Expired;
            state.view.message = "授权链接已过期，请重新开始".into();
            return Ok(state.view.clone());
        }
        if state
            .last_poll
            .is_some_and(|t| t.elapsed() < state.interval)
        {
            return Ok(state.view.clone());
        }
        state.last_poll = Some(Instant::now());
        let fetched = match state.view.platform {
            ImPlatform::Feishu => {
                registration(
                    state.domain,
                    &[
                        ("action", "poll"),
                        ("device_code", &state.code),
                        ("tp", "ob_app"),
                    ],
                )
                .await
            }
            ImPlatform::Wecom => {
                async {
                    http_client()?
                        .get("https://work.weixin.qq.com/ai/qc/query_result")
                        .query(&[("scode", &state.code)])
                        .timeout(Duration::from_secs(10))
                        .send()
                        .await
                        .map_err(|_| "扫码状态查询失败".to_string())?
                        .json::<Value>()
                        .await
                        .map_err(|_| "扫码状态返回无效数据".to_string())
                }
                .await
            }
            ImPlatform::WecomCallback => return Err("自建应用不支持扫码创建".into()),
        };
        let result = match fetched {
            Ok(value) => value,
            Err(message) => {
                state.view.message = message;
                return Ok(state.view.clone());
            }
        };
        let completed = match state.view.platform {
            ImPlatform::Feishu => {
                if result["user_info"]["tenant_brand"] == "lark" {
                    state.domain = FeishuDomain::Lark;
                }
                match result["error"].as_str().unwrap_or_default() {
                    "access_denied" => {
                        state.view.status = ImSetupState::Denied;
                        state.view.message = "用户拒绝了扫码配置".into();
                    }
                    "expired_token" => {
                        state.view.status = ImSetupState::Expired;
                        state.view.message = "授权链接已过期".into();
                    }
                    "slow_down" => {
                        state.interval =
                            (state.interval + Duration::from_secs(5)).min(Duration::from_secs(60));
                    }
                    _ => {}
                }
                let id = str_field(&result, "client_id");
                let secret = str_field(&result, "client_secret");
                if !id.is_empty() && !secret.is_empty() {
                    Some((
                        ImSetupIdentity {
                            app_id: id,
                            bot_id: String::new(),
                            domain: state.domain,
                            owner_id: str_field(&result["user_info"], "open_id"),
                            bot_name: String::new(),
                        },
                        secret,
                    ))
                } else {
                    None
                }
            }
            ImPlatform::Wecom => {
                let data = &result["data"];
                if data["status"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("success"))
                {
                    let id = data["bot_info"]["botid"]
                        .as_str()
                        .or_else(|| data["bot_info"]["bot_id"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    let secret = str_field(&data["bot_info"], "secret");
                    if id.is_empty() || secret.is_empty() {
                        state.view.status = ImSetupState::Error;
                        state.view.message = "扫码完成但未创建机器人，请手动配置".into();
                        None
                    } else {
                        Some((
                            ImSetupIdentity {
                                app_id: String::new(),
                                bot_id: id,
                                domain: state.domain,
                                owner_id: String::new(),
                                bot_name: str_field(&data["bot_info"], "name"),
                            },
                            secret,
                        ))
                    }
                } else {
                    None
                }
            }
            ImPlatform::WecomCallback => None,
        };
        if let Some((identity, secret)) = completed {
            let cancelled = flow.cancelled.lock();
            if *cancelled {
                state.view.status = ImSetupState::Cancelled;
            } else {
                let id = if state.view.platform == ImPlatform::Feishu {
                    &identity.app_id
                } else {
                    &identity.bot_id
                };
                credentials::save(
                    state.view.platform,
                    id,
                    CredentialInput {
                        secret,
                        ..Default::default()
                    },
                )?;
                state.view.identity = Some(identity);
                state.view.status = ImSetupState::Completed;
                state.view.message = "机器人凭证已保存在系统凭证库；请核对访问规则再启用".into();
            }
        }
        Ok(state.view.clone())
    }
}
