use super::flow::{MemoryVault, Service, Vault};
use super::store::Session;
use super::tiktok;
use super::transport::{ScriptStep, ScriptedTransport, Transport};
use super::youtube;
use super::{AccountStatus, ContentPlatform, Credential, Privacy, PublishAccount, PublishRecord, PublishRequest, PublishStatus, StatKey};
use std::sync::Arc;

fn script(method: &'static str, url: &'static str, status: u16, body: &str) -> ScriptStep {
    ScriptStep { method, url_contains: url, status, headers: vec![], body: body.into(), transport_error: None }
}

fn json_step(method: &'static str, url: &'static str, body: &str) -> ScriptStep {
    script(method, url, 200, body)
}

fn header_step(method: &'static str, url: &'static str, status: u16, headers: Vec<(&'static str, String)>, body: &str) -> ScriptStep {
    ScriptStep { method, url_contains: url, status, headers, body: body.into(), transport_error: None }
}

fn transport_step(method: &'static str, url: &'static str) -> ScriptStep {
    ScriptStep {
        method,
        url_contains: url,
        status: 0,
        headers: vec![],
        body: String::new(),
        transport_error: Some("connection reset".into()),
    }
}

fn credential() -> Credential {
    Credential {
        client_id: "ck_test".into(),
        client_secret: "cs_test".into(),
        redirect_uri: "https://example.com/cb".into(),
        access_token: "act.existing".into(),
        refresh_token: "rft.existing".into(),
        expires_at: chrono::Utc::now().timestamp() + 3600,
        scope: "video.publish".into(),
    }
}

fn account(platform: ContentPlatform) -> PublishAccount {
    PublishAccount {
        id: uuid::Uuid::new_v4().to_string(),
        platform,
        remote_id: "remote".into(),
        name: "Creator".into(),
        bound_at: "2026-01-01T00:00:00Z".into(),
        checked_at: "2026-01-01T00:00:00Z".into(),
        status: AccountStatus::Connected,
        detail: None,
        fans: None,
    }
}

fn harness(steps: Vec<ScriptStep>) -> (Service<MemoryVault>, Arc<ScriptedTransport>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let scripted = ScriptedTransport::new(steps);
    let service = Service {
        db_path: dir.path().join("publish.sqlite3"),
        transport: scripted.clone() as Arc<dyn Transport>,
        vault: MemoryVault::new(),
    };
    (service, scripted, dir)
}

fn write_video(dir: &std::path::Path, name: &str, bytes: usize) -> String {
    let path = dir.join(name);
    std::fs::write(&path, vec![7u8; bytes]).expect("video");
    path.to_string_lossy().into_owned()
}

fn request(account_ids: Vec<String>, video: &str) -> PublishRequest {
    PublishRequest {
        account_ids,
        video_path: video.into(),
        title: "A cat".into(),
        description: "desc".into(),
        privacy: Privacy::Public,
        tags: vec!["cat".into()],
        media_task_id: None,
    }
}

fn seed_record(service: &Service<MemoryVault>, account_id: &str, platform: ContentPlatform, path: &str, status: PublishStatus) -> PublishRecord {
    let now = "2026-01-02T00:00:00Z".to_string();
    let record = PublishRecord {
        id: uuid::Uuid::new_v4().to_string(),
        group_id: uuid::Uuid::new_v4().to_string(),
        account_id: account_id.into(),
        platform,
        video_path: path.into(),
        title: "A cat".into(),
        description: "desc".into(),
        privacy: Privacy::Public,
        tags: vec![],
        status,
        remote_id: Some("old".into()),
        url: None,
        reason: Some("earlier".into()),
        attempts: 1,
        created_at: now.clone(),
        updated_at: now,
        origin: "workbench/publish".into(),
        media_task_id: None,
    };
    service.save_for_test(&record, &Session::default());
    record
}

const TIKTOK_TOKEN: &str = r#"{
  "access_token": "act.example12345Example12345Example",
  "expires_in": 86400,
  "open_id": "afd97af1-b87b-48b9-ac98-410aghda5344",
  "refresh_expires_in": 31536000,
  "refresh_token": "rft.example12345Example12345Example",
  "scope": "user.info.basic,video.list,video.publish",
  "token_type": "Bearer"
}"#;

