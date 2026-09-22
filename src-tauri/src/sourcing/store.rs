use super::types::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{path::Path, time::Duration};

pub(super) struct Store(Connection);
/// Preserve the paid API result even if opening or writing local history fails.
pub(super) fn remember_search(
    mut result: SourcingSearch,
    db: Result<Store, String>,
) -> SourcingSearch {
    result.history_warning = None;
    if let Err(error) = db.and_then(|mut db| db.save_search(&result)) {
        result.history_warning = Some(format!(
            "搜索记录未保存：{error}。当前商品结果仍可查看，重新搜索会再次调用 1688。"
        ));
    }
    result
}
fn err(e: impl std::fmt::Display) -> String {
    format!("选品存储失败：{e}")
}
fn encode<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(err)
}
fn decode<T: serde::de::DeserializeOwned>(s: String) -> Result<T, String> {
    serde_json::from_str(&s).map_err(err)
}
pub(super) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub(super) fn page(page: u32, size: u32) -> Result<(), String> {
    if page == 0 || page > 100_000 || size == 0 || size > 50 {
        return Err("页码必须为 1–100000，每页数量必须为 1–50".into());
    }
    Ok(())
}
fn clean(s: &str, max: usize, name: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() > max {
        return Err(format!("{name}过长"));
    }
    Ok(s.to_string())
}
fn web_url(v: &mut Option<String>) -> Result<(), String> {
    *v = v
        .take()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if let Some(s) = v {
        let u = reqwest::Url::parse(s).map_err(|_| "链接格式无效")?;
        if s.len() > 4096
            || !matches!(u.scheme(), "http" | "https")
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
        {
            return Err("链接必须是无凭据的 HTTP(S) 地址".into());
        }
    }
    Ok(())
}
fn validate(p: &mut PickDraft) -> Result<(), String> {
    p.title = clean(&p.title, 1000, "标题")?;
    if p.title.is_empty() {
        return Err("请输入选品标题".into());
    }
    p.note = clean(&p.note, 20_000, "备注")?;
    p.source_id = p
        .source_id
        .take()
        .map(|s| clean(&s, 256, "来源 ID"))
        .transpose()?
        .filter(|s| !s.is_empty());
    if p.source != PickSource::Manual && p.source_id.is_none() {
        return Err("外部选品必须保留来源 ID".into());
    }
    web_url(&mut p.url)?;
    web_url(&mut p.image_url)?;
    for (field, name) in [
        (&mut p.price, "价格"),
        (&mut p.currency, "币种"),
        (&mut p.supplier, "供应商"),
    ] {
        *field = field
            .take()
            .map(|s| clean(&s, 1000, name))
            .transpose()?
            .filter(|s| !s.is_empty());
    }
    if p.tags.len() > 30 {
        return Err("标签最多 30 个".into());
    }
    p.tags = p
        .tags
        .iter()
        .map(|s| clean(s, 100, "标签"))
        .collect::<Result<_, _>>()?;
    p.tags.retain(|s| !s.is_empty());
    p.tags.sort();
    p.tags.dedup();
    Ok(())
}
impl Store {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        let db = Connection::open(path).map_err(err)?;
        db.busy_timeout(Duration::from_secs(5)).map_err(err)?;
        db.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS picks(id TEXT PRIMARY KEY, source_key TEXT UNIQUE, body TEXT NOT NULL, title TEXT NOT NULL, search_text TEXT NOT NULL, source TEXT NOT NULL, stage TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS image_searches(id TEXT PRIMARY KEY, name TEXT NOT NULL, fetched_at TEXT NOT NULL, body TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS picks_updated ON picks(updated_at DESC);
            CREATE INDEX IF NOT EXISTS image_searches_recent ON image_searches(fetched_at DESC);").map_err(err)?;
        Ok(Self(db))
    }
    pub(super) fn save_pick(&mut self, mut r: SavePickRequest) -> Result<PickItem, String> {
        validate(&mut r.product)?;
        if r.id.is_some() != r.expected_revision.is_some() {
            return Err("更新选品需要 ID 和版本号".into());
        }
        let key = r
            .product
            .source_id
            .as_ref()
            .map(|id| format!("{:?}:{id}", r.product.source));
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let old: Option<PickItem> = if let Some(id) = &r.id {
            let raw = tx
                .query_row("SELECT body FROM picks WHERE id=?1", [id], |r| {
                    r.get::<_, String>(0)
                })
                .optional()
                .map_err(err)?
                .ok_or("选品不存在")?;
            let old: PickItem = decode(raw)?;
            if Some(old.revision) != r.expected_revision {
                return Err("选品已被修改，请刷新后重试".into());
            }
            if old.product.source != r.product.source
                || old.product.source_id != r.product.source_id
            {
                return Err("不能修改选品来源身份".into());
            }
            Some(old)
        } else {
            if let Some(key) = &key {
                if let Some(raw) = tx
                    .query_row("SELECT body FROM picks WHERE source_key=?1", [key], |r| {
                        r.get::<_, String>(0)
                    })
                    .optional()
                    .map_err(err)?
                {
                    return decode(raw); // repeated collection preserves user notes and stage
                }
            }
            None
        };
        let at = now();
        let item = PickItem {
            id: old
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            revision: old.as_ref().map_or(1, |p| p.revision + 1),
            created_at: old.map_or_else(|| at.clone(), |p| p.created_at),
            updated_at: at,
            product: r.product,
        };
        tx.execute("INSERT INTO picks VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET body=excluded.body,title=excluded.title,search_text=excluded.search_text,stage=excluded.stage,updated_at=excluded.updated_at",
            params![item.id, key, encode(&item)?, item.product.title, format!("{} {} {}", item.product.title, item.product.note, item.product.tags.join(" ")), encode(&item.product.source)?, encode(&item.product.stage)?, item.updated_at]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(item)
    }
    pub(super) fn list_picks(&mut self, f: PickFilter) -> Result<PickPage, String> {
        page(f.page, f.page_size)?;
        let keyword = clean(&f.keyword, 256, "关键词")?.to_lowercase();
        let source = f.source.map(|s| encode(&s)).transpose()?;
        let stage = f.stage.map(|s| encode(&s)).transpose()?;
        let tx = self.0.transaction().map_err(err)?;
        let clause = "FROM picks WHERE instr(lower(search_text),?1)>0 AND (?2 IS NULL OR source=?2) AND (?3 IS NULL OR stage=?3)";
        let total = tx
            .query_row(
                &format!("SELECT count(*) {clause}"),
                params![keyword, source, stage],
                |r| r.get(0),
            )
            .map_err(err)?;
        let mut statement = tx
            .prepare(&format!(
                "SELECT body {clause} ORDER BY updated_at DESC,id LIMIT ?4 OFFSET ?5"
            ))
            .map_err(err)?;
        let rows = statement
            .query_map(
                params![
                    keyword,
                    source,
                    stage,
                    f.page_size,
                    (f.page - 1) * f.page_size
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(err)?;
        let items = rows
            .map(|r| decode(r.map_err(err)?))
            .collect::<Result<_, String>>()?;
        Ok(PickPage { items, total })
    }
    pub(super) fn delete_pick(&mut self, id: &str, revision: u32) -> Result<(), String> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let raw = tx
            .query_row("SELECT body FROM picks WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(err)?
            .ok_or("选品不存在")?;
        if decode::<PickItem>(raw)?.revision != revision {
            return Err("选品已被修改，请刷新后重试".into());
        }
        tx.execute("DELETE FROM picks WHERE id=?1", [id])
            .map_err(err)?;
        tx.commit().map_err(err)
    }
    pub(super) fn save_search(&mut self, s: &SourcingSearch) -> Result<(), String> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        tx.execute(
            "INSERT INTO image_searches(id,name,fetched_at,body) VALUES (?1,?2,?3,?4)",
            params![s.id, s.name, s.fetched_at, encode(s)?],
        )
        .map_err(err)?;
        tx.execute("DELETE FROM image_searches WHERE id NOT IN (SELECT id FROM image_searches ORDER BY fetched_at DESC,id DESC LIMIT 100)", []).map_err(err)?;
        tx.commit().map_err(err)
    }
    pub(super) fn history(&self) -> Result<Vec<SourcingSearchSummary>, String> {
        let mut statement = self
            .0
            .prepare("SELECT body FROM image_searches ORDER BY fetched_at DESC,id DESC LIMIT 100")
            .map_err(err)?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?;
        rows.map(|r| {
            let s: SourcingSearch = decode(r.map_err(err)?)?;
            Ok(SourcingSearchSummary {
                id: s.id,
                name: s.name,
                fetched_at: s.fetched_at,
                count: s.products.len() as u32,
            })
        })
        .collect()
    }
    pub(super) fn search(&self, id: &str) -> Result<SourcingSearch, String> {
        decode(
            self.0
                .query_row("SELECT body FROM image_searches WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(err)?
                .ok_or("搜索记录不存在或已过期")?,
        )
    }
    pub(super) fn delete_search(&self, id: &str) -> Result<(), String> {
        self.0
            .execute("DELETE FROM image_searches WHERE id=?1", [id])
            .map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft(id: &str) -> PickDraft {
        PickDraft {
            source: PickSource::Alibaba1688,
            source_id: Some(id.into()),
            title: "杯子 100%".into(),
            url: Some(format!("https://detail.1688.com/offer/{id}.html")),
            image_url: None,
            price: Some("12.80".into()),
            currency: Some("CNY".into()),
            supplier: None,
            tags: vec!["家居".into()],
            note: "".into(),
            stage: PickStage::New,
        }
    }
    fn create(product: PickDraft) -> SavePickRequest {
        SavePickRequest {
            id: None,
            expected_revision: None,
            product,
        }
    }
    fn filter() -> PickFilter {
        PickFilter {
            keyword: "".into(),
            source: None,
            stage: None,
            page: 1,
            page_size: 20,
        }
    }
    #[test]
    fn persists_updates_deduplicates_and_rejects_stale_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sourcing.db");
        let mut db = Store::open(&path).unwrap();
        let first = db.save_pick(create(draft("123"))).unwrap();
        let mut edit = first.product.clone();
        edit.note = "保留备注".into();
        edit.stage = PickStage::Selected;
        let saved = db
            .save_pick(SavePickRequest {
                id: Some(first.id.clone()),
                expected_revision: Some(first.revision),
                product: edit.clone(),
            })
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert!(db
            .save_pick(SavePickRequest {
                id: Some(first.id.clone()),
                expected_revision: Some(first.revision),
                product: edit
            })
            .is_err());
        assert!(db.delete_pick(&first.id, first.revision).is_err());
        drop(db);
        let mut db = Store::open(&path).unwrap();
        let duplicate = db.save_pick(create(draft("123"))).unwrap();
        assert_eq!(duplicate.id, first.id);
        assert_eq!(duplicate.product.note, "保留备注");
        assert_eq!(db.list_picks(filter()).unwrap().total, 1);
        db.delete_pick(&saved.id, saved.revision).unwrap();
        assert_eq!(db.list_picks(filter()).unwrap().total, 0);
    }
    #[test]
    fn filters_literal_queries_and_reopens_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.db");
        let mut db = Store::open(&path).unwrap();
        db.save_pick(create(draft("1"))).unwrap();
        let mut other = draft("2");
        other.title = "鞋".into();
        db.save_pick(create(other)).unwrap();
        let mut f = filter();
        f.keyword = "%".into();
        assert_eq!(db.list_picks(f).unwrap().total, 1);
        let mut f = filter();
        f.page = 0;
        assert!(db.list_picks(f).is_err());
        let mut bad = draft("3");
        bad.url = Some("javascript:bad".into());
        assert!(db.save_pick(create(bad)).is_err());
        let search = SourcingSearch {
            id: "search".into(),
            name: "杯子.png".into(),
            fetched_at: now(),
            sort: ProductSort::Relevance,
            purchase_amount: 1,
            products: vec![],
            history_warning: None,
        };
        db.save_search(&search).unwrap();
        drop(db);
        let db = Store::open(&path).unwrap();
        assert_eq!(db.history().unwrap()[0].name, "杯子.png");
        assert_eq!(db.search("search").unwrap().id, "search");
        db.delete_search("search").unwrap();
        assert!(db.history().unwrap().is_empty());
    }
    #[test]
    fn failed_history_write_keeps_returned_products_available() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.db");
        let mut db = Store::open(&path).unwrap();
        db.0.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON image_searches BEGIN SELECT RAISE(ABORT, 'disk full'); END;").unwrap();
        let result = SourcingSearch {
            id: "completed-search".into(),
            name: "backpack.png".into(),
            fetched_at: now(),
            sort: ProductSort::Relevance,
            purchase_amount: 1,
            products: vec![],
            history_warning: None,
        };
        let mut result = result;
        result.products.push(serde_json::from_value(serde_json::json!({"id":"123", "title":"背包", "url":"https://detail.1688.com/offer/123.html"})).unwrap());
        let unavailable = remember_search(result.clone(), Err("无法打开数据库".into()));
        assert_eq!(unavailable.products.len(), 1);
        assert!(unavailable.history_warning.is_some());
        let returned = remember_search(result, Ok(db));
        assert_eq!(returned.products[0].id, "123");
        assert_eq!(returned.id, "completed-search");
        assert!(returned
            .history_warning
            .as_deref()
            .unwrap()
            .contains("未保存"));
        db = Store::open(&path).unwrap();
        assert!(db.history().unwrap().is_empty());
        db.0.execute_batch("DROP TRIGGER fail_history").unwrap();
        let returned = remember_search(returned, Ok(db));
        assert!(returned.history_warning.is_none());
        assert_eq!(
            Store::open(&path)
                .unwrap()
                .search("completed-search")
                .unwrap()
                .id,
            "completed-search"
        );
    }

    #[test]
    fn concurrent_collection_has_one_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.db");
        drop(Store::open(&path).unwrap());
        let handles = (0..4)
            .map(|_| {
                let p = path.clone();
                std::thread::spawn(move || {
                    Store::open(&p)
                        .unwrap()
                        .save_pick(create(draft("123")))
                        .unwrap()
                        .id
                })
            })
            .collect::<Vec<_>>();
        let ids = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), 1);
    }
}
