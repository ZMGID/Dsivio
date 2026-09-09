//! One persisted workflow: source material -> reusable rules -> trial -> feedback -> production.
use super::{agent, engine, generation, persist, stopped, storage, types::*};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    sync::{atomic::AtomicBool, Arc},
};
use tauri::AppHandle;

pub(super) fn is_action(kind: &str) -> bool {
    matches!(
        kind,
        "workflow_build"
            | "workflow_trial"
            | "workflow_refine"
            | "workflow_edit"
            | "workflow_approve"
            | "workflow_produce"
    )
}

pub(super) fn validate_input(brief: &Brief) -> Result<(), String> {
    let input = brief.workflow_input.as_ref().ok_or("请选择制作方式")?;
    if !matches!(input.mode.as_str(), "smart" | "replace") || input.sources.len() > 30 {
        return Err("请选择有效的制作方式，原始参考素材最多 30 张".into());
    }
    let mut ids = HashSet::new();
    for source in &input.sources {
        if !ids.insert(&source.id) || !source.path.starts_with("assets/") {
            return Err("原始参考素材必须先导入，且不能重复".into());
        }
        storage::resolve(&source.path)?;
    }
    Ok(())
}

fn same_rules_input(a: &Brief, b: &Brief) -> bool {
    a.workflow_input == b.workflow_input
        && a.requirement == b.requirement
        && a.language == b.language
        && a.platform == b.platform
        && a.ratio == b.ratio
        && a.resolution == b.resolution
        && a.count == b.count
        && a.style == b.style
}

fn unresolved(task: &Task) -> bool {
    latest(task).values().any(|r| {
        r.remote_id.is_some()
            && r.path.is_none()
            && !r
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with("远程图片任务失败"))
    })
}

fn latest(task: &Task) -> HashMap<(String, String), &ImageResult> {
    let mut results = HashMap::new();
    for result in &task.results {
        if result.revision == task.revision {
            results.insert((result.product_id.clone(), result.slot_id.clone()), result);
        }
    }
    results
}

/// Appending products preserves reviewed rules and the attempts of unchanged products.
/// Move their revision in place: saving a growing catalog must not duplicate every image record.
/// Actual rule/product changes leave prior attempts in history, including failed edits.
pub(super) fn save_brief(task: &mut Task, mut brief: Brief) -> Result<(), String> {
    brief.template_id = task.templates.first().map(|t| t.id.clone());
    for product in &mut brief.products {
        product.template_id = None;
    }
    if task.revision > 0 && task.brief == brief {
        return Ok(());
    }
    let same_rules = same_rules_input(&task.brief, &brief);
    let unchanged: HashSet<_> = brief
        .products
        .iter()
        .filter(|p| task.brief.products.iter().any(|old| old == *p))
        .map(|p| p.id.clone())
        .collect();
    if unresolved(task)
        && (!same_rules
            || task
                .brief
                .products
                .iter()
                .any(|p| !unchanged.contains(&p.id)))
    {
        return Err("已有远程图片尚未完成，请先恢复查询，再修改素材或制作要求".into());
    }
    let previous_revision = task.revision;
    task.revision += 1;
    task.plans
        .retain(|p| same_rules && unchanged.contains(&p.product_id));
    for result in &mut task.results {
        if same_rules
            && result.revision == previous_revision
            && unchanged.contains(&result.product_id)
        {
            result.revision = task.revision;
        }
    }
    let w = task.workflow.get_or_insert_with(Workflow::default);
    if !same_rules {
        w.rules_current = false;
        w.approved_version = None;
    }
    if w.sample_ids.iter().any(|id| !unchanged.contains(id)) {
        w.approved_version = None;
    }
    w.sample_ids
        .retain(|id| brief.products.iter().any(|p| p.id == *id));
    task.brief = brief;
    task.error = None;
    task.status = "draft".into();
    task.progress = if w.rules_current {
        "商品已保存，可继续试品或生成"
    } else {
        "素材与要求已保存，下一步由 Agent 制作共用规则"
    }
    .into();
    Ok(())
}

pub(super) fn sample_complete(task: &Task) -> bool {
    let Some(w) = &task.workflow else {
        return false;
    };
    let Some(template) = task.templates.first() else {
        return false;
    };
    let results = latest(task);
    w.rules_current
        && !w.sample_ids.is_empty()
        && w.sample_ids.iter().all(|id| {
            task.brief.products.iter().any(|p| p.id == *id)
                && template.data["slots"].as_array().is_some_and(|slots| {
                    slots.iter().all(|slot| {
                        results
                            .get(&(id.clone(), slot["id"].as_str().unwrap_or("").into()))
                            .is_some_and(|r| r.path.is_some())
                    })
                })
        })
}