const TIKTOK_REFRESH: &str = r#"{
  "access_token": "act.refreshedExample",
  "expires_in": 86400,
  "open_id": "afd97af1-b87b-48b9-ac98-410aghda5344",
  "refresh_expires_in": 31536000,
  "refresh_token": "rft.refreshedExample",
  "scope": "user.info.basic,video.list,video.publish",
  "token_type": "Bearer"
}"#;

const CREATOR_INFO: &str = r#"{
  "data": {
    "creator_avatar_url": "https://lf16-tiktok-common.ttwstatic.com/obj/tiktok-open-platform/8d5740ac3844be417beeacd0df75aef1",
    "creator_username": "user",
    "creator_nickname": "Nickname",
    "privacy_level_options": ["PUBLIC_TO_EVERYONE", "MUTUAL_FOLLOW_FRIENDS", "SELF_ONLY"],
    "comment_disabled": false,
    "duet_disabled": false,
    "stitch_disabled": false,
    "max_video_post_duration_sec": 300
  },
  "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
}"#;

const YOUTUBE_TOKEN: &str = r#"{
  "access_token": "ya29.a0AfH6SMC",
  "expires_in": 3599,
  "refresh_token": "1//0g-refresh",
  "scope": "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly",
  "token_type": "Bearer"
}"#;

const YOUTUBE_REFRESH: &str = r#"{
  "access_token": "ya29.new",
  "expires_in": 3599,
  "scope": "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly",
  "token_type": "Bearer"
}"#;

const CHANNEL: &str = r#"{
  "kind": "youtube#channelListResponse",
  "etag": "etag",
  "items": [{
    "kind": "youtube#channel",
    "etag": "etag",
    "id": "UC_channel",
    "snippet": { "title": "My Channel", "customUrl": "@me" },
    "statistics": { "viewCount": "10", "subscriberCount": "42", "hiddenSubscriberCount": false, "videoCount": "1" }
  }],
  "pageInfo": { "totalResults": 1, "resultsPerPage": 1 }
}"#;

#[tokio::test]
async fn tiktok_oauth_exchange_and_refresh_use_the_documented_token_bodies() {
    let (verifier, challenge) = super::pkce_pair();
    let url = tiktok::authorize_url("ck_test", "https://example.com/cb", "state1", &challenge).unwrap();
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("client_key=ck_test"));
    assert!(url.contains(&challenge));
    let scripted = ScriptedTransport::new(vec![
        json_step("POST", "/v2/oauth/token/", TIKTOK_TOKEN),
        json_step("POST", "/creator_info/query/", CREATOR_INFO),
    ]);
    let transport: Arc<dyn Transport> = scripted.clone();
    let authorized = tiktok::exchange(&transport, "ck_test", "cs_test", "https://example.com/cb", "code123", &verifier)
        .await
        .unwrap();
    assert_eq!(authorized.credential.access_token, "act.example12345Example12345Example");
    assert_eq!(authorized.credential.refresh_token, "rft.example12345Example12345Example");
    assert_eq!(authorized.remote_id, "afd97af1-b87b-48b9-ac98-410aghda5344");
    assert_eq!(authorized.name, "Nickname");
    let body = String::from_utf8(scripted.log()[0].body.clone()).unwrap();
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("code_verifier="));
    assert!(body.contains("client_key=ck_test"));
    assert!(!body.contains("client_id="));

    let scripted = ScriptedTransport::new(vec![json_step("POST", "/v2/oauth/token/", TIKTOK_REFRESH)]);
    let transport: Arc<dyn Transport> = scripted.clone();
    let mut cred = authorized.credential;
    tiktok::refresh(&transport, &mut cred).await.unwrap();
    assert_eq!(cred.access_token, "act.refreshedExample");
    assert_eq!(cred.refresh_token, "rft.refreshedExample");
    let body = String::from_utf8(scripted.log()[0].body.clone()).unwrap();
    assert!(body.contains("grant_type=refresh_token"));
    assert!(body.contains("refresh_token=rft.example12345Example12345Example"));
}

