//! Smart sourcing owns image-search sessions and the persisted pick library.
mod alibaba;
mod store;
pub mod types;
use store::Store;
use types::*;

fn open_store() -> Result<Store, String> {
    Store::open(
        &crate::app_data::app_data_dir()
            .ok_or("无法定位应用数据目录")?
            .join("sourcing.sqlite3"),
    )
}
async fn storage<T: Send + 'static>(
    f: impl FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&mut open_store()?))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn sourcing_history() -> Result<Vec<SourcingSearchSummary>, String> {
    storage(|db| db.history()).await
}
#[tauri::command]
pub async fn sourcing_get_search(id: String) -> Result<SourcingSearch, String> {
    storage(move |db| db.search(&id)).await
}
#[tauri::command]
pub async fn sourcing_delete_search(id: String) -> Result<(), String> {
    storage(move |db| db.delete_search(&id)).await
}
#[tauri::command]
pub async fn sourcing_list_picks(filter: PickFilter) -> Result<PickPage, String> {
    storage(move |db| db.list_picks(filter)).await
}
#[tauri::command]
pub async fn sourcing_save_pick(request: SavePickRequest) -> Result<PickItem, String> {
    storage(move |db| db.save_pick(request)).await
}
#[tauri::command]
pub async fn sourcing_delete_pick(id: String, revision: u32) -> Result<(), String> {
    storage(move |db| db.delete_pick(&id, revision)).await
}

#[tauri::command]
pub async fn sourcing_search(
    state: tauri::State<'_, crate::state::AppState>,
    request: LookalikeRequest,
) -> Result<SourcingSearch, String> {
    let ak = state.settings_read().sourcing.alibaba_ak.clone();
    let result = alibaba::search(ak, request).await?;
    tauri::async_runtime::spawn_blocking(move || store::remember_search(result, open_store()))
        .await
        .map_err(|_| "处理搜索记录失败")
        .map_err(str::to_owned)
}

/// Exercises the actual desktop command functions; leaves no test pick behind.
#[cfg(debug_assertions)]
pub(crate) async fn probe_local_sourcing(
    app: &tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    let configured = !state.settings_read().sourcing.alibaba_ak.is_empty();
    let token = uuid::Uuid::new_v4().to_string();
    let draft = PickDraft {
        source: PickSource::Manual,
        source_id: Some(token.clone()),
        title: format!("sourcing-probe-{token}"),
        url: None,
        image_url: None,
        price: Some("12.80".into()),
        currency: Some("CNY".into()),
        supplier: None,
        tags: vec!["probe".into()],
        note: String::new(),
        stage: PickStage::New,
    };
    let created = sourcing_save_pick(SavePickRequest {
        id: None,
        expected_revision: None,
        product: draft.clone(),
    })
    .await?;
    let mut cleanup_revision = created.revision;
    let check = async {
        let duplicate = sourcing_save_pick(SavePickRequest { id: None, expected_revision: None, product: draft }).await?;
        if duplicate.id != created.id { return Err("重复收藏未去重".to_string()); }
        let page = sourcing_list_picks(PickFilter { keyword: token, stage: None, source: None, page: 1, page_size: 20 }).await?;
        if page.total != 1 || page.items[0].id != created.id { return Err("持久化读取失败".into()); }
        let mut edited_draft = created.product.clone();
        edited_draft.note = "probe-updated".into();
        edited_draft.stage = PickStage::Selected;
        let edited = sourcing_save_pick(SavePickRequest { id: Some(created.id.clone()), expected_revision: Some(created.revision), product: edited_draft.clone() }).await?;
        cleanup_revision = edited.revision;
        let stale = sourcing_save_pick(SavePickRequest { id: Some(created.id.clone()), expected_revision: Some(created.revision), product: edited_draft }).await;
        if stale.is_ok() { return Err("过期编辑未被拒绝".into()); }
        let filtered = sourcing_list_picks(PickFilter { keyword: created.product.title.clone(), stage: Some(PickStage::Rejected), source: None, page: 1, page_size: 20 }).await?;
        if filtered.total != 0 { return Err("状态筛选失败".into()); }
        let updated = sourcing_list_picks(PickFilter { keyword: created.product.title.clone(), stage: Some(PickStage::Selected), source: Some(PickSource::Manual), page: 1, page_size: 20 }).await?;
        if updated.items.first().is_none_or(|p| p.id != created.id || p.product.note != "probe-updated") { return Err("修改后的选品未持久化".into()); }
        let missing_ak = if !configured {
            sourcing_search(state, LookalikeRequest { image: String::new(), name: "probe".into(), sort: ProductSort::Relevance, limit: 1, purchase_amount: 1 }).await.err()
        } else { None };
        let history = sourcing_history().await?;
        Ok(serde_json::json!({"persisted": true, "deduplicated": true, "updated":true, "staleRejected":true, "filtered":true, "akConfigured": configured, "missingAkError": missing_ak, "historyCount": history.len()}))
    }.await;
    sourcing_delete_pick(created.id, cleanup_revision).await?;
    check
}

/// Explicit live test through the same settings owner and sourcing command as the UI.
#[cfg(debug_assertions)]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveProbe {
    pub ak_path: Option<std::path::PathBuf>,
    pub image_path: std::path::PathBuf,
}
#[cfg(debug_assertions)]
pub(crate) async fn probe_live_sourcing(
    app: &tauri::AppHandle,
    input: LiveProbe,
) -> Result<serde_json::Value, String> {
    use base64::Engine as _;
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    if let Some(path) = input.ak_path {
        let ak = std::fs::read_to_string(path).map_err(|_| "无法读取测试 AK 文件")?;
        alibaba::validate_key(ak.trim())?;
        crate::settings::update_settings(app, &state, |settings| {
            settings.sourcing.alibaba_ak = ak.trim().to_owned();
            Ok(())
        })
        .map_err(|_| "1688 设置保存失败")?;
    }
    let bytes = std::fs::read(&input.image_path).map_err(|_| "无法读取测试图片")?;
    let format = image::guess_format(&bytes).map_err(|_| "测试图片格式无效")?;
    let mime = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::WebP => "webp",
        _ => return Err("测试图片格式不支持".into()),
    };
    let result = sourcing_search(
        state,
        LookalikeRequest {
            image: format!(
                "data:image/{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
            name: "1688 接入验收样图".into(),
            sort: ProductSort::Relevance,
            limit: 10,
            purchase_amount: 1,
        },
    )
    .await?;
    let saved = sourcing_get_search(result.id.clone()).await?;
    if serde_json::to_value(&saved.products).ok() != serde_json::to_value(&result.products).ok() {
        return Err("搜索快照与返回不一致".into());
    }
    Ok(
        serde_json::json!({"searchId":result.id,"count":result.products.len(),"persisted":true,"products":result.products}),
    )
}
