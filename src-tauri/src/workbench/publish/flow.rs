//! One publish implementation: a record per account, no re-upload after the platform has the bytes.
use super::store::{Session, Store};
use super::tiktok::{self, AuthFail, RefreshFail, StepFail};
use super::transport::json_number;
use super::youtube;
use super::{
    AccountStatus, ContentPlatform, Credential, Privacy, PublishAccount, PublishBlock, PublishError, PublishRecord,
    PublishRecordFilter, PublishRequest, PublishStatus, PublishSubmitResult, StatKey, VideoStats,
};
use super::transport::Transport;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) trait Vault: Send + Sync {
    fn load(&self, id: &str) -> Result<Credential, PublishError>;
    fn save(&self, id: &str, value: &Credential) -> Result<(), PublishError>;
    fn delete(&self, id: &str) -> Result<(), PublishError>;
}

pub(crate) struct KeyringVault;
impl Vault for KeyringVault {
    fn load(&self, id: &str) -> Result<Credential, PublishError> {
        let encoded = entry(id)?.get_password().map_err(|_| PublishError::invalid("发布账号凭据已丢失，请重新授权"))?;
        serde_json::from_str(&encoded).map_err(|_| PublishError::invalid("发布账号凭据损坏，请重新授权"))
    }
    fn save(&self, id: &str, value: &Credential) -> Result<(), PublishError> {
        let encoded = serde_json::to_string(value).map_err(|_| PublishError::internal("发布账号凭据编码失败"))?;
        entry(id)?.set_password(&encoded).map_err(|_| PublishError::internal("发布账号凭据保存到系统凭据库失败"))
    }
    fn delete(&self, id: &str) -> Result<(), PublishError> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(PublishError::internal("无法清除系统凭据库中的发布账号凭据")),
        }
    }
}

fn entry(id: &str) -> Result<keyring::Entry, PublishError> {
    uuid::Uuid::parse_str(id).map_err(|_| PublishError::invalid("发布账号凭据引用无效"))?;
    keyring::Entry::new("Dsivio.PublishAccount", id).map_err(|_| PublishError::internal("系统凭据库不可用"))
}

#[cfg(test)]
pub(crate) struct MemoryVault {
    inner: std::sync::Mutex<std::collections::HashMap<String, Credential>>,
}
#[cfg(test)]
impl MemoryVault {
    pub(crate) fn new() -> Self {
        Self { inner: std::sync::Mutex::new(std::collections::HashMap::new()) }
    }
}
#[cfg(test)]
impl Vault for MemoryVault {
    fn load(&self, id: &str) -> Result<Credential, PublishError> {
        self.inner.lock().expect("vault").get(id).cloned().ok_or_else(|| PublishError::invalid("发布账号凭据已丢失，请重新授权"))
    }
    fn save(&self, id: &str, value: &Credential) -> Result<(), PublishError> {
        self.inner.lock().expect("vault").insert(id.to_string(), value.clone());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), PublishError> {
        self.inner.lock().expect("vault").remove(id);
        Ok(())
    }
}

struct Outcome {
    status: PublishStatus,
    remote_id: Option<String>,
    url: Option<String>,
    reason: Option<String>,
    reauth: bool,
}
impl Outcome {
    fn status(status: PublishStatus, reason: impl Into<String>, reauth: bool) -> Self {
        Self { status, remote_id: None, url: None, reason: Some(reason.into()), reauth }
    }
    fn failed(reason: impl Into<String>) -> Self {
        Self::status(PublishStatus::Failed, reason, false)
    }
}

fn reason_needs_reauth(reason: &str) -> bool {
    reason.contains("重新授权") || reason.contains("凭据") || reason.contains("access_token")
}

pub(crate) struct Service<V: Vault> {
    pub db_path: PathBuf,
    pub transport: Arc<dyn Transport>,
    pub vault: V,
}
impl<V: Vault> Service<V> {
    fn store(&self) -> Result<Store, PublishError> {
        Store::open_at(&self.db_path)
    }
    pub(crate) fn save(&self, record: &PublishRecord, session: &Session) -> Result<(), PublishError> {
        self.store()?.save_record(record, session)
    }

