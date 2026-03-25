use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use anyhow::Context;

/// Encrypt `plaintext` with AES-256-GCM. Returns base64(nonce || ciphertext).
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid key length")?;
    let nonce_bytes: [u8; 12] = {
        use rand::RngCore;
        let mut n = [0u8; 12];
        OsRng.fill_bytes(&mut n);
        n
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(&combined))
}

/// Decrypt a value produced by `encrypt`.
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let combined = B64.decode(encoded).context("base64 decode failed")?;
    anyhow::ensure!(combined.len() > 12, "ciphertext too short");
    let (nonce_bytes, ct) = combined.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).context("invalid key length")?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))?;
    String::from_utf8(plaintext).context("plaintext is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] { [0u8; 32] }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "ghp_supersecrettoken123";
        let ciphertext = encrypt(plaintext, &key).unwrap();
        assert_ne!(ciphertext, plaintext);
        let recovered = decrypt(&ciphertext, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn different_encryptions_of_same_plaintext_differ() {
        let key = test_key();
        let a = encrypt("token", &key).unwrap();
        let b = encrypt("token", &key).unwrap();
        assert_ne!(a, b);
        assert_eq!(decrypt(&a, &key).unwrap(), "token");
        assert_eq!(decrypt(&b, &key).unwrap(), "token");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = test_key();
        let ct = encrypt("secret", &key).unwrap();
        let wrong_key = [1u8; 32];
        assert!(decrypt(&ct, &wrong_key).is_err());
    }
}
