//! Feishu long connection. Dynamic `wss` URL, protobuf ping, fragment assembly, async ack.
//! The read loop never waits on the agent; completed events leave through `events`.

use super::api::Endpoint;
use super::frame::{self, Assembler, Frame};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};

const UA: &str = "Dsivio/1.1 IM channel";

pub struct Assembled {
    pub frame: Frame,
    pub payload: Vec<u8>,
}

enum Wake {
    Stop,
    Ping,
    Ack(Vec<u8>),
    Message(Message),
}

enum Class {
    Pong(Option<u64>),
    Ack(Frame),
    Ignore,
}

pub async fn connect(
    endpoint: &Endpoint,
    shutdown: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<Assembled>,
    acks: mpsc::UnboundedReceiver<Vec<u8>>,
    ready: oneshot::Sender<Result<(), String>>,
) -> Result<(), String> {
    let established = async {
        let mut request = endpoint
            .url
            .as_str()
            .into_client_request()
            .map_err(|_| "飞书长连接地址无效".to_string())?;
        request
            .headers_mut()
            .insert("User-Agent", HeaderValue::from_static(UA));
        let (socket, response) = connect_async(request)
            .await
            .map_err(|error| handshake_error(&error))?;
        if let Some(code) = response
            .headers()
            .get("handshake-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i32>().ok())
        {
            if code != 0 {
                let auth = response
                    .headers()
                    .get("handshake-autherrcode")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok());
                return Err(handshake_code(code, auth));
            }
        }
        let service = url::Url::parse(&endpoint.url)
            .ok()
            .and_then(|url| {
                url.query_pairs()
                    .find(|(key, _)| key == "service_id")
                    .and_then(|(_, value)| value.parse().ok())
            })
            .unwrap_or(1);
        Ok((socket, service))
    }
    .await;
    let (socket, service) = match established {
        Ok(ready_socket) => {
            let _ = ready.send(Ok(()));
            ready_socket
        }
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    drive(
        socket,
        shutdown,
        events,
        acks,
        service,
        endpoint.ping_interval,
    )
    .await
}

pub async fn drive<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut shutdown: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<Assembled>,
    mut acks: mpsc::UnboundedReceiver<Vec<u8>>,
    service: i32,
    ping_interval: u64,
) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut socket = socket;
    let mut assembler = Assembler::default();
    let mut ping_every = Duration::from_secs(ping_interval.clamp(1, 600));
    let mut next_ping = tokio::time::Instant::now() + ping_every;
    let mut pong_by: Option<tokio::time::Instant> = None;
    loop {
        let wake = tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { Wake::Stop } else { continue; }
            }
            _ = async {
                match pong_by {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            }, if pong_by.is_some() => return Err("飞书心跳未响应".into()),
            _ = tokio::time::sleep_until(next_ping) => Wake::Ping,
            ack = acks.recv() => match ack {
                Some(ack) => Wake::Ack(ack),
                None => return Err("飞书确认通道已关闭".into()),
            },
            incoming = socket.next() => match incoming {
                None => return Err("飞书长连接已断开".into()),
                Some(Err(_)) => return Err("飞书长连接已断开".into()),
                Some(Ok(message)) => Wake::Message(message),
            },
        };
        match wake {
            Wake::Stop => {
                let _ = socket.send(Message::Close(None)).await;
                return Ok(());
            }
            Wake::Ping => {
                socket
                    .send(Message::Binary(frame::ping(service).into()))
                    .await
                    .map_err(|_| "飞书心跳失败".to_string())?;
                let now = tokio::time::Instant::now();
                let limit = ping_every.min(Duration::from_secs(20));
                pong_by = Some(now + limit);
                next_ping = now + ping_every;
            }
            Wake::Ack(bytes) => {
                socket
                    .send(Message::Binary(bytes.into()))
                    .await
                    .map_err(|_| "飞书确认发送失败".to_string())?;
            }
            Wake::Message(Message::Ping(payload)) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|_| "飞书长连接已断开".to_string())?;
            }
            Wake::Message(Message::Close(_)) => return Err("飞书长连接已断开".into()),
            Wake::Message(Message::Binary(bytes)) => {
                match classify(&bytes, &mut assembler, &events)? {
                    Class::Pong(seconds) => {
                        pong_by = None;
                        if let Some(seconds) = seconds {
                            ping_every = Duration::from_secs(seconds);
                            next_ping = tokio::time::Instant::now() + ping_every;
                        }
                    }
                    Class::Ack(frame) => {
                        socket
                            .send(Message::Binary(frame::ack(&frame, 200).into()))
                            .await
                            .map_err(|_| "飞书确认发送失败".to_string())?;
                    }
                    Class::Ignore => {}
                }
            }
            Wake::Message(_) => {}
        }
    }
}

