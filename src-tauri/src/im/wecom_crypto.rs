// Portions derived from Hermes WeCom crypto (MIT License, Copyright (c) 2025 Nous Research).
// AES-CBC with 32-byte PKCS#7, SHA1 signature, and receive_id check. Wire-compatible with WXBizMsgCrypt.

use aes::Aes256;
use base64::Engine;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rand::RngCore;
use sha1::{Digest, Sha1};

const BLOCK: usize = 32;
// Match WXBizMsgCrypt/Hermes: the 43-character key omits padding and Python's
// decoder accepts unused trailing bits. Ciphertext still uses strict decoding.
const KEY_BASE64: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new()
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CryptError {
    Signature,
    ReceiveId,
    Payload,
}

pub(crate) struct WxCrypt {
    token: String,
    receive_id: String,
    key: [u8; 32],
    iv: [u8; 16],
}

impl WxCrypt {
    pub(crate) fn new(
        token: &str,
        encoding_aes_key: &str,
        receive_id: &str,
    ) -> Result<Self, CryptError> {
        if token.is_empty() || encoding_aes_key.len() != 43 || receive_id.is_empty() {
            return Err(CryptError::Payload);
        }
        let raw = KEY_BASE64
            .decode(encoding_aes_key)
            .map_err(|_| CryptError::Payload)?;
        if raw.len() != 32 {
            return Err(CryptError::Payload);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&raw[..16]);
        Ok(Self {
            token: token.to_owned(),
            receive_id: receive_id.to_owned(),
            key,
            iv,
        })
    }

    pub(crate) fn decrypt(
        &self,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
        encrypt: &str,
    ) -> Result<Vec<u8>, CryptError> {
        let expected = signature(&self.token, timestamp, nonce, encrypt);
        if !constant_eq(expected.as_bytes(), msg_signature.as_bytes()) {
            return Err(CryptError::Signature);
        }
        let cipher_text = base64::engine::general_purpose::STANDARD
            .decode(encrypt.trim())
            .map_err(|_| CryptError::Payload)?;
        if cipher_text.is_empty() || cipher_text.len() % 16 != 0 {
            return Err(CryptError::Payload);
        }
        let plain = cbc::Decryptor::<Aes256>::new(&self.key.into(), &self.iv.into())
            .decrypt_padded_vec_mut::<NoPadding>(&cipher_text)
            .map_err(|_| CryptError::Payload)?;
        let unpadded = pkcs7_unpad(&plain)?;
        if unpadded.len() < 20 {
            return Err(CryptError::Payload);
        }
        let content = &unpadded[16..];
        let xml_len =
            u32::from_be_bytes(content[..4].try_into().map_err(|_| CryptError::Payload)?) as usize;
        let rest = &content[4..];
        if xml_len > rest.len() {
            return Err(CryptError::Payload);
        }
        let (xml, rid) = rest.split_at(xml_len);
        let rid = std::str::from_utf8(rid).map_err(|_| CryptError::Payload)?;
        if rid != self.receive_id {
            return Err(CryptError::ReceiveId);
        }
        Ok(xml.to_vec())
    }

    pub(crate) fn encrypt_b64(&self, plaintext: &[u8]) -> Result<String, CryptError> {
        let mut random = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let mut payload = Vec::with_capacity(20 + plaintext.len() + self.receive_id.len());
        payload.extend_from_slice(&random);
        payload.extend_from_slice(&(plaintext.len() as u32).to_be_bytes());
        payload.extend_from_slice(plaintext);
        payload.extend_from_slice(self.receive_id.as_bytes());
        let padded = pkcs7_pad(&payload);
        let encrypted = cbc::Encryptor::<Aes256>::new(&self.key.into(), &self.iv.into())
            .encrypt_padded_vec_mut::<NoPadding>(&padded);
        Ok(base64::engine::general_purpose::STANDARD.encode(encrypted))
    }
}

pub(crate) fn signature(token: &str, timestamp: &str, nonce: &str, encrypt: &str) -> String {
    let mut parts = [token, timestamp, nonce, encrypt];
    parts.sort_unstable();
    hex(&Sha1::digest(parts.concat().as_bytes()))
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let amount = match data.len() % BLOCK {
        0 => BLOCK,
        rest => BLOCK - rest,
    };
    let mut out = data.to_vec();
    out.extend(std::iter::repeat(amount as u8).take(amount));
    out
}

fn pkcs7_unpad(data: &[u8]) -> Result<Vec<u8>, CryptError> {
    let pad = *data.last().ok_or(CryptError::Payload)? as usize;
    if pad == 0
        || pad > BLOCK
        || pad > data.len()
        || data[data.len() - pad..].iter().any(|b| *b as usize != pad)
    {
        return Err(CryptError::Payload);
    }
    Ok(data[..data.len() - pad].to_vec())
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "token-demo";
    const AES: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
    const RECEIVE: &str = "wwcorpid123";
    const TS: &str = "1700000000";
    const NONCE: &str = "nonce12345";
    const B64: &str = "IG+0eOkLdhYW9kHtuT3HYylxtRPO82KhEOvngKt97EgXEL3yUopnokb1N/rvwOU834Ll1wUDj2wTUHqSsAWVd49x05yFkurAx3/rexCEjAxWrvClaFXVIS/fYVW0M2gH";
    const SIG: &str = "4d52a9e6ade9e7a45fe80128e4aad970b49b247a";

    #[test]
    fn fixture_signature_and_receive_id() {
        let crypt = WxCrypt::new(TOKEN, AES, RECEIVE).unwrap();
        let xml = crypt.decrypt(SIG, TS, NONCE, B64).unwrap();
        assert_eq!(xml, "<xml><Content>你好</Content></xml>".as_bytes());

        let mut bad_sig = SIG.to_owned();
        bad_sig.replace_range(0..1, "0");
        assert_eq!(
            crypt.decrypt(&bad_sig, TS, NONCE, B64),
            Err(CryptError::Signature)
        );

        let other = WxCrypt::new(TOKEN, AES, "ww-other").unwrap();
        assert_eq!(
            other.decrypt(SIG, TS, NONCE, B64),
            Err(CryptError::ReceiveId)
        );
    }

    #[test]
    fn roundtrip_keeps_unicode_and_receive_id() {
        let crypt = WxCrypt::new(TOKEN, AES, RECEIVE).unwrap();
        let plain = "你好 🙂".as_bytes();
        let b64 = crypt.encrypt_b64(plain).unwrap();
        let sig = signature(TOKEN, TS, NONCE, &b64);
        assert_eq!(crypt.decrypt(&sig, TS, NONCE, &b64).unwrap(), plain);
        let other = WxCrypt::new(TOKEN, AES, "different-corp").unwrap();
        assert_eq!(
            other.decrypt(&sig, TS, NONCE, &b64),
            Err(CryptError::ReceiveId)
        );
    }
}