#[tokio::test]
async fn youtube_oauth_exchange_and_refresh_keep_the_old_refresh_token_when_omitted() {
    let (verifier, challenge) = super::pkce_pair();
    let url = youtube::authorize_url("desktop-client", "http://127.0.0.1:8787/", "state1", &challenge).unwrap();
    assert!(url.contains("accounts.google.com"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("access_type=offline"));
    assert!(url.contains("127.0.0.1"));
    let scripted = ScriptedTransport::new(vec![
        json_step("POST", "oauth2.googleapis.com/token", YOUTUBE_TOKEN),
        json_step("GET", "youtube/v3/channels", CHANNEL),
    ]);
    let transport: Arc<dyn Transport> = scripted.clone();
    let authorized = youtube::exchange(
        &transport,
        "desktop-client",
        "desktop-secret",
        "http://127.0.0.1:8787/",
        "4/code",
        &verifier,
    )
    .await
    .unwrap();
    assert_eq!(authorized.credential.access_token, "ya29.a0AfH6SMC");
    assert_eq!(authorized.remote_id, "UC_channel");
    assert_eq!(authorized.name, "My Channel");
    assert_eq!(authorized.fans, Some(42.0));
    let body = String::from_utf8(scripted.log()[0].body.clone()).unwrap();
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("code_verifier="));
    assert!(body.contains("client_id=desktop-client"));

    let scripted = ScriptedTransport::new(vec![json_step("POST", "oauth2.googleapis.com/token", YOUTUBE_REFRESH)]);
    let transport: Arc<dyn Transport> = scripted.clone();
    let mut cred = authorized.credential;
    youtube::refresh(&transport, &mut cred).await.unwrap();
    assert_eq!(cred.access_token, "ya29.new");
    assert_eq!(cred.refresh_token, "1//0g-refresh");
}

#[test]
fn youtube_loopback_captures_the_authorization_code() {
    let (redirect, rx) = youtube::capture_loopback().unwrap();
    let port = url::Url::parse(&redirect).unwrap().port().unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    use std::io::Write;
    stream
        .write_all(b"GET /?code=abc&state=st HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let got = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert!(got.contains("127.0.0.1"));
    assert!(got.contains("code=abc"));
    assert!(got.contains("state=st"));
}

#[tokio::test]
async fn tiktok_init_chunk_upload_and_status_follow_the_content_posting_api() {
    let init = r#"{
      "data": { "publish_id": "v_pub_file~v2.123456789", "upload_url": "https://open-upload.tiktokapis.com/upload/?upload_id=67890&upload_token=x" },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let status = r#"{
      "data": { "status": "PUBLISH_COMPLETE", "publicaly_available_post_id": ["1234567890123456789"] },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let query = r#"{
      "data": {
        "videos": [{
          "id": "1234567890123456789",
          "share_url": "https://www.tiktok.com/@user/video/1234567890123456789",
          "view_count": 0,
          "like_count": 0,
          "comment_count": 0,
          "share_count": 0
        }],
        "cursor": 0,
        "has_more": false
      },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let (service, scripted, dir) = harness(vec![
        json_step("POST", "/v2/post/publish/video/init/", init),
        script("PUT", "open-upload.tiktokapis.com", 206, ""),
        script("PUT", "open-upload.tiktokapis.com", 201, ""),
        json_step("POST", "/v2/post/publish/status/fetch/", status),
        json_step("POST", "/v2/video/query/", query),
    ]);
    let creator = account(ContentPlatform::Tiktok);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "clip.mp4", 10_000_000);
    let result = service.submit(request(vec![creator.id.clone(), creator.id.clone()], &video), "workbench/publish").await.unwrap();
    assert!(result.blocked.is_empty());
    assert_eq!(result.records.len(), 1, "one record per account");
    let record = &result.records[0];
    assert_eq!(record.status, PublishStatus::Published);
    assert_eq!(record.remote_id.as_deref(), Some("1234567890123456789"));
    assert_eq!(record.url.as_deref(), Some("https://www.tiktok.com/@user/video/1234567890123456789"));
    assert_eq!(record.attempts, 1);
    let log = scripted.log();
    let init_body: serde_json::Value = serde_json::from_slice(&log[0].body).unwrap();
    assert_eq!(init_body["source_info"]["source"], "FILE_UPLOAD");
    assert_eq!(init_body["source_info"]["video_size"], 10_000_000);
    assert_eq!(init_body["source_info"]["chunk_size"], 5_000_000);
    assert_eq!(init_body["source_info"]["total_chunk_count"], 2);
    assert_eq!(init_body["post_info"]["privacy_level"], "PUBLIC_TO_EVERYONE");
    assert!(init_body["post_info"]["title"].as_str().unwrap().contains("#cat"));
    assert_eq!(log[1].headers.iter().find(|(key, _)| key == "Content-Range").unwrap().1, "bytes 0-4999999/10000000");
    assert_eq!(log[2].headers.iter().find(|(key, _)| key == "Content-Range").unwrap().1, "bytes 5000000-9999999/10000000");
    assert!(log[1].url.contains("upload_id=67890"));
    assert_eq!(tiktok::map_status("FAILED", Some("picture_size_check_failed")).0, PublishStatus::Failed);
    assert_eq!(tiktok::map_status("FAILED", Some("spam_risk_too_many_posts")).0, PublishStatus::Rejected);
    assert_eq!(tiktok::map_status("PROCESSING_UPLOAD", None).0, PublishStatus::Processing);
}

