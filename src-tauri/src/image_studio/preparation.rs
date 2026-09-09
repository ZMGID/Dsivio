//! Automatic material preparation. Never mixes derived references with deliverable images.
use super::{agent, engine, generation, persist, stopped, storage, types::*, workflow};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::{atomic::AtomicBool, Arc},
};
use tauri::AppHandle;

fn originals(product: &Product, state: Option<&MaterialState>) -> Vec<Asset> {
    product
        .assets
        .iter()
        .filter(|asset| {
            !state.is_some_and(|state| {
                state.derived.values().any(|view| {
                    view.asset
                        .as_ref()
                        .is_some_and(|generated| generated.id == asset.id)
                })
            })
        })
        .cloned()
        .collect()
}

pub(super) fn unresolved(task: &Task) -> bool {
    task.materials
        .values()
        .flat_map(|state| state.derived.values())
        .any(|view| {
            view.attempts
                .last()
                .is_some_and(|attempt| attempt.path.is_none() && !retryable(attempt))
        })
}

// A recorded failure can be retried by the next explicit action. An unfinished remote
// request (or a crash before its response was saved) must not silently be submitted again.
fn retryable(attempt: &ImageResult) -> bool {
    attempt
        .error
        .as_deref()
        .is_some_and(|error| attempt.remote_id.is_none() || error.starts_with("远程图片任务失败"))
}

/// IDs returned by the vision model must refer to this product, never arbitrary paths.
pub(super) fn identified_view(
    value: &Value,
    key: &str,
    assets: &[Asset],
) -> Result<Option<String>, String> {
    match value.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if assets.iter().any(|asset| asset.id == *id) => {
            Ok(Some(id.clone()))
        }
        _ => Err("商品识别结果不完整，请重新尝试".into()),
    }
}

