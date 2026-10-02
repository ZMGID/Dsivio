//! HTTP seam. Production uses reqwest; tests replay official-doc-shaped responses.
use std::{future::Future, pin::Pin, time::Duration};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone)]
pub(crate) struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl HttpResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    pub(crate) fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body).map_err(|_| "平台响应不是 JSON".to_string())
    }
}

#[derive(Debug)]
pub(crate) struct TransportError(pub String);

pub(crate) trait Transport: Send + Sync {
    fn send<'a>(&'a self, request: HttpRequest) -> BoxFuture<'a, Result<HttpResponse, TransportError>>;
}

pub(crate) struct LiveTransport {
    client: reqwest::Client,
}
impl LiveTransport {
    pub(crate) fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("HTTP 客户端不可用：{error}"))?;
        Ok(Self { client })
    }
}
impl Transport for LiveTransport {
    fn send<'a>(&'a self, request: HttpRequest) -> BoxFuture<'a, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes())
                .map_err(|_| TransportError(format!("无效的 HTTP 方法 {}", request.method)))?;
            let mut builder = self.client.request(method, &request.url);
            for (key, value) in &request.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }
            if !request.body.is_empty() {
                builder = builder.body(request.body);
            }
            let response = builder.send().await.map_err(|error| TransportError(error.to_string()))?;
            let status = response.status().as_u16();
            let headers = response
                .headers()
                .iter()
                .map(|(key, value)| (key.as_str().to_string(), value.to_str().unwrap_or("").to_string()))
                .collect();
            let body = response.bytes().await.map_err(|error| TransportError(error.to_string()))?.to_vec();
            Ok(HttpResponse { status, headers, body })
        })
    }
}

#[cfg(test)]
mod scripted {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    pub(crate) struct ScriptStep {
        pub method: &'static str,
        pub url_contains: &'static str,
        pub status: u16,
        pub headers: Vec<(&'static str, String)>,
        pub body: String,
        pub transport_error: Option<String>,
    }

    pub(crate) struct ScriptedTransport {
        steps: Mutex<VecDeque<ScriptStep>>,
        log: Mutex<Vec<HttpRequest>>,
    }
    impl ScriptedTransport {
        pub(crate) fn new(steps: Vec<ScriptStep>) -> Arc<Self> {
            Arc::new(Self { steps: Mutex::new(steps.into()), log: Mutex::new(Vec::new()) })
        }
        pub(crate) fn log(&self) -> Vec<HttpRequest> {
            self.log.lock().expect("script log").clone()
        }
    }
    impl Transport for ScriptedTransport {
        fn send<'a>(&'a self, request: HttpRequest) -> BoxFuture<'a, Result<HttpResponse, TransportError>> {
            Box::pin(async move {
                self.log.lock().expect("script log").push(request.clone());
                let Some(step) = self.steps.lock().expect("script steps").pop_front() else {
                    return Err(TransportError(format!("没有预设响应：{} {}", request.method, request.url)));
                };
                if step.method != request.method || !request.url.contains(step.url_contains) {
                    return Err(TransportError(format!(
                        "请求与预设不符：{} {}，期望 {} {}",
                        request.method, request.url, step.method, step.url_contains
                    )));
                }
                if let Some(message) = step.transport_error {
                    return Err(TransportError(message));
                }
                Ok(HttpResponse {
                    status: step.status,
                    headers: step.headers.into_iter().map(|(key, value)| (key.to_string(), value)).collect(),
                    body: step.body.into_bytes(),
                })
            })
        }
    }
}
#[cfg(test)]
pub(crate) use scripted::{ScriptStep, ScriptedTransport};

pub(crate) fn json_number(value: &serde_json::Value) -> Option<f64> {
    if let Some(number) = value.as_u64() {
        return Some(number as f64);
    }
    if let Some(number) = value.as_i64() {
        return Some(number as f64);
    }
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    value.as_str().and_then(|text| text.parse().ok())
}

pub(crate) fn form_body(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish().into_bytes()
}

pub(crate) fn bearer(token: &str) -> (String, String) {
    ("Authorization".into(), format!("Bearer {token}"))
}
