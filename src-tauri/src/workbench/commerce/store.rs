use super::types::{CommerceError, ListingDraft, ListingFilter, ListingRecord, ListingStatus, ListingTarget};
use crate::workbench::shops::Platform;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Store(Connection);

fn internal(error: impl std::fmt::Display) -> CommerceError {
    CommerceError::internal(format!("上架记录保存失败：{error}"))
}

impl Store {
    pub fn open() -> Result<Self, CommerceError> {
        let path = default_path()?;
        Self::open_at(&path)
    }

    pub fn open_at(path: &Path) -> Result<Self, CommerceError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(internal)?;
        }
        let db = Connection::open(path).map_err(internal)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(internal)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS listings (
                id TEXT PRIMARY KEY,
                group_id TEXT NOT NULL,
                shop_id TEXT NOT NULL,
                platform TEXT NOT NULL,
                status TEXT NOT NULL,
                reason TEXT,
                remote_id TEXT,
                title TEXT NOT NULL,
                currency TEXT,
                draft_json TEXT NOT NULL,
                target_json TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS listings_group_shop ON listings(group_id, shop_id);",
        )
        .map_err(internal)?;
        Ok(Self(db))
    }

    pub fn list(&self, filter: &ListingFilter) -> Result<Vec<ListingRecord>, CommerceError> {
        let mut stmt = self
            .0
            .prepare(
                "SELECT id,group_id,shop_id,platform,status,reason,remote_id,title,currency,draft_json,target_json,attempts,created_at,updated_at
                 FROM listings ORDER BY created_at DESC",
            )
            .map_err(internal)?;
        let rows = stmt
            .query_map([], |row| row_record(row))
            .map_err(internal)?;
        let mut records = Vec::new();
        for row in rows {
            let record = row.map_err(internal)?;
            if filter.shop_id.as_ref().is_some_and(|id| id != &record.shop_id) {
                continue;
            }
            if filter.group_id.as_ref().is_some_and(|id| id != &record.group_id) {
                continue;
            }
            if filter.status.is_some_and(|status| status != record.status) {
                continue;
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn get(&self, id: &str) -> Result<ListingRecord, CommerceError> {
        self.0
            .query_row(
                "SELECT id,group_id,shop_id,platform,status,reason,remote_id,title,currency,draft_json,target_json,attempts,created_at,updated_at
                 FROM listings WHERE id=?1",
                params![id],
                row_record,
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(|| CommerceError::not_found(format!("上架记录不存在：{id}")))
    }

    pub fn for_group_shop(
        &self,
        group_id: &str,
        shop_id: &str,
    ) -> Result<Option<ListingRecord>, CommerceError> {
        self.0
            .query_row(
                "SELECT id,group_id,shop_id,platform,status,reason,remote_id,title,currency,draft_json,target_json,attempts,created_at,updated_at
                 FROM listings WHERE group_id=?1 AND shop_id=?2",
                params![group_id, shop_id],
                row_record,
            )
            .optional()
            .map_err(internal)
    }

    pub fn insert(&self, record: &ListingRecord) -> Result<(), CommerceError> {
        let draft = serde_json::to_string(&record.draft).map_err(internal)?;
        let target = serde_json::to_string(&record.target).map_err(internal)?;
        self.0
            .execute(
                "INSERT INTO listings(id,group_id,shop_id,platform,status,reason,remote_id,title,currency,draft_json,target_json,attempts,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    record.id,
                    record.group_id,
                    record.shop_id,
                    record.platform.as_str(),
                    record.status.as_str(),
                    record.reason,
                    record.remote_id,
                    record.title,
                    record.currency,
                    draft,
                    target,
                    record.attempts,
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map_err(internal)?;
        Ok(())
    }

    pub fn update(&self, record: &ListingRecord) -> Result<(), CommerceError> {
        let draft = serde_json::to_string(&record.draft).map_err(internal)?;
        let target = serde_json::to_string(&record.target).map_err(internal)?;
        let changed = self
            .0
            .execute(
                "UPDATE listings SET status=?2, reason=?3, remote_id=?4, title=?5, currency=?6, draft_json=?7, target_json=?8, attempts=?9, updated_at=?10
                 WHERE id=?1",
                params![
                    record.id,
                    record.status.as_str(),
                    record.reason,
                    record.remote_id,
                    record.title,
                    record.currency,
                    draft,
                    target,
                    record.attempts,
                    record.updated_at,
                ],
            )
            .map_err(internal)?;
        if changed == 0 {
            return Err(CommerceError::not_found(format!("上架记录不存在：{}", record.id)));
        }
        Ok(())
    }
}

fn row_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ListingRecord> {
    let platform: String = row.get(3)?;
    let status: String = row.get(4)?;
    let draft: String = row.get(9)?;
    let target: String = row.get(10)?;
    Ok(ListingRecord {
        id: row.get(0)?,
        group_id: row.get(1)?,
        shop_id: row.get(2)?,
        platform: Platform::parse(&platform).map_err(|_| rusqlite::Error::InvalidQuery)?,
        status: ListingStatus::parse(&status).ok_or(rusqlite::Error::InvalidQuery)?,
        reason: row.get(5)?,
        remote_id: row.get(6)?,
        title: row.get(7)?,
        currency: row.get(8)?,
        draft: serde_json::from_str::<ListingDraft>(&draft).map_err(|_| rusqlite::Error::InvalidQuery)?,
        target: serde_json::from_str::<ListingTarget>(&target).map_err(|_| rusqlite::Error::InvalidQuery)?,
        attempts: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

pub fn default_path() -> Result<PathBuf, CommerceError> {
    Ok(crate::app_data::app_data_dir()
        .ok_or_else(|| CommerceError::internal("无法定位应用数据目录"))?
        .join("workbench")
        .join("commerce.sqlite3"))
}

pub fn with_store<T>(
    path: &Path,
    body: impl FnOnce(&Store) -> Result<T, CommerceError>,
) -> Result<T, CommerceError> {
    body(&Store::open_at(path)?)
}