fn classify(
    bytes: &[u8],
    assembler: &mut Assembler,
    events: &mpsc::UnboundedSender<Assembled>,
) -> Result<Class, String> {
    let frame = match frame::decode(bytes) {
        Ok(frame) => frame,
        Err(()) => return Ok(Class::Ignore),
    };
    if frame.method == 0 {
        if frame.header("type") != Some("pong") {
            return Ok(Class::Ignore);
        }
        return Ok(Class::Pong(control_interval(&frame)));
    }
    if frame.header("type") != Some("event") {
        return Ok(Class::Ack(frame));
    }
    match assembler.push(&frame, std::time::Instant::now()) {
        Ok(Some(payload)) => {
            if events.send(Assembled { frame, payload }).is_err() {
                return Err("飞书入站通道已关闭".into());
            }
            Ok(Class::Ignore)
        }
        Ok(None) | Err(()) => Ok(Class::Ignore),
    }
}

fn control_interval(frame: &Frame) -> Option<u64> {
    if frame.header("type") != Some("pong") {
        return None;
    }
    let value: serde_json::Value =
        serde_json::from_slice(&frame.payload).unwrap_or(serde_json::Value::Null);
    value
        .get("PingInterval")
        .or_else(|| value.get("ping_interval"))
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(10, 600))
}