pub(super) async fn identify(
    app: &AppHandle,
    task: &mut Task,
    index: usize,
    cfg: &StudioConfig,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    stopped(flag)?;
    let product = task.brief.products[index].clone();
    let assets = originals(&product, task.materials.get(&product.id));
    if assets.is_empty() {
        return Err(format!("{} 没有可用的商品图片", product.name));
    }
    let sources: Vec<_> = assets.iter().map(|asset| asset.path.clone()).collect();
    if task.materials.get(&product.id).is_some_and(|state| {
        state.identified && state.sources == sources && state.requirement == task.brief.requirement
    }) && agent::template_for(task, &product)
        .and_then(|template| template.data["product_kinds"].as_object())
        .is_none_or(|kinds| kinds.is_empty() || kinds.contains_key(&product.kind))
    {
        return Ok(());
    }
    let template = agent::template_for(task, &product).cloned();
    task.progress = format!("正在整理商品素材 · {}", product.name);
    persist(task)?;
    let observation = agent::run(app, &task.id, cfg,
        "Inspect the actual images, ignoring filenames. Return {\"front\":\"exact asset ID of the clearest complete front/product view, or null if none\",\"back\":\"exact asset ID of a real rear view, or null if none\",\"category\":\"short Chinese broad product category\",\"kind\":\"matching product_kinds key, or empty\",\"needsBack\":false}. Front and back must be different images. Use consistent broad categories across products, never color or size categories. needsBack is true only when the requested output or template calls for a rear view. Do not guess IDs or treat a scene/mockup with an unrelated product as a valid source.",
        json!({"product":product,"assets":assets,"requirement":task.brief.requirement,"template":template.as_ref().map(|t| &t.data),"categories":task.brief.products.iter().map(group_of).collect::<Vec<_>>()}),
        assets.iter().map(|asset| (format!("商品照片 ID {}", asset.id), asset.path.clone())).collect(), flag.clone()).await?;
    let front = identified_view(&observation, "front", &assets)?;
    let back = identified_view(&observation, "back", &assets)?;
    if front.is_some() && front == back {
        return Err("商品视角识别不一致，请重新尝试".into());
    }
    let category = observation["category"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("商品分类识别不完整，请重新尝试")?;
    let kind = observation["kind"].as_str().unwrap_or("");
    if let Some(kinds) = template
        .as_ref()
        .and_then(|t| t.data["product_kinds"].as_object())
    {
        if !kinds.is_empty() && !kinds.contains_key(kind) {
            return Err("未能匹配商品款型，请重新尝试".into());
        }
    }
    let product = &mut task.brief.products[index];
    product.assets = assets;
    product.front = front;
    product.back = back;
    if task.brief.feature == "client" {
        product.category = category.into();
    }
    product.kind = kind.into();
    task.materials.insert(
        product.id.clone(),
        MaterialState {
            sources,
            requirement: task.brief.requirement.clone(),
            identified: true,
            needs_back: observation["needsBack"].as_bool().unwrap_or(false),
            ..Default::default()
        },
    );
    persist(task)
}

pub(super) fn needs_back(template: Option<&Template>, product: &Product) -> bool {
    let Some(template) = template else {
        return false;
    };
    template.data["derive"]
        .get("back")
        .is_some_and(|value| !value.is_null() && value != false)
        || template.data["slots"].as_array().is_some_and(|slots| {
            slots.iter().any(|slot| {
                slot["refs_by_kind"]
                    .get(&product.kind)
                    .or_else(|| slot.get("refs"))
                    .and_then(Value::as_array)
                    .is_some_and(|refs| refs.iter().any(|r| r == "@product.back"))
            })
        })
}

pub(super) async fn prepare_product(
    app: &AppHandle,
    task: &mut Task,
    index: usize,
    cfg: &StudioConfig,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    identify(app, task, index, cfg, flag).await?;
    if task.brief.products[index].front.is_none() {
        derive(app, task, index, "front", cfg, flag).await?;
    }
    let product = &task.brief.products[index];
    if product.back.is_none()
        && (needs_back(agent::template_for(task, product), product)
            || task
                .materials
                .get(&product.id)
                .is_some_and(|state| state.needs_back))
    {
        derive(app, task, index, "back", cfg, flag).await?;
    }
    Ok(())
}

async fn derive(
    app: &AppHandle,
    task: &mut Task,
    index: usize,
    side: &str,
    cfg: &StudioConfig,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    stopped(flag)?;
    let product = task.brief.products[index].clone();
    task.progress = format!("正在准备生成素材 · {}", product.name);
    persist(task)?;
    let prior = task.materials[&product.id]
        .derived
        .get(side)
        .and_then(|view| view.attempts.last())
        .filter(|attempt| {
            !retryable(attempt)
                && !attempt
                    .review
                    .as_deref()
                    .is_some_and(|review| review.starts_with("rejected:"))
        })
        .cloned();
    let mut result = if let Some(prior) = prior {
        prior
    } else {
        let template = agent::template_for(task, &product);
        let derive_spec = template.map(|template| &template.data["derive"][side]);
        let mut result = ImageResult {
            id: storage::id(), product_id: product.id.clone(), slot_id: format!("_material_{side}"),
            revision: task.revision, path: None, error: None, remote_id: None,
            prompt: format!("Generate one clean {side} reference view of exactly the supplied product on a plain white background. Preserve the same product identity, color, material, silhouette, scale, branding and construction. Use every supplied view as evidence. Reconstruct missing geometry conservatively; no added features, labels, copy, accessories or measurements. Show the complete product, evenly lit, without a collage. This is an internally generated reference, not documentary evidence of unseen specifications."),
            width: 0, height: 0, review: None, config: cfg.clone(),
        };
        if let Some(prompt) = derive_spec.and_then(|spec| spec["prompt"].as_str()) {
            result
                .prompt
                .push_str(&format!("\nTemplate view instructions: {prompt}"));
        }
        let originals = originals(&product, task.materials.get(&product.id));
        let mut refs = if let (Some(template), Some(spec)) =
            (template, derive_spec.filter(|spec| spec["refs"].is_array()))
        {
            // Respect reference order: template prompts may refer to the first/second image.
            let mut reference_product = product.clone();
            let fallback = originals.first().map(|asset| asset.id.clone());
            reference_product.front = reference_product.front.or(fallback.clone());
            reference_product.back = reference_product.back.or(fallback);
            agent::template_refs(template, &reference_product, spec)?
        } else {
            Vec::new()
        };
        for asset in originals {
            if refs.len() < 5 && !refs.contains(&asset.path) {
                refs.push(asset.path);
            }
        }
        task.materials
            .get_mut(&product.id)
            .unwrap()
            .derived
            .entry(side.into())
            .or_default()
            .attempts
            .push(result.clone());
        persist(task)?; // Save the attempt before any billable submission.
        let plan = ImagePlan {
            product_id: product.id.clone(),
            slot_id: result.slot_id.clone(),
            purpose: "内部参考素材".into(),
            copy: String::new(),
            prompt: result.prompt.clone(),
            refs,
        };
        let submission = engine::submit(app, cfg, &task.id, &task.brief, &plan).await;
        let mut result = result;
        match submission {
            Ok(engine::Submission::Image(bytes)) => {
                if let Err(error) = store_derived(&mut result, &bytes) {
                    result.error = Some(error);
                }
            }
            Ok(engine::Submission::Pending(id)) => result.remote_id = Some(id),
            Err(error) => result.error = Some(error),
        }
        *task
            .materials
            .get_mut(&product.id)
            .unwrap()
            .derived
            .get_mut(side)
            .unwrap()
            .attempts
            .last_mut()
            .unwrap() = result.clone();
        persist(task)?;
        result
    };
    if result.path.is_none() {
        if let Some(remote) = &result.remote_id {
            let backend = engine::NativeBackend {
                app,
                cfg: &result.config,
                task_id: &task.id,
                brief: &task.brief,
            };
            let bytes = match generation::resume(&backend, &result.config, remote, flag).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    result.error = Some(error.clone());
                    *task
                        .materials
                        .get_mut(&product.id)
                        .unwrap()
                        .derived
                        .get_mut(side)
                        .unwrap()
                        .attempts
                        .last_mut()
                        .unwrap() = result;
                    persist(task)?;
                    return Err(error);
                }
            };
            store_derived(&mut result, &bytes)?;
            *task
                .materials
                .get_mut(&product.id)
                .unwrap()
                .derived
                .get_mut(side)
                .unwrap()
                .attempts
                .last_mut()
                .unwrap() = result.clone();
            persist(task)?;
        } else {
            return Err(result.error.unwrap_or_else(|| {
                "素材生成请求可能已提交，请核对供应商记录后新建任务重试".into()
            }));
        }
    }
    let path = result.path.clone().ok_or("素材生成未完成")?;
    let mut images: Vec<_> = originals(&product, task.materials.get(&product.id))
        .iter()
        .take(4)
        .map(|asset| ("原始商品照片".into(), asset.path.clone()))
        .collect();
    images.push((format!("自动生成的 {side} 参考"), path.clone()));
    let review = agent::run(app, &task.id, cfg,
        "Check the generated reference against the original product images. Return {\"pass\":true,\"summary\":\"short Chinese assessment\"}. Check product identity, material, color, shape, branding, and requested view. Accept conservative reconstruction of unseen geometry; reject clearly different products, invented visible features, wrong views, or distorted structure. Do not claim unseen details are verified facts.",
        json!({"view":side}), images, flag.clone()).await?;
    if review["pass"].as_bool() != Some(true) {
        // Only a new explicit retry generates another attempt; retain its provenance and image.
        result.review = Some(format!(
            "rejected: {}",
            review["summary"].as_str().unwrap_or("产品一致性检查未通过")
        ));
        *task
            .materials
            .get_mut(&product.id)
            .unwrap()
            .derived
            .get_mut(side)
            .unwrap()
            .attempts
            .last_mut()
            .unwrap() = result;
        persist(task)?;
        return Err("商品素材自动检查未通过，请点击重新尝试".into());
    }
    let asset = Asset {
        id: result.id,
        name: format!("__dsimage_{side}.png"),
        path,
    };
    task.materials
        .get_mut(&product.id)
        .unwrap()
        .derived
        .get_mut(side)
        .unwrap()
        .asset = Some(asset.clone());
    let product = &mut task.brief.products[index];
    if side == "front" {
        product.front = Some(asset.id.clone());
    } else {
        product.back = Some(asset.id.clone());
    }
    product.assets.push(asset);
    persist(task)
}

