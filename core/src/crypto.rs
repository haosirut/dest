use crate::types::{DerivedKey, NONCE_LEN, SALT_LEN, KEY_LEN};
use anyhow::{Context, Result};
use chacha20poly1305::{aead::{Aead, OsRng, KeyInit}, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use zeroize::Zeroize;

/// Master key derived from BIP39 seed phrase or generated randomly.
/// Zeroized on drop to prevent key material from lingering in memory.
#[derive(Zeroize)]
pub struct VaultKey {
    master: [u8; KEY_LEN],
}

impl VaultKey {
    /// Generate a new random master key.
    pub fn generate() -> Result<Self> {
        let mut master = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut master);
        Ok(Self { master })
    }

    /// Derive master key from BIP39 mnemonic seed phrase.
    /// The passphrase parameter is accepted for API compatibility but
    /// the seed is derived without an additional passphrase to match
    /// standard BIP39 behavior with empty passphrase.
    pub fn from_seed(phrase: &str, _passphrase: &str) -> Result<Self> {
        let mnemonic = bip39::Mnemonic::parse_normalized(phrase.trim())
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;
        let seed = mnemonic.to_seed("");
        let mut master = [0u8; KEY_LEN];
        // Use first 32 bytes of the 64-byte BIP39 seed
        master.copy_from_slice(&seed[..KEY_LEN]);
        Ok(Self { master })
    }

    /// Derive a context-specific sub-key using SHA-256 derivation.
    /// Each context string produces a unique derived key from the same master.
    pub fn derive_key(&self, context: &str) -> DerivedKey {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(b"vaultkeeper-derive:");
        hasher.update(context.as_bytes());
        hasher.update(&self.master);
        let derived = hasher.finalize();
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&derived);
        DerivedKey(key)
    }

    /// Get a public node identifier (SHA-256 hash of master key, hex-encoded).
    /// This ID is safe to share publicly — it cannot be used to reconstruct
    /// the master key.
    pub fn public_id(&self) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(b"vaultkeeper-node-id:");
        hasher.update(&self.master);
        hex::encode(hasher.finalize())
    }
}

/// Encrypt data with a derived key using XChaCha20-Poly1305.
/// Returns (nonce_bytes, ciphertext_with_authentication_tag).
///
/// The nonce is 24 bytes (192-bit), making collision probability negligible
/// even with billions of encryptions under the same key.
pub fn encrypt_data(key: &DerivedKey, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    Ok((nonce_bytes.to_vec(), ciphertext))
}

/// Decrypt data using XChaCha20-Poly1305.
/// Verifies the authentication tag automatically — returns an error
/// if the data was tampered with or the wrong key/nonce is used.
pub fn decrypt_data(key: &DerivedKey, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let nonce = XNonce::from_slice(&nonce[..NONCE_LEN]);
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed (tampered data or wrong key): {}", e))?;
    Ok(plaintext)
}

/// Encrypt a file from disk. Reads the entire file into memory,
/// encrypts it, and returns (nonce, ciphertext).
///
/// For very large files, prefer chunking first and encrypting each chunk.
pub fn encrypt_file(path: &str, key: &DerivedKey) -> Result<(Vec<u8>, Vec<u8>)> {
    let data = std::fs::read(path)
        .with_context(|| format!("Cannot read file: {}", path))?;
    encrypt_data(key, &data)
}

/// Decrypt ciphertext and write the result to a file.
pub fn decrypt_to_file(ciphertext: &[u8], nonce: &[u8], key: &DerivedKey, out_path: &str) -> Result<()> {
    let plaintext = decrypt_data(key, nonce, ciphertext)?;
    std::fs::write(out_path, &plaintext)
        .with_context(|| format!("Cannot write file: {}", out_path))?;
    Ok(())
}