    pub(crate) fn put_account(&self, account: &PublishAccount) -> Result<(), PublishError> {
        self.store()?.put_account(account)
    }
    pub(crate) fn list_accounts(&self) -> Result<Vec<PublishAccount>, PublishError> {
        self.store()?.list_accounts()
    }
    pub(crate) fn existing_account(&self, platform: ContentPlatform, remote_id: &str) -> Result<Option<String>, PublishError> {
        self.store()?.existing_account(platform, remote_id)
    }
    pub(crate) fn get_account(&self, id: &str) -> Result<PublishAccount, PublishError> {
        self.store()?.get_account(id)
    }
    pub(crate) fn delete_account(&self, id: &str) -> Result<(), PublishError> {
        self.store()?.delete_account(id)
    }
    pub(crate) fn list_records(&self, filter: &PublishRecordFilter) -> Result<Vec<PublishRecord>, PublishError> {
        self.store()?.list_records(filter.account_id.as_deref(), filter.status)
    }

    pub(crate) async fn submit(&self, request: PublishRequest, origin: &str) -> Result<PublishSubmitResult, PublishError> {
        let request = validate(request)?;
        let mut seen = Vec::new();
        let mut account_ids = Vec::new();
        for id in request.account_ids {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id.clone());
            account_ids.push(id);
        }
        let mut accounts = Vec::new();
        for id in &account_ids {
            let account = self.get_account(id)?;
            ensure_target(account.platform, request.privacy, &request.video_path)?;
            accounts.push(account);
        }
        let group_id = uuid::Uuid::new_v4().to_string();
        let mut records = Vec::new();
        let mut blocked = Vec::new();
        for account in accounts {
            let account_id = account.id.clone();
            if account.status != AccountStatus::Connected {
                blocked.push(PublishBlock {
                    account_id,
                    status: PublishStatus::Failed,
                    detail: "账号未连接，请先重新授权".into(),
                });
                continue;
            }
            if let Some((_, status)) = self.store()?.blocking(&account_id, &request.video_path)? {
                blocked.push(PublishBlock {
                    account_id,
                    status,
                    detail: "该账号这条视频已在上传、处理、已发布或结果不确定，不会重新上传".into(),
                });
                continue;
            }
            let now = now();
            let mut record = PublishRecord {
                id: uuid::Uuid::new_v4().to_string(),
                group_id: group_id.clone(),
                account_id: account_id.clone(),
                platform: account.platform,
                video_path: request.video_path.clone(),
                title: request.title.clone(),
                description: request.description.clone(),
                privacy: request.privacy,
                tags: request.tags.clone(),
                status: PublishStatus::Uploading,
                remote_id: None,
                url: None,
                reason: None,
                attempts: 1,
                created_at: now.clone(),
                updated_at: now,
                origin: origin.to_string(),
                media_task_id: request.media_task_id.clone(),
            };
            let mut session = Session::default();
            self.save(&record, &session)?;
            self.run_upload(&account, &mut record, &mut session).await?;
            records.push(record);
        }
        Ok(PublishSubmitResult { records, blocked })
    }

    pub(crate) async fn retry(&self, id: &str) -> Result<PublishRecord, PublishError> {
        let stored = self.store()?.get_record(id)?;
        let mut record = stored.record;
        if !record.status.can_retry() {
            return Err(PublishError::invalid("这条记录不能重新上传，只能查询平台状态"));
        }
        if self.store()?.blocking(&record.account_id, &record.video_path)?.is_some() {
            return Err(PublishError::invalid("该账号已有进行中或已发布的记录，不能重新上传"));
        }
        let account = self.get_account(&record.account_id)?;
        ensure_target(account.platform, record.privacy, &record.video_path)?;
        record.attempts = record.attempts.saturating_add(1);
        record.status = PublishStatus::Uploading;
        record.reason = None;
        record.remote_id = None;
        record.url = None;
        record.updated_at = now();
        let mut session = Session::default();
        self.save(&record, &session)?;
        self.run_upload(&account, &mut record, &mut session).await?;
        Ok(record)
    }

    pub(crate) async fn refresh_record(&self, id: &str) -> Result<PublishRecord, PublishError> {
        let stored = self.store()?.get_record(id)?;
        let mut record = stored.record;
        let mut session = stored.session;
        let account = self.get_account(&record.account_id)?;
        self.run_query(&account, &mut record, &mut session).await?;
        Ok(record)
    }

    pub(crate) async fn stats(&self, id: &str) -> Result<VideoStats, PublishError> {
        let stored = self.store()?.get_record(id)?;
        let record = stored.record;
        let Some(remote_id) = record.remote_id.clone() else {
            return Ok(VideoStats::unsupported_all(record.id, Some("平台还没有这条视频的编号".into())));
        };
        let mut cred = self.vault.load(&record.account_id)?;
        if let Err(error) = ensure_token(&self.transport, record.platform, &mut cred).await {
            let _ = self.vault.save(&record.account_id, &cred);
            return Ok(VideoStats::unsupported_all(record.id, Some(error)));
        }
        self.vault.save(&record.account_id, &cred)?;
        let values = match record.platform {
            ContentPlatform::Tiktok => match tiktok::video_stats(&self.transport, &cred.access_token, &remote_id).await {
                Ok(video) => collect(
                    &video,
                    &[
                        (StatKey::Views, "view_count"),
                        (StatKey::Likes, "like_count"),
                        (StatKey::Comments, "comment_count"),
                        (StatKey::Shares, "share_count"),
                    ],
                ),
                Err(error) => return Ok(VideoStats::unsupported_all(record.id, Some(error))),
            },
            ContentPlatform::Youtube => match youtube::video_list(&self.transport, &cred.access_token, &remote_id).await {
                Ok(body) => {
                    let item = body["items"].as_array().and_then(|items| items.first()).cloned().unwrap_or(Value::Null);
                    collect(
                        &item["statistics"],
                        &[
                            (StatKey::Views, "viewCount"),
                            (StatKey::Likes, "likeCount"),
                            (StatKey::Comments, "commentCount"),
                        ],
                    )
                }
                Err(StepFail::Transport(error) | StepFail::Definite(_, error)) => {
                    return Ok(VideoStats::unsupported_all(record.id, Some(error)));
                }
            },
        };
        Ok(VideoStats::from_values(record.id, values, None))
    }

    pub(crate) async fn refresh_account(&self, id: &str) -> Result<PublishAccount, PublishError> {
        let mut account = self.get_account(id)?;
        let mut cred = match self.vault.load(&account.id) {
            Ok(cred) => cred,
            Err(error) => {
                account.status = AccountStatus::NeedsReauthorization;
                account.detail = Some(error.to_string());
                account.checked_at = now();
                self.put_account(&account)?;
                return Ok(account);
            }
        };
        let refreshed = match account.platform {
            ContentPlatform::Tiktok => tiktok::refresh_if_needed(&self.transport, &mut cred).await,
            ContentPlatform::Youtube => ensure_youtube(&mut cred, &self.transport).await,
        };
        if let Err(fail) = refreshed {
            return self.finish_account(account, cred, fail).await;
        }
        let profile = match account.platform {
            ContentPlatform::Tiktok => tiktok::creator_info(&self.transport, &cred.access_token).await,
            ContentPlatform::Youtube => youtube::channel(&self.transport, &cred.access_token).await.map(|(remote, name, fans)| {
                account.remote_id = remote;
                (name, fans)
            }),
        };
        match profile {
            Ok((name, fans)) => {
                account.name = name;
                account.fans = fans;
                account.status = AccountStatus::Connected;
                account.detail = None;
            }
            Err(fail) => {
                account.status = match &fail {
                    AuthFail::Rejected(_) => AccountStatus::NeedsReauthorization,
                    AuthFail::Failed(_) | AuthFail::Transport(_) => AccountStatus::Error,
                };
                account.detail = Some(fail.message().to_string());
            }
        }
        account.checked_at = now();
        self.vault.save(&account.id, &cred)?;
        self.put_account(&account)?;
        Ok(account)
    }

    async fn finish_account(&self, mut account: PublishAccount, cred: Credential, fail: RefreshFail) -> Result<PublishAccount, PublishError> {
        match fail {
            RefreshFail::Reauth(message) => {
                account.status = AccountStatus::NeedsReauthorization;
                account.detail = Some(message);
            }
            RefreshFail::Transient(message) => {
                account.status = AccountStatus::Error;
                account.detail = Some(message);
            }
        }
        account.checked_at = now();
        self.vault.save(&account.id, &cred)?;
        self.put_account(&account)?;
        Ok(account)
    }

    async fn run_upload(&self, account: &PublishAccount, record: &mut PublishRecord, session: &mut Session) -> Result<(), PublishError> {
        let mut cred = match self.vault.load(&account.id) {
            Ok(cred) => cred,
            Err(error) => {
                apply(record, &Outcome::failed(error.to_string()));
                self.save(record, session)?;
                return Ok(());
            }
        };
        let outcome = match account.platform {
            ContentPlatform::Tiktok => self.upload_tiktok(&mut cred, record, session).await,
            ContentPlatform::Youtube => self.upload_youtube(&mut cred, record, session).await,
        };
        self.vault.save(&account.id, &cred)?;
        apply(record, &outcome);
        self.save(record, session)?;
        if outcome.reauth {
            let mut account = account.clone();
            account.status = AccountStatus::NeedsReauthorization;
            account.detail = outcome.reason.clone();
            account.checked_at = now();
            self.put_account(&account)?;
        }
        Ok(())
    }

    async fn run_query(&self, account: &PublishAccount, record: &mut PublishRecord, session: &mut Session) -> Result<(), PublishError> {
        let mut cred = match self.vault.load(&account.id) {
            Ok(cred) => cred,
            Err(error) => {
                if record.status != PublishStatus::Published {
                    apply(record, &uncertain(session, record, error.to_string()));
                    self.save(record, session)?;
                }
                return Ok(());
            }
        };
        let outcome = match account.platform {
            ContentPlatform::Tiktok => self.query_tiktok(&mut cred, record, session).await,
            ContentPlatform::Youtube => self.query_youtube(&mut cred, record, session).await,
        };
        self.vault.save(&account.id, &cred)?;
        if record.status == PublishStatus::Published && matches!(outcome.status, PublishStatus::Uncertain | PublishStatus::Failed) {
            record.updated_at = now();
            record.reason = outcome.reason.clone();
        } else {
            apply(record, &outcome);
        }
        self.save(record, session)?;
        Ok(())
    }

    async fn upload_tiktok(&self, cred: &mut Credential, record: &mut PublishRecord, session: &mut Session) -> Outcome {
        if let Err(error) = ensure_target(ContentPlatform::Tiktok, record.privacy, &record.video_path) {
            return Outcome::failed(error.to_string());
        }
        if let Err(fail) = tiktok::refresh_if_needed(&self.transport, cred).await {
            return refresh_outcome(fail);
        }
        let size = match file_size(&record.video_path) {
            Ok(size) => size,
            Err(error) => return Outcome::failed(error.to_string()),
        };
        let ranges = match tiktok::plan_chunks(size) {
            Ok(ranges) => ranges,
            Err(error) => return Outcome::failed(error),
        };
        let started = match tiktok::init_upload(
            &self.transport,
            &cred.access_token,
            &record.title,
            &record.description,
            &record.tags,
            record.privacy,
            size,
            &ranges,
        )
        .await
        {
            Ok(started) => started,
            Err(StepFail::Transport(error)) => return Outcome::failed(error),
            Err(StepFail::Definite(status, reason)) => return definite(status, reason),
        };
        session.handle = Some(started.publish_id.clone());
        session.session_url = Some(started.upload_url.clone());
        record.remote_id = Some(started.publish_id.clone());
        record.status = PublishStatus::Uploading;
        record.updated_at = now();
        if let Err(error) = self.save(record, session) {
            return uncertain(session, record, error.to_string());
        }
        let mime = tiktok::video_mime(&record.video_path);
        for (start, end) in ranges {
            let bytes = match tiktok::read_range(&record.video_path, start, end) {
                Ok(bytes) => bytes,
                Err(error) => return uncertain(session, record, error),
            };
            match tiktok::put_chunk(&self.transport, &started.upload_url, mime, start, end, size, bytes).await {
                Ok(()) => {}
                Err(StepFail::Transport(error)) => return uncertain(session, record, error),
                Err(StepFail::Definite(status, reason)) => return definite(status, reason),
            }
        }
        self.finish_tiktok(cred, record, session, &started.publish_id).await
    }

    async fn finish_tiktok(&self, cred: &mut Credential, record: &PublishRecord, session: &Session, publish_id: &str) -> Outcome {
        match tiktok::fetch_status(&self.transport, &cred.access_token, publish_id).await {
            Ok(snapshot) => {
                let mut remote_id = snapshot.public_id.clone().or_else(|| Some(publish_id.to_string()));
                let mut url = None;
                if snapshot.status == PublishStatus::Published {
                    if let Some(public_id) = snapshot.public_id.clone() {
                        remote_id = Some(public_id.clone());
                        match tiktok::video_share_url(&self.transport, &cred.access_token, &public_id).await {
                            Ok(share) => url = share,
                            Err(_) => {}
                        }
                    }
                }
                Outcome {
                    status: snapshot.status,
                    remote_id,
                    url,
                    reason: snapshot.reason,
                    reauth: false,
                }
            }
            Err(StepFail::Transport(error)) => uncertain(session, record, error),
            Err(StepFail::Definite(status, reason)) => {
                if status == PublishStatus::Uncertain {
                    uncertain(session, record, reason)
                } else {
                    definite(status, reason)
                }
            }
        }
    }

    async fn query_tiktok(&self, cred: &mut Credential, record: &PublishRecord, session: &Session) -> Outcome {
        if let Err(fail) = tiktok::refresh_if_needed(&self.transport, cred).await {
            return if record.status == PublishStatus::Published {
                keep_published(record, refresh_text(&fail))
            } else {
                uncertain(session, record, refresh_text(&fail))
            };
        }
        let Some(publish_id) = session.handle.clone().or_else(|| record.remote_id.clone()) else {
            return if record.status.blocks_upload() {
                uncertain(session, record, "没有可查询的 TikTok 发布编号")
            } else {
                Outcome::failed("没有可查询的 TikTok 发布编号")
            };
        };
        self.finish_tiktok(cred, record, session, &publish_id).await
    }

    async fn upload_youtube(&self, cred: &mut Credential, record: &mut PublishRecord, session: &mut Session) -> Outcome {
        if let Err(error) = ensure_target(ContentPlatform::Youtube, record.privacy, &record.video_path) {
            return Outcome::failed(error.to_string());
        }
        if let Err(fail) = ensure_youtube(cred, &self.transport).await {
            return refresh_outcome(fail);
        }
        let size = match file_size(&record.video_path) {
            Ok(size) => size,
            Err(error) => return Outcome::failed(error.to_string()),
        };
        let mime = tiktok::video_mime(&record.video_path);
        let location = match youtube::start_session(
            &self.transport,
            &cred.access_token,
            &record.title,
            &record.description,
            &record.tags,
            record.privacy,
            mime,
            size,
        )
        .await
        {
            Ok(location) => location,
            Err(StepFail::Transport(error)) => return Outcome::failed(error),
            Err(StepFail::Definite(status, reason)) => return definite(status, reason),
        };
        session.session_url = Some(location.clone());
        record.status = PublishStatus::Uploading;
        record.updated_at = now();
        if let Err(error) = self.save(record, session) {
            return uncertain(session, record, error.to_string());
        }
        const CHUNK: u64 = 8 * 1024 * 1024;
        let mut offset = 0u64;
        let mut uploaded = None;
        while offset < size {
            let end = (offset + CHUNK - 1).min(size - 1);
            let bytes = match tiktok::read_range(&record.video_path, offset, end) {
                Ok(bytes) => bytes,
                Err(error) => return uncertain(session, record, error),
            };
            match youtube::upload_bytes(&self.transport, &cred.access_token, &location, mime, offset, end, size, bytes).await {
                Ok(Some(body)) => {
                    uploaded = Some(body);
                    break;
                }
                Ok(None) => offset = end + 1,
                Err(StepFail::Transport(error)) => return uncertain(session, record, error),
                Err(StepFail::Definite(status, reason)) => return definite(status, reason),
            }
        }
        let Some(body) = uploaded else {
            return uncertain(session, record, "YouTube 没有确认上传完成");
        };
        self.apply_youtube_body(cred, record, session, &body).await
    }

    async fn apply_youtube_body(&self, cred: &mut Credential, record: &PublishRecord, session: &Session, body: &Value) -> Outcome {
        let video_id = body["id"].as_str().filter(|value| !value.is_empty()).map(str::to_string);
        if let Some(id) = &video_id {
            return self.query_youtube_id(cred, record, session, id).await;
        }
        let (status, reason) = youtube::map_video(body);
        Outcome {
            status,
            remote_id: video_id,
            url: None,
            reason,
            reauth: false,
        }
    }

    async fn query_youtube_id(&self, cred: &Credential, record: &PublishRecord, session: &Session, video_id: &str) -> Outcome {
        match youtube::video_list(&self.transport, &cred.access_token, video_id).await {
            Ok(body) => {
                let Some(item) = body["items"].as_array().and_then(|items| items.first()) else {
                    return uncertain(session, record, "YouTube 没有返回视频状态");
                };
                let (status, reason) = youtube::map_video(item);
                Outcome {
                    status,
                    remote_id: Some(video_id.to_string()),
                    url: Some(youtube::watch_url(video_id)),
                    reason,
                    reauth: false,
                }
            }
            Err(StepFail::Transport(error)) => uncertain_with_id(session, record, video_id, error),
            Err(StepFail::Definite(status, reason)) => {
                let reauth = status == PublishStatus::Rejected && reason_needs_reauth(&reason);
                if status == PublishStatus::Uncertain || (record.status == PublishStatus::Published && status != PublishStatus::Published) {
                    uncertain_with_id(session, record, video_id, reason)
                } else {
                    Outcome {
                        status,
                        remote_id: Some(video_id.to_string()),
                        url: Some(youtube::watch_url(video_id)),
                        reason: Some(reason),
                        reauth,
                    }
                }
            }
        }
    }

    async fn query_youtube(&self, cred: &mut Credential, record: &mut PublishRecord, session: &mut Session) -> Outcome {
        if let Err(fail) = ensure_youtube(cred, &self.transport).await {
            return if record.status == PublishStatus::Published {
                keep_published(record, refresh_text(&fail))
            } else {
                uncertain(session, record, refresh_text(&fail))
            };
        }
        if let Some(video_id) = record.remote_id.clone().or_else(|| session.handle.clone()) {
            return self.query_youtube_id(cred, record, session, &video_id).await;
        }
        let Some(location) = session.session_url.clone() else {
            return if record.status.blocks_upload() {
                uncertain(session, record, "没有可查询的 YouTube 上传会话")
            } else {
                Outcome::failed("没有可查询的 YouTube 上传会话")
            };
        };
        let size = match file_size(&record.video_path) {
            Ok(size) => size,
            Err(error) => return uncertain(session, record, error.to_string()),
        };
        match youtube::probe_session(&self.transport, &cred.access_token, &location, size).await {
            Ok(Some(body)) => self.apply_youtube_body(cred, record, session, &body).await,
            Ok(None) => uncertain(session, record, "YouTube 仍未确认上传结果"),
            Err(StepFail::Transport(error)) => uncertain(session, record, error),
            Err(StepFail::Definite(_, reason)) => uncertain(session, record, reason),
        }
    }
}

