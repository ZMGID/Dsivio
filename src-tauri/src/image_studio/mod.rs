//! Built-in image workspace: typed input, Agent planning, persisted sample gates,
//! and native image execution. Does not depend on installed skills or Python.
pub(crate) mod agent;
mod builtins;
mod engine;
mod generation;
mod imports;
mod output;
mod preparation;
mod storage;
#[cfg(test)]
mod tests;
pub mod types;
mod workflow;

use crate::state::AppState;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tauri::{AppHandle, Manager};
use types::*;

static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

/// Absolute files stay as-is; studio assets (`assets/…`) resolve inside the workspace.
pub(crate) fn resolve_existing_image(path: &str) -> Result<PathBuf, String> {
    let raw = PathBuf::from(path);
    if raw.is_file() {
        return Ok(raw);
    }
    storage::resolve(path)
}

pub fn initialize_skill_workspace(app: &AppHandle) -> Result<(), String> {
    let _guard = lock()?;
    storage::templates()?;
    let settings_file = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    storage::write(
        &storage::root()?.join("runtime.json"),
        &json!({ "settingsFile": settings_file }),
    )
}
fn active() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    ACTIVE.get_or_init(Default::default)
}
fn lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    storage::STORE_LOCK
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "图片存储锁不可用".into())
}
fn check_idle(id: &str) -> Result<(), String> {
    if active()
        .lock()
        .map_err(|_| "任务锁不可用")?
        .contains_key(id)
    {
        Err("任务正在执行，请等待或停止后修改".into())
    } else {
        Ok(())
    }
}
fn check_revision(task: &Task, revision: u64) -> Result<(), String> {
    if task.revision != revision {
        Err("此任务已在其他窗口更新，请重新打开后操作".into())
    } else {
        Ok(())
    }
}
fn persist(task: &mut Task) -> Result<(), String> {
    let _lock = lock()?;
    storage::save_task(task)
}

#[tauri::command]
pub fn image_studio_bootstrap(app: AppHandle) -> Result<Value, String> {
    let _lock = lock()?;
    let mut tasks = Vec::<Task>::new();
    for e in fs::read_dir(storage::root()?.join("tasks"))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if e.path().extension().is_some_and(|e| e == "json") {
            tasks.push(recover(storage::read(&e.path())?)?);
        }
    }
    tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let settings = app.state::<AppState>().settings_read().clone();
    let providers:Vec<_>=settings.providers.iter().filter(|p|p.enabled).map(|p|json!({"id":p.id,"name":p.name,"models":p.available_models,"ready":p.has_credentials()})).collect();
    let mut config = storage::config()?;
    config.output_root = output::root(&config)?.to_string_lossy().into();
    Ok(
        json!({"tasks":tasks,"templates":storage::templates()?,"config":config,"providers":providers}),
    )
}
fn recover(mut t: Task) -> Result<Task, String> {
    if t.status == "running"
        && !active()
            .lock()
            .map_err(|_| "任务锁不可用")?
            .contains_key(&t.id)
    {
        t.status = "interrupted".into();
        t.progress = "应用退出时任务未完成。已有结果已保留；远程任务可恢复查询。".into();
        for result in &mut t.results {
            if result.path.is_none() && result.remote_id.is_none() && result.error.is_none() {
                result.error = Some("请求可能已提交；请先核对供应商记录再重试".into());
            }
        }
        storage::save_task(&mut t)?;
    }
    Ok(t)
}
#[tauri::command]
pub fn image_studio_get(id: String) -> Result<Task, String> {
    let _lock = lock()?;
    recover(storage::load_task(&id)?)
}

