use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioConfig {
    pub provider_id: String,
    pub model: String,
    pub protocol: String,
    pub agent_provider_id: String,
    pub agent_model: String,
    pub output_root: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    pub name: String,
    pub category: String,
    pub kind: String,
    pub facts: String,
    pub front: Option<String>,
    pub back: Option<String>,
    pub assets: Vec<Asset>,
    pub template_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub feature: String,
    pub name: String,
    pub requirement: String,
    pub language: String,
    pub platform: String,
    pub ratio: String,
    pub resolution: String,
    pub count: usize,
    pub style: String,
    pub template_id: Option<String>,
    pub products: Vec<Product>,
    #[serde(default)]
    pub workflow_input: Option<WorkflowInput>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInput {
    pub mode: String,
    pub sources: Vec<Asset>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Workflow {
    pub rule_version: u64,
    pub rules_current: bool,
    pub approved_version: Option<u64>,
    pub sample_ids: Vec<String>,
    pub changes: Vec<RuleChange>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleChange {
    pub version: u64,
    #[serde(default)]
    pub revision: u64,
    pub note: String,
    pub summary: String,
    pub created_at: String,
    pub template: Template,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePlan {
    pub product_id: String,
    pub slot_id: String,
    pub purpose: String,
    pub copy: String,
    pub prompt: String,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResult {
    pub id: String,
    pub product_id: String,
    pub slot_id: String,
    pub revision: u64,
    pub path: Option<String>,
    pub error: Option<String>,
    pub remote_id: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    pub review: Option<String>,
    // The provider snapshot is needed to resume an asynchronous job safely.
    pub config: StudioConfig,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
    pub brief: Brief,
    pub plans: Vec<ImagePlan>,
    pub results: Vec<ImageResult>,
    pub approved_groups: Vec<String>,
    pub status: String,
    pub progress: String,
    pub error: Option<String>,
    pub templates: Vec<Template>,
    #[serde(default)]
    pub output_directory: Option<String>,
    #[serde(default)]
    pub workflow: Option<Workflow>,
    #[serde(default)]
    pub materials: HashMap<String, MaterialState>,
}

/// Internal preparation stays separate from user-facing results and exports.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MaterialState {
    pub sources: Vec<String>,
    pub requirement: String,
    pub identified: bool,
    pub needs_back: bool,
    pub derived: HashMap<String, DerivedView>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DerivedView {
    pub asset: Option<Asset>,
    pub attempts: Vec<ImageResult>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub data: Value,
    pub directory: String,
    pub builtin: bool,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    pub kind: String,
    pub group: String,
    pub product_id: String,
    pub slot_id: String,
    pub result_id: String,
    pub note: String,
    pub sample_ids: Vec<String>,
    pub template_data: Option<Value>,
}

pub fn group_of(p: &Product) -> String {
    if p.category.trim().is_empty() {
        "未分类".into()
    } else {
        p.category.trim().into()
    }
}

pub fn sample_ids(task: &Task, group: &str) -> Vec<String> {
    task.brief
        .products
        .iter()
        .filter(|p| group_of(p) == group)
        .take(2)
        .map(|p| p.id.clone())
        .collect()
}

pub fn has_result(task: &Task, plan: &ImagePlan) -> bool {
    task.results
        .iter()
        .rev()
        .find(|r| {
            r.revision == task.revision
                && r.product_id == plan.product_id
                && r.slot_id == plan.slot_id
        })
        .is_some_and(|r| r.path.is_some())
}

pub fn samples_complete(task: &Task, group: &str) -> bool {
    let ids = sample_ids(task, group);
    !ids.is_empty()
        && ids.iter().all(|id| {
            let plans: Vec<_> = task.plans.iter().filter(|p| &p.product_id == id).collect();
            !plans.is_empty() && plans.iter().all(|p| has_result(task, p))
        })
}
