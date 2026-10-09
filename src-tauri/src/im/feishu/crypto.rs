//! Webhook decrypt and signature. AES-256-CBC key = SHA256(encrypt_key), IV prefixed.
//! Ported from lark-oapi `core/utils/decryptor.py` and `event/dispatcher_handler.py` (MIT).

#[cfg(test)]
use aes::cipher::BlockEncryptMut;
use aes::cipher::{BlockDecryptMut, KeyIvInit};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

const BLOCK: usize = 16;
const SKEW_SECS: i64 = 5 * 60;
const REPLAY_TTL: i64 = 10 * 60;
const MAX_KEYS: usize = 4096;
const RATE_WINDOW: i64 = 60;
const RATE_MAX: u32 = 120;

pub fn decrypt(encrypt_key: &str, encoded: &str) -> Result<Vec<u8>, ()> {
    let raw = STANDARD.decode(encoded.trim()).map_err(|_| ())?;
    if raw.len() < BLOCK * 2 || raw.len() % BLOCK != 0 {
        return Err(());
    }
    let (iv, ct) = raw.split_at(BLOCK);
    let key = Sha256::digest(encrypt_key.as_bytes());
    let mut dec = cbc::Decryptor::<aes::Aes256>::new_from_slices(&key, iv).map_err(|_| ())?;
    let mut buf = ct.to_vec();
    for chunk in buf.chunks_mut(BLOCK) {
        let mut block = aes::cipher::Block::<aes::Aes256>::default();
        block.copy_from_slice(chunk);
        dec.decrypt_block_mut(&mut block);
        chunk.copy_from_slice(&block);
    }
    unpad(&buf).map(|n| buf[..n].to_vec())
}

fn unpad(buf: &[u8]) -> Result<usize, ()> {
    let pad = *buf.last().ok_or(())? as usize;
    if pad == 0 || pad > BLOCK || pad > buf.len() {
        return Err(());
    }
    if buf[buf.len() - pad..].iter().any(|b| *b as usize != pad) {
        return Err(());
    }
    Ok(buf.len() - pad)
}

pub fn signature(timestamp: &str, nonce: &str, encrypt_key: &str, body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(timestamp.as_bytes());
    hasher.update(nonce.as_bytes());
    hasher.update(encrypt_key.as_bytes());
    hasher.update(body);
    format!("{:x}", hasher.finalize())
}

