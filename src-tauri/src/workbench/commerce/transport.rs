//! Shared HTTP seam for platform adapters. The adapter sets `host`; this module does not.
use super::types::{CommerceError, ListingStatus};
use crate::workbench::shops::Platform;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ResolvedShop {
    pub platform: Platform,
    pub partner_id: String,
    pub partner_key: String,
    pub access_token: String,
    pub remote_id: String,
    pub region: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Outbound {
    pub method: &'static str,
    pub host: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub json: Option<Value>,
    pub file_name: Option<String>,
    pub file_bytes: Option<Vec<u8>>,
    /// True only for the call that creates or updates the remote listing.
    pub mutating: bool,
}

impl Outbound {
    pub fn query_value(&self, key: &str) -> Option<&str> {
        self.query.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    pub fn header_value(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct Inbound {
    pub status: u16,
    pub body: Value,
}

#[derive(Clone, Debug)]
pub enum TransportFault {
    BeforeDispatch(String),
    AfterDispatch(String),
}

#[derive(Clone, Debug)]
pub enum Step {
    Json(Value),
    Fault(TransportFault),
}

#[derive(Clone, Debug)]
pub struct PushOutcome {
    pub remote_id: Option<String>,
    pub status: ListingStatus,
    pub reason: Option<String>,
}

#[derive(Debug)]
pub struct ScriptTransport {
    queues: Mutex<HashMap<String, VecDeque<Step>>>,
    calls: Mutex<Vec<Outbound>>,
}

impl ScriptTransport {
    pub fn new() -> Self {
        Self { queues: Mutex::new(HashMap::new()), calls: Mutex::new(Vec::new()) }
    }

    pub fn push(&self, path: &str, step: Step) {
        self.queues.lock().expect("script queue").entry(path.to_string()).or_default().push_back(step);
    }

    pub fn calls(&self) -> Vec<Outbound> {
        self.calls.lock().expect("script calls").clone()
    }

    fn take(&self, request: &Outbound) -> Result<Step, TransportFault> {
        let mut queues = self.queues.lock().expect("script queue");
        let full = format!("{}{}", request.host.trim_end_matches('/'), request.path);
        for key in [request.path.as_str(), full.as_str()] {
            if let Some(step) = queues.get_mut(key).and_then(|queue| queue.pop_front()) {
                return Ok(step);
            }
        }
        Err(TransportFault::BeforeDispatch(format!("测试脚本没有 {} 的响应", request.path)))
    }
}

pub enum Transport {
    Live(reqwest::Client),
    Script(Arc<ScriptTransport>),
}

impl Transport {
    pub fn live() -> Result<Self, CommerceError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| CommerceError::internal(format!("无法创建 HTTP 客户端：{error}")))?;
        Ok(Self::Live(client))
    }

    pub fn script(script: Arc<ScriptTransport>) -> Self {
        Self::Script(script)
    }

    pub async fn execute(&self, request: Outbound) -> Result<Inbound, TransportFault> {
        match self {
            Self::Script(script) => {
                script.calls.lock().expect("script calls").push(request.clone());
                match script.take(&request)? {
                    Step::Json(body) => Ok(Inbound { status: 200, body }),
                    Step::Fault(fault) => Err(fault),
                }
            }
            Self::Live(client) => live_execute(client, request).await,
        }
    }
}

async fn live_execute(client: &reqwest::Client, request: Outbound) -> Result<Inbound, TransportFault> {
    if request.host.trim().is_empty() || !request.path.starts_with('/') {
        return Err(TransportFault::BeforeDispatch("请求缺少 host 或 path".into()));
    }
    let url = format!("{}{}", request.host.trim_end_matches('/'), request.path);
    let mut builder = match request.method {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => client.get(url),
    };
    let form = request.header_value("content-type").is_some_and(|value| value.to_ascii_lowercase().contains("application/x-www-form-urlencoded"));
    let file_field = file_part_name(&request);
    let query_in_form = request.path.contains("upload-pic");
    if !form && !query_in_form {
        builder = builder.query(&request.query);
    }
    for (name, value) in &request.headers {
        if is_local_header(name) {
            continue;
        }
        if name.eq_ignore_ascii_case("content-type") && (request.json.is_some() || request.file_bytes.is_some() || form) {
            continue;
        }
        builder = builder.header(name, value);
    }
    if let Some(bytes) = request.file_bytes {
        let mut part = reqwest::multipart::Part::bytes(bytes);
        if let Some(name) = request.file_name.clone() {
            part = part.file_name(name);
        }
        let mut form_body = reqwest::multipart::Form::new().part(file_field, part);
        if query_in_form {
            for (key, value) in &request.query {
                form_body = form_body.text(key.clone(), value.clone());
            }
        }
        if let Some(Value::Object(fields)) = &request.json {
            for (key, value) in fields {
                let Some(text) = json_text(value) else { continue };
                form_body = form_body.text(key.clone(), text);
            }
        }
        builder = builder.multipart(form_body);
    } else if form {
        let encoded = request
            .query
            .iter()
            .map(|(key, value)| format!("{}={}", form_encode(key), form_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        let content_type = request.header_value("content-type").unwrap_or("application/x-www-form-urlencoded");
        builder = builder.header("content-type", content_type).body(encoded);
    } else if let Some(body) = &request.json {
        builder = builder.json(body);
    }
    let response = match builder.send().await {
        Ok(response) => response,
        Err(error) => {
            let message = error.to_string();
            return Err(if error.is_connect() {
                TransportFault::BeforeDispatch(message)
            } else {
                TransportFault::AfterDispatch(message)
            });
        }
    };
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|error| TransportFault::AfterDispatch(error.to_string()))?;
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": "http", "message": text.chars().take(300).collect::<String>() }));
    Ok(Inbound { status, body })
}

fn file_part_name(request: &Outbound) -> String {
    if let Some(name) = request.header_value("x-dsivio-file-field").or_else(|| request.header_value("x-file-field")) {
        return name.to_string();
    }
    if request.path.contains("upload-pic") || request.path.contains("/pictures/") {
        return "file".into();
    }
    if request.path.contains("/images/upload") {
        return "data".into();
    }
    "image".into()
}

fn is_local_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("x-dsivio-file-field") || name.eq_ignore_ascii_case("x-file-field")
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
