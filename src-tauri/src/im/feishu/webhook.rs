//! Webhook authentication: size limit, decrypt, token, signature, timestamp/nonce replay.
//! Challenge is echoed only after the configured credential matches. A forged challenge is not reflected.

use super::crypto::{self, GuardReject, WebGuard};
use serde_json::{json, Value};

pub const MAX_BODY: usize = 1024 * 1024;

pub struct WebReply {
    pub status: u16,
    pub body: String,
    pub event: Option<Value>,
}

pub struct WebInput<'a> {
    pub content_type: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub signature: &'a str,
    pub body: &'a [u8],
    pub encrypt_key: &'a str,
    pub verification_token: &'a str,
    pub now: i64,
    pub peer: &'a str,
}

pub fn evaluate(input: &WebInput<'_>, guard: &mut WebGuard) -> WebReply {
    if input.body.len() > MAX_BODY {
        return reject(413, "payload too large");
    }
    let content_type = input.content_type.split(';').next().unwrap_or("").trim();
    if !content_type.is_empty() && !content_type.eq_ignore_ascii_case("application/json") {
        return reject(415, "unsupported media type");
    }
    if guard.allow_rate(input.peer, input.now).is_err() {
        return reject(429, "too many requests");
    }
    let parsed: Value = match serde_json::from_slice(input.body) {
        Ok(value) => value,
        Err(_) => return reject(400, "invalid json"),
    };
    let (plain, decrypted) = if let Some(encrypt) = parsed.get("encrypt").and_then(|v| v.as_str()) {
        if input.encrypt_key.is_empty() {
            return reject(400, "encrypt key required");
        }
        match crypto::decrypt(input.encrypt_key, encrypt) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(value) => (value, true),
                Err(_) => return reject(400, "invalid encrypted payload"),
            },
            Err(()) => return reject(400, "invalid encrypted payload"),
        }
    } else {
        (parsed, false)
    };
    let token = plain
        .get("header")
        .and_then(|h| h.get("token"))
        .or_else(|| plain.get("token"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let token_ok = !input.verification_token.is_empty()
        && ct_eq(token.as_bytes(), input.verification_token.as_bytes());
    let signed = !input.encrypt_key.is_empty()
        && crypto::signatures_match(
            &crypto::signature(input.timestamp, input.nonce, input.encrypt_key, input.body),
            input.signature,
        );
    let kind = plain.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if kind == "url_verification" || plain.get("challenge").is_some() && kind.is_empty() {
        if !challenge_trusted(input, token_ok, decrypted, signed) {
            return reject(401, "invalid verification token");
        }
        let challenge = plain
            .get("challenge")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return WebReply {
            status: 200,
            body: json!({"challenge": challenge}).to_string(),
            event: None,
        };
    }
    if !input.verification_token.is_empty() && !token_ok {
        return reject(401, "invalid verification token");
    }
    if !input.encrypt_key.is_empty() {
        if !signed {
            return reject(401, "invalid signature");
        }
        match guard.allow_nonce(input.timestamp, input.nonce, input.now) {
            Err(GuardReject::Replay) => return reject(401, "replayed nonce"),
            Err(GuardReject::Stale) => return reject(401, "stale timestamp"),
            Err(GuardReject::Rate) => return reject(429, "too many requests"),
            Err(GuardReject::Duplicate) | Ok(()) => {}
        }
    }
    WebReply {
        status: 200,
        body: json!({"code": 0, "msg": "ok"}).to_string(),
        event: Some(plain),
    }
}

fn challenge_trusted(input: &WebInput<'_>, token_ok: bool, decrypted: bool, signed: bool) -> bool {
    if !input.verification_token.is_empty() {
        return token_ok;
    }
    if !input.encrypt_key.is_empty() {
        return decrypted || signed;
    }
    false
}

fn ct_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

fn reject(status: u16, msg: &str) -> WebReply {
    WebReply {
        status,
        body: json!({"code": status, "msg": msg}).to_string(),
        event: None,
    }
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    state: std::sync::Arc<crate::im::feishu::Hook>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let app = axum::Router::new()
        .fallback(handler)
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .with_state(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            if shutdown.changed().await.is_err() {
                break;
            }
        }
    })
    .await
    .map_err(|_| "\u{98de}\u{4e66} Webhook \u{5df2}\u{505c}\u{6b62}".to_string())
}

