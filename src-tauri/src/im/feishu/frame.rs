//! pbbp2.Frame codec. Ported from lark-oapi `ws/pb/pbbp2_pb2.py` (Lark Technologies, MIT).
//! Proto2 required zeros are encoded on purpose; a proto3-style encoder would omit them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
const MAX_PARTS: usize = 32;
const PART_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub seq_id: u64,
    pub log_id: u64,
    pub service: i32,
    pub method: i32,
    pub headers: Vec<Header>,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.as_str())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint_field(&mut out, 1, self.seq_id);
        put_varint_field(&mut out, 2, self.log_id);
        put_varint_field(&mut out, 3, self.service as u64);
        put_varint_field(&mut out, 4, self.method as u64);
        for header in &self.headers {
            let mut msg = Vec::new();
            put_string(&mut msg, 1, &header.key);
            put_string(&mut msg, 2, &header.value);
            put_bytes(&mut out, 5, &msg);
        }
        if !self.payload.is_empty() {
            put_bytes(&mut out, 8, &self.payload);
        }
        out
    }
}

pub fn decode(mut data: &[u8]) -> Result<Frame, ()> {
    if data.len() > MAX_FRAME_BYTES {
        return Err(());
    }
    let mut frame = Frame {
        seq_id: 0,
        log_id: 0,
        service: 0,
        method: 0,
        headers: Vec::new(),
        payload: Vec::new(),
    };
    while !data.is_empty() {
        let key = take_varint(&mut data)?;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u32;
        match (field, wire) {
            (1, 0) => frame.seq_id = take_varint(&mut data)?,
            (2, 0) => frame.log_id = take_varint(&mut data)?,
            (3, 0) => frame.service = take_varint(&mut data)? as i32,
            (4, 0) => frame.method = take_varint(&mut data)? as i32,
            (5, 2) => frame.headers.push(decode_header(&take_len(&mut data)?)?),
            (8, 2) => frame.payload = take_len(&mut data)?.to_vec(),
            (_, 0) => {
                take_varint(&mut data)?;
            }
            (_, 1) => skip(&mut data, 8)?,
            (_, 2) => {
                let n = take_varint(&mut data)? as usize;
                skip(&mut data, n)?;
            }
            (_, 5) => skip(&mut data, 4)?,
            _ => return Err(()),
        }
    }
    Ok(frame)
}

fn decode_header(mut data: &[u8]) -> Result<Header, ()> {
    let mut key = String::new();
    let mut value = String::new();
    while !data.is_empty() {
        let tag = take_varint(&mut data)?;
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        match (field, wire) {
            (1, 2) => key = String::from_utf8(take_len(&mut data)?.to_vec()).map_err(|_| ())?,
            (2, 2) => value = String::from_utf8(take_len(&mut data)?.to_vec()).map_err(|_| ())?,
            (_, 0) => {
                take_varint(&mut data)?;
            }
            (_, 2) => {
                let n = take_varint(&mut data)? as usize;
                skip(&mut data, n)?;
            }
            (_, 1) => skip(&mut data, 8)?,
            (_, 5) => skip(&mut data, 4)?,
            _ => return Err(()),
        }
    }
    if key.is_empty() {
        return Err(());
    }
    Ok(Header { key, value })
}

fn take_varint(data: &mut &[u8]) -> Result<u64, ()> {
    let mut out = 0u64;
    let mut shift = 0;
    while shift < 64 {
        let byte = *data.first().ok_or(())?;
        *data = &data[1..];
        out |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(out);
        }
        shift += 7;
    }
    Err(())
}

fn take_len<'a>(data: &mut &'a [u8]) -> Result<&'a [u8], ()> {
    let n = take_varint(data)? as usize;
    if n > MAX_FRAME_BYTES || data.len() < n {
        return Err(());
    }
    let (head, tail) = data.split_at(n);
    *data = tail;
    Ok(head)
}