#[tokio::test]
async fn youtube_resumable_session_upload_and_status_use_videos_insert_and_list() {
    let inserted = r#"{
      "kind": "youtube#video",
      "etag": "etag",
      "id": "M7FIvfx5J10",
      "snippet": { "publishedAt": "2026-01-01T00:00:00Z", "channelId": "UC_channel", "title": "A cat", "description": "desc", "categoryId": "22" },
      "status": { "uploadStatus": "uploaded", "privacyStatus": "private", "license": "youtube", "embeddable": true, "publicStatsViewable": true }
    }"#;
    let listed = r#"{
      "kind": "youtube#videoListResponse",
      "etag": "etag",
      "items": [{
        "kind": "youtube#video",
        "etag": "etag",
        "id": "M7FIvfx5J10",
        "status": { "uploadStatus": "processed", "privacyStatus": "public", "license": "youtube", "embeddable": true, "publicStatsViewable": true },
        "statistics": { "viewCount": "1000", "likeCount": "50", "favoriteCount": "0", "commentCount": "3" },
        "processingDetails": { "processingStatus": "succeeded", "processingProgress": { "partsTotal": "1000", "partsProcessed": "1000", "timeLeftMs": "0" } }
      }],
      "pageInfo": { "totalResults": 1, "resultsPerPage": 1 }
    }"#;
    let location = "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&upload_id=session-1";
    let (service, scripted, dir) = harness(vec![
        header_step("POST", "uploadType=resumable", 200, vec![("Location", location.into())], ""),
        json_step("PUT", "upload_id=session-1", inserted),
        json_step("GET", "part=status,statistics,processingDetails", listed),
    ]);
    let creator = account(ContentPlatform::Youtube);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "clip.mp4", 32);
    let mut body = request(vec![creator.id.clone()], &video);
    body.privacy = Privacy::Private;
    body.tags = vec!["shorts".into()];
    let result = service.submit(body, "workbench/publish").await.unwrap();
    let record = &result.records[0];
    assert_eq!(record.status, PublishStatus::Published);
    assert_eq!(record.remote_id.as_deref(), Some("M7FIvfx5J10"));
    assert_eq!(record.url.as_deref(), Some("https://www.youtube.com/watch?v=M7FIvfx5J10"));
    let log = scripted.log();
    assert!(log[0].url.contains("uploadType=resumable"));
    assert!(log[0].url.contains("part=snippet,status"));
    assert_eq!(log[0].headers.iter().find(|(key, _)| key == "X-Upload-Content-Length").unwrap().1, "32");
    let meta: serde_json::Value = serde_json::from_slice(&log[0].body).unwrap();
    assert_eq!(meta["status"]["privacyStatus"], "private");
    assert_eq!(meta["snippet"]["tags"][0], "shorts");
    assert_eq!(log[1].url, location);
    assert!(log[1].headers.iter().any(|(key, value)| key == "Content-Range" && value == "bytes 0-31/32"));
    assert!(log[2].url.contains("id=M7FIvfx5J10"));
    let rejected = serde_json::json!({"status": {"uploadStatus": "rejected", "rejectionReason": "copyright"}});
    assert_eq!(youtube::map_video(&rejected).0, PublishStatus::Rejected);
    let failed = serde_json::json!({"status": {"uploadStatus": "failed", "failureReason": "codec"}});
    assert_eq!(youtube::map_video(&failed).0, PublishStatus::Failed);
}

