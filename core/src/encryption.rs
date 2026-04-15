//! Client-side encryption using XChaCha20-Poly1305.
//! Keys NEVER leave the device. Nonces are unique per encryption operation.

use crate::types::*;
use anyhow::{Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};

/// Encrypt plaintext using XChaCha20-Poly1305 with the given key.
/// Returns (nonce, ciphertext_with_tag).
/// Nonce is generated randomly (96-bit nonce space makes collisions negligible).
pub fn encrypt(key: &DerivedKey, plaintext: &[u8]) -> Result<(Nonce, Vec<u8>)> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .context("Failed to create cipher from key")?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .context("Encryption failed")?;

    Ok((Nonce(nonce_bytes), ciphertext))
}

/// Decrypt ciphertext using XChaCha20-Poly1305 with the given key and nonce.
/// Verifies authentication tag — returns error on tampering.
pub fn decrypt(key: &DerivedKey, nonce: &Nonce, ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .context("Failed to create cipher from key")?;

    let nonce = XNonce::from_slice(nonce.as_bytes());

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .context("Decryption failed (invalid key, nonce, or tampered data)")?;

    Ok(plaintext)
}

/// Derive a key from password using Argon2id.
/// Salt must be stored alongside the ciphertext (non-secret).
pub fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> Result<DerivedKey> {
    let params = argon2::Params::new(65536, 3, 4, Some(DERIVED_KEY_LEN as u32))
        .context("Invalid Argon2id parameters")?;

    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut key = [0u8; DERIVED_KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .context("Key derivation failed")?;

    Ok(DerivedKey(key))
}

/// Generate a random salt for key derivation.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Generate a new random master key.
pub fn generate_master_key() -> MasterKey {
    let mut key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    MasterKey(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        let plaintext = b"Hello, VaultKeeper! This is a secret message.";
        let (nonce, ciphertext) = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        let key2 = DerivedKey([0x43u8; DERIVED_KEY_LEN]);
        let plaintext = b"Secret data";
        let (nonce, ciphertext) = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_tampered_data_fails() {
        let key = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        let plaintext = b"Original data";
        let (nonce, mut ciphertext) = encrypt(&key, plaintext).unwrap();
        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }
        let result = decrypt(&key, &nonce, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_unique_nonces() {
        let key = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..1000 {
            let (nonce, _) = encrypt(&key, b"test").unwrap();
            let hex = hex::encode(nonce.as_bytes());
            assert!(nonces.insert(hex), "Duplicate nonce detected!");
        }
    }

    #[test]
    fn test_derive_key_deterministic() {
        let salt = generate_salt();
        let key1 = derive_key("password123", &salt).unwrap();
        let key2 = derive_key("password123", &salt).unwrap();
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_different_passwords() {
        let salt = generate_salt();
        let key1 = derive_key("password123", &salt).unwrap();
        let key2 = derive_key("password456", &salt).unwrap();
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_large_data_encryption() {
        let key = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        let plaintext = vec![0xABu8; CHUNK_SIZE * 2];
        let (nonce, ciphertext) = encrypt(&key, &plaintext).unwrap();
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(plaintext.len(), decrypted.len());
        assert_eq!(plaintext, decrypted);
    }
}