fn skip(data: &mut &[u8], n: usize) -> Result<(), ()> {
    if data.len() < n {
        return Err(());
    }
    *data = &data[n..];
    Ok(())
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn put_varint_field(out: &mut Vec<u8>, field: u32, value: u64) {
    put_varint(out, u64::from(field << 3));
    put_varint(out, value);
}

fn put_bytes(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    put_varint(out, u64::from((field << 3) | 2));
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn put_string(out: &mut Vec<u8>, field: u32, value: &str) {
    put_bytes(out, field, value.as_bytes());
}

pub fn ping(service: i32) -> Vec<u8> {
    Frame {
        seq_id: 0,
        log_id: 0,
        service,
        method: 0,
        headers: vec![Header {
            key: "type".into(),
            value: "ping".into(),
        }],
        payload: Vec::new(),
    }
    .encode()
}

pub fn ack(frame: &Frame, code: u16) -> Vec<u8> {
    let mut headers = frame.headers.clone();
    headers.push(Header {
        key: "biz_rt".into(),
        value: "0".into(),
    });
    Frame {
        seq_id: frame.seq_id,
        log_id: frame.log_id,
        service: frame.service,
        method: 1,
        headers,
        payload: format!(r#"{{"code":{code},"headers":null,"data":null}}"#).into_bytes(),
    }
    .encode()
}

struct Partial {
    sum: usize,
    pieces: Vec<Option<Vec<u8>>>,
    updated: Instant,
    bytes: usize,
}

#[derive(Default)]
pub struct Assembler {
    pending: HashMap<String, Partial>,
}

impl Assembler {
    pub fn push(&mut self, frame: &Frame, now: Instant) -> Result<Option<Vec<u8>>, ()> {
        self.pending
            .retain(|_, part| now.duration_since(part.updated) < PART_TTL);
        let kind = frame.header("type").unwrap_or("");
        if kind != "event" && kind != "card" {
            return Err(());
        }
        let sum: usize = frame.header("sum").unwrap_or("1").parse().map_err(|_| ())?;
        let seq: usize = frame.header("seq").unwrap_or("0").parse().map_err(|_| ())?;
        if sum == 0 || sum > MAX_PARTS || seq >= sum || frame.payload.len() > MAX_FRAME_BYTES {
            return Err(());
        }
        if sum == 1 {
            return Ok(Some(frame.payload.clone()));
        }
        let id = frame.header("message_id").unwrap_or("").to_owned();
        if id.is_empty() {
            return Err(());
        }
        let part = self.pending.entry(id.clone()).or_insert_with(|| Partial {
            sum,
            pieces: vec![None; sum],
            updated: now,
            bytes: 0,
        });
        if part.sum != sum {
            return Err(());
        }
        if part.pieces[seq].is_none() {
            part.bytes += frame.payload.len();
        }
        if part.bytes > MAX_FRAME_BYTES {
            self.pending.remove(&id);
            return Err(());
        }
        part.pieces[seq] = Some(frame.payload.clone());
        part.updated = now;
        if part.pieces.iter().any(|p| p.is_none()) {
            return Ok(None);
        }
        let mut payload = Vec::new();
        for piece in part.pieces.iter().flatten() {
            payload.extend_from_slice(piece);
        }
        self.pending.remove(&id);
        Ok(Some(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_keeps_proto2_zero_fields() {
        let bytes = ping(1);
        assert_eq!(bytes, hex("08001000180120002a0c0a0474797065120470696e67"));
        let frame = decode(&bytes).unwrap();
        assert_eq!(frame.service, 1);
        assert_eq!(frame.method, 0);
        assert_eq!(frame.header("type"), Some("ping"));
    }

    #[test]
    fn fragments_join_in_seq_order_and_ack_carries_code() {
        let mut asm = Assembler::default();
        let now = Instant::now();
        let first = data_frame(0, 2, b"AB");
        assert!(asm.push(&first, now).unwrap().is_none());
        let second = data_frame(1, 2, b"CD");
        let joined = asm.push(&second, now).unwrap().unwrap();
        assert_eq!(joined, b"ABCD");
        let ack_frame = decode(&ack(&second, 200)).unwrap();
        assert_eq!(ack_frame.method, 1);
        let body: serde_json::Value = serde_json::from_slice(&ack_frame.payload).unwrap();
        assert_eq!(body["code"], 200);
    }

    #[test]
    fn incomplete_fragment_does_not_ack_and_bad_bytes_fail() {
        let mut asm = Assembler::default();
        assert!(asm
            .push(&data_frame(0, 2, b"x"), Instant::now())
            .unwrap()
            .is_none());
        assert!(decode(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]).is_err());
        let mut bad = ping(1);
        bad.extend_from_slice(&[0xff, 0xff]);
        assert!(decode(&bad).is_err());
    }

    fn data_frame(seq: u64, sum: usize, payload: &[u8]) -> Frame {
        Frame {
            seq_id: seq,
            log_id: 1,
            service: 1,
            method: 1,
            headers: vec![
                Header {
                    key: "type".into(),
                    value: "event".into(),
                },
                Header {
                    key: "message_id".into(),
                    value: "m1".into(),
                },
                Header {
                    key: "trace_id".into(),
                    value: "t1".into(),
                },
                Header {
                    key: "sum".into(),
                    value: sum.to_string(),
                },
                Header {
                    key: "seq".into(),
                    value: seq.to_string(),
                },
            ],
            payload: payload.to_vec(),
        }
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