fn validate(mut request: PublishRequest) -> Result<PublishRequest, PublishError> {
    request.title = request.title.trim().to_string();
    request.description = request.description.trim().to_string();
    request.video_path = request.video_path.trim().to_string();
    request.tags = request
        .tags
        .iter()
        .map(|tag| tag.trim().trim_start_matches('#').to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    if request.account_ids.is_empty() {
        return Err(PublishError::invalid("请选择至少一个账号"));
    }
    if request.title.is_empty() {
        return Err(PublishError::invalid("请填写标题"));
    }
    if request.title.chars().count() > 2200 {
        return Err(PublishError::invalid("标题过长"));
    }
    if request.description.chars().count() > 5000 {
        return Err(PublishError::invalid("简介过长"));
    }
    if request.tags.len() > 30 || request.tags.iter().any(|tag| tag.chars().count() > 100) {
        return Err(PublishError::invalid("标签无效"));
    }
    Ok(request)
}

/// Privacy and documented file limits, before a record is written or bytes are sent.
fn ensure_target(platform: ContentPlatform, privacy: Privacy, path: &str) -> Result<(), PublishError> {
    let size = file_size(path)?;
    match platform {
        ContentPlatform::Tiktok => {
            tiktok::privacy_level(privacy)?;
            if !tiktok::allowed_container(path) {
                return Err(PublishError::invalid("TikTok 只接受 MP4、WebM 或 MOV"));
            }
            if size > tiktok::MAX_VIDEO_BYTES {
                return Err(PublishError::invalid("视频超过 TikTok 文档规定的 4GB 上限"));
            }
        }
        ContentPlatform::Youtube => {
            youtube::privacy_status(privacy)?;
            if size > youtube::MAX_VIDEO_BYTES {
                return Err(PublishError::invalid("视频超过 YouTube 文档规定的 256GB 上限"));
            }
        }
    }
    Ok(())
}

fn file_size(path: &str) -> Result<u64, PublishError> {
    let meta = std::fs::metadata(path).map_err(|_| PublishError::invalid("视频文件不存在"))?;
    if !meta.is_file() {
        return Err(PublishError::invalid("视频路径不是文件"));
    }
    if meta.len() == 0 {
        return Err(PublishError::invalid("视频文件是空的"));
    }
    Ok(meta.len())
}

fn collect(value: &Value, keys: &[(StatKey, &str)]) -> BTreeMap<StatKey, f64> {
    let mut values = BTreeMap::new();
    for (key, field) in keys {
        if let Some(number) = value.get(*field).and_then(json_number) {
            values.insert(*key, number);
        }
    }
    values
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn apply(record: &mut PublishRecord, outcome: &Outcome) {
    record.status = outcome.status;
    if outcome.remote_id.is_some() {
        record.remote_id = outcome.remote_id.clone();
    }
    if outcome.url.is_some() {
        record.url = outcome.url.clone();
    }
    record.reason = outcome.reason.clone();
    record.updated_at = now();
}

fn definite(status: PublishStatus, reason: String) -> Outcome {
    Outcome {
        status,
        remote_id: None,
        url: None,
        reason: Some(reason.clone()),
        reauth: status == PublishStatus::Rejected && reason_needs_reauth(&reason),
    }
}

fn uncertain(session: &Session, record: &PublishRecord, reason: impl Into<String>) -> Outcome {
    Outcome {
        status: PublishStatus::Uncertain,
        remote_id: record.remote_id.clone().or_else(|| session.handle.clone()),
        url: record.url.clone(),
        reason: Some(reason.into()),
        reauth: false,
    }
}

fn uncertain_with_id(session: &Session, record: &PublishRecord, video_id: &str, reason: impl Into<String>) -> Outcome {
    let mut outcome = uncertain(session, record, reason);
    outcome.remote_id = Some(video_id.to_string());
    if outcome.url.is_none() {
        outcome.url = Some(youtube::watch_url(video_id));
    }
    outcome
}

fn keep_published(record: &PublishRecord, reason: impl Into<String>) -> Outcome {
    Outcome {
        status: PublishStatus::Published,
        remote_id: record.remote_id.clone(),
        url: record.url.clone(),
        reason: Some(reason.into()),
        reauth: false,
    }
}

fn refresh_outcome(fail: RefreshFail) -> Outcome {
    match fail {
        RefreshFail::Reauth(message) => Outcome {
            status: PublishStatus::Rejected,
            remote_id: None,
            url: None,
            reason: Some(message),
            reauth: true,
        },
        RefreshFail::Transient(message) => Outcome::failed(message),
    }
}

fn refresh_text(fail: &RefreshFail) -> String {
    match fail {
        RefreshFail::Reauth(message) | RefreshFail::Transient(message) => message.clone(),
    }
}

async fn ensure_token(transport: &Arc<dyn Transport>, platform: ContentPlatform, cred: &mut Credential) -> Result<(), String> {
    match platform {
        ContentPlatform::Tiktok => tiktok::refresh_if_needed(transport, cred).await.map_err(|fail| refresh_text(&fail)),
        ContentPlatform::Youtube => ensure_youtube(cred, transport).await.map_err(|fail| refresh_text(&fail)),
    }
}

async fn ensure_youtube(cred: &mut Credential, transport: &Arc<dyn Transport>) -> Result<(), RefreshFail> {
    if cred.expires_at > chrono::Utc::now().timestamp() + 60 && !cred.access_token.is_empty() {
        return Ok(());
    }
    youtube::refresh(transport, cred).await
}