#[tokio::test]
async fn stats_report_unsupported_keys_instead_of_zero() {
    let listed = r#"{
      "kind": "youtube#videoListResponse",
      "etag": "etag",
      "items": [{
        "id": "M7FIvfx5J10",
        "status": { "uploadStatus": "processed" },
        "statistics": { "viewCount": "1000", "likeCount": "50", "commentCount": "3" },
        "processingDetails": { "processingStatus": "succeeded" }
      }]
    }"#;
    let (service, _, dir) = harness(vec![json_step("GET", "youtube/v3/videos", listed)]);
    let creator = account(ContentPlatform::Youtube);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "clip.mp4", 8);
    let mut record = seed_record(&service, &creator.id, ContentPlatform::Youtube, &video, PublishStatus::Published);
    record.remote_id = Some("M7FIvfx5J10".into());
    service.save_for_test(&record, &Session { handle: Some("M7FIvfx5J10".into()), session_url: None });
    let stats = service.stats(&record.id).await.unwrap();
    assert_eq!(stats.values.get(&StatKey::Views), Some(&1000.0));
    assert_eq!(stats.values.get(&StatKey::Likes), Some(&50.0));
    assert_eq!(stats.values.get(&StatKey::Comments), Some(&3.0));
    assert!(!stats.values.contains_key(&StatKey::Shares));
    assert_eq!(stats.unsupported, vec![StatKey::Shares]);
    assert!(stats.error.is_none());

    let query = r#"{
      "data": { "videos": [{ "id": "123", "view_count": 4, "comment_count": 1, "share_count": 2 }] },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let (service, _, _) = harness(vec![json_step("POST", "/v2/video/query/", query)]);
    let creator = account(ContentPlatform::Tiktok);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let mut record = seed_record(&service, &creator.id, ContentPlatform::Tiktok, &video, PublishStatus::Published);
    record.remote_id = Some("123".into());
    service.save_for_test(&record, &Session::default());
    let stats = service.stats(&record.id).await.unwrap();
    assert_eq!(stats.values.get(&StatKey::Views), Some(&4.0));
    assert!(!stats.values.contains_key(&StatKey::Likes));
    assert_eq!(stats.unsupported, vec![StatKey::Likes]);
}

#[tokio::test]
async fn duplicate_guard_blocks_published_and_processing_records() {
    let (service, scripted, dir) = harness(vec![]);
    let video = write_video(dir.path(), "clip.mp4", 8);
    let published = account(ContentPlatform::Tiktok);
    let processing = account(ContentPlatform::Youtube);
    service.put_account(&published).unwrap();
    service.put_account(&processing).unwrap();
    service.vault.save(&published.id, &credential()).unwrap();
    service.vault.save(&processing.id, &credential()).unwrap();
    seed_record(&service, &published.id, ContentPlatform::Tiktok, &video, PublishStatus::Published);
    seed_record(&service, &processing.id, ContentPlatform::Youtube, &video, PublishStatus::Processing);
    let result = service
        .submit(request(vec![published.id.clone(), processing.id.clone()], &video), "workbench/publish")
        .await
        .unwrap();
    assert!(result.records.is_empty());
    assert_eq!(result.blocked.len(), 2);
    assert!(result.blocked.iter().any(|item| item.account_id == published.id && item.status == PublishStatus::Published));
    assert!(result.blocked.iter().any(|item| item.account_id == processing.id && item.status == PublishStatus::Processing));
    assert!(scripted.log().is_empty());
}