fn validate_brief(b: &Brief) -> Result<(), String> {
    if !matches!(
        b.feature.as_str(),
        "gen" | "replace" | "smart" | "design" | "client" | "workflow"
    ) {
        return Err("未知图片功能".into());
    }
    if b.count == 0 || b.count > 30 || b.products.len() > 200 {
        return Err("每套 1–30 页、每批最多 200 个商品".into());
    }
    let mut ids = HashSet::new();
    if b.feature == "workflow" || b.workflow_input.is_some() {
        workflow::validate_input(b)?;
    }
    for p in &b.products {
        if !ids.insert(&p.id) {
            return Err("商品 ID 重复".into());
        }
        if p.assets.len() > 18
            || p.assets
                .iter()
                .filter(|a| !a.name.starts_with("__dsimage_"))
                .count()
                > 16
        {
            return Err("每款最多 16 张参考图".into());
        }
        for a in &p.assets {
            if !a.path.starts_with("assets/") {
                return Err("商品素材必须先导入工作台".into());
            }
            storage::resolve(&a.path)?;
        }
        for id in [&p.front, &p.back].into_iter().flatten() {
            if !p.assets.iter().any(|a| &a.id == id) {
                return Err("正反面指向无效素材".into());
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub fn image_studio_save(
    id: Option<String>,
    revision: Option<u64>,
    mut brief: Brief,
) -> Result<Task, String> {
    let _lock = lock()?;
    validate_brief(&brief)?;
    let mut t = if let Some(id) = id {
        check_idle(&id)?;
        let t = storage::load_task(&id)?;
        check_revision(&t, revision.unwrap_or(0))?;
        t
    } else {
        Task {
            id: storage::id(),
            revision: 0,
            created_at: storage::now(),
            updated_at: storage::now(),
            brief: brief.clone(),
            plans: vec![],
            results: vec![],
            approved_groups: vec![],
            status: "draft".into(),
            progress: String::new(),
            error: None,
            templates: vec![],
            output_directory: None,
            workflow: None,
            materials: HashMap::new(),
        }
    };
    output::prepare(&mut t, &storage::config()?)?;
    if preparation::unresolved(&t) && {
        let mut previous = t.brief.clone();
        previous.name = brief.name.clone();
        previous != brief
    } {
        return Err("商品素材仍有未完成的生成请求，请先继续当前任务".into());
    }
    if brief.feature == "workflow" {
        workflow::save_brief(&mut t, brief)?;
        storage::save_task(&mut t)?;
        return Ok(t);
    }
    let mut comparable = t.brief.clone();
    comparable.name = brief.name.clone();
    if t.revision == 0 || comparable != brief {
        // A freshly designed set follows the new brief; do not silently reuse its old generated rules.
        let new_rules = t.brief.requirement != brief.requirement
            || t.brief.style != brief.style
            || t.brief.count != brief.count
            || t.brief.language != brief.language
            || t.brief.platform != brief.platform
            || t.brief.ratio != brief.ratio
            || t.brief.resolution != brief.resolution
            || t.brief.workflow_input != brief.workflow_input;
        if new_rules
            && brief.template_id.is_none()
            && matches!(brief.feature.as_str(), "design" | "client" | "replace")
        {
            for product in &mut brief.products {
                product.template_id = None;
            }
        }
        t.revision += 1;
        t.plans.clear();
        t.approved_groups.clear();
        t.status = "draft".into();
        t.progress = "草稿已保存".into();
        t.error = None;
        let all = storage::templates()?;
        let ids: HashSet<_> = brief
            .products
            .iter()
            .filter_map(|p| p.template_id.as_ref())
            .chain(brief.template_id.iter())
            .collect();
        t.templates = all
            .into_iter()
            .filter(|tpl| ids.contains(&tpl.id))
            .collect();
    }
    t.brief = brief;
    storage::save_task(&mut t)?;
    Ok(t)
}
#[tauri::command]
pub fn image_studio_save_plans(
    id: String,
    revision: u64,
    plans: Vec<ImagePlan>,
) -> Result<Task, String> {
    let _lock = lock()?;
    check_idle(&id)?;
    let mut t = storage::load_task(&id)?;
    check_revision(&t, revision)?;
    if t.brief.feature == "workflow" {
        return Err("请在制作与试品中修改共用规则，或只修改一张结果图".into());
    }
    if plans.len() != t.plans.len() {
        return Err("不能删除或添加已规划的页面，请修改需求后重新规划".into());
    }
    for (p, old) in plans.iter().zip(&t.plans) {
        if p.product_id != old.product_id
            || p.slot_id != old.slot_id
            || p.refs != old.refs
            || p.prompt.trim().is_empty()
        {
            return Err("方案身份或参考图不能在提示词编辑时改变".into());
        }
    }
    t.revision += 1;
    t.plans = plans;
    t.approved_groups.clear();
    t.status = "planned".into();
    t.error = None;
    storage::save_task(&mut t)?;
    Ok(t)
}

#[tauri::command]
pub async fn image_studio_import(
    paths: Vec<String>,
    as_products: bool,
) -> Result<Vec<Product>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let grouped = imports::collect(&paths, as_products)?;
        let mut products = vec![];
        for (name, files) in grouped {
            let assets: Vec<_> = files
                .iter()
                .map(|p| storage::import_asset(p))
                .collect::<Result<_, _>>()?;
            let back = assets
                .iter()
                .find(|a| {
                    let n = a.name.to_lowercase();
                    n.contains("back") || n.contains("背面") || n.contains("反面")
                })
                .map(|a| a.id.clone());
            let front = assets
                .iter()
                .find(|a| {
                    let n = a.name.to_lowercase();
                    n.contains("front") || n.contains("正面")
                })
                .or_else(|| assets.iter().find(|a| Some(&a.id) != back.as_ref()))
                .map(|a| a.id.clone());
            products.push(Product {
                id: storage::id(),
                name,
                category: "未分类".into(),
                kind: String::new(),
                facts: String::new(),
                front,
                back,
                assets,
                template_id: None,
            });
        }
        if products.is_empty() {
            return Err("未找到 PNG、JPEG 或 WebP 图片".into());
        }
        Ok(products)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn image_studio_config(config: StudioConfig) -> Result<(), String> {
    output::root(&config)?;
    let _lock = lock()?;
    if config.agent_provider_id.is_empty() != config.agent_model.is_empty() {
        return Err("Agent 供应商和模型需要一起选择，或同时留空使用当前聊天模型".into());
    }
    storage::write(&storage::root()?.join("config.json"), &config)
}
#[tauri::command]
pub async fn image_studio_preview(path: String, original: Option<bool>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || storage::preview(&path, original.unwrap_or(false)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn image_studio_action(
    app: AppHandle,
    id: String,
    revision: u64,
    action: Action,
) -> Result<Task, String> {
    let _lock = lock()?;
    if action.kind == "cancel" {
        if let Some(flag) = active().lock().map_err(|_| "任务锁不可用")?.get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
        let mut t = storage::load_task(&id)?;
        t.progress = "正在停止，等待已提交的图片收尾并保存…".into();
        storage::save_task(&mut t)?;
        return Ok(t);
    }
    check_idle(&id)?;
    let mut t = storage::load_task(&id)?;
    check_revision(&t, revision)?;
    if t.brief.feature == "workflow" || workflow::is_action(&action.kind) {
        workflow::validate_action(&t, &action)?;
    }
    if action.kind == "approve" {
        if !samples_complete(&t, &action.group) {
            return Err(
                "该分类的两款样品（不足两款时为全部商品）尚未全部成功，不能批量放行".into(),
            );
        }
        if !t.approved_groups.contains(&action.group) {
            t.approved_groups.push(action.group);
        }
        t.progress = "样品已通过，可为剩余商品规划并批量出图".into();
        storage::save_task(&mut t)?;
        return Ok(t);
    }
    if !matches!(
        action.kind.as_str(),
        "start"
            | "plan"
            | "classify"
            | "sample"
            | "bulk"
            | "generate"
            | "retry"
            | "revise"
            | "resume"
            | "review"
            | "template"
    ) && !workflow::is_action(&action.kind)
    {
        return Err("未知图片操作".into());
    }
    if t.brief.products.is_empty() && !workflow::is_action(&action.kind) {
        if t.brief.feature == "gen" {
            t.brief.products.push(Product {
                id: storage::id(),
                name: "创作".into(),
                category: "未分类".into(),
                kind: String::new(),
                facts: String::new(),
                front: None,
                back: None,
                assets: vec![],
                template_id: None,
            });
        } else {
            return Err("请先导入商品图片".into());
        }
    }
    if !workflow::is_action(&action.kind)
        && t.brief.requirement.trim().is_empty()
        && t.brief.template_id.is_none()
        && t.brief.products.iter().all(|p| p.template_id.is_none())
        && !t
            .brief
            .workflow_input
            .as_ref()
            .is_some_and(|input| !input.sources.is_empty())
    {
        return Err("请填写图片要求或选择模板".into());
    }
    if action.kind == "bulk" && !t.approved_groups.contains(&action.group) {
        return Err("请先确认该分类样品通过".into());
    }
    if action.kind == "generate" && t.brief.feature != "gen" {
        return Err("套图任务必须先走样品确认".into());
    }
    if matches!(action.kind.as_str(), "sample" | "generate") && t.plans.is_empty() {
        return Err("请先生成并检查方案".into());
    }
    let cfg = storage::config()?;
    output::prepare(&mut t, &cfg)?;
    if matches!(
        action.kind.as_str(),
        "start" | "sample" | "bulk" | "generate" | "retry" | "revise"
    ) || matches!(
        action.kind.as_str(),
        "workflow_trial" | "workflow_refine" | "workflow_edit" | "workflow_produce"
    ) {
        engine::validate(&cfg, &t.brief)?;
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    active()
        .lock()
        .map_err(|_| "任务锁不可用")?
        .insert(id.clone(), cancelled.clone());
    t.status = "running".into();
    t.error = None;
    t.progress = "任务已开始".into();
    if let Err(e) = storage::save_task(&mut t) {
        active().lock().map_err(|_| "任务锁不可用")?.remove(&id);
        return Err(e);
    }
    let response = t.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = execute(&app, &mut t, &cfg, &action, &cancelled).await;
        if let Ok(_lock) = lock() {
            t.status = if cancelled.load(Ordering::Relaxed) {
                "stopped"
            } else if outcome.is_err() {
                "error"
            } else {
                "ready"
            }
            .into();
            t.error = outcome.err();
            if t.error.is_some() {
                t.progress = "本步骤未完成，已有结果已保存".into();
            } else if cancelled.load(Ordering::Relaxed) {
                t.progress = "已停止，已有结果已保存".into();
            }
            let _ = storage::save_task(&mut t);
            if let Ok(mut running) = active().lock() {
                running.remove(&id);
            }
        }
    });
    Ok(response)
}

fn stopped(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::Relaxed) {
        Err("已停止".into())
    } else {
        Ok(())
    }
}
async fn execute(
    app: &AppHandle,
    t: &mut Task,
    cfg: &StudioConfig,
    a: &Action,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    if a.kind == "start" {
        if t.brief.feature != "gen" {
            for index in 0..t.brief.products.len() {
                preparation::identify(app, t, index, cfg, flag).await?;
            }
            preparation::templates(app, t, cfg, flag).await?;
        }
        let groups: std::collections::BTreeSet<_> = t.brief.products.iter().map(group_of).collect();
        for group in groups {
            let sample_ids = sample_ids(t, &group);
            if t.brief.feature != "gen" {
                for index in 0..t.brief.products.len() {
                    if sample_ids.contains(&t.brief.products[index].id) {
                        preparation::prepare_product(app, t, index, cfg, flag).await?;
                    }
                }
            }
            execute_step(
                app,
                t,
                cfg,
                &Action {
                    kind: "plan".into(),
                    group: group.clone(),
                    ..Default::default()
                },
                flag,
            )
            .await?;
            let kind = if t.brief.feature == "gen" {
                "generate"
            } else {
                "sample"
            };
            let pending = t.plans.iter().any(|plan| {
                (t.brief.feature == "gen" || sample_ids.contains(&plan.product_id))
                    && !t.results.iter().any(|r| {
                        r.revision == t.revision
                            && r.product_id == plan.product_id
                            && r.slot_id == plan.slot_id
                    })
            });
            if pending {
                execute_step(
                    app,
                    t,
                    cfg,
                    &Action {
                        kind: kind.into(),
                        group,
                        ..Default::default()
                    },
                    flag,
                )
                .await?;
            }
            if t.brief.feature == "gen" {
                break;
            }
        }
        return Ok(());
    }
    if a.kind == "bulk" {
        for index in 0..t.brief.products.len() {
            if group_of(&t.brief.products[index]) == a.group {
                preparation::prepare_product(app, t, index, cfg, flag).await?;
            }
        }
    }
    execute_step(app, t, cfg, a, flag).await
}

async fn execute_step(
    app: &AppHandle,
    t: &mut Task,
    cfg: &StudioConfig,
    a: &Action,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    if workflow::is_action(&a.kind) {
        return workflow::execute(app, t, cfg, a, flag).await;
    }
    if matches!(a.kind.as_str(), "retry" | "revise") {
        if let Some(w) = &mut t.workflow {
            if w.sample_ids.contains(&a.product_id) {
                w.approved_version = None;
            }
        }
    }
    if a.kind == "template" {
        let product = t
            .brief
            .products
            .iter()
            .find(|p| p.id == a.product_id)
            .ok_or("请选择已有方案的商品")?;
        let plans: Vec<_> = t
            .plans
            .iter()
            .filter(|p| p.product_id == product.id)
            .collect();
        if plans.is_empty() {
            return Err("请先完成画面方案，再保存为规则模板".into());
        }
        let images = agent::product_images(product);
        let input = json!({"name":t.brief.name,"requirement":t.brief.requirement,"language":t.brief.language,"style":t.brief.style,"product":product,"plans":plans,"ratio":t.brief.ratio,"resolution":t.brief.resolution});
        t.progress = "Agent 正在提取可复用的风格与页面规则…".into();
        persist(t)?;
        let data = agent::run(app,&t.id,cfg,"Extract a reusable SMART template from the reviewed plan. Return exactly {\"name\":\"Chinese template name\",\"mode\":\"smart\",\"category\":\"applicable product category\",\"language\":\"requested language\",\"style\":\"complete shared visual rules\",\"output\":{\"ratio\":\"input ratio\",\"resolution\":\"input resolution\"},\"slots\":[{\"id\":\"original slot id\",\"purpose\":\"Chinese purpose\",\"brief\":\"reusable visual objective and composition, explicitly adapted to each new product\"}],\"notes\":\"Chinese review advice\"}. Keep every original slot, in order. Remove source-SKU-specific dimensions, claims, colors and copy. Preserve reusable art direction. Do not output example file paths, refs, derived backs, or locked product facts.",input,images,flag.clone()).await?;
        storage::validate_template(&data)?;
        let original_ids: Vec<_> = t
            .plans
            .iter()
            .filter(|p| p.product_id == a.product_id)
            .map(|p| p.slot_id.as_str())
            .collect();
        let slot_ids: Vec<_> = data["slots"]
            .as_array()
            .ok_or("模板没有页面")?
            .iter()
            .filter_map(|s| s["id"].as_str())
            .collect();
        if data["mode"] != "smart" || original_ids != slot_ids {
            return Err("Agent 返回的模板类型或页面不匹配，请重新提取规则".into());
        }
        let tid = storage::id();
        let directory = format!("templates/{tid}");
        let dest = storage::root()?.join(&directory);
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let template = Template {
            id: tid,
            data,
            directory,
            builtin: false,
        };
        storage::save_template(&template)?;
        t.progress = "规则模板已保存到模板库，请检查后复用".into();
        return Ok(());
    }
    if a.kind == "classify" {
        for i in 0..t.brief.products.len() {
            stopped(flag)?;
            let p = t.brief.products[i].clone();
            t.progress = format!("识别商品 {}/{}：{}", i + 1, t.brief.products.len(), p.name);
            persist(t)?;
            let v=agent::run(app,&t.id,cfg,"Identify this product conservatively. Return {\"category\":\"short Chinese category\",\"kind\":\"a matching product_kinds key from available templates, otherwise empty\",\"facts\":\"Chinese observed facts only; explicitly mention uncertain details\"}. Use consistent categories across products. Do not invent specifications.",json!({"product":p,"existingCategories":t.brief.products.iter().map(|p|&p.category).collect::<Vec<_>>(),"templates":t.templates.iter().map(|t|&t.data).collect::<Vec<_>>()}),agent::product_images(&p),flag.clone()).await?;
            let category = v["category"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or("Agent 未返回商品分类")?;
            t.brief.products[i].category = category.into();
            t.brief.products[i].kind = v["kind"].as_str().unwrap_or("").into();
            if t.brief.products[i].facts.trim().is_empty() {
                t.brief.products[i].facts = v["facts"].as_str().unwrap_or("").into();
            }
            t.plans.clear();
            t.approved_groups.clear();
            t.revision += 1;
            persist(t)?;
        }
        t.progress = "商品识别完成，请检查分类、正反面和已知信息，再选择模板".into();
        return Ok(());
    }
    if a.kind == "plan" || a.kind == "bulk" {
        let products: Vec<_> = t
            .brief
            .products
            .iter()
            .filter(|p| {
                if t.brief.feature == "gen" {
                    return true;
                }
                if group_of(p) != a.group {
                    return false;
                }
                a.kind == "bulk" || sample_ids(t, &a.group).contains(&p.id)
            })
            .cloned()
            .collect();
        if products.is_empty() {
            return Err("该分类没有商品".into());
        }
        for p in products {
            stopped(flag)?;
            if t.plans.iter().any(|plan| plan.product_id == p.id) {
                continue;
            }
            t.progress = format!("Agent 正在规划 {} 的每页画面…", p.name);
            persist(t)?;
            let plans = agent::plan(app, t, &p, cfg, flag.clone()).await?;
            t.plans.extend(plans);
            persist(t)?;
        }
        if a.kind == "plan" {
            t.progress = "方案已生成，可逐页编辑提示词，再生成样品".into();
            return Ok(());
        }
    }
    if a.kind == "review" {
        let index = t
            .results
            .iter()
            .position(|r| r.id == a.result_id)
            .ok_or("找不到图片版本")?;
        let r = &t.results[index];
        let path = r.path.clone().ok_or("图片尚未成功")?;
        let p = t
            .brief
            .products
            .iter()
            .find(|p| p.id == r.product_id)
            .ok_or("找不到商品")?;
        let mut images = agent::product_images(p);
        images.push(("待检查的生成图片".into(), path));
        let v=agent::run(app,&t.id,cfg,"Review the generated image against the actual product and requested prompt. Return {\"summary\":\"Chinese actionable review of identity, geometry, text spelling/language, layout and any invented claims; distinguish uncertainty\",\"pass\":false}. Passing is advisory only; never approve the batch on behalf of the user.",json!({"prompt":r.prompt,"product":p,"language":t.brief.language}),images,flag.clone()).await?;
        t.results[index].review = Some(v["summary"].as_str().ok_or("Agent 未返回质检说明")?.into());
        t.progress = "质检意见已附在该图片版本上，请人工确认样品".into();
        return Ok(());
    }
    if a.kind == "resume" {
        let index = t
            .results
            .iter()
            .position(|r| r.id == a.result_id)
            .ok_or("找不到远程图片任务")?;
        if t.results[index].path.is_some() {
            return Err("该图片已保存，无需恢复".into());
        }
        if let Err(e) = resume(app, t, index, flag).await {
            t.results[index].error = Some(e.clone());
            return Err(e);
        }
        t.progress = "远程图片已保存".into();
        return Ok(());
    }
    let retry = matches!(a.kind.as_str(), "retry" | "revise");
    let mut plans: Vec<_> = t
        .plans
        .iter()
        .filter(|plan| {
            if retry {
                return plan.product_id == a.product_id && plan.slot_id == a.slot_id;
            }
            let Some(p) = t.brief.products.iter().find(|p| p.id == plan.product_id) else {
                return false;
            };
            if t.brief.feature != "gen" && group_of(p) != a.group {
                return false;
            }
            if a.kind == "sample" && !sample_ids(t, &a.group).contains(&p.id) {
                return false;
            }
            !t.results.iter().any(|r| {
                r.revision == t.revision
                    && r.product_id == plan.product_id
                    && r.slot_id == plan.slot_id
            })
        })
        .cloned()
        .collect();
    if plans.is_empty() {
        return Err("没有待生成的页面。失败的页面请单独重试；已有远程任务请恢复查询。".into());
    }
    for plan in &mut plans {
        stopped(flag)?;
        if retry {
            if t.results
                .iter()
                .rev()
                .find(|r| {
                    r.product_id == plan.product_id
                        && r.slot_id == plan.slot_id
                        && r.revision == t.revision
                })
                .is_some_and(|r| {
                    r.remote_id.is_some()
                        && r.path.is_none()
                        && !r
                            .error
                            .as_deref()
                            .is_some_and(|e| e.starts_with("远程图片任务失败"))
                })
            {
                return Err("该页已有远程任务，请先恢复查询，避免重复计费".into());
            }
            if let Some(p) = t.brief.products.iter().find(|p| p.id == plan.product_id) {
                t.approved_groups.retain(|g| g != &group_of(p));
            }
        }
        if a.kind == "revise" {
            if a.note.trim().is_empty() {
                return Err("请填写这一张图要修改的内容".into());
            }
            let previous = t
                .results
                .iter()
                .rev()
                .find(|r| {
                    r.product_id == plan.product_id
                        && r.slot_id == plan.slot_id
                        && r.path.is_some()
                        && (a.result_id.is_empty() || r.id == a.result_id)
                })
                .ok_or("该页没有可修改的图片")?;
            plan.refs
                .insert(0, previous.path.clone().unwrap_or_default());
            plan.prompt=format!("Edit the FIRST image. Preserve everything except the requested change. Remaining images are ground-truth product references. Requested change: {}\nOriginal plan: {}",a.note,plan.prompt);
        }
    }
    let task_id = t.id.clone();
    let brief = t.brief.clone();
    let backend = engine::NativeBackend {
        app,
        cfg,
        task_id: &task_id,
        brief: &brief,
    };
    generation::run(t, cfg, plans, flag, &backend, persist).await?;
    t.progress = if t.brief.feature == "gen" {
        "图片已生成，可逐张修改、质检或导出"
    } else {
        "本轮图片已保存，请检查结果；两款样品通过后可继续批量"
    }
    .into();
    Ok(())
}
async fn resume(
    app: &AppHandle,
    t: &mut Task,
    index: usize,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    let remote = t.results[index]
        .remote_id
        .clone()
        .ok_or("此版本没有远程任务编号；请核对供应商记录")?;
    let cfg = t.results[index].config.clone();
    t.progress = "远程服务正在生图，任务编号已保存，可离开此页面".into();
    persist(t)?;
    let backend = engine::NativeBackend {
        app,
        cfg: &cfg,
        task_id: &t.id,
        brief: &t.brief,
    };
    let bytes = generation::resume(&backend, &cfg, &remote, flag).await?;
    engine::store_image(&t.id, &mut t.results[index], &bytes)
}

#[tauri::command]
pub async fn image_studio_template_import(path: String) -> Result<Template, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        let base = if path.is_dir() {
            path
        } else {
            path.parent().ok_or("无效模板路径")?
        };
        let base = base.canonicalize().map_err(|e| e.to_string())?;
        let data: Value = storage::read(&base.join("template.json"))?;
        storage::validate_template(&data)?;
        let template_id = storage::id();
        let directory = format!("templates/{template_id}");
        let dest = storage::root()?.join(&directory);
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        fn collect(v: &Value, out: &mut HashSet<String>) {
            match v {
                Value::Object(m) => {
                    for (k, v) in m {
                        if k == "example" {
                            if let Some(s) = v.as_str() {
                                out.insert(s.into());
                            }
                        } else if k == "refs" || k == "refs_by_kind" {
                            collect_refs(v, out);
                        } else {
                            collect(v, out);
                        }
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        collect(v, out);
                    }
                }
                _ => {}
            }
        }
        fn collect_refs(v: &Value, out: &mut HashSet<String>) {
            match v {
                Value::String(s) => {
                    if !s.starts_with('@') {
                        out.insert(s.into());
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        collect_refs(v, out);
                    }
                }
                Value::Object(m) => {
                    for v in m.values() {
                        collect_refs(v, out);
                    }
                }
                _ => {}
            }
        }
        let mut refs = HashSet::new();
        collect(&data, &mut refs);
        for r in refs {
            let rel = Path::new(&r);
            if rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("模板资产只能使用模板目录内的相对路径".into());
            }
            let source = base
                .join(rel)
                .canonicalize()
                .map_err(|_| format!("模板素材不存在：{r}"))?;
            if !source.starts_with(&base) {
                return Err("模板素材超出目录".into());
            }
            if fs::metadata(&source).map_err(|e| e.to_string())?.len() > 50 * 1024 * 1024 {
                return Err("模板图片超过 50 MB".into());
            }
            let bytes = fs::read(source).map_err(|e| e.to_string())?;
            storage::decode(&bytes)?;
            let target = dest.join(rel);
            if let Some(p) = target.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            fs::write(target, bytes).map_err(|e| e.to_string())?;
        }
        let t = Template {
            id: template_id,
            data,
            directory,
            builtin: false,
        };
        let _lock = lock()?;
        storage::save_template(&t)?;
        Ok(t)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn image_studio_template_save(mut template: Template) -> Result<Template, String> {
    let _lock = lock()?;
    storage::validate_template(&template.data)?;
    if template.id.is_empty() || template.builtin {
        let source_builtin = template.builtin.then(|| template.id.clone());
        template.id = storage::id();
        template.directory = format!("templates/{}", template.id);
        template.builtin = false;
        fs::create_dir_all(storage::root()?.join(&template.directory))
            .map_err(|e| e.to_string())?;
        if let Some(source_id) = source_builtin {
            builtins::install_assets(&source_id, &storage::root()?.join(&template.directory))?;
        }
    } else {
        let old = storage::templates()?
            .into_iter()
            .find(|t| t.id == template.id)
            .ok_or("模板不存在")?;
        template.directory = old.directory;
    }
    // Validate all slot references before accepting a replacement template.
    if template.data["mode"] == "replace" {
        for s in template.data["slots"].as_array().ok_or("缺少页面")? {
            storage::resolve(&format!(
                "{}/{}",
                template.directory,
                s["example"].as_str().unwrap_or_default()
            ))?;
        }
    }
    storage::save_template(&template)?;
    Ok(template)
}

#[tauri::command]
pub fn image_studio_freeze(
    id: String,
    product_id: String,
    name: String,
) -> Result<Template, String> {
    let _lock = lock()?;
    check_idle(&id)?;
    let t = storage::load_task(&id)?;
    let plans: Vec<_> = t
        .plans
        .iter()
        .filter(|p| p.product_id == product_id)
        .collect();
    if plans.is_empty() {
        return Err("该商品没有完整方案".into());
    }
    let tid = storage::id();
    let directory = format!("templates/{tid}");
    let dest = storage::root()?.join(&directory);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut slots = vec![];
    for (i, plan) in plans.iter().enumerate() {
        let result = t
            .results
            .iter()
            .rev()
            .find(|r| {
                r.revision == t.revision && r.product_id == product_id && r.slot_id == plan.slot_id
            })
            .filter(|r| r.path.is_some())
            .ok_or("请先完成该商品全部页面再冻结为模板")?;
        let file = format!("example-{}.png", i + 1);
        let bytes = fs::read(storage::resolve(result.path.as_ref().unwrap())?)
            .map_err(|e| e.to_string())?;
        storage::decode(&bytes)?
            .save_with_format(dest.join(&file), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        slots.push(json!({"id":plan.slot_id,"purpose":plan.purpose,"example":file,"refs":["@example","@product.front"],"prompt":"Replace only the product in the example with the exact product from the product reference. Preserve the page composition, typography, background and lighting. Update only product-specific facts explicitly supplied by the user. Do not copy the example product's features onto the new product.","source_prompt":plan.prompt}));
    }
    let tpl = Template {
        id: tid,
        data: json!({"name":name,"mode":"replace","category":t.brief.products.iter().find(|p|p.id==product_id).map(|p|&p.category),"language":t.brief.language,"style":t.brief.style,"output":{"ratio":t.brief.ratio,"resolution":t.brief.resolution},"slots":slots,"notes":"从成图冻结。请检查页面文案和产品专属信息后复用。"}),
        directory,
        builtin: false,
    };
    storage::validate_template(&tpl.data)?;
    storage::save_template(&tpl)?;
    Ok(tpl)
}

#[tauri::command]
pub async fn image_studio_export(
    id: String,
    destination: String,
    width: u32,
    height: u32,
    max_kb: u32,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let task = {
            let _guard = lock()?;
            check_idle(&id)?;
            let mut task = storage::load_task(&id)?;
            if task.output_directory.is_none() {
                output::prepare(&mut task, &storage::config()?)?;
                storage::save_task(&mut task)?;
            }
            task
        };
        output::export(&task, &destination, width, height, max_kb)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn image_studio_open(path: Option<String>) -> Result<(), String> {
    let relative = path.unwrap_or_else(|| "results".into());
    let full = storage::resolve(&relative)?;
    let parent = full.parent().ok_or("图片路径缺少上级目录")?;
    let name = full.file_name().ok_or("图片路径缺少文件名")?;
    crate::dock::fs::dock_fs_open_path(
        parent.to_string_lossy().into(),
        name.to_string_lossy().into(),
        None,
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub async fn image_studio_template_export(
    id: String,
    destination: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tpl = storage::templates()?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("模板不存在")?;
        let base = Path::new(&destination)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let dest = base.join(format!("dsivio-template-{}", storage::id()));
        fs::create_dir(&dest).map_err(|e| e.to_string())?;
        if !tpl.directory.is_empty() {
            let source = storage::resolve(&tpl.directory)?;
            fn copy(source: &Path, dest: &Path) -> Result<(), String> {
                for e in fs::read_dir(source).map_err(|e| e.to_string())?.flatten() {
                    let p = e.path();
                    if p.is_symlink() {
                        return Err("模板素材不能是符号链接".into());
                    }
                    if p.file_name().is_some_and(|s| s == "record.json") {
                        continue;
                    }
                    let target = dest.join(e.file_name());
                    if p.is_dir() {
                        fs::create_dir(&target).map_err(|e| e.to_string())?;
                        copy(&p, &target)?;
                    } else {
                        fs::copy(&p, &target).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }
            copy(&source, &dest)?;
        }
        storage::write(&dest.join("template.json"), &tpl.data)?;
        Ok(dest.to_string_lossy().into())
    })
    .await
    .map_err(|e| e.to_string())?
}