fn store_derived(result: &mut ImageResult, bytes: &[u8]) -> Result<(), String> {
    let image = storage::decode(bytes)?;
    let relative = format!("assets/{}.png", result.id);
    image
        .save_with_format(storage::root()?.join(&relative), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    result.path = Some(relative);
    result.width = image.width();
    result.height = image.height();
    result.error = None;
    Ok(())
}

pub(super) async fn templates(
    app: &AppHandle,
    task: &mut Task,
    cfg: &StudioConfig,
    flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    if task.brief.feature == "smart" && task.brief.template_id.is_none() {
        return Err("请先选择一套模板".into());
    }
    let groups: HashSet<_> = task.brief.products.iter().map(group_of).collect();
    for group in groups {
        let indices: Vec<_> = task
            .brief
            .products
            .iter()
            .enumerate()
            .filter(|(_, product)| group_of(product) == group)
            .map(|(index, _)| index)
            .collect();
        if indices
            .iter()
            .all(|index| agent::template_for(task, &task.brief.products[*index]).is_some())
        {
            continue;
        }
        if task.brief.feature == "replace"
            && !task
                .brief
                .workflow_input
                .as_ref()
                .is_some_and(|input| !input.sources.is_empty())
        {
            return Err("请添加要沿用的样图，或选择已有模板".into());
        }
        task.progress = format!("正在安排画面 · {group}");
        persist(task)?;
        let mut source = task.clone();
        source.templates.clear();
        source.brief.products = indices
            .iter()
            .map(|index| task.brief.products[*index].clone())
            .collect();
        if task.brief.feature != "replace" {
            source.brief.workflow_input = Some(WorkflowInput {
                mode: "smart".into(),
                sources: originals(
                    &source.brief.products[0],
                    source.materials.get(&source.brief.products[0].id),
                ),
            });
        }
        let (template, _) = workflow::make_template(app, &source, cfg, flag).await?;
        storage::save_template(&template)?;
        for index in indices {
            if agent::template_for(task, &task.brief.products[index]).is_none() {
                task.brief.products[index].template_id = Some(template.id.clone());
            }
        }
        task.templates.push(template);
        persist(task)?;
    }
    Ok(())
}