async fn handler(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::im::feishu::Hook>>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl axum::response::IntoResponse {
    if method != axum::http::Method::POST {
        return response(405, r#"{"code":405,"msg":"method not allowed"}"#.into());
    }
    if uri.path() != state.path {
        return response(404, r#"{"code":404,"msg":"not found"}"#.into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let timestamp = headers
        .get("x-lark-request-timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let nonce = headers
        .get("x-lark-request-nonce")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let signature = headers
        .get("x-lark-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let peer_ip = peer.ip().to_string();
    let input = WebInput {
        content_type,
        timestamp: &timestamp,
        nonce: &nonce,
        signature: &signature,
        body: &body,
        encrypt_key: &state.encrypt_key,
        verification_token: &state.verification_token,
        now,
        peer: &peer_ip,
    };
    let reply = {
        let mut guard = state.guard.lock();
        evaluate(&input, &mut guard)
    };
    if reply.event.is_none() {
        return response(reply.status, reply.body);
    }
    let event = reply.event.unwrap_or(Value::Null);
    let code = super::accept(&state.runtime, &event).await;
    if code == 200 {
        response(200, reply.body)
    } else {
        response(code, json!({"code": code, "msg": "queue full"}).to_string())
    }
}

fn response(
    status: u16,
    body: String,
) -> (
    axum::http::StatusCode,
    [(axum::http::HeaderName, &'static str); 1],
    String,
) {
    (
        axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im::feishu::crypto::encrypt;

    fn input<'a>(
        body: &'a [u8],
        token: &'a str,
        key: &'a str,
        ts: &'a str,
        nonce: &'a str,
        sig: &'a str,
    ) -> WebInput<'a> {
        WebInput {
            content_type: "application/json",
            timestamp: ts,
            nonce,
            signature: sig,
            body,
            encrypt_key: key,
            verification_token: token,
            now: 1_700_000_000,
            peer: "127.0.0.1",
        }
    }

    #[test]
    fn forged_challenge_is_not_echoed_and_bad_signature_is_rejected() {
        let mut guard = WebGuard::default();
        let forged = br#"{"type":"url_verification","token":"evil","challenge":"pwned-challenge"}"#;
        let reply = evaluate(&input(forged, "good-token", "", "", "", ""), &mut guard);
        assert_eq!(reply.status, 401);
        assert!(!reply.body.contains("pwned-challenge"));
        let ok =
            br#"{"type":"url_verification","token":"good-token","challenge":"real-challenge"}"#;
        let reply = evaluate(&input(ok, "good-token", "", "", "", ""), &mut guard);
        assert_eq!(reply.status, 200);
        assert!(reply.body.contains("real-challenge"));

        let event = br#"{"header":{"event_type":"im.message.receive_v1","token":"good-token","event_id":"evt-1"},"event":{"message":{"message_id":"om"}}}"#;
        let ts = "1700000000";
        let nonce = "n-1";
        let key = "enc-key";
        let naked = br#"{"type":"url_verification","challenge":"pwned-challenge"}"#;
        let naked_reply = evaluate(
            &input(naked, "", "enc-key", "", "", ""),
            &mut WebGuard::default(),
        );
        assert_eq!(naked_reply.status, 401);
        assert!(!naked_reply.body.contains("pwned-challenge"));
        let bad = evaluate(
            &input(
                event,
                "good-token",
                key,
                ts,
                nonce,
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            ),
            &mut WebGuard::default(),
        );
        assert_eq!(bad.status, 401);
        assert!(bad.event.is_none());
        let sig = crypto::signature(ts, nonce, key, event);
        let mut once = WebGuard::default();
        let good = evaluate(&input(event, "good-token", key, ts, nonce, &sig), &mut once);
        assert_eq!(good.status, 200);
        assert!(good.event.is_some());
        let replay = evaluate(&input(event, "good-token", key, ts, nonce, &sig), &mut once);
        assert_eq!(replay.status, 401);
    }

    #[test]
    fn encrypted_challenge_and_event_roundtrip() {
        let mut guard = WebGuard::default();
        let plain = br#"{"type":"url_verification","token":"good-token","challenge":"hidden"}"#;
        let body = serde_json::to_vec(&json!({"encrypt": encrypt("enc-key", plain)})).unwrap();
        let reply = evaluate(
            &input(&body, "good-token", "enc-key", "", "", ""),
            &mut guard,
        );
        assert!(reply.body.contains("hidden"));
        let event = br#"{"header":{"event_id":"evt-9","token":"good-token","event_type":"im.message.receive_v1"},"event":{}}"#;
        let wrapped = serde_json::to_vec(&json!({"encrypt": encrypt("enc-key", event)})).unwrap();
        let ts = "1700000000";
        let nonce = "n-enc";
        let sig = crypto::signature(ts, nonce, "enc-key", &wrapped);
        let reply = evaluate(
            &input(&wrapped, "good-token", "enc-key", ts, nonce, &sig),
            &mut guard,
        );
        assert_eq!(reply.status, 200);
        assert_eq!(reply.event.unwrap()["header"]["event_id"], "evt-9");
    }

    #[test]
    fn oversized_body_is_rejected() {
        let mut guard = WebGuard::default();
        let body = vec![b' '; MAX_BODY + 1];
        let reply = evaluate(&input(&body, "t", "", "", "", ""), &mut guard);
        assert_eq!(reply.status, 413);
    }
}
