use super::types::*;
use std::collections::{HashMap, HashSet};

// Backend validates persisted/imported graphs before any task can start. UI checks are feedback only.
fn port(kind: &str, output: bool, handle: &str) -> Option<(&'static str, bool)> {
    let image_many = matches!(
        kind,
        "image.uploadMany" | "image.previewMany" | "image.downloadZip"
    );
    if output {
        return match (kind, handle) {
            (
                "prompt.input" | "llm.text" | "prompt.optimize" | "text.join" | "image.understand",
                "text",
            ) => Some(("text", false)),
            ("image.upload" | "image.uploadMany" | "image.generate", "image") => {
                Some(("image", image_many))
            }
            ("video.upload" | "video.generate", "video") => Some(("video", false)),
            _ => None,
        };
    }
    match (kind, handle) {
        ("image.understand" | "image.generate" | "video.generate", "prompt") => {
            Some(("text", false))
        }
        ("text.preview" | "llm.text" | "prompt.optimize" | "text.join", "text") => {
            Some(("text", false))
        }
        (
            "image.understand" | "image.generate" | "video.generate" | "image.preview"
            | "image.previewMany" | "image.download" | "image.downloadZip",
            "image",
        ) => Some(("image", image_many)),
        ("video.preview" | "video.download", "video") => Some(("video", false)),
        _ => None,
    }
}
pub(super) fn incoming<'a>(
    flow: &'a GenerationWorkflow,
    node: &str,
    handle: &str,
) -> Option<&'a WorkflowEdge> {
    flow.edges
        .iter()
        .find(|e| e.target == node && e.target_handle.as_deref() == Some(handle))
}
pub(super) fn validate(flow: &GenerationWorkflow) -> Result<Vec<String>, String> {
    if flow.id.trim().is_empty() || flow.name.trim().is_empty() {
        return Err("请填写工作流名称".into());
    }
    if flow.nodes.is_empty() || flow.nodes.len() > 100 || flow.edges.len() > 400 {
        return Err("工作流需包含 1–100 个节点，最多 400 条连线".into());
    }
    let nodes: HashMap<_, _> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if nodes.len() != flow.nodes.len() || nodes.contains_key("") {
        return Err("节点编号缺失或重复".into());
    }
    let mut inputs = HashSet::new();
    let mut ids = HashSet::new();
    for edge in &flow.edges {
        let source = nodes.get(edge.source.as_str()).ok_or("连线来源不存在")?;
        let target = nodes.get(edge.target.as_str()).ok_or("连线目标不存在")?;
        let from = port(
            &source.kind,
            true,
            edge.source_handle.as_deref().unwrap_or_default(),
        )
        .ok_or("输出端口不存在")?;
        let to = port(
            &target.kind,
            false,
            edge.target_handle.as_deref().unwrap_or_default(),
        )
        .ok_or("输入端口不存在")?;
        if from != to {
            return Err("连线类型不匹配，单个素材和素材组不能混接".into());
        }
        if edge.id.is_empty()
            || !ids.insert(&edge.id)
            || !inputs.insert((&edge.target, &edge.target_handle))
        {
            return Err("连线编号重复或输入已有连接".into());
        }
    }
    for node in &flow.nodes {
        let fail = |message: &str| Err(format!("{}：{message}", node.title));
        if node.title.trim().is_empty()
            || !node.position.x.is_finite()
            || !node.position.y.is_finite()
        {
            return fail("节点名称或位置无效");
        }
        let connected = |handle| incoming(flow, &node.id, handle).is_some();
        let assets_ok = |assets: &[WorkflowAsset], many: bool| {
            !assets.is_empty()
                && (many || assets.len() == 1)
                && assets.len() <= 100
                && assets.iter().all(|a| {
                    !a.path.trim().is_empty() && std::path::Path::new(&a.path).is_absolute()
                })
        };
        let model_ok = |model: &Option<WorkflowModelChoice>| {
            model
                .as_ref()
                .is_some_and(|m| !m.provider_id.trim().is_empty() && !m.model.trim().is_empty())
        };
        let valid = match (node.kind.as_str(), node.config.as_ref()) {
            ("prompt.input", Some(WorkflowConfig::Prompt { text })) => !text.trim().is_empty(),
            (
                "image.upload" | "image.uploadMany" | "video.upload",
                Some(WorkflowConfig::Assets { assets }),
            ) => assets_ok(assets, node.kind == "image.uploadMany"),
            (
                "image.understand",
                Some(WorkflowConfig::Understand {
                    instruction,
                    assets,
                    model,
                }),
            ) => {
                model_ok(model)
                    && (connected("prompt") || !instruction.trim().is_empty())
                    && (connected("image") || assets_ok(assets, false))
            }
            (
                "image.generate" | "video.generate",
                Some(WorkflowConfig::Generate {
                    prompt,
                    assets,
                    model,
                    ..
                }),
            ) => {
                model_ok(model)
                    && (connected("prompt") || !prompt.trim().is_empty())
                    && (connected("image") || assets.is_empty() || assets_ok(assets, false))
            }
            (
                "llm.text" | "prompt.optimize" | "text.join",
                Some(WorkflowConfig::Text { text, model, .. }),
            ) => {
                (node.kind == "text.join" || model_ok(model))
                    && (connected("text") || !text.trim().is_empty())
            }
            ("text.preview", Some(WorkflowConfig::Output)) => connected("text"),
            (
                "image.preview" | "image.previewMany" | "image.download" | "image.downloadZip",
                Some(WorkflowConfig::Output),
            ) => connected("image"),
            ("video.preview" | "video.download", Some(WorkflowConfig::Output)) => {
                connected("video")
            }
            _ => return fail("节点尚不支持运行或配置类型错误"),
        };
        if !valid {
            return fail("请补全输入、素材和模型配置");
        }
    }
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    while order.len() < flow.nodes.len() {
        let before = order.len();
        for node in &flow.nodes {
            if !seen.contains(&node.id)
                && flow
                    .edges
                    .iter()
                    .filter(|e| e.target == node.id)
                    .all(|e| seen.contains(&e.source))
            {
                seen.insert(node.id.clone());
                order.push(node.id.clone());
            }
        }
        if before == order.len() {
            return Err("工作流存在循环连线".into());
        }
    }
    Ok(order)
}
