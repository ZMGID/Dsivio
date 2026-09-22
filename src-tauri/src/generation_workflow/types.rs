use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowAsset {
    pub path: String,
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowModelChoice {
    pub provider_id: String,
    pub model: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkflowConfig {
    Assets {
        assets: Vec<WorkflowAsset>,
    },
    Prompt {
        text: String,
    },
    Understand {
        instruction: String,
        assets: Vec<WorkflowAsset>,
        model: Option<WorkflowModelChoice>,
    },
    Generate {
        prompt: String,
        assets: Vec<WorkflowAsset>,
        model: Option<WorkflowModelChoice>,
        options: BTreeMap<String, String>,
    },
    Text {
        instruction: String,
        text: String,
        model: Option<WorkflowModelChoice>,
    },
    Output,
    Placeholder,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkflowPosition {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkflowNode {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub position: WorkflowPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub config: Option<WorkflowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_handle: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GenerationWorkflow {
    pub id: String,
    pub name: String,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct WorkflowValue {
    pub text: Option<String>,
    pub files: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNodeRun {
    pub node_id: String,
    pub status: WorkflowStatus,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub media_task_id: Option<String>,
    pub outputs: BTreeMap<String, WorkflowValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub id: String,
    pub workflow: GenerationWorkflow,
    pub status: WorkflowStatus,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
    pub nodes: Vec<WorkflowNodeRun>,
}
