use super::{AccountStatus, ContentPlatform, Privacy, PublishAccount, PublishError, PublishRecord, PublishStatus};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) struct Store(Connection);

#[derive(Debug, Clone, Default)]
pub(crate) struct Session {
    pub handle: Option<String>,
    pub session_url: Option<String>,
}

pub(crate) struct StoredRecord {
    pub record: PublishRecord,
    pub session: Session,
}

fn error(err: impl std::fmt::Display) -> PublishError {
    PublishError::internal(format!("发布记录保存失败：{err}"))
}

pub(crate) fn database_path() -> Result<PathBuf, PublishError> {
    Ok(crate::app_data::app_data_dir()
        .ok_or_else(|| PublishError::internal("无法定位应用数据目录"))?
        .join("workbench")
        .join("publish.sqlite3"))
}

impl Store {
    pub(crate) fn open() -> Result<Self, PublishError> {
        Self::open_at(&database_path()?)
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, PublishError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let db = Connection::open(path).map_err(error)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(error)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                name TEXT NOT NULL,
                bound_at TEXT NOT NULL,
                checked_at TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT,
                fans REAL,
                UNIQUE(platform, remote_id)
             );
             CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                group_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                video_path TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                privacy TEXT NOT NULL,
                tags TEXT NOT NULL,
                status TEXT NOT NULL,
                remote_id TEXT,
                url TEXT,
                reason TEXT,
                attempts INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                origin TEXT NOT NULL,
                media_task_id TEXT,
                handle TEXT,
                session_url TEXT
             );
             CREATE INDEX IF NOT EXISTS records_account_video ON records(account_id, video_path);",
        )
        .map_err(error)?;
        Ok(Self(db))
    }

    pub(crate) fn list_accounts(&self) -> Result<Vec<PublishAccount>, PublishError> {
        let mut stmt = self
            .0
            .prepare("SELECT id,platform,remote_id,name,bound_at,checked_at,status,detail,fans FROM accounts ORDER BY bound_at DESC")
            .map_err(error)?;
        let rows = stmt
            .query_map([], |row| {
                let platform: String = row.get(1)?;
                let status: String = row.get(6)?;
                Ok(PublishAccount {
                    id: row.get(0)?,
                    platform: ContentPlatform::parse(&platform).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    remote_id: row.get(2)?,
                    name: row.get(3)?,
                    bound_at: row.get(4)?,
                    checked_at: row.get(5)?,
                    status: AccountStatus::parse(&status).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    detail: row.get(7)?,
                    fans: row.get(8)?,
                })
            })
            .map_err(error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(error)
    }

    pub(crate) fn get_account(&self, id: &str) -> Result<PublishAccount, PublishError> {
        self.list_accounts()?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| PublishError::not_found("发布账号不存在"))
    }

    pub(crate) fn existing_account(&self, platform: ContentPlatform, remote_id: &str) -> Result<Option<String>, PublishError> {
        self.0
            .query_row(
                "SELECT id FROM accounts WHERE platform=?1 AND remote_id=?2",
                params![platform.as_str(), remote_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(error)
    }

    pub(crate) fn put_account(&self, account: &PublishAccount) -> Result<(), PublishError> {
        self.0
            .execute(
                "INSERT INTO accounts(id,platform,remote_id,name,bound_at,checked_at,status,detail,fans)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(id) DO UPDATE SET
                    platform=excluded.platform, remote_id=excluded.remote_id, name=excluded.name,
                    checked_at=excluded.checked_at, status=excluded.status, detail=excluded.detail, fans=excluded.fans",
                params![
                    account.id,
                    account.platform.as_str(),
                    account.remote_id,
                    account.name,
                    account.bound_at,
                    account.checked_at,
                    account.status.as_str(),
                    account.detail,
                    account.fans,
                ],
            )
            .map_err(error)?;
        Ok(())
    }

    pub(crate) fn delete_account(&self, id: &str) -> Result<(), PublishError> {
        self.0.execute("DELETE FROM accounts WHERE id=?1", [id]).map_err(error)?;
        Ok(())
    }

    pub(crate) fn save_record(&self, record: &PublishRecord, session: &Session) -> Result<(), PublishError> {
        let tags = serde_json::to_string(&record.tags).map_err(error)?;
        self.0
            .execute(
                "INSERT INTO records(id,group_id,account_id,platform,video_path,title,description,privacy,tags,status,remote_id,url,reason,attempts,created_at,updated_at,origin,media_task_id,handle,session_url)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
                 ON CONFLICT(id) DO UPDATE SET
                    status=excluded.status, remote_id=excluded.remote_id, url=excluded.url, reason=excluded.reason,
                    attempts=excluded.attempts, updated_at=excluded.updated_at, title=excluded.title,
                    description=excluded.description, privacy=excluded.privacy, tags=excluded.tags,
                    handle=excluded.handle, session_url=excluded.session_url, video_path=excluded.video_path",
                params![
                    record.id,
                    record.group_id,
                    record.account_id,
                    record.platform.as_str(),
                    record.video_path,
                    record.title,
                    record.description,
                    record.privacy.as_str(),
                    tags,
                    record.status.as_str(),
                    record.remote_id,
                    record.url,
                    record.reason,
                    record.attempts,
                    record.created_at,
                    record.updated_at,
                    record.origin,
                    record.media_task_id,
                    session.handle,
                    session.session_url,
                ],
            )
            .map_err(error)?;
        Ok(())
    }

    pub(crate) fn get_record(&self, id: &str) -> Result<StoredRecord, PublishError> {
        self.0
            .query_row("SELECT id,group_id,account_id,platform,video_path,title,description,privacy,tags,status,remote_id,url,reason,attempts,created_at,updated_at,origin,media_task_id,handle,session_url FROM records WHERE id=?1", [id], row_record)
            .optional()
            .map_err(error)?
            .ok_or_else(|| PublishError::not_found("发布记录不存在"))
    }

    pub(crate) fn list_records(&self, account_id: Option<&str>, status: Option<PublishStatus>) -> Result<Vec<PublishRecord>, PublishError> {
        let mut stmt = self.0.prepare(
            "SELECT id,group_id,account_id,platform,video_path,title,description,privacy,tags,status,remote_id,url,reason,attempts,created_at,updated_at,origin,media_task_id,handle,session_url
             FROM records
             WHERE (?1 IS NULL OR account_id=?1) AND (?2 IS NULL OR status=?2)
             ORDER BY created_at DESC",
        ).map_err(error)?;
        let status = status.map(|value| value.as_str().to_string());
        let rows = stmt
            .query_map(params![account_id, status], row_record)
            .map_err(error)?;
        rows.map(|row| row.map(|stored| stored.record).map_err(error)).collect()
    }

    pub(crate) fn blocking(&self, account_id: &str, video_path: &str) -> Result<Option<(String, PublishStatus)>, PublishError> {
        let found = self
            .0
            .query_row(
                "SELECT id, status FROM records WHERE account_id=?1 AND video_path=?2 AND status IN ('uploading','processing','published','uncertain') LIMIT 1",
                params![account_id, video_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(error)?;
        found
            .map(|(id, status)| {
                PublishStatus::parse(&status)
                    .map(|status| (id, status))
                    .map_err(|_| PublishError::internal("发布状态无效"))
            })
            .transpose()
    }
}

fn row_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecord> {
    let platform: String = row.get(3)?;
    let privacy: String = row.get(7)?;
    let tags: String = row.get(8)?;
    let status: String = row.get(9)?;
    let attempts: i64 = row.get(13)?;
    Ok(StoredRecord {
        record: PublishRecord {
            id: row.get(0)?,
            group_id: row.get(1)?,
            account_id: row.get(2)?,
            platform: ContentPlatform::parse(&platform).map_err(|_| rusqlite::Error::InvalidQuery)?,
            video_path: row.get(4)?,
            title: row.get(5)?,
            description: row.get(6)?,
            privacy: Privacy::parse(&privacy).map_err(|_| rusqlite::Error::InvalidQuery)?,
            tags: serde_json::from_str(&tags).unwrap_or_default(),
            status: PublishStatus::parse(&status).map_err(|_| rusqlite::Error::InvalidQuery)?,
            remote_id: row.get(10)?,
            url: row.get(11)?,
            reason: row.get(12)?,
            attempts: u32::try_from(attempts).unwrap_or(0),
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
            origin: row.get(16)?,
            media_task_id: row.get(17)?,
        },
        session: Session { handle: row.get(18)?, session_url: row.get(19)? },
    })
}
