//! Backup encryption: age (https://age-encryption.org) with a passphrase (scrypt + ChaCha20-Poly1305,
//! streamed in 64 KiB chunks). Files can also be decrypted with the standard `age -d` tool.

use crate::error::{AppError, AppResult};
use age::secrecy::SecretString;
use std::io::{Read, Write};
use std::path::Path;

fn err(e: impl std::fmt::Display) -> AppError {
    AppError::new(proto::ErrorCode::BackupFailed, format!("encryption: {e}"))
}

/// Encrypts `src` into `dst`. `work_factor` is scrypt's log2(N) (default chosen by age ≈ 1 s when `None`).
pub fn encrypt_file(src: &Path, dst: &Path, passphrase: &str, work_factor: Option<u8>) -> AppResult<()> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    if let Some(w) = work_factor {
        recipient.set_work_factor(w);
    }
    let enc = age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient)).map_err(err)?;
    let mut input = std::fs::File::open(src).map_err(err)?;
    let out = std::fs::File::create(dst).map_err(err)?;
    let mut w = enc.wrap_output(std::io::BufWriter::new(out)).map_err(err)?;
    std::io::copy(&mut input, &mut w).map_err(err)?;
    let mut inner = w.finish().map_err(err)?;
    inner.flush().map_err(err)?;
    Ok(())
}

/// Decrypts `src` into `dst`. A wrong passphrase or a damaged file is reported as an error.
pub fn decrypt_file(src: &Path, dst: &Path, passphrase: &str) -> AppResult<()> {
    let f = std::fs::File::open(src).map_err(err)?;
    let dec = age::Decryptor::new(std::io::BufReader::new(f)).map_err(|e| AppError::validation(format!("this is not a valid encrypted backup: {e}")))?;
    let ident = age::scrypt::Identity::new(SecretString::from(passphrase.to_string()));
    let mut r = dec
        .decrypt(std::iter::once(&ident as &dyn age::Identity))
        .map_err(|_| AppError::validation("wrong passphrase or damaged backup file"))?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(dst).map_err(err)?);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf).map_err(|_| AppError::validation("wrong passphrase or damaged backup file"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(err)?;
    }
    out.flush().map_err(err)?;
    Ok(())
}

pub fn encrypt_bytes(data: &[u8], passphrase: &str, work_factor: Option<u8>) -> AppResult<Vec<u8>> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    if let Some(w) = work_factor {
        recipient.set_work_factor(w);
    }
    let enc = age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient)).map_err(err)?;
    let mut out = vec![];
    let mut w = enc.wrap_output(&mut out).map_err(err)?;
    w.write_all(data).map_err(err)?;
    w.finish().map_err(err)?;
    Ok(out)
}

pub fn decrypt_bytes(data: &[u8], passphrase: &str) -> AppResult<Vec<u8>> {
    let dec = age::Decryptor::new(data).map_err(|e| AppError::validation(format!("this is not a valid encrypted backup: {e}")))?;
    let ident = age::scrypt::Identity::new(SecretString::from(passphrase.to_string()));
    let mut r = dec
        .decrypt(std::iter::once(&ident as &dyn age::Identity))
        .map_err(|_| AppError::validation("wrong passphrase or damaged backup file"))?;
    let mut out = vec![];
    r.read_to_end(&mut out).map_err(|_| AppError::validation("wrong passphrase or damaged backup file"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_passphrase() {
        let data = b"secret chats".repeat(10_000);
        let enc = encrypt_bytes(&data, "correct horse", Some(10)).unwrap();
        assert!(!enc.windows(11).any(|w| w == b"secret chat"), "ciphertext must not contain plaintext");
        assert_eq!(decrypt_bytes(&enc, "correct horse").unwrap(), data);
        assert!(decrypt_bytes(&enc, "wrong").is_err());
        let mut damaged = enc.clone();
        let n = damaged.len();
        damaged[n / 2] ^= 0xff;
        assert!(decrypt_bytes(&damaged, "correct horse").is_err());
    }

    #[test]
    fn files() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (d.path().join("a"), d.path().join("b"), d.path().join("c"));
        std::fs::write(&a, vec![7u8; 300_000]).unwrap();
        encrypt_file(&a, &b, "pw-pw-pw-pw", Some(10)).unwrap();
        decrypt_file(&b, &c, "pw-pw-pw-pw").unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&c).unwrap());
        assert!(decrypt_file(&b, &c, "nope").is_err());
    }
}