pub fn signatures_match(expected: &str, provided: &str) -> bool {
    let provided = provided.trim().to_ascii_lowercase();
    if expected.len() != provided.len() || expected.len() != 64 {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[derive(Default)]
pub struct WebGuard {
    nonces: VecDeque<(String, i64)>,
    events: VecDeque<(String, i64)>,
    rates: VecDeque<(String, i64, u32)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GuardReject {
    Rate,
    Stale,
    Replay,
    Duplicate,
}

impl WebGuard {
    pub fn allow_rate(&mut self, peer: &str, now: i64) -> Result<(), GuardReject> {
        self.rates.retain(|(_, start, _)| now - start < RATE_WINDOW);
        if let Some(entry) = self.rates.iter_mut().find(|(key, _, _)| key == peer) {
            if entry.2 >= RATE_MAX {
                return Err(GuardReject::Rate);
            }
            entry.2 += 1;
            return Ok(());
        }
        if self.rates.len() >= MAX_KEYS {
            return Err(GuardReject::Rate);
        }
        self.rates.push_back((peer.to_owned(), now, 1));
        Ok(())
    }

    pub fn allow_nonce(
        &mut self,
        timestamp: &str,
        nonce: &str,
        now: i64,
    ) -> Result<(), GuardReject> {
        let ts: i64 = timestamp.parse().map_err(|_| GuardReject::Stale)?;
        if now.abs_diff(ts) > SKEW_SECS as u64 || nonce.is_empty() || nonce.len() > 128 {
            return Err(GuardReject::Stale);
        }
        self.nonces.retain(|(_, expiry)| *expiry > now);
        if self.nonces.iter().any(|(seen, _)| seen == nonce) {
            return Err(GuardReject::Replay);
        }
        if self.nonces.len() >= MAX_KEYS {
            return Err(GuardReject::Rate);
        }
        self.nonces.push_back((nonce.to_owned(), now + REPLAY_TTL));
        Ok(())
    }

    pub fn allow_event(&mut self, event_id: &str, now: i64) -> Result<(), GuardReject> {
        if event_id.is_empty() {
            return Ok(());
        }
        self.events.retain(|(_, expiry)| *expiry > now);
        if self.events.iter().any(|(seen, _)| seen == event_id) {
            return Err(GuardReject::Duplicate);
        }
        if self.events.len() >= MAX_KEYS {
            self.events.pop_front();
        }
        self.events
            .push_back((event_id.to_owned(), now + REPLAY_TTL));
        Ok(())
    }

    pub fn forget_event(&mut self, event_id: &str) {
        if event_id.is_empty() {
            return;
        }
        self.events.retain(|(seen, _)| seen != event_id);
    }
}

#[cfg(test)]
pub fn encrypt(encrypt_key: &str, plain: &[u8]) -> String {
    let key = Sha256::digest(encrypt_key.as_bytes());
    let iv = [7u8; BLOCK];
    let mut buf = plain.to_vec();
    let pad = BLOCK - (buf.len() % BLOCK);
    buf.extend(std::iter::repeat(pad as u8).take(pad));
    let mut enc = cbc::Encryptor::<aes::Aes256>::new_from_slices(&key, &iv).unwrap();
    for chunk in buf.chunks_mut(BLOCK) {
        let mut block = aes::cipher::Block::<aes::Aes256>::default();
        block.copy_from_slice(chunk);
        enc.encrypt_block_mut(&mut block);
        chunk.copy_from_slice(&block);
    }
    let mut raw = iv.to_vec();
    raw.extend_from_slice(&buf);
    STANDARD.encode(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypt_roundtrip_and_rejects_bad_padding() {
        let plain = "{\"challenge\":\"ok\"}".as_bytes();
        let encoded = encrypt("enc-key", plain);
        assert_eq!(decrypt("enc-key", &encoded).unwrap(), plain);
        assert!(decrypt("other", &encoded).is_err());
        assert!(decrypt("enc-key", "aaaa").is_err());
    }

    #[test]
    fn signature_is_timestamp_nonce_key_body() {
        let body = br#"{"type":"im"}"#;
        let got = signature("100", "nonce", "key", body);
        let mut hasher = Sha256::new();
        hasher.update(b"100noncekey");
        hasher.update(body);
        assert_eq!(got, format!("{:x}", hasher.finalize()));
        assert!(signatures_match(&got, &got.to_ascii_uppercase()));
        let mut flipped = got.clone();
        flipped.replace_range(0..1, if got.starts_with('a') { "b" } else { "a" });
        assert!(!signatures_match(&got, &flipped));
        assert!(!signatures_match(&got, "deadbeef"));
    }

    #[test]
    fn replay_guard_rejects_stale_and_reused_nonce() {
        let mut guard = WebGuard::default();
        assert_eq!(
            guard.allow_nonce("10", "n1", 1000).err(),
            Some(GuardReject::Stale)
        );
        assert_eq!(
            guard
                .allow_nonce(&i64::MIN.to_string(), "extreme", 1000)
                .err(),
            Some(GuardReject::Stale)
        );
        assert!(guard.allow_nonce("1000", "n1", 1000).is_ok());
        assert_eq!(
            guard.allow_nonce("1000", "n1", 1001).err(),
            Some(GuardReject::Replay)
        );
        assert!(guard.allow_event("e1", 1000).is_ok());
        assert_eq!(
            guard.allow_event("e1", 1001).err(),
            Some(GuardReject::Duplicate)
        );
    }
}
