//! Versioned public model facts and the single pre-submission argument validator.
//! Descriptions are built by the route owners; vendor field names never enter this contract.
use super::{MediaKind, image_providers, video_providers};
use crate::settings::ModelProvider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDescription {
    pub description_version: u32,
    pub identity: String,
    pub operation: MediaKind,
    pub facts_revision: String,
    pub facts_complete: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_facts: Vec<String>,
    pub arguments: BTreeMap<String, Argument>,
    pub constraints: Vec<Constraint>,
    pub products: Value,
    pub lifecycle: Value,
    pub billing_info: Option<Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataType { String, Boolean, Integer, Number, MediaList }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Argument {
    pub data_type: DataType,
    pub required: bool,
    #[serde(flatten)]
    pub facts: BTreeMap<String, Value>,
    pub transport: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Constraint {
    pub rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Value>,
    pub check: String,
    pub arguments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<Value>>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgumentError {
    pub code: &'static str,
    pub argument_path: String,
    pub rule_id: String,
    pub actual: Value,
    pub expected: Value,
    pub message: String,
}
fn error(code: &'static str, key: &str, rule: &str, actual: Value, expected: Value) -> String {
    serde_json::to_string(&ArgumentError { code, argument_path: key.into(), rule_id: rule.into(), actual, expected,
        message: format!("{code}: {key} violates {rule}") }).expect("argument error is serializable")
}
pub(crate) fn argument(data_type: DataType, flag: Option<&str>, facts: Value) -> Argument {
    Argument { data_type, required: false, facts: serde_json::from_value(facts).expect("argument facts object"),
        transport: flag.map_or_else(|| json!({"encoding":"options-json"}), |flag| json!({"flag":flag,"encoding":"scalar"})) }
}
pub(crate) fn finish(provider: &ModelProvider, model: &str, kind: MediaKind, mut arguments: BTreeMap<String, Argument>, constraints: Vec<Value>, complete: bool, max_count: Option<u32>, asynchronous: bool) -> ModelDescription {
    for (name, arg) in &mut arguments {
        if arg.transport.get("flag").is_none() { arg.transport["optionKey"] = json!(name); }
    }
    let media_kind = match kind { MediaKind::Image => "image", MediaKind::Video => "video", MediaKind::Speech => "audio", MediaKind::Transcribe => "json", MediaKind::Edit => "edit", MediaKind::Text => "text" };
    let mut products = json!({"mediaKind":media_kind,"ordered":true,"countMeaning":"exact"});
    if let Some(max_count) = max_count {
        products["minCount"] = json!(1);
        products["maxCount"] = json!(max_count);
        products["mimeTypes"] = json!(match kind {
            MediaKind::Image => vec!["image/png","image/jpeg","image/webp","image/gif"],
            MediaKind::Video => vec!["video/mp4","video/webm","video/quicktime"],
            MediaKind::Speech => vec!["audio/mpeg","audio/ogg","audio/aac","audio/flac","audio/wav"],
            MediaKind::Transcribe => vec!["application/json"],
            MediaKind::Edit => vec!["video/mp4","application/x-subrip"],
            MediaKind::Text => vec!["text/markdown"],
        });
    }
    let mut description = ModelDescription {
        description_version: 1, identity: format!("{}/{model}", provider.id), operation: kind.clone(), facts_revision: String::new(),
        facts_complete: complete, unknown_facts: if complete { vec![] } else if max_count.is_none() { vec!["workflowDefaults".into(),"workflowProducts".into()] } else { vec!["providerLimits".into()] }, arguments,
        constraints: constraints.into_iter().map(|v| serde_json::from_value(v).expect("route constraint")).collect(),
        products,
        lifecycle: json!({"submission":if asynchronous {"asynchronous"} else {"synchronous"},"remoteCancel":"unsupported"}), billing_info: None,
    };
    rehash(&mut description);
    description
}
pub(crate) fn rehash(description: &mut ModelDescription) {
    // RFC 8785 gives Rust and JavaScript the same numeric and Unicode canonicalization.
    let mut content = serde_json::to_value(&*description).expect("description serializable");
    content.as_object_mut().unwrap().remove("factsRevision");
    description.facts_revision = format!("sha256:{:x}", Sha256::digest(serde_jcs::to_vec(&content).expect("finite description JSON")));
}
pub fn describe(provider: &ModelProvider, model: &str, kind: &MediaKind) -> ModelDescription {
    if let Some(flow) = provider.request.comfy.as_ref().and_then(|c| c.workflows.iter().find(|w| w.id == model)) {
        return workflow_description(provider, flow, kind);
    }
    match kind {
        MediaKind::Image => image_providers::model_description(provider, model),
        MediaKind::Video => video_providers::model_description(provider, model),
        MediaKind::Speech => super::speech_providers::model_description(provider, model),
        MediaKind::Transcribe => super::transcribe_description(provider, model),
        MediaKind::Edit => super::local_edit::description(provider, model),
        MediaKind::Text => super::text_record_description(provider, model),
    }
}

pub(crate) fn workflow_description(provider: &ModelProvider, flow: &crate::comfyui::ComfyWorkflow, kind: &MediaKind) -> ModelDescription {
    use crate::comfyui::{ComfyInputKind, ComfyInputSource};
    let mut args = BTreeMap::new();
    let mut image_count = 0;
    for input in &flow.inputs {
        let name = format!("{}:{}", input.node_id, input.input);
        let data_type = if input.kind == ComfyInputKind::Number { DataType::Number } else { DataType::String };
        // A graph default can contain private text or credentials; it is not a public model fact.
        args.insert(name, argument(data_type, None, json!({})));
        match &input.source {
            Some(ComfyInputSource::Prompt) => {
                let mut prompt = argument(DataType::String, Some("--prompt-file"), json!({}));
                prompt.transport["encoding"] = json!("utf8-file");
                args.insert("prompt".into(), prompt);
            }
            Some(ComfyInputSource::Image { index }) => { image_count = image_count.max(index + 1); }
            _ => {}
        }
    }
    if image_count > 0 {
        args.insert("images".into(), argument(DataType::MediaList, Some("--ref"), json!({"maxCount":image_count,"maxBytes":24*1024*1024,"mimePatterns":["image/png","image/jpeg","image/webp"],"locations":["local","inline"]})));
    }
    // Workflow output cardinality and formats depend on its graph, not the cloud model name.
    finish(provider, &flow.id, kind.clone(), args, vec![], false, None, true)
}
pub(crate) fn canonical_key(key: &str) -> &str {
    match key { "aspect_ratio" => "aspectRatio", "output_format" => "outputFormat", _ => key }
}
pub(crate) fn normalize(args: BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
    let mut result = BTreeMap::new();
    for (key, value) in args {
        let name = canonical_key(&key);
        if result.insert(name.into(), value).is_some() {
            return Err(error("MODEL_ARGUMENT_DUPLICATE", name, "unique", json!(key), json!("one spelling")));
        }
    }
    Ok(result)
}
fn present(value: Option<&Value>, truthy: bool) -> bool {
    value.is_some_and(|v| !v.is_null() && (!v.is_array() || !v.as_array().unwrap().is_empty()) &&
        (!truthy || !(v == &json!(false) || v == &json!(0) || v == &json!(""))))
}
fn condition(when: &Value, args: &BTreeMap<String, Value>, names: &BTreeSet<&str>) -> Result<bool, String> {
    let bad = || error("MODEL_DESCRIPTION_INVALID", "constraints", "condition", when.clone(), json!("provided/equals/all/any/not"));
    let obj = when.as_object().filter(|o| o.len() == 1).ok_or_else(bad)?;
    let (op, value) = obj.iter().next().unwrap();
    match op.as_str() {
        "provided" => { let name = value.as_str().filter(|n| names.contains(n)).ok_or_else(bad)?; Ok(present(args.get(name), false)) },
        "equals" => { let o = value.as_object().filter(|o|o.len() == 2 && o.contains_key("value")).ok_or_else(bad)?; let name = o.get("argument").and_then(Value::as_str).filter(|n| names.contains(n)).ok_or_else(bad)?; Ok(args.get(name) == o.get("value")) },
        "not" => Ok(!condition(value, args, names)?),
        "all" | "any" => {
            let items = value.as_array().ok_or_else(bad)?;
            let mut result = op == "all";
            for item in items {
                let current = condition(item, args, names)?;
                if op == "all" { result &= current; } else { result |= current; }
            }
            Ok(result)
        },
        _ => Err(bad()),
    }
}
fn validate_value(name: &str, arg: &Argument, value: &Value) -> Result<(), String> {
    let f = &arg.facts;
    let bad = |rule: &str, expected: Value| error("MODEL_ARGUMENT_INVALID", name, rule, value.clone(), expected);
    if f.get("specialValues").and_then(Value::as_array).is_some_and(|a| a.contains(value)) { return Ok(()); }
    let typed = match arg.data_type { DataType::String => value.is_string(), DataType::Boolean => value.is_boolean(), DataType::Integer => value.as_i64().is_some() || value.as_u64().is_some(), DataType::Number => value.is_number(), DataType::MediaList => value.is_array() };
    if !typed { return Err(bad("dataType", json!(arg.data_type))); }
    let allowed = f.get("allowed").and_then(Value::as_array);
    let in_allowed = allowed.is_some_and(|a| a.contains(value)) || f.get("pixelDimensions") == Some(&json!(true)) && value.as_str().is_some_and(|s| s.split_once('x').is_some_and(|(w,h)| w.parse::<u32>().is_ok_and(|n|n > 0) && h.parse::<u32>().is_ok_and(|n|n > 0)));
    let number = value.as_f64();
    let range = f.contains_key("minimum") || f.contains_key("maximum");
    let in_range = range && number.is_some_and(|n| f.get("minimum").and_then(Value::as_f64).is_none_or(|min| n >= min) && f.get("maximum").and_then(Value::as_f64).is_none_or(|max| n <= max));
    if (allowed.is_some() || range) && !in_allowed && !in_range { return Err(bad("allowed", json!(f))); }
    if let Some(text) = value.as_str() {
        let count = if f.get("lengthUnit").and_then(Value::as_str) == Some("utf16CodeUnit") {text.encode_utf16().count()} else {text.chars().count()};
        if f.get("minLength").and_then(Value::as_u64).is_some_and(|n| count < n as usize) || f.get("maxLength").and_then(Value::as_u64).is_some_and(|n| count > n as usize) { return Err(bad("length", json!(f))); }
    }
    if let Some(items) = value.as_array() {
        if f.get("minCount").and_then(Value::as_u64).is_some_and(|n| items.len() < n as usize) || f.get("maxCount").and_then(Value::as_u64).is_some_and(|n| items.len() > n as usize) { return Err(bad("count", json!(f))); }
        if arg.data_type == DataType::MediaList {
            for item in items {
                let source = item.as_str().or_else(|| item.get("source").and_then(Value::as_str)).ok_or_else(|| bad("mediaSource", json!("source string")))?;
                if let Some(object) = item.as_object() {
                    if object.keys().any(|k| k != "source" && k != "attributes") || object.get("attributes").is_some_and(|a| !a.is_object()) { return Err(bad("mediaEntry", json!("source/attributes"))); }
                }
                let location = if source.starts_with("https://") {"https"} else if source.starts_with("http://") {"http"} else if source.starts_with("data:") || source.starts_with("mm_file://") {"inline"} else {"local"};
                if f.get("locations").and_then(Value::as_array).is_some_and(|a| !a.contains(&json!(location))) { return Err(bad("location", json!(f.get("locations")))); }
                if location == "local" && f.get("opaqueSources") != Some(&json!(true)) {
                    let path = std::path::Path::new(source);
                    if !path.is_absolute() { return Err(bad("absolutePath", json!("absolute local path"))); }
                    let metadata = std::fs::metadata(path).map_err(|_| bad("file", json!("readable file")))?;
                    if !metadata.is_file() || f.get("maxBytes").and_then(Value::as_u64).is_some_and(|n| metadata.len() > n) { return Err(bad("maxBytes", json!(f.get("maxBytes")))); }
                }
            }
        }
    }
    Ok(())
}
pub fn validate_and_resolve(provider: &ModelProvider, model: &str, kind: &MediaKind, args: BTreeMap<String, Value>, revision: Option<&str>) -> Result<BTreeMap<String, Value>, String> {
    resolve(&describe(provider, model, kind), args, revision)
}

fn validate_description(description: &ModelDescription, names: &BTreeSet<&str>) -> Result<(), String> {
    let invalid = |name: &str, rule: &str, actual: Value| error("MODEL_DESCRIPTION_INVALID", name, rule, actual, Value::Null);
    if description.description_version != 1 { return Err(invalid("descriptionVersion", "version", json!(description.description_version))); }
    let mut option_keys = BTreeSet::new();
    for (name, arg) in &description.arguments {
        if let Some(key) = arg.transport.get("optionKey") {
            let key = key.as_str().ok_or_else(|| invalid(name, "optionKey", key.clone()))?;
            if !option_keys.insert(key) { return Err(invalid(name, "uniqueOptionKey", json!(key))); }
        }
        if !matches!(arg.transport.get("encoding").and_then(Value::as_str), Some("scalar"|"utf8-file"|"options-json")) {
            return Err(invalid(name, "transportEncoding", arg.transport.clone()));
        }
        if let Some(default) = arg.facts.get("defaultValue") {
            validate_value(name, arg, default).map_err(|_| invalid(name, "defaultValue", default.clone()))?;
        }
        if let Some(derived) = arg.facts.get("derivedFrom") {
            // Finite derivations read media evidence, never another derived scalar or script:
            // this makes default/derive cycles unrepresentable.
            let valid = match derived.as_str() {
                Some("imageAspectRatio") => arg.data_type == DataType::String && ["images","firstFrame","referenceImages"].iter().any(|n| description.arguments.get(*n).is_some_and(|a|a.data_type == DataType::MediaList)),
                Some("videoDuration") => matches!(arg.data_type, DataType::Number|DataType::Integer) && ["videos","referenceVideos"].iter().any(|n| description.arguments.get(*n).is_some_and(|a|a.data_type == DataType::MediaList)),
                _ => false,
            };
            if !valid { return Err(invalid(name, "derivedFrom", derived.clone())); }
        }
    }
    let empty = BTreeMap::new();
    let mut rules = BTreeSet::new();
    for rule in &description.constraints {
        let unique: BTreeSet<_> = rule.arguments.iter().collect();
        if rule.rule_id.is_empty() || !rules.insert(&rule.rule_id) || unique.len() != rule.arguments.len() || rule.arguments.is_empty() || rule.arguments.iter().any(|n| !names.contains(n.as_str())) ||
            !matches!(rule.check.as_str(),"require"|"excludeTogether"|"countAtMost"|"weightedCountAtMost"|"durationTotalAtMost"|"restrictAllowed") ||
            rule.presence_mode.as_deref().is_some_and(|m| !matches!(m,"truthy"|"provided")) ||
            matches!(rule.check.as_str(),"countAtMost"|"weightedCountAtMost"|"durationTotalAtMost") && rule.limit.is_none_or(|n| !n.is_finite() || n < 0.0) ||
            rule.check == "restrictAllowed" && rule.allowed.is_none() ||
            rule.weights.is_some() && rule.check != "weightedCountAtMost" ||
            rule.weights.as_ref().is_some_and(|w| w.iter().any(|(name,n)| !unique.contains(name) || !n.is_finite() || *n < 0.0)) {
            return Err(invalid("constraints",&rule.rule_id,json!(rule)));
        }
        if let Some(when) = &rule.when { condition(when, &empty, names)?; }
    }
    Ok(())
}
pub(crate) fn resolve(description: &ModelDescription, args: BTreeMap<String, Value>, revision: Option<&str>) -> Result<BTreeMap<String, Value>, String> {
    if revision.is_some_and(|r| r != description.facts_revision) { return Err(error("MODEL_DESCRIPTION_CHANGED", "descriptionRevision", "revision", json!(revision), json!(description.facts_revision))); }
    let mut args = normalize(args)?;
    if let Some(Value::String(size)) = args.get_mut("size") {
        if size.eq_ignore_ascii_case("1k") || size.eq_ignore_ascii_case("2k") || size.eq_ignore_ascii_case("4k") { size.make_ascii_uppercase(); }
    }
    let mut defaults = BTreeMap::new();
    let names: BTreeSet<_> = description.arguments.keys().map(String::as_str).collect();
    validate_description(description, &names)?;
    for (name, value) in &args {
        let arg = description.arguments.get(name).ok_or_else(|| error("MODEL_ARGUMENT_UNSUPPORTED", name, "declared", json!(name), json!(names)))?;
        validate_value(name, arg, value)?;
    }
    for (name, arg) in &description.arguments {
        if !args.contains_key(name) {
            if let Some(default) = arg.facts.get("defaultValue") { validate_value(name, arg, default)?; defaults.insert(name.clone(), default.clone()); }
            else if arg.required { return Err(error("MODEL_ARGUMENT_REQUIRED", name, "required", Value::Null, json!(true))); }
        }
    }
    for rule in &description.constraints {
        if let Some(when) = &rule.when { if !condition(when, &args, &names)? { continue; } }
        let truthy = rule.presence_mode.as_deref() == Some("truthy");
        let supplied = rule.arguments.iter().filter(|n| present(args.get(*n), truthy));
        let valid = match rule.check.as_str() {
            "require" => rule.arguments.iter().all(|n| present(args.get(n).or_else(|| defaults.get(n)), truthy)),
            "excludeTogether" => supplied.count() <= 1,
            "restrictAllowed" => rule.arguments.iter().all(|n| args.get(n).or_else(|| defaults.get(n)).is_none_or(|v| rule.allowed.as_ref().is_some_and(|a| a.contains(v)))),
            "countAtMost"|"weightedCountAtMost" => { let count: f64 = supplied.map(|n| {let count = args.get(n).and_then(Value::as_array).map_or(1, Vec::len) as f64; count * rule.weights.as_ref().and_then(|w| w.get(n)).copied().unwrap_or(1.0)}).sum(); rule.limit.is_some_and(|n| count <= n) },
            "durationTotalAtMost" => {
                let duration = supplied.flat_map(|n| args.get(n).and_then(Value::as_array).into_iter().flatten()).try_fold(0.0, |sum,v| v.get("attributes").and_then(|a| a.get("duration")).and_then(Value::as_f64).filter(|n| *n >= 0.0).map(|duration| sum + duration));
                duration.is_some_and(|duration| rule.limit.is_some_and(|limit| duration <= limit))
            },
            _ => unreachable!(),
        };
        if !valid { return Err(error("MODEL_ARGUMENT_CONSTRAINT", &rule.arguments.join(","), &rule.rule_id, json!(args), json!(rule))); }
    }
    args.extend(defaults);
    Ok(args)
}
pub(crate) fn media_sources(value: &Value) -> Result<Vec<String>, String> {
    value.as_array().ok_or("media list must be an array")?.iter().map(|v| v.as_str().or_else(|| v.get("source").and_then(Value::as_str)).map(str::to_owned).ok_or_else(|| "media entry requires source".into())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn provider() -> ModelProvider {
        serde_json::from_value(json!({"id":"p","name":"P","baseUrl":"https://example.test/v1","apiKeys":["test"],"enabled":true,"enabledModels":["gpt-image-2","grok-imagine-video-1.5"]})).unwrap()
    }
    fn description() -> ModelDescription {
        let args = [
            ("enabled".into(), argument(DataType::Boolean, None, json!({"defaultValue":false}))),
            ("seed".into(), argument(DataType::Integer, None, json!({"minimum":0,"maximum":10,"defaultValue":0}))),
            ("duration".into(), argument(DataType::Integer, None, json!({"allowed":[0],"minimum":1,"maximum":4,"specialValues":["auto"]}))),
            ("images".into(), argument(DataType::MediaList, None, json!({"maxCount":4,"locations":["https"]}))),
            ("videos".into(), argument(DataType::MediaList, None, json!({"maxCount":2,"locations":["https"]}))),
        ].into();
        finish(&provider(), "custom", MediaKind::Video, args, vec![], true, Some(1), true)
    }
    fn code(error: String) -> Value { serde_json::from_str(&error).unwrap() }
    #[test]
    fn unsupported_and_stale_requests_are_rejected_before_resolution() {
        let p = provider();
        let args = serde_json::from_value(json!({"prompt":"an icon","apiKey":"must not become vendor input"})).unwrap();
        let err = code(validate_and_resolve(&p,"gpt-image-2",&MediaKind::Image,args,None).unwrap_err());
        assert_eq!(err["code"],"MODEL_ARGUMENT_UNSUPPORTED");
        assert_eq!(err["argumentPath"],"apiKey");
        let d = describe(&p,"gpt-image-2",&MediaKind::Image);
        let args = serde_json::from_value(json!({"prompt":"an icon"})).unwrap();
        let err = code(resolve(&d,args,Some("sha256:stale")).unwrap_err());
        assert_eq!(err["code"],"MODEL_DESCRIPTION_CHANGED");
        assert_eq!(err["expected"],d.facts_revision);
    }
    #[test]
    fn zero_false_allowed_range_union_and_auto_do_not_use_truthiness() {
        let d = description();
        for duration in [json!(0),json!(2),json!("auto")] {
            let args = resolve(&d,[("duration".into(),duration.clone())].into(),None).unwrap();
            assert_eq!(args["seed"],0);
            assert_eq!(args["enabled"],false);
            assert_eq!(args["duration"],duration);
        }
        assert_eq!(code(resolve(&d,[("duration".into(),json!(5))].into(),None).unwrap_err())["code"],"MODEL_ARGUMENT_INVALID");
        let invalid: Result<super::super::video_providers::VideoDuration,_> = serde_json::from_value(json!("arbitrary"));
        assert!(invalid.is_err());
    }
    #[test]
    fn defaults_do_not_trigger_user_presence_exclusion_but_false_is_provided() {
        let mut d = description();
        d.constraints = vec![serde_json::from_value(json!({"ruleId":"one-control","check":"excludeTogether","arguments":["enabled","seed"]})).unwrap()];
        assert_eq!(resolve(&d,[("enabled".into(),json!(false))].into(),None).unwrap()["seed"],0);
        let err = code(resolve(&d,[("enabled".into(),json!(false)),("seed".into(),json!(0))].into(),None).unwrap_err());
        assert_eq!(err["ruleId"],"one-control");
        d.constraints[0].presence_mode = Some("truthy".into());
        assert_eq!(resolve(&d,[("enabled".into(),json!(false)),("seed".into(),json!(0))].into(),None).unwrap()["enabled"],false);
    }
    #[test]
    fn nested_conditions_and_shared_weighted_quota_are_enforced() {
        let mut d = description();
        d.constraints = vec![serde_json::from_value(json!({"ruleId":"shared-quota","when":{"all":[{"provided":"images"},{"not":{"equals":{"argument":"enabled","value":true}}}]},"check":"weightedCountAtMost","arguments":["images","videos"],"weights":{"images":1,"videos":2},"limit":3})).unwrap()];
        let images = json!([{"source":"https://example.test/a.png","attributes":{"personPresent":false}}]);
        let videos = json!([{"source":"https://example.test/a.mp4","attributes":{"duration":1}}]);
        let args: BTreeMap<_,_> = [("images".into(),images.clone()),("videos".into(),videos.clone())].into();
        assert_eq!(resolve(&d,args,None).unwrap()["images"][0]["attributes"]["personPresent"],false);
        let args = [("images".into(),images),("videos".into(),json!([videos[0],videos[0]]))].into();
        assert_eq!(code(resolve(&d,args,None).unwrap_err())["ruleId"],"shared-quota");
        d.constraints[0].when = Some(json!({"provided":"missing"}));
        assert_eq!(code(resolve(&d,BTreeMap::new(),None).unwrap_err())["code"],"MODEL_DESCRIPTION_INVALID");
    }
    #[test]
    fn aliases_conflict_and_unknown_models_do_not_accept_extras() {
        let p = provider();
        let args = serde_json::from_value(json!({"prompt":"icon","aspect_ratio":"1:1","aspectRatio":"1:1"})).unwrap();
        assert_eq!(code(validate_and_resolve(&p,"gpt-image-2",&MediaKind::Image,args,None).unwrap_err())["code"],"MODEL_ARGUMENT_DUPLICATE");
        let args = serde_json::from_value(json!({"prompt":"scene","seed":0})).unwrap();
        assert_eq!(code(validate_and_resolve(&p,"custom-video",&MediaKind::Video,args,None).unwrap_err())["code"],"MODEL_ARGUMENT_UNSUPPORTED");
    }
    #[test]
    fn video_route_intersection_rejects_unmapped_arguments_and_auto() {
        let mut p = provider();
        let args = serde_json::from_value(json!({"prompt":"scene","generateAudio":false,"duration":1})).unwrap();
        assert_eq!(validate_and_resolve(&p,"grok-imagine-video-1.5",&MediaKind::Video,args,None).unwrap()["generateAudio"],false);
        let args = serde_json::from_value(json!({"prompt":"scene","duration":"auto"})).unwrap();
        assert_eq!(code(validate_and_resolve(&p,"grok-imagine-video-1.5",&MediaKind::Video,args,None).unwrap_err())["code"],"MODEL_ARGUMENT_INVALID");
        p.model_overrides.insert("grok-imagine-video-1.5".into(),serde_json::from_value(json!({"videoProtocol":"minimax_hailuo"})).unwrap());
        let args = serde_json::from_value(json!({"prompt":"scene","generateAudio":false})).unwrap();
        assert_eq!(code(validate_and_resolve(&p,"grok-imagine-video-1.5",&MediaKind::Video,args,None).unwrap_err())["code"],"MODEL_ARGUMENT_UNSUPPORTED");
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn typed_video_boundary_does_not_discard_unknown_null_parameters() {
        let input = serde_json::from_value(json!({"prompt":"a scene","apiKey":null})).unwrap();
        let error = video_providers::prepare("xai_video","https://example.test","grok-imagine-video-1.5",&input).unwrap_err();
        let error: Value = serde_json::from_str(&error).unwrap();
        assert_eq!(error["code"],"MODEL_ARGUMENT_UNSUPPORTED");
        assert_eq!(error["argumentPath"],"apiKey");
    }
    #[test]
    fn workflow_model_facts_never_publish_private_graph_defaults() {
        let provider: ModelProvider = serde_json::from_value(json!({
            "id":"local","name":"Local","baseUrl":"http://localhost:8188","enabled":true,
            "request":{"comfy":{"workflows":[{
                "id":"gpt-image-2","name":"private workflow","kind":"image",
                "graph":{"1":{"class_type":"TextNode","inputs":{"text":"private-credential-text"}},"2":{"class_type":"SaveImage","inputs":{"image":["1",0]}}},
                "inputs":[{"nodeId":"1","input":"text","label":"Text","kind":"text","source":{"type":"prompt"}}],"outputNodes":["2"]
            }]}}
        })).unwrap();
        let description = describe(&provider,"gpt-image-2",&MediaKind::Image);
        assert_eq!(description.arguments["1:text"].data_type,DataType::String);
        assert!(!description.arguments.contains_key("background"));
        assert!(!description.arguments["1:text"].facts.contains_key("defaultValue"));
        assert!(description.products.get("maxCount").is_none());
        assert!(description.unknown_facts.iter().any(|n|n == "workflowDefaults"));
        assert!(!serde_json::to_string(&description).unwrap().contains("private-credential-text"));
    }
}
