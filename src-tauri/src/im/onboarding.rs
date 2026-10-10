//! Scan-to-create protocols adapted from Hermes Agent (Nous Research, MIT).
//! An authorized secret stays in this flow until commit and is never returned to the webview.
use super::{common::http_client, credentials, types::*};
use parking_lot::Mutex;
use serde_json::Value;
#[cfg(test)]
use std::collections::VecDeque;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct SetupFlows {
    flows: Mutex<HashMap<String, Arc<SetupFlow>>>,
    secrets: credentials::CredentialStore,
    #[cfg(test)]
    remote: Mutex<VecDeque<Result<Value, String>>>,
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
    secret: Option<StagedSecret>,
}
/// In-memory setup secret. Intentionally not Debug or Serialize.
#[derive(Clone)]
struct StagedSecret(String);
#[derive(Debug)]
pub(crate) struct SetupCommit {
    pub session: ImSetupSession,
    pub reconnect: bool,
}
pub(crate) fn activate_commit(
    runtime: &super::ImRuntime,
    outcome: SetupCommit,
) -> Result<ImSetupSession, String> {
    if let Some(identity) = &outcome.session.identity {
        let owner_id = identity.owner_id.trim();
        if !owner_id.is_empty() {
            let platform = outcome.session.platform;
            let bot_id = canonical_identity(platform, identity);
            let is_owner = |user: &ImApprovedUser| {
                user.platform == platform && user.identity == bot_id && user.user_id == owner_id
            };
            let needs_pairing = {
                let state = runtime.state.lock();
                !state.store.approved.iter().any(is_owner)
                    || state.store.pending.iter().any(|pair| {
                        pair.request.platform == platform
                            && pair.identity == bot_id
                            && pair.request.user_id == owner_id
                    })
            };
            if needs_pairing {
                // The registration response authenticates this account. Persist its bot-scoped
                // grant before starting transport; never trust the first incoming sender instead.
                runtime.transact(|store| {
                    if !store.approved.iter().any(is_owner) {
                        store.approved.push(ImApprovedUser {
                            platform,
                            identity: bot_id.to_owned(),
                            user_id: owner_id.to_owned(),
                            user_name: String::new(),
                            approved_at: super::now(),
                        });
                    }
                    store.pending.retain(|pair| {
                        !(pair.request.platform == platform
                            && pair.identity == bot_id
                            && pair.request.user_id == owner_id)
                    });
                    Ok(())
                })?;
            }
        }
    }
    if outcome.reconnect {
        runtime.reconnect(outcome.session.platform);
    }
    Ok(outcome.session)
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
fn take_string(value: &mut Value, field: &str) -> String {
    match value.get_mut(field).map(Value::take) {
        Some(Value::String(text)) => text,
        _ => String::new(),
    }
}
fn canonical_identity(platform: ImPlatform, identity: &ImSetupIdentity) -> &str {
    match platform {
        ImPlatform::Feishu => identity.app_id.as_str(),
        ImPlatform::Wecom => identity.bot_id.as_str(),
        ImPlatform::WecomCallback => "",
    }
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
fn blank_flow(
    platform: ImPlatform,
    expires_at: i64,
    view: ImSetupSession,
    code: String,
    domain: FeishuDomain,
    interval: Duration,
    secret: Option<StagedSecret>,
) -> Arc<SetupFlow> {
    Arc::new(SetupFlow {
        platform,
        expires_at,
        inner: tokio::sync::Mutex::new(Flow {
            view,
            code,
            domain,
            interval,
            last_poll: None,
            secret,
        }),
        cancelled: Mutex::new(false),
    })
}
impl SetupFlows {
    pub(crate) fn new(secrets: credentials::CredentialStore) -> Self {
        Self {
            flows: Mutex::new(HashMap::new()),
            secrets,
            #[cfg(test)]
            remote: Mutex::new(VecDeque::new()),
        }
    }
    #[cfg(test)]
    fn push_remote(&self, value: Result<Value, String>) {
        self.remote.lock().push_back(value);
    }
    #[cfg(test)]
    fn seed_pending(&self, platform: ImPlatform, domain: FeishuDomain) -> String {
        self.seed(
            platform,
            domain,
            ImSetupState::Pending,
            None,
            None,
            chrono::Utc::now().timestamp() + 600,
        )
    }
    #[cfg(test)]
    fn seed_authorized(
        &self,
        platform: ImPlatform,
        identity: &str,
        secret: &str,
        expires_at: i64,
    ) -> String {
        let staged = match platform {
            ImPlatform::Feishu => ImSetupIdentity {
                app_id: identity.to_owned(),
                bot_id: String::new(),
                domain: FeishuDomain::Feishu,
                owner_id: String::new(),
                bot_name: String::new(),
            },
            _ => ImSetupIdentity {
                app_id: String::new(),
                bot_id: identity.to_owned(),
                domain: FeishuDomain::Feishu,
                owner_id: String::new(),
                bot_name: String::new(),
            },
        };
        self.seed(
            platform,
            FeishuDomain::Feishu,
            ImSetupState::Authorized,
            Some(staged),
            Some(secret.to_owned()),
            expires_at,
        )
    }
    #[cfg(test)]
    fn seed(
        &self,
        platform: ImPlatform,
        domain: FeishuDomain,
        status: ImSetupState,
        identity: Option<ImSetupIdentity>,
        secret: Option<String>,
        expires_at: i64,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let view = ImSetupSession {
            id: id.clone(),
            platform,
            url: "https://accounts.feishu.cn/oauth".into(),
            status,
            expires_at,
            message: String::new(),
            identity,
        };
        self.flows.lock().insert(
            id.clone(),
            blank_flow(
                platform,
                expires_at,
                view,
                "script".into(),
                domain,
                Duration::from_secs(3),
                secret.map(StagedSecret),
            ),
        );
        id
    }
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
        flows.retain(|_, flow| {
            if flow.platform == platform || flow.expires_at <= chrono::Utc::now().timestamp() {
                *flow.cancelled.lock() = true;
                return false;
            }
            true
        });
        if flows.len() >= 8 {
            return Err("扫码配置过多，请取消已有配置后重试".into());
        }
        flows.insert(
            view.id.clone(),
            blank_flow(
                platform,
                view.expires_at,
                view.clone(),
                code,
                domain,
                Duration::from_secs(interval),
                None,
            ),
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
        if state.view.status == ImSetupState::Completed {
            return Ok(state.view.clone());
        }
        if *flow.cancelled.lock() {
            state.secret = None;
            state.view.status = ImSetupState::Cancelled;
            state.view.message = "扫码配置已取消".into();
            return Ok(state.view.clone());
        }
        if state.view.status == ImSetupState::Authorized {
            return Ok(state.view.clone());
        }
        if state.view.status != ImSetupState::Pending {
            return Ok(state.view.clone());
        }
        if chrono::Utc::now().timestamp() >= state.view.expires_at {
            state.secret = None;
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
        let platform = state.view.platform;
        let domain = state.domain;
        let code = state.code.clone();
        let fetched = self.fetch(platform, domain, &code).await;
        let mut result = match fetched {
            Ok(value) => value,
            Err(message) => {
                state.view.message = message;
                return Ok(state.view.clone());
            }
        };
        let completed = absorb(platform, &mut state, &mut result);
        if let Some((identity, secret)) = completed {
            if *flow.cancelled.lock() {
                state.secret = None;
                state.view.status = ImSetupState::Cancelled;
                state.view.message = "扫码配置已取消".into();
            } else if chrono::Utc::now().timestamp() >= state.view.expires_at {
                state.secret = None;
                state.view.status = ImSetupState::Expired;
                state.view.message = "授权链接已过期，请重新开始".into();
            } else if state.view.status == ImSetupState::Pending {
                state.secret = Some(StagedSecret(secret));
                state.view.identity = Some(identity);
                state.view.status = ImSetupState::Authorized;
                state.view.message = "已完成授权，确认机器人身份后将保存凭证".into();
            }
        }
        Ok(state.view.clone())
    }
    pub async fn commit<C>(
        &self,
        id: &str,
        expected_identity: &str,
        canonical: C,
    ) -> Result<SetupCommit, String>
    where
        C: Fn(ImPlatform) -> String + Send + 'static,
    {
        let flow = self
            .flows
            .lock()
            .get(id)
            .cloned()
            .ok_or("扫码配置不存在或已被替换")?;
        let mut state = flow.inner.lock().await;
        if state.view.status == ImSetupState::Completed {
            return Ok(SetupCommit {
                session: state.view.clone(),
                reconnect: false,
            });
        }
        if *flow.cancelled.lock() {
            state.secret = None;
            state.view.status = ImSetupState::Cancelled;
            state.view.message = "扫码配置已取消".into();
            return Err("扫码配置已取消".into());
        }
        if chrono::Utc::now().timestamp() >= state.view.expires_at {
            state.secret = None;
            state.view.status = ImSetupState::Expired;
            state.view.message = "授权链接已过期，请重新开始".into();
            return Err("授权链接已过期，请重新开始".into());
        }
        if state.view.status != ImSetupState::Authorized {
            return Err("扫码尚未完成授权".into());
        }
        let platform = state.view.platform;
        let staged = state
            .view
            .identity
            .as_ref()
            .map(|item| canonical_identity(platform, item).to_owned())
            .unwrap_or_default();
        let Some(secret) = state.secret.clone() else {
            return Err("扫码尚未完成授权".into());
        };
        let current = canonical(platform);
        if expected_identity.is_empty()
            || current != expected_identity
            || expected_identity != staged
        {
            return Err("机器人身份与当前配置不一致，未保存密钥".into());
        }
        let store = self.secrets.clone();
        let guard = Arc::clone(&flow);
        let expected = expected_identity.to_owned();
        let identity = staged;
        let input = CredentialInput {
            secret: secret.0,
            ..Default::default()
        };
        let stored = credentials::off_runtime(move || {
            if *guard.cancelled.lock() {
                return Ok(false);
            }
            store.commit_secret(
                platform,
                &identity,
                &expected,
                input,
                || canonical(platform),
                &|| !*guard.cancelled.lock(),
            )
        })
        .await?;
        if *flow.cancelled.lock() || !stored {
            state.secret = None;
            state.view.status = ImSetupState::Cancelled;
            state.view.message = "扫码配置已取消".into();
            return Err("扫码配置已取消".into());
        }
        state.secret = None;
        state.view.status = ImSetupState::Completed;
        state.view.message = "机器人凭证已保存到本地 JSON".into();
        Ok(SetupCommit {
            session: state.view.clone(),
            reconnect: true,
        })
    }
    async fn fetch(
        &self,
        platform: ImPlatform,
        domain: FeishuDomain,
        code: &str,
    ) -> Result<Value, String> {
        #[cfg(test)]
        if let Some(scripted) = self.remote.lock().pop_front() {
            return scripted;
        }
        match platform {
            ImPlatform::Feishu => {
                registration(
                    domain,
                    &[("action", "poll"), ("device_code", code), ("tp", "ob_app")],
                )
                .await
            }
            ImPlatform::Wecom => http_client()?
                .get("https://work.weixin.qq.com/ai/qc/query_result")
                .query(&[("scode", code)])
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .map_err(|_| "扫码状态查询失败".to_string())?
                .json::<Value>()
                .await
                .map_err(|_| "扫码状态返回无效数据".to_string()),
            ImPlatform::WecomCallback => Err("自建应用不支持扫码创建".into()),
        }
    }
}
fn absorb(
    platform: ImPlatform,
    state: &mut Flow,
    result: &mut Value,
) -> Option<(ImSetupIdentity, String)> {
    match platform {
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
            let id = str_field(result, "client_id");
            let secret = take_string(result, "client_secret");
            if state.view.status == ImSetupState::Pending && !id.is_empty() && !secret.is_empty() {
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
            let status = result["data"]["status"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if !status.eq_ignore_ascii_case("success") {
                return None;
            }
            let id = result["data"]["bot_info"]["botid"]
                .as_str()
                .or_else(|| result["data"]["bot_info"]["bot_id"].as_str())
                .unwrap_or_default()
                .to_owned();
            let name = str_field(&result["data"]["bot_info"], "name");
            let secret = result
                .get_mut("data")
                .and_then(|data| data.get_mut("bot_info"))
                .map(|info| take_string(info, "secret"))
                .unwrap_or_default();
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
                        bot_name: name,
                    },
                    secret,
                ))
            }
        }
        ImPlatform::WecomCallback => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{credentials, types::*};
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    const KEPT: &str = "kept-secret-a";
    const STAGED: &str = "staged-secret-b-9f3a";

    fn input(secret: &str) -> CredentialInput {
        CredentialInput {
            secret: secret.to_owned(),
            ..Default::default()
        }
    }
    fn hidden(session: &ImSetupSession, secret: &str) {
        let debug = format!("{session:?}");
        let json = serde_json::to_string(session).unwrap();
        assert!(!debug.contains(secret), "{debug}");
        assert!(!json.contains(secret), "{json}");
    }
    fn feishu(id: &str, secret: &str) -> Value {
        serde_json::json!({
            "client_id": id,
            "client_secret": secret,
            "user_info": { "open_id": "ou_owner", "tenant_brand": "feishu" }
        })
    }
    fn owner_message() -> InboundMessage {
        InboundMessage {
            platform: ImPlatform::Feishu,
            message_id: "first-message".into(),
            chat_id: "owner-dm".into(),
            user_id: "ou_owner".into(),
            user_name: "Owner".into(),
            is_group: false,
            thread_id: None,
            reply_token: None,
            text: "hello".into(),
            attachments: Vec::new(),
            attachment_failures: Vec::new(),
        }
    }
    fn wecom(id: &str, secret: &str) -> Value {
        serde_json::json!({
            "data": {
                "status": "success",
                "bot_info": { "botid": id, "secret": secret, "name": "bot" }
            }
        })
    }
    struct BlockingVault {
        inner: Arc<credentials::MemoryVault>,
        arm: AtomicBool,
        entered: AtomicBool,
        release: AtomicBool,
    }
    impl credentials::SecretVault for BlockingVault {
        fn set_secret(
            &self,
            platform: ImPlatform,
            identity: &str,
            input: CredentialInput,
        ) -> Result<(), String> {
            if self.arm.load(Ordering::SeqCst) {
                self.entered.store(true, Ordering::SeqCst);
                while !self.release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            self.inner.set_secret(platform, identity, input)
        }
        fn get_secret(
            &self,
            platform: ImPlatform,
            identity: &str,
        ) -> Result<Option<CredentialInput>, String> {
            self.inner.get_secret(platform, identity)
        }
        fn delete_secret(&self, platform: ImPlatform, identity: &str) -> Result<(), String> {
            self.inner.delete_secret(platform, identity)
        }
    }

    #[tokio::test]
    async fn poll_and_cancel_keep_the_existing_secret() {
        let (store, vault) = credentials::CredentialStore::memory();
        store
            .save(ImPlatform::Feishu, "app-a", input(KEPT))
            .unwrap();
        let sets = vault.set_count();
        let flows = SetupFlows::new(store.clone());
        let id = flows.seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        flows.push_remote(Ok(feishu("app-b", STAGED)));
        let session = flows.poll(&id).await.unwrap();
        assert_eq!(session.status, ImSetupState::Authorized);
        assert_eq!(session.identity.as_ref().unwrap().app_id, "app-b");
        assert_eq!(
            serde_json::to_value(&session).unwrap()["status"],
            "authorized"
        );
        hidden(&session, STAGED);
        assert_eq!(vault.set_count(), sets);
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            KEPT
        );
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());

        flows.cancel(&id).unwrap();
        let session = flows.poll(&id).await.unwrap();
        assert_eq!(session.status, ImSetupState::Cancelled);
        hidden(&session, STAGED);
        assert_eq!(vault.set_count(), sets);
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            KEPT
        );
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
    }

    #[tokio::test]
    async fn committed_scan_pairs_its_owner_without_approval_and_preserves_other_access_boundaries()
    {
        let dir = tempfile::tempdir().unwrap();
        let runtime = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        runtime
            .credentials
            .save(ImPlatform::Wecom, "bot-kept", input(KEPT))
            .unwrap();
        let mut config = ImConfig::default();
        config.feishu.app_id = "app-new".into();
        let mut owner = owner_message();
        let mut stranger = owner.clone();
        stranger.user_id = "ou_stranger".into();
        runtime.pair(&config, &owner).unwrap();
        runtime.pair(&config, &stranger).unwrap();
        let id = runtime
            .setup
            .seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        runtime.setup.push_remote(Ok(feishu("app-new", STAGED)));
        let authorized = runtime.setup.poll(&id).await.unwrap();
        assert_eq!(authorized.status, ImSetupState::Authorized);
        assert!(!runtime.authorize(&config, &owner));
        let outcome = runtime
            .setup
            .commit(&id, "app-new", |_| "app-new".into())
            .await
            .unwrap();
        let completed = activate_commit(&runtime, outcome).unwrap();
        assert_eq!(completed.status, ImSetupState::Completed);
        assert!(runtime.authorize(&config, &owner));
        assert!(!runtime.authorize(&config, &stranger));
        assert_eq!(
            runtime
                .state
                .lock()
                .store
                .pending
                .iter()
                .map(|pair| pair.request.user_id.as_str())
                .collect::<Vec<_>>(),
            ["ou_stranger"]
        );
        let reloaded = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        assert!(reloaded.authorize(&config, &owner));
        assert_eq!(
            reloaded
                .credentials
                .load(ImPlatform::Feishu, "app-new")
                .unwrap()
                .secret,
            STAGED
        );
        assert_eq!(
            reloaded
                .credentials
                .load(ImPlatform::Wecom, "bot-kept")
                .unwrap()
                .secret,
            KEPT
        );
        config.feishu.app_id = "another-app".into();
        assert!(!reloaded.authorize(&config, &owner));
        config.feishu.app_id = "app-new".into();
        owner.is_group = true;
        assert!(!reloaded.authorize(&config, &owner));
    }

    #[tokio::test]
    async fn owner_pairing_write_failure_denies_access_and_retries_the_committed_scan() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = credentials::CredentialStore::memory();
        let mut runtime = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        runtime.setup = SetupFlows::new(store);
        let mut config = ImConfig::default();
        config.feishu.app_id = "app-new".into();
        let id = runtime
            .setup
            .seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        runtime.setup.push_remote(Ok(feishu("app-new", STAGED)));
        runtime.setup.poll(&id).await.unwrap();
        let outcome = runtime
            .setup
            .commit(&id, "app-new", |_| "app-new".into())
            .await
            .unwrap();
        let state_path = dir.path().join("state.json");
        std::fs::create_dir(&state_path).unwrap();
        assert!(activate_commit(&runtime, outcome).is_err());
        assert!(!runtime.authorize(&config, &owner_message()));
        std::fs::remove_dir(&state_path).unwrap();
        let retry = runtime
            .setup
            .commit(&id, "app-new", |_| "app-new".into())
            .await
            .unwrap();
        activate_commit(&runtime, retry).unwrap();
        assert!(runtime.authorize(&config, &owner_message()));
        let reloaded = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        assert!(reloaded.authorize(&config, &owner_message()));
    }

    #[tokio::test]
    async fn scan_without_authenticated_owner_does_not_trust_incoming_senders() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = credentials::CredentialStore::memory();
        let mut runtime = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        runtime.setup = SetupFlows::new(store);
        let mut config = ImConfig::default();
        config.feishu.app_id = "app-new".into();
        let id = runtime
            .setup
            .seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        let mut remote = feishu("app-new", STAGED);
        remote["user_info"] = serde_json::json!({});
        runtime.setup.push_remote(Ok(remote));
        runtime.setup.poll(&id).await.unwrap();
        let outcome = runtime
            .setup
            .commit(&id, "app-new", |_| "app-new".into())
            .await
            .unwrap();
        activate_commit(&runtime, outcome).unwrap();
        let owner = owner_message();
        assert!(!runtime.authorize(&config, &owner));
        runtime.pair(&config, &owner).unwrap();
        assert!(!runtime.authorize(&config, &owner));
    }

    #[tokio::test]
    async fn commit_accepts_only_the_staged_identity_and_reconnects_once() {
        let (store, vault) = credentials::CredentialStore::memory();
        store
            .save(ImPlatform::Feishu, "app-a", input(KEPT))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut runtime = super::super::ImRuntime::load(dir.path().to_owned()).unwrap();
        runtime.setup = SetupFlows::new(store.clone());
        let id = runtime
            .setup
            .seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        runtime.setup.push_remote(Ok(feishu("app-b", STAGED)));
        let session = runtime.setup.poll(&id).await.unwrap();
        assert_eq!(session.status, ImSetupState::Authorized);
        let sets = vault.set_count();

        let err = runtime
            .setup
            .commit(&id, "app-b", |_| "app-a".into())
            .await
            .unwrap_err();
        assert!(!err.contains(STAGED));
        let err = runtime
            .setup
            .commit(&id, "app-a", |_| "app-a".into())
            .await
            .unwrap_err();
        assert!(!err.contains(STAGED));
        assert_eq!(vault.set_count(), sets);
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            KEPT
        );
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
        assert_eq!(
            runtime
                .state
                .lock()
                .revisions
                .get(&ImPlatform::Feishu)
                .copied()
                .unwrap_or(0),
            0
        );

        let outcome = runtime
            .setup
            .commit(&id, "app-b", |_| "app-b".into())
            .await
            .unwrap();
        assert!(outcome.reconnect);
        let session = activate_commit(&runtime, outcome).unwrap();
        assert_eq!(session.status, ImSetupState::Completed);
        hidden(&session, STAGED);
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-b").unwrap().secret,
            STAGED
        );
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            KEPT
        );
        assert_eq!(
            runtime
                .state
                .lock()
                .revisions
                .get(&ImPlatform::Feishu)
                .copied()
                .unwrap_or(0),
            1
        );

        let outcome = runtime
            .setup
            .commit(&id, "app-b", |_| "app-b".into())
            .await
            .unwrap();
        assert!(!outcome.reconnect);
        activate_commit(&runtime, outcome).unwrap();
        assert_eq!(
            runtime
                .state
                .lock()
                .revisions
                .get(&ImPlatform::Feishu)
                .copied()
                .unwrap_or(0),
            1
        );
        assert_eq!(vault.set_count(), sets + 1);
        let session = runtime.setup.poll(&id).await.unwrap();
        assert_eq!(session.status, ImSetupState::Completed);
        hidden(&session, STAGED);
    }

    #[tokio::test]
    async fn wecom_commit_uses_bot_id_and_expired_commit_does_not_store() {
        let (store, _) = credentials::CredentialStore::memory();
        store
            .save(ImPlatform::Wecom, "bot-old", input(KEPT))
            .unwrap();
        let flows = SetupFlows::new(store.clone());
        let id = flows.seed_pending(ImPlatform::Wecom, FeishuDomain::Feishu);
        flows.push_remote(Ok(wecom("bot-new", STAGED)));
        let session = flows.poll(&id).await.unwrap();
        assert_eq!(session.identity.as_ref().unwrap().bot_id, "bot-new");
        hidden(&session, STAGED);
        let outcome = flows
            .commit(&id, "bot-new", |_| "bot-new".into())
            .await
            .unwrap();
        assert!(outcome.reconnect);
        assert_eq!(
            store.load(ImPlatform::Wecom, "bot-new").unwrap().secret,
            STAGED
        );
        assert_eq!(
            store.load(ImPlatform::Wecom, "bot-old").unwrap().secret,
            KEPT
        );

        let expired = flows.seed_authorized(
            ImPlatform::Feishu,
            "app-b",
            STAGED,
            chrono::Utc::now().timestamp() - 30,
        );
        let err = flows
            .commit(&expired, "app-b", |_| "app-b".into())
            .await
            .unwrap_err();
        assert!(!err.contains(STAGED));
        assert!(store.load(ImPlatform::Feishu, "app-b").is_err());
        let pending = flows.seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        let err = flows
            .commit(&pending, "app-b", |_| "app-b".into())
            .await
            .unwrap_err();
        assert!(!err.contains(STAGED));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocking_credential_write_does_not_stall_cancel() {
        let inner = Arc::new(credentials::MemoryVault::default());
        let blocking = Arc::new(BlockingVault {
            inner: inner.clone(),
            arm: AtomicBool::new(false),
            entered: AtomicBool::new(false),
            release: AtomicBool::new(false),
        });
        let store = credentials::CredentialStore::from_vault(blocking.clone());
        store
            .save(ImPlatform::Feishu, "app-a", input(KEPT))
            .unwrap();
        let flows = Arc::new(SetupFlows::new(store.clone()));
        let id = flows.seed_pending(ImPlatform::Feishu, FeishuDomain::Feishu);
        flows.push_remote(Ok(feishu("app-b", STAGED)));
        let session = tokio::time::timeout(Duration::from_secs(2), flows.poll(&id))
            .await
            .expect("poll blocked on credential storage")
            .unwrap();
        assert_eq!(session.status, ImSetupState::Authorized);
        assert!(!blocking.entered.load(Ordering::SeqCst));
        hidden(&session, STAGED);
        blocking.arm.store(true, Ordering::SeqCst);

        let flows_commit = Arc::clone(&flows);
        let commit_id = id.clone();
        let entered = Arc::clone(&blocking);
        let commit = tokio::spawn(async move {
            flows_commit
                .commit(&commit_id, "app-b", |_| "app-b".into())
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !entered.entered.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("commit did not reach credential storage");

        let flows_cancel = Arc::clone(&flows);
        let cancel_id = id.clone();
        let cancel = tokio::spawn(async move { flows_cancel.cancel(&cancel_id).unwrap() });
        tokio::time::timeout(Duration::from_secs(2), cancel)
            .await
            .expect("cancel stalled behind credential storage")
            .unwrap();
        blocking.release.store(true, Ordering::SeqCst);
        let err = tokio::time::timeout(Duration::from_secs(2), commit)
            .await
            .expect("commit stalled")
            .unwrap()
            .unwrap_err();
        assert!(!err.contains(STAGED));
        let session = flows.poll(&id).await.unwrap();
        assert_eq!(session.status, ImSetupState::Cancelled);
        hidden(&session, STAGED);
        assert_eq!(
            store.load(ImPlatform::Feishu, "app-a").unwrap().secret,
            KEPT
        );
    }
}