pub(super) fn validate_action(task: &Task, action: &Action) -> Result<(), String> {
    if task.brief.feature != "workflow" {
        return Err("请在制作与试品任务中操作".into());
    }
    if matches!(
        action.kind.as_str(),
        "resume" | "review" | "retry" | "revise"
    ) {
        if !task.workflow.as_ref().is_some_and(|w| w.rules_current) {
            return Err("制作要求已变更，请先更新共用规则".into());
        }
        if matches!(action.kind.as_str(), "resume" | "review" | "revise")
            && !task
                .results
                .iter()
                .any(|r| r.id == action.result_id && r.revision == task.revision)
        {
            return Err("请选择当前版本的图片操作".into());
        }
        if matches!(action.kind.as_str(), "retry" | "revise")
            && !task
                .plans
                .iter()
                .any(|p| p.product_id == action.product_id && p.slot_id == action.slot_id)
        {
            return Err("请选择当前版本已规划的商品页面".into());
        }
        return Ok(());
    }
    if !is_action(&action.kind) {
        return Err("请使用制作与试品流程中的操作".into());
    }
    if action.kind == "workflow_build" {
        let input = task
            .brief
            .workflow_input
            .as_ref()
            .ok_or("请添加原始参考素材")?;
        if input.sources.is_empty() || task.brief.requirement.trim().is_empty() {
            return Err("请添加商品图或成套参考图，并填写制作要求".into());
        }
    } else {
        let w = task
            .workflow
            .as_ref()
            .filter(|w| w.rules_current)
            .ok_or("请先根据当前素材和要求制作规则")?;
        if action.kind == "workflow_approve" && !sample_complete(task) {
            return Err("请先完成本版全部试品页面，再确认继续使用".into());
        }
        if action.kind == "workflow_produce" && w.approved_version != Some(w.rule_version) {
            return Err("请先确认当前版本的试品效果".into());
        }
        if action.kind == "workflow_refine" && action.note.trim().is_empty() {
            return Err("请说明哪些地方需要修改，以及后续商品应遵守的要求".into());
        }
        if matches!(action.kind.as_str(), "workflow_trial" | "workflow_produce")
            && task.brief.products.is_empty()
        {
            return Err("请先添加用于试做或继续生成的商品".into());
        }
        if action.kind == "workflow_trial" && !action.sample_ids.is_empty() {
            let unique: HashSet<_> = action.sample_ids.iter().collect();
            if unique.len() != action.sample_ids.len()
                || unique.len() > 2
                || unique
                    .iter()
                    .any(|id| !task.brief.products.iter().any(|p| p.id == **id))
            {
                return Err("请选择 1–2 款不同的现有商品试做".into());
            }
        }
    }
    if matches!(
        action.kind.as_str(),
        "workflow_build" | "workflow_refine" | "workflow_edit"
    ) && unresolved(task)
    {
        return Err("已有远程图片尚未完成，请先恢复查询，再修改共用规则和重新试品".into());
    }
    Ok(())
}

