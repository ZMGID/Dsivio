//! Video template files. Display normalization never replaces the original JSON.
use super::files::{id, read, write};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const BUILTIN_ID: &str = "bedroom-ugc-product-presenter-15s";
const BUILTIN: &str = include_str!("../../resources/plugins/dsvideo-plugin/skills/ecom-h3-video/templates/bedroom-ugc-product-presenter-15s.json");

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct VideoTemplate {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub script: String,
    #[ts(type = "Record<string, unknown>[]")]
    pub shots: Vec<Value>,
    #[ts(type = "Record<string, unknown>")]
    pub spec: Value,
    #[ts(type = "Record<string, unknown>")]
    pub data: Value,
}

pub fn root() -> Result<PathBuf, String> {
    Ok(crate::app_data::app_data_dir()
        .ok_or("无法定位应用数据目录")?
        .join("video-studio/templates"))
}

pub fn normalize(data: Value, filename: &str) -> Result<VideoTemplate, String> {
    let name = data["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("视频模板需要名称")?
        .to_owned();
    // Canonical fields preserve explicit edits, including clears. Only absent
    // fields use legacy aliases; keep the original aliases unchanged for export.
    let script = match data.get("script") {
        Some(value) => value.as_str().unwrap_or(""),
        None => ["full_video_prompt", "prompt_pattern"]
            .iter()
            .find_map(|key| data[key].as_str().filter(|s| !s.trim().is_empty()))
            .unwrap_or(""),
    }
    .to_owned();
    let shots = match data.get("shots") {
        Some(shots) => shots.as_array().into_iter().flatten().filter(|s| s.is_object()).cloned().collect::<Vec<_>>(),
        None => data["shot_breakdown"].as_array().into_iter().flatten().filter(|s| s.is_object()).map(|s| json!({
            "time": s["time"].as_str().unwrap_or(""), "purpose": s["purpose"].as_str().unwrap_or(""),
            "action": s["generation_prompt"].as_str().filter(|s| !s.is_empty()).or(s["subject"].as_str()).unwrap_or(""),
            "camera": s["composition_camera"].as_str().unwrap_or("")
        })).collect(),
    };
    if script.trim().is_empty() && shots.is_empty() {
        return Err("视频模板需要剧本或镜头内容".into());
    }
    let id = data["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(filename)
        .to_owned();
    let reference = match &data["reference_template"] {
        Value::Null | Value::Bool(false) => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::Bool(true) => true,
    };
    let kind = if !reference
        && (data["validated_from"]["user_approved"] == true || data["kind"] == "generation")
    {
        "generation"
    } else {
        "reference"
    }
    .to_owned();
    let mut spec = data["spec"].as_object().cloned().unwrap_or_default();
    for key in ["duration_seconds", "aspect_ratio"] {
        if !spec.contains_key(key) && !data["source"][key].is_null() {
            spec.insert(key.into(), data["source"][key].clone());
        }
    }
    Ok(VideoTemplate {
        id,
        name,
        kind,
        script,
        shots,
        spec: Value::Object(spec),
        data,
    })
}

fn entries(root: &Path) -> Result<Vec<(PathBuf, VideoTemplate)>, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if path.is_symlink()
            || path.extension().and_then(|s| s.to_str()) != Some("json")
            || name.starts_with('_')
        {
            continue;
        }
        let value =
            normalize(read(&path)?, name).map_err(|e| format!("{}：{e}", path.display()))?;
        result.push((path, value));
    }
    let target = root.join(format!("{BUILTIN_ID}.json"));
    if !target.exists() && !result.iter().any(|(_, t)| t.id == BUILTIN_ID) {
        let mut data: Value = serde_json::from_str(BUILTIN).map_err(|e| e.to_string())?;
        data["kind"] = json!("generation");
        result.push((target, normalize(data, BUILTIN_ID)?));
    }
    result.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    Ok(result)
}
pub fn list_at(root: &Path) -> Result<Vec<VideoTemplate>, String> {
    Ok(entries(root)?.into_iter().map(|(_, t)| t).collect())
}
pub fn get_at(root: &Path, id: &str) -> Result<VideoTemplate, String> {
    list_at(root)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("视频模板不存在".into())
}
pub fn import_at(root: &Path, path: &Path) -> Result<VideoTemplate, String> {
    insert_at(root, read(path)?)
}
pub fn insert_at(root: &Path, mut data: Value) -> Result<VideoTemplate, String> {
    normalize(data.clone(), "")?;
    let new_id = id();
    data["id"] = json!(new_id);
    let template = normalize(data, &new_id)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    write(&root.join(format!("{new_id}.json")), &template.data)?;
    Ok(template)
}
pub fn save_at(root: &Path, edited: VideoTemplate) -> Result<VideoTemplate, String> {
    let (path, old) = entries(root)?
        .into_iter()
        .find(|(_, t)| t.id == edited.id)
        .ok_or("视频模板不存在")?;
    // Start from disk, not display JSON or client-supplied kind/provenance.
    let mut data = old.data;
    data["name"] = json!(edited.name);
    if edited.script != old.script {
        data["script"] = json!(edited.script);
    }
    if edited.shots != old.shots {
        data["shots"] = json!(edited.shots);
    }
    if edited.spec != old.spec {
        data["spec"] = edited.spec;
    }
    let result = normalize(data, &old.id)?;
    write(&path, &result.data)?;
    Ok(result)
}
pub fn export_at(root: &Path, id: &str, destination: &Path) -> Result<String, String> {
    let template = get_at(root, id)?;
    let dest = destination.join(format!("dsivio-video-template-{}.json", super::files::id()));
    write(&dest, &template.data)?;
    Ok(dest.to_string_lossy().into())
}
/// Old generation forms consume normalized fields, while management retains raw JSON separately.
pub fn for_studio(template: VideoTemplate) -> Value {
    let mut data = template.data;
    data["id"] = json!(template.id);
    data["name"] = json!(template.name);
    data["kind"] = json!(template.kind);
    data["script"] = json!(template.script);
    data["shots"] = json!(template.shots);
    data["spec"] = template.spec;
    data
}