fn handshake_error(error: &tokio_tungstenite::tungstenite::Error) -> String {
    if let tokio_tungstenite::tungstenite::Error::Http(response) = error {
        let code = response
            .headers()
            .get("handshake-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let auth = response
            .headers()
            .get("handshake-autherrcode")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        return handshake_code(code, auth);
    }
    "飞书长连接失败".into()
}

fn handshake_code(code: i32, auth: Option<i32>) -> String {
    if auth == Some(1000040350) {
        return "fatal: 飞书长连接已被其他客户端占用".into();
    }
    if code == 403 || code == 514 {
        return format!("fatal: 飞书长连接被拒绝（{code}）");
    }
    if code == 0 {
        return "飞书长连接失败".into();
    }
    format!("飞书长连接失败（{code}）")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};

    #[tokio::test]
    async fn fragmented_event_is_acked_on_a_real_socket() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<Assembled>();
        let (ack_tx, ack_rx) = mpsc::unbounded_channel();
        let (_stop_tx, stop_rx) = watch::channel(false);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            drive(socket, stop_rx, ev_tx, ack_rx, 1, 120).await
        });
        let (mut client, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        client
            .send(Message::Binary(part(0, br#"{"a":"#).into()))
            .await
            .unwrap();
        client
            .send(Message::Binary(part(1, b"1}").into()))
            .await
            .unwrap();
        let assembled = tokio::time::timeout(Duration::from_secs(2), ev_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(assembled.payload, br#"{"a":1}"#);
        ack_tx.send(frame::ack(&assembled.frame, 200)).unwrap();
        let ack = tokio::time::timeout(Duration::from_secs(2), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Binary(bytes) = ack else {
            panic!("expected binary ack")
        };
        let frame = frame::decode(&bytes).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&frame.payload).unwrap();
        assert_eq!(body["code"], 200);
        drop(client);
        let _ = server.await;
    }

    fn part(seq: u64, payload: &[u8]) -> Vec<u8> {
        frame::Frame {
            seq_id: seq,
            log_id: 1,
            service: 1,
            method: 1,
            headers: vec![
                frame::Header {
                    key: "type".into(),
                    value: "event".into(),
                },
                frame::Header {
                    key: "message_id".into(),
                    value: "m1".into(),
                },
                frame::Header {
                    key: "trace_id".into(),
                    value: "t".into(),
                },
                frame::Header {
                    key: "sum".into(),
                    value: "2".into(),
                },
                frame::Header {
                    key: "seq".into(),
                    value: seq.to_string(),
                },
            ],
            payload: payload.to_vec(),
        }
        .encode()
    }

    #[test]
    fn duplicate_socket_and_forbidden_are_fatal() {
        assert!(handshake_code(514, Some(1000040350)).starts_with("fatal:"));
        assert!(handshake_code(403, None).starts_with("fatal:"));
        assert!(!handshake_code(500, None).starts_with("fatal:"));
    }

    fn pong_frame(interval: u64) -> Vec<u8> {
        frame::Frame {
            seq_id: 0,
            log_id: 0,
            service: 1,
            method: 0,
            headers: vec![frame::Header {
                key: "type".into(),
                value: "pong".into(),
            }],
            payload: format!(r#"{{"PingInterval":{interval}}}"#).into_bytes(),
        }
        .encode()
    }

    #[tokio::test]
    async fn silent_peer_makes_heartbeat_retryable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
        let (_ack_tx, ack_rx) = mpsc::unbounded_channel();
        let (_stop_tx, stop_rx) = watch::channel(false);
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut seen = Some(seen_tx);
            while let Some(item) = socket.next().await {
                if matches!(item, Ok(Message::Binary(_))) {
                    if let Some(tx) = seen.take() {
                        let _ = tx.send(());
                    }
                }
            }
        });
        let (socket, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let drive = tokio::spawn(drive(socket, stop_rx, ev_tx, ack_rx, 1, 1));
        tokio::time::timeout(Duration::from_secs(3), seen_rx)
            .await
            .unwrap()
            .unwrap();
        let error = tokio::time::timeout(Duration::from_secs(4), drive)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.contains("心跳"), "{error}");
        assert!(!error.starts_with("fatal:"));
    }

    #[tokio::test]
    async fn shutdown_during_pong_wait_returns_immediately() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
        let (_ack_tx, ack_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut seen = Some(seen_tx);
            while let Some(item) = socket.next().await {
                if matches!(item, Ok(Message::Binary(_))) {
                    if let Some(tx) = seen.take() {
                        let _ = tx.send(());
                    }
                }
            }
        });
        let (socket, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let drive = tokio::spawn(drive(socket, stop_rx, ev_tx, ack_rx, 1, 1));
        tokio::time::timeout(Duration::from_secs(3), seen_rx)
            .await
            .unwrap()
            .unwrap();
        let started = std::time::Instant::now();
        stop_tx.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), drive)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(started.elapsed() < Duration::from_millis(500), "{:?}", started.elapsed());
    }

    #[tokio::test]
    async fn pong_keeps_the_connection_until_shutdown() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
        let (_ack_tx, ack_rx) = mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let (ponged_tx, ponged_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let mut ponged = Some(ponged_tx);
            while let Some(item) = socket.next().await {
                if matches!(item, Ok(Message::Binary(_))) {
                    socket
                        .send(Message::Binary(pong_frame(30).into()))
                        .await
                        .unwrap();
                    if let Some(tx) = ponged.take() {
                        let _ = tx.send(());
                    }
                }
            }
        });
        let (socket, _) = connect_async(format!("ws://{addr}")).await.unwrap();
        let drive = tokio::spawn(drive(socket, stop_rx, ev_tx, ack_rx, 1, 1));
        tokio::time::timeout(Duration::from_secs(3), ponged_rx)
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(2500)).await;
        stop_tx.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), drive)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}
