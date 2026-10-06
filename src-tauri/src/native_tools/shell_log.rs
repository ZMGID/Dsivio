//! Redact a captured extension credential before bytes reach a background log.
use std::{io::Write, sync::{Arc, Mutex}};
use tokio::io::{AsyncRead, AsyncReadExt};

struct Redactor { secret: Vec<u8>, pending: Vec<u8> }
impl Redactor {
    fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        if self.secret.is_empty() { return std::mem::take(&mut self.pending); }
        let mut output = Vec::new();
        let mut cursor = 0;
        while cursor < self.pending.len() {
            let tail = &self.pending[cursor..];
            if tail.starts_with(&self.secret) {
                output.extend_from_slice(b"[redacted]");
                cursor += self.secret.len();
            } else if !eof && self.secret.starts_with(tail) {
                break;
            } else {
                output.push(self.pending[cursor]);
                cursor += 1;
            }
        }
        self.pending.drain(..cursor);
        output
    }
}

pub(super) async fn capture(
    mut input: impl AsyncRead + Unpin,
    file: Arc<Mutex<std::fs::File>>,
    secret: String,
) -> std::io::Result<()> {
    let mut redactor = Redactor { secret: secret.into_bytes(), pending: Vec::new() };
    let mut bytes = [0; 8192];
    loop {
        let count = input.read(&mut bytes).await?;
        let output = redactor.push(&bytes[..count], count == 0);
        file.lock().unwrap_or_else(|e| e.into_inner()).write_all(&output)?;
        if count == 0 { return Ok(()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_every_split_and_keeps_nonsecret_prefix_at_eof() {
        let text = b"before-token-123-token-123-after-token-12";
        for split in 0..=text.len() {
            let mut r = Redactor { secret: b"token-123".to_vec(), pending: Vec::new() };
            let mut actual = r.push(&text[..split], false);
            actual.extend(r.push(&text[split..], true));
            assert_eq!(actual, b"before-[redacted]-[redacted]-after-token-12");
        }
    }
    #[tokio::test]
    async fn stored_log_contains_only_redacted_bytes() {
        let log = tempfile::NamedTempFile::new().unwrap();
        let file = Arc::new(Mutex::new(log.reopen().unwrap()));
        capture(&b"echo token-123 done"[..], file, "token-123".into()).await.unwrap();
        assert_eq!(std::fs::read(log.path()).unwrap(), b"echo [redacted] done");
    }
}