pub(super) fn validate_rules(data: &Value, old: Option<&Template>) -> Result<(), String> {
    storage::validate_template(data)?;
    if data["mode"] == "smart" && data["style"].as_str().unwrap_or("").trim().is_empty() {
        return Err("共用规则缺少统一风格，请重新制作".into());
    }
    let slots = data["slots"].as_array().ok_or("规则没有页面")?;
    for slot in slots {
        let key = if data["mode"] == "replace" {
            "prompt"
        } else {
            "brief"
        };
        if slot[key].as_str().unwrap_or("").trim().is_empty() {
            return Err(format!("{} 的画面规则为空", slot["id"]));
        }
        // All generation assets are assigned by the application, never by model-generated paths.
        if let Some(old) = old {
            let previous = old.data["slots"]
                .as_array()
                .and_then(|s| s.iter().find(|s| s["id"] == slot["id"]))
                .ok_or("修改规则不能改变页面身份")?;
            for key in ["example", "refs", "refs_by_kind"] {
                if slot[key] != previous[key] {
                    return Err("修改画面规则不能替换参考素材路径".into());
                }
            }
        }
    }
    if let Some(old) = old {
        let mut fixed = data.clone();
        let mut previous = old.data.clone();
        // Only editable rule text may change. Asset bindings and execution settings stay intact.
        for value in [&mut fixed, &mut previous] {
            let object = value.as_object_mut().ok_or("共用规则格式无效")?;
            for key in ["name", "style", "text_policy"] {
                object.remove(key);
            }
            for slot in object
                .get_mut("slots")
                .and_then(Value::as_array_mut)
                .ok_or("缺少页面")?
            {
                let slot = slot.as_object_mut().ok_or("页面规则格式无效")?;
                for key in ["purpose", "brief", "prompt"] {
                    slot.remove(key);
                }
            }
        }
        if fixed != previous {
            return Err("只能修改风格、文案和页面规则；素材、输出规格和页面身份应保持不变".into());
        }
        if data["mode"] != old.data["mode"]
            || slots.len() != old.data["slots"].as_array().map_or(0, Vec::len)
            || slots.iter().map(|s| &s["id"]).ne(old.data["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| &s["id"]))
        {
            return Err(
                "修改规则不能改变制作方式或页面顺序；需要重排时请修改制作要求后重新制作".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn install_rules(task: &mut Task, template: Template, note: String, summary: String) {
    task.revision += 1;
    task.plans.clear();
    task.approved_groups.clear();
    task.brief.template_id = Some(template.id.clone());
    for product in &mut task.brief.products {
        product.template_id = None;
    }
    let w = task.workflow.get_or_insert_with(Workflow::default);
    w.rule_version += 1;
    w.rules_current = true;
    w.approved_version = None;
    w.changes.push(RuleChange {
        version: w.rule_version,
        revision: task.revision,
        note,
        summary,
        created_at: storage::now(),
        template: template.clone(),
    });
    task.templates = vec![template];
}

async fn build(
    app: &AppHandle,
    task: &mut Task,
    cfg: &StudioConfig,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    let input = task
        .brief
        .workflow_input
        .as_ref()
        .ok_or("缺少制作素材")?
        .clone();
    let count = if input.mode == "replace" {
        input.sources.len()
    } else {
        task.brief.count
    };
    let slots: Vec<_> = (0..count).map(|i| json!({"id":format!("h{}",i+1),"sourceIndex": if input.mode == "replace" { Some(i+1) } else { None }})).collect();
    task.progress = "Agent 正在看原始素材，整理每页构图、文字和以后换品要遵守的规则…".into();
    persist(task)?;
    let images = input
        .sources
        .iter()
        .enumerate()
        .map(|(i, a)| {
            (
                format!("原始制作参考 {}：{}", i + 1, a.name),
                a.path.clone(),
            )
        })
        .collect();
    let out = agent::run(app, &task.id, cfg,
        "Create reusable e-commerce image rules from the supplied source images and requirements. Return {\"summary\":\"Chinese explanation\",\"template\":{\"name\":\"Chinese name\",\"mode\":\"exact requested mode\",\"category\":\"category\",\"style\":\"complete shared visual system\",\"text_policy\":\"exact copy and language policy\",\"slots\":[{\"id\":\"exact supplied id\",\"purpose\":\"Chinese page label\",\"brief\":\"complete reusable per-page composition for smart\",\"prompt\":\"complete English replacement instructions for replace\"}]}}. Include every supplied slot in order. For replace, inspect each corresponding source image, specify the original text, product position, angle, typography, what stays and what changes; preserve layout and change only the product using future product references. For smart, use product material to design reusable rules for similar products, not a prompt locked to this SKU. Separate shared layout/style from product-specific facts. Do not invent dimensions or copy this source product's features onto future products. Do not output file paths, refs, examples, or product kind branches.",
        json!({"brief":task.brief,"mode":input.mode,"slots":slots,"shootingGuide":include_str!("../../resources/image-studio/shots.md")}), images, flag.clone()).await?;
    stopped(flag)?;
    let mut data = out["template"].clone();
    if data["mode"] != input.mode {
        return Err("Agent 返回的制作方式不匹配，请重新制作".into());
    }
    let proposed = data["slots"]
        .as_array_mut()
        .ok_or("Agent 没有返回页面规则")?;
    if proposed.len() != count {
        return Err("Agent 返回的页面数量不匹配，请重新制作".into());
    }
    for (i, slot) in proposed.iter_mut().enumerate() {
        if slot["id"] != format!("h{}", i + 1) {
            return Err("Agent 返回的页面顺序不匹配，请重新制作".into());
        }
        slot.as_object_mut()
            .ok_or("页面规则无效")?
            .retain(|k, _| matches!(k.as_str(), "id" | "purpose" | "brief" | "prompt"));
        slot["refs"] = if input.mode == "replace" {
            json!(["@example", "@product.front"])
        } else {
            json!(["@product.front"])
        };
    }
    data.as_object_mut().ok_or("规则无效")?.retain(|k, _| {
        matches!(
            k.as_str(),
            "name" | "mode" | "category" | "style" | "text_policy" | "slots"
        )
    });
    data["language"] = json!(task.brief.language);
    data["output"] = json!({"ratio":task.brief.ratio,"resolution":task.brief.resolution});
    // Use imported asset IDs and extensions as filenames, never source filenames or model paths.
    if input.mode == "replace" {
        for (slot, source) in data["slots"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&input.sources)
        {
            slot["example"] = json!(std::path::Path::new(&source.path)
                .file_name()
                .ok_or("素材文件名无效")?
                .to_string_lossy());
        }
    }
    validate_rules(&data, None)?;
    let id = task
        .templates
        .first()
        .map(|t| t.id.clone())
        .unwrap_or_else(storage::id);
    let directory = format!("templates/{id}");
    fs::create_dir_all(storage::root()?.join(&directory)).map_err(|e| e.to_string())?;
    if input.mode == "replace" {
        for source in &input.sources {
            let path = storage::resolve(&source.path)?;
            fs::copy(
                &path,
                storage::root()?
                    .join(&directory)
                    .join(path.file_name().ok_or("素材文件名无效")?),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    let template = Template {
        id,
        directory,
        builtin: false,
        data,
    };
    install_rules(
        task,
        template,
        task.brief.requirement.clone(),
        out["summary"]
            .as_str()
            .unwrap_or("已完成共用规则制作")
            .into(),
    );
    persist(task)?;
    storage::save_template(&task.templates[0])?;
    task.progress = "规则已制作并保存。添加其他商品，选择 1–2 款试做。".into();
    Ok(())
}

async fn refine(
    app: &AppHandle,
    task: &mut Task,
    cfg: &StudioConfig,
    action: &Action,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    let old = task.templates.first().ok_or("请先制作规则")?.clone();
    let (data, summary) = if action.kind == "workflow_edit" {
        (
            action.template_data.clone().ok_or("缺少修改后的规则")?,
            "已保存手动修改的共用规则".to_string(),
        )
    } else {
        let w = task.workflow.as_ref().ok_or("缺少试品流程")?;
        let mut images = Vec::new();
        for p in task
            .brief
            .products
            .iter()
            .filter(|p| w.sample_ids.contains(&p.id))
        {
            images.extend(agent::product_images(p));
        }
        let results: Vec<_> = latest(task)
            .into_values()
            .filter(|r| w.sample_ids.contains(&r.product_id))
            .cloned()
            .collect();
        for r in &results {
            if let Some(path) = &r.path {
                images.push((
                    format!("试品结果 {} {}", r.product_id, r.slot_id),
                    path.clone(),
                ));
            }
        }
        if let Some(input) = &task.brief.workflow_input {
            images.extend(
                input
                    .sources
                    .iter()
                    .map(|a| (format!("原始制作参考：{}", a.name), a.path.clone())),
            );
        }
        task.progress = "Agent 正在对照原始素材和试品，把反馈写入后续商品共用的规则…".into();
        persist(task)?;
        let out = agent::run(app,&task.id,cfg,
            "Apply the user's feedback to the reusable rules for ALL future products. Return {\"summary\":\"Chinese list of actual changes\",\"template\":<complete updated template>}. Preserve mode, slot IDs/order, example/ref paths, product kind branches, output, and all unrelated rules. Change shared style/text_policy and the affected slots' brief/prompt/brief_by_kind/prompt_by_kind only as needed. Preserve exact copy unless the user requests a copy change. Do not lock rules to the trial SKU's color, features or dimensions. Use original references and trial outputs to diagnose recurring layout or product-identity errors. Do not claim trial success; the user must inspect new trial images.",
            json!({"template":old.data,"feedback":action.note,"products":task.brief.products,"trialResults":results,"history":task.workflow.as_ref().map(|w| w.changes.iter().map(|c| json!({"version":c.version,"feedback":c.note,"summary":c.summary})).collect::<Vec<_>>())}), images, flag.clone()).await?;
        (
            out["template"].clone(),
            out["summary"]
                .as_str()
                .unwrap_or("已按反馈更新共用规则")
                .into(),
        )
    };
    stopped(flag)?;
    validate_rules(&data, Some(&old))?;
    if data == old.data {
        return Err("规则没有发生变化，请补充具体修改要求".into());
    }
    install_rules(task, Template { data, ..old }, action.note.clone(), summary);
    persist(task)?;
    storage::save_template(&task.templates[0])?;
    if task
        .workflow
        .as_ref()
        .is_some_and(|w| !w.sample_ids.is_empty())
    {
        generate(app, task, cfg, false, flag).await?;
    } else {
        task.progress = "共用规则已更新，请添加商品试做".into();
    }
    Ok(())
}

async fn generate(
    app: &AppHandle,
    task: &mut Task,
    cfg: &StudioConfig,
    production: bool,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    let w = task.workflow.as_ref().ok_or("请先制作规则")?;
    let ids: Vec<_> = if production {
        task.brief.products.iter().map(|p| p.id.clone()).collect()
    } else {
        w.sample_ids.clone()
    };
    let products: Vec<_> = task
        .brief
        .products
        .iter()
        .filter(|p| ids.contains(&p.id))
        .cloned()
        .collect();
    for product in products {
        stopped(flag)?;
        if task.plans.iter().any(|p| p.product_id == product.id) {
            continue;
        }
        task.progress = format!("正在为 {} 编写本版规则下的画面方案…", product.name);
        persist(task)?;
        let mut plans = agent::plan(app, task, &product, cfg, flag.clone()).await?;
        for plan in &mut plans {
            for asset in &product.assets {
                if !plan.refs.contains(&asset.path) {
                    plan.refs.push(asset.path.clone());
                }
            }
        }
        task.plans.extend(plans);
        persist(task)?;
    }
    let plans = pending_plans(task, &ids);
    if !plans.is_empty() {
        let id = task.id.clone();
        let brief = task.brief.clone();
        let backend = engine::NativeBackend {
            app,
            cfg,
            task_id: &id,
            brief: &brief,
        };
        generation::run(task, cfg, plans, flag, &backend, persist).await?;
    }
    task.progress = if production {
        "本批已处理，可继续添加其他商品。未完成页可单独重试或恢复查询。"
    } else {
        "试品已处理，请对照原始参考查看结果；有问题可修改共用规则后再试。"
    }
    .into();
    Ok(())
}

pub(super) fn pending_plans(task: &Task, ids: &[String]) -> Vec<ImagePlan> {
    let existing = latest(task);
    task.plans
        .iter()
        .filter(|p| {
            ids.contains(&p.product_id)
                && !existing.contains_key(&(p.product_id.clone(), p.slot_id.clone()))
        })
        .cloned()
        .collect()
}

pub(super) async fn execute(
    app: &AppHandle,
    task: &mut Task,
    cfg: &StudioConfig,
    action: &Action,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    validate_action(task, action)?;
    match action.kind.as_str() {
        "workflow_build" => build(app, task, cfg, flag).await,
        "workflow_refine" | "workflow_edit" => refine(app, task, cfg, action, flag).await,
        "workflow_approve" => {
            let w = task.workflow.as_mut().ok_or("请先完成试品")?;
            w.approved_version = Some(w.rule_version);
            task.progress = format!("第 {} 版效果已确认，可持续添加商品生成", w.rule_version);
            persist(task)
        }
        "workflow_trial" => {
            let w = task.workflow.as_mut().ok_or("请先制作规则")?;
            let ids = if !action.sample_ids.is_empty() {
                action.sample_ids.clone()
            } else if !w.sample_ids.is_empty() {
                w.sample_ids.clone()
            } else {
                task.brief
                    .products
                    .iter()
                    .take(2)
                    .map(|p| p.id.clone())
                    .collect()
            };
            if ids != w.sample_ids {
                w.approved_version = None;
            }
            w.sample_ids = ids;
            persist(task)?;
            generate(app, task, cfg, false, flag).await
        }
        "workflow_produce" => generate(app, task, cfg, true, flag).await,
        _ => Err("未知制作流程操作".into()),
    }
}