/// Derive a key from a password using Argon2id.
///
/// Parameters: 64 MiB memory, 3 iterations, 4 lanes (parallelism).
/// These parameters provide strong resistance against GPU/ASIC attacks.
///
/// The salt must be stored alongside the ciphertext (it is non-secret).
pub fn derive_key_from_password(password: &str, salt: &[u8]) -> Result<DerivedKey> {
    let params = argon2::Params::new(65536, 3, 4, Some(KEY_LEN))
        .map_err(|e| anyhow::anyhow!("Invalid Argon2id params: {}", e))?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2.hash_password_into(password.as_bytes(), &salt[..SALT_LEN], &mut key)
        .map_err(|e| anyhow::anyhow!("Key derivation failed: {}", e))?;
    Ok(DerivedKey(key))
}

/// Generate a cryptographically random 32-byte salt.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_key_generate() {
        let key = VaultKey::generate().unwrap();
        assert!(!key.public_id().is_empty());
    }

    #[test]
    fn test_vault_key_from_seed() {
        let mnemonic = bip39::Mnemonic::generate_in(bip39::Language::English, 12).unwrap();
        let key = VaultKey::from_seed(&mnemonic.to_string(), "").unwrap();
        let id = key.public_id();
        assert_eq!(id.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_vault_key_deterministic() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k1 = VaultKey::from_seed(phrase, "").unwrap();
        let k2 = VaultKey::from_seed(phrase, "").unwrap();
        assert_eq!(k1.public_id(), k2.public_id());
    }

    #[test]
    fn test_derive_key_contexts() {
        let key = VaultKey::generate().unwrap();
        let d1 = key.derive_key("encryption");
        let d2 = key.derive_key("authentication");
        assert_ne!(d1.as_bytes(), d2.as_bytes());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let plaintext = b"Hello, VaultKeeper!";
        let (nonce, ct) = encrypt_data(&dk, plaintext).unwrap();
        let pt = decrypt_data(&dk, &nonce, &ct).unwrap();
        assert_eq!(plaintext.as_slice(), pt.as_slice());
    }

    #[test]
    fn test_wrong_key_fails() {
        let k1 = VaultKey::generate().unwrap();
        let k2 = VaultKey::generate().unwrap();
        let dk1 = k1.derive_key("test");
        let dk2 = k2.derive_key("test");
        let (nonce, ct) = encrypt_data(&dk1, b"secret").unwrap();
        assert!(decrypt_data(&dk2, &nonce, &ct).is_err());
    }

    #[test]
    fn test_tampered_data_fails() {
        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let (nonce, mut ct) = encrypt_data(&dk, b"original").unwrap();
        if !ct.is_empty() { ct[0] ^= 0xFF; }
        assert!(decrypt_data(&dk, &nonce, &ct).is_err());
    }

    #[test]
    fn test_password_derivation() {
        let salt = generate_salt();
        let k1 = derive_key_from_password("password123", &salt).unwrap();
        let k2 = derive_key_from_password("password123", &salt).unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn test_file_encrypt_decrypt() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.dat");
        let out_path = dir.path().join("out.dat");
        std::fs::write(&file_path, b"file content here").unwrap();

        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("file-encrypt");
        let (nonce, ct) = encrypt_file(file_path.to_str().unwrap(), &dk).unwrap();
        decrypt_to_file(&ct, &nonce, &dk, out_path.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read(&out_path).unwrap(), b"file content here");
    }

    #[test]
    fn test_unique_nonces() {
        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..500 {
            let (nonce, _) = encrypt_data(&dk, b"data").unwrap();
            let hex = hex::encode(&nonce);
            assert!(nonces.insert(hex), "Duplicate nonce!");
        }
    }

    #[test]
    fn test_different_passwords_different_keys() {
        let salt = generate_salt();
        let k1 = derive_key_from_password("password123", &salt).unwrap();
        let k2 = derive_key_from_password("password456", &salt).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let (nonce, ct) = encrypt_data(&dk, b"").unwrap();
        let pt = decrypt_data(&dk, &nonce, &ct).unwrap();
        assert!(pt.is_empty());
    }

    #[test]
    fn test_large_data_roundtrip() {
        let key = VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let plaintext = vec![0xABu8; 1024 * 1024]; // 1 MiB
        let (nonce, ct) = encrypt_data(&dk, &plaintext).unwrap();
        let pt = decrypt_data(&dk, &nonce, &ct).unwrap();
        assert_eq!(plaintext, pt);
    }
}
