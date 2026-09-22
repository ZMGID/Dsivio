//! Page drafts and library organization; chat shares templates only.
pub(crate) mod wait;
use serde_json::{json, Value};
use std::sync::Mutex;
pub mod library;
static DRAFT_LOCK: Mutex<()> = Mutex::new(());
fn field<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(v.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("{key}: {e}"))
}
#[tauri::command]
pub fn studio_draft(
    domain: String,
    entry: String,
    revision: Option<u64>,
    value: Option<Value>,
) -> Result<Value, String> {
    let root = crate::app_data::app_data_dir().ok_or("无数据目录")?;
    draft_at(&root, &domain, &entry, revision, value)
}
fn draft_at(
    root: &std::path::Path,
    domain: &str,
    entry: &str,
    revision: Option<u64>,
    value: Option<Value>,
) -> Result<Value, String> {
    if !matches!(
        (domain, entry),
        ("image", "main" | "gen" | "replace" | "smart" | "design" | "client" | "workflow") | ("video", "creation" | "analysis" | "remake")
    ) {
        return Err("未知草稿入口".into());
    }
    let _guard = DRAFT_LOCK.lock().map_err(|_| "草稿锁不可用")?;
    let dir = root.join(format!("{domain}-studio")).join("drafts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{entry}.json"));
    let current: Value = if path.exists() {
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        json!({"revision":0,"value":null})
    };
    let Some(value) = value else {
        return Ok(current);
    };
    if revision != current["revision"].as_u64() {
        return Err("草稿已在其他入口更新，请重新读取后合并".into());
    }
    validate_draft(domain, &value)?;
    let next = json!({"revision":revision.unwrap_or(0)+1,"value":value});
    crate::chat::storage::atomic_write(&path, &next.to_string(), "studio draft")?;
    Ok(next)
}

fn validate_draft(domain: &str, value: &Value) -> Result<(), String> {
    if domain == "image" {
        let _: crate::workbench::image_projects::types::Brief = field(value, "brief")?;
        let _: Option<String> = field(value, "taskId")?;
        let _: Option<u64> = field(value, "revision")?;
        let _: Option<Vec<crate::workbench::image_projects::types::ImagePlan>> = field(value, "plans")?;
    } else {
        let brief = &value["brief"];
        for key in [
            "name",
            "request",
            "mode",
            "ratio",
            "route",
            "resolution",
            "language",
            "source",
        ] {
            let _: String = field(brief, key)?;
        }
        let _: Vec<String> = field(brief, "images")?;
        let _: f64 = field(brief, "duration")?;
        let _: String = field(value, "script")?;
        let _: bool = field(value, "dirty")?;
        let step: u8 = field(value, "step")?;
        if step > 2 {
            return Err("草稿步骤无效".into());
        }
        if let Some(task) = value.get("task").filter(|v| !v.is_null()) {
            let _: String = field(task, "id")?;
            let _: u64 = field(task, "revision")?;
            let _: String = field(task, "status")?;
            let _: String = field(task, "script")?;
            if !task["brief"].is_object() {
                return Err("任务缺少 brief".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_draft_roundtrip_rejects_stale_writes_and_isolates_entries() {
        let root = std::env::temp_dir().join(format!("studio-drafts-{}", uuid::Uuid::new_v4()));
        let read = draft_at(&root, "image", "main", None, None).unwrap();
        assert_eq!(read["revision"], 0);
        let brief = json!({"feature":"gen","name":"test","requirement":"page","language":"zh-CN","platform":"","ratio":"1:1","resolution":"1k","count":1,"style":"","products":[]});
        let saved = draft_at(
            &root,
            "image",
            "main",
            Some(0),
            Some(json!({"brief":brief})),
        )
        .unwrap();
        assert_eq!(saved["revision"], 1);
        assert!(draft_at(&root, "image", "main", Some(0), Some(json!({"brief":{}}))).is_err());
        assert_eq!(
            draft_at(&root, "image", "main", None, None).unwrap()["value"]["brief"]["requirement"],
            "page"
        );
        assert_eq!(
            draft_at(&root, "video", "creation", None, None).unwrap()["revision"],
            0
        );
        assert!(draft_at(&root, "image", "../main", None, None).is_err());
        assert!(draft_at(&root, "image", "main", Some(1), Some(json!({"wrong":true}))).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