#[tokio::test]
async fn rejected_and_failed_records_may_retry() {
    let init = r#"{
      "data": { "publish_id": "v_pub_file~v2.123456789", "upload_url": "https://open-upload.tiktokapis.com/upload/?upload_id=1&upload_token=x" },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let status = r#"{
      "data": { "status": "PUBLISH_COMPLETE", "publicaly_available_post_id": ["999"] },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let query = r#"{
      "data": { "videos": [{ "id": "999", "share_url": "https://www.tiktok.com/@user/video/999" }] },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let (service, _, dir) = harness(vec![
        json_step("POST", "/v2/post/publish/video/init/", init),
        script("PUT", "open-upload.tiktokapis.com", 201, ""),
        json_step("POST", "/v2/post/publish/status/fetch/", status),
        json_step("POST", "/v2/video/query/", query),
        json_step("POST", "/v2/post/publish/video/init/", init),
        script("PUT", "open-upload.tiktokapis.com", 201, ""),
        json_step("POST", "/v2/post/publish/status/fetch/", status),
        json_step("POST", "/v2/video/query/", query),
    ]);
    let creator = account(ContentPlatform::Tiktok);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "failed.mp4", 64);
    let failed = seed_record(&service, &creator.id, ContentPlatform::Tiktok, &video, PublishStatus::Failed);
    let retried = service.retry(&failed.id).await.unwrap();
    assert_eq!(retried.attempts, 2);
    assert_eq!(retried.status, PublishStatus::Published);
    let other = write_video(dir.path(), "rejected.mp4", 64);
    let rejected = seed_record(&service, &creator.id, ContentPlatform::Tiktok, &other, PublishStatus::Rejected);
    let again = service.retry(&rejected.id).await.unwrap();
    assert_eq!(again.attempts, 2);
    assert_eq!(again.status, PublishStatus::Published);
    let published = seed_record(&service, &creator.id, ContentPlatform::Tiktok, &video, PublishStatus::Published);
    assert!(service.retry(&published.id).await.is_err());
}

#[tokio::test]
async fn transport_failure_after_upload_start_is_uncertain_and_query_only() {
    let init = r#"{
      "data": { "publish_id": "v_pub_file~v2.123456789", "upload_url": "https://open-upload.tiktokapis.com/upload/?upload_id=67890&upload_token=x" },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let status = r#"{
      "data": { "status": "PROCESSING_DOWNLOAD" },
      "error": { "code": "ok", "message": "", "log_id": "2022101122484428E45F2C0D2A04020A1" }
    }"#;
    let (service, scripted, dir) = harness(vec![
        json_step("POST", "/v2/post/publish/video/init/", init),
        transport_step("PUT", "open-upload.tiktokapis.com"),
        json_step("POST", "/v2/post/publish/status/fetch/", status),
    ]);
    let creator = account(ContentPlatform::Tiktok);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "clip.mp4", 32);
    let result = service.submit(request(vec![creator.id.clone()], &video), "workbench/publish").await.unwrap();
    let record = &result.records[0];
    assert_eq!(record.status, PublishStatus::Uncertain);
    assert_eq!(record.remote_id.as_deref(), Some("v_pub_file~v2.123456789"));
    assert!(service.retry(&record.id).await.is_err());
    let refreshed = service.refresh_record(&record.id).await.unwrap();
    assert_eq!(refreshed.status, PublishStatus::Processing);
    let log = scripted.log();
    assert_eq!(log.iter().filter(|item| item.url.contains("/video/init/")).count(), 1);
    assert!(log.last().unwrap().url.contains("/v2/post/publish/status/fetch/"));

    let location = "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&upload_id=session-9";
    let (service, scripted, dir) = harness(vec![
        header_step("POST", "uploadType=resumable", 200, vec![("Location", location.into())], ""),
        transport_step("PUT", "upload_id=session-9"),
        script("PUT", "upload_id=session-9", 308, ""),
    ]);
    let creator = account(ContentPlatform::Youtube);
    service.put_account(&creator).unwrap();
    service.vault.save(&creator.id, &credential()).unwrap();
    let video = write_video(dir.path(), "clip.mp4", 16);
    let result = service.submit(request(vec![creator.id], &video), "workbench/publish").await.unwrap();
    assert_eq!(result.records[0].status, PublishStatus::Uncertain);
    assert!(service.retry(&result.records[0].id).await.is_err());
    let refreshed = service.refresh_record(&result.records[0].id).await.unwrap();
    assert_eq!(refreshed.status, PublishStatus::Uncertain);
    let log = scripted.log();
    assert_eq!(log.iter().filter(|item| item.method == "POST" && item.url.contains("uploadType=resumable")).count(), 1);
    assert_eq!(log.last().unwrap().headers.iter().find(|(key, _)| key == "Content-Range").unwrap().1, "bytes */16");
}

