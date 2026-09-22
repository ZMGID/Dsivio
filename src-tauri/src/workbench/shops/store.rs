use super::{Platform, Shop, ShopStatus};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Duration;

pub(super) struct Store(Connection);

fn error(err: impl std::fmt::Display) -> String {
    format!("店铺记录保存失败：{err}")
}

impl Store {
    pub(super) fn open() -> Result<Self, String> {
        let path = crate::app_data::app_data_dir()
            .ok_or("无法定位应用数据目录")?
            .join("workbench")
            .join("shops.sqlite3");
        Self::open_at(&path)
    }

    fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let db = Connection::open(path).map_err(error)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(error)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS shops (
                id TEXT PRIMARY KEY,
                platform TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                name TEXT NOT NULL,
                region TEXT,
                bound_at TEXT NOT NULL,
                checked_at TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT,
                UNIQUE(platform, remote_id)
             );",
        )
        .map_err(error)?;
        Ok(Self(db))
    }

    pub(super) fn list(&self) -> Result<Vec<Shop>, String> {
        let mut stmt = self.0.prepare("SELECT id,platform,remote_id,name,region,bound_at,checked_at,status,detail FROM shops ORDER BY bound_at DESC")
            .map_err(error)?;
        let rows = stmt
            .query_map([], |r| {
                let platform: String = r.get(1)?;
                let status: String = r.get(7)?;
                Ok(Shop {
                    id: r.get(0)?,
                    platform: Platform::parse(&platform)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    remote_id: r.get(2)?,
                    name: r.get(3)?,
                    region: r.get(4)?,
                    bound_at: r.get(5)?,
                    checked_at: r.get(6)?,
                    status: ShopStatus::parse(&status)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    detail: r.get(8)?,
                })
            })
            .map_err(error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(error)
    }

    pub(super) fn get(&self, id: &str) -> Result<Shop, String> {
        self.list()?
            .into_iter()
            .find(|shop| shop.id == id)
            .ok_or("店铺不存在".into())
    }

    pub(super) fn existing_id(
        &self,
        platform: Platform,
        remote_id: &str,
    ) -> Result<Option<String>, String> {
        self.0
            .query_row(
                "SELECT id FROM shops WHERE platform=?1 AND remote_id=?2",
                params![platform.as_str(), remote_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(error)
    }

    pub(super) fn put(&self, shop: &Shop) -> Result<(), String> {
        self.0.execute(
            "INSERT INTO shops(id,platform,remote_id,name,region,bound_at,checked_at,status,detail)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(platform,remote_id) DO UPDATE SET
                name=excluded.name, region=excluded.region, checked_at=excluded.checked_at,
                status=excluded.status, detail=excluded.detail",
            params![shop.id, shop.platform.as_str(), shop.remote_id, shop.name, shop.region,
                shop.bound_at, shop.checked_at, shop.status.as_str(), shop.detail],
        ).map_err(error)?;
        Ok(())
    }

    pub(super) fn delete(&self, id: &str) -> Result<(), String> {
        self.0
            .execute("DELETE FROM shops WHERE id=?1", [id])
            .map_err(error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_round_trip_preserves_identity_and_status() {
        let dir = tempfile::tempdir().unwrap();
        let db = Store::open_at(&dir.path().join("shops.sqlite3")).unwrap();
        let mut shop = Shop {
            id: "local-1".into(),
            platform: Platform::Shopee,
            remote_id: "42".into(),
            name: "First".into(),
            region: Some("SG".into()),
            bound_at: "2026-01-01T00:00:00Z".into(),
            checked_at: "2026-01-01T00:00:00Z".into(),
            status: ShopStatus::Connected,
            detail: None,
        };
        db.put(&shop).unwrap();
        assert_eq!(
            db.existing_id(Platform::Shopee, "42").unwrap().as_deref(),
            Some("local-1")
        );
        shop.name = "Renamed".into();
        shop.status = ShopStatus::NeedsReauthorization;
        shop.detail = Some("expired".into());
        db.put(&shop).unwrap();
        let loaded = db.list().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].bound_at, "2026-01-01T00:00:00Z");
        assert_eq!(loaded[0].name, "Renamed");
        assert_eq!(loaded[0].status, ShopStatus::NeedsReauthorization);
        db.delete("local-1").unwrap();
        assert!(db.list().unwrap().is_empty());
    }
}