impl Service<MemoryVault> {
    fn save_for_test(&self, record: &PublishRecord, session: &Session) {
        self.save(record, session).unwrap();
    }
}

#[test]
fn handle_errors_use_typed_exit_codes_not_string_matching() {
    use crate::app_cli::exit;
    use super::PublishError;
    assert_eq!(PublishError::invalid("未知发布操作 nope").exit_code(), exit::INVALID);
    assert_eq!(PublishError::invalid("发布参数无效：missing field `title`").exit_code(), exit::INVALID);
    assert_eq!(PublishError::not_found("发布账号不存在").exit_code(), exit::INVALID);
    assert_eq!(PublishError::not_found("发布记录不存在").exit_code(), exit::INVALID);
    assert_eq!(PublishError::unsupported("YouTube 没有 friends，可选 public、private、unlisted").exit_code(), exit::INVALID);
    assert_eq!(PublishError::invalid("这条记录不能重新上传，只能查询平台状态").exit_code(), exit::INVALID);
    assert_eq!(PublishError::internal("发布记录保存失败：disk full").exit_code(), exit::INTERNAL);
    assert_eq!(PublishError::internal("无法定位应用数据目录").exit_code(), exit::INTERNAL);
    assert_eq!(PublishError::internal("HTTP 客户端不可用：builder").exit_code(), exit::INTERNAL);
    assert_eq!(PublishError::rejected("授权被拒绝：access_denied").exit_code(), exit::REJECTED);
    assert_eq!(PublishError::failed("connection reset").exit_code(), exit::FAILED);
    assert_eq!(PublishError::uncertain("上传结果未知").exit_code(), exit::UNCERTAIN);
    assert_eq!(PublishError::timeout("等待本机授权回调超时").exit_code(), exit::TIMEOUT);
}

#[tokio::test]
async fn unsupported_privacy_and_tiktok_container_fail_before_any_request() {
    use crate::app_cli::exit;
    let (service, scripted, dir) = harness(vec![]);
    let video = write_video(dir.path(), "clip.mp4", 32);
    let youtube = account(ContentPlatform::Youtube);
    service.put_account(&youtube).unwrap();
    service.vault.save(&youtube.id, &credential()).unwrap();
    let mut body = request(vec![youtube.id.clone()], &video);
    body.privacy = Privacy::Friends;
    let error = service.submit(body, "workbench/publish").await.unwrap_err();
    assert_eq!(error.exit_code(), exit::INVALID);
    assert!(scripted.log().is_empty());

    let (service, scripted, dir) = harness(vec![]);
    let video = write_video(dir.path(), "clip.mp4", 32);
    let tiktok = account(ContentPlatform::Tiktok);
    service.put_account(&tiktok).unwrap();
    service.vault.save(&tiktok.id, &credential()).unwrap();
    let mut body = request(vec![tiktok.id], &video);
    body.privacy = Privacy::Unlisted;
    let error = service.submit(body, "workbench/publish").await.unwrap_err();
    assert_eq!(error.exit_code(), exit::INVALID);
    assert!(error.to_string().contains("unlisted"));
    assert!(scripted.log().is_empty());

    let (service, scripted, dir) = harness(vec![]);
    let video = write_video(dir.path(), "clip.txt", 32);
    let tiktok = account(ContentPlatform::Tiktok);
    service.put_account(&tiktok).unwrap();
    service.vault.save(&tiktok.id, &credential()).unwrap();
    let error = service.submit(request(vec![tiktok.id], &video), "workbench/publish").await.unwrap_err();
    assert_eq!(error.exit_code(), exit::INVALID);
    assert!(scripted.log().is_empty());
    assert_eq!(tiktok::privacy_level(Privacy::Public).unwrap(), "PUBLIC_TO_EVERYONE");
    assert_eq!(tiktok::privacy_level(Privacy::Friends).unwrap(), "MUTUAL_FOLLOW_FRIENDS");
    assert_eq!(tiktok::privacy_level(Privacy::Private).unwrap(), "SELF_ONLY");
    assert_eq!(youtube::privacy_status(Privacy::Unlisted).unwrap(), "unlisted");
    assert_eq!(youtube::privacy_status(Privacy::Private).unwrap(), "private");
    assert!(youtube::privacy_status(Privacy::Friends).is_err());
}
