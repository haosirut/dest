//! BIP39 mnemonic seed recovery for wallet/key backup.
//! Generates deterministic master key from 12/24 word mnemonic.

use crate::types::{DerivedKey, DERIVED_KEY_LEN, MnemonicWordCount};
use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use rand::rngs::OsRng;

/// Generate a new mnemonic phrase with the specified word count.
pub fn generate_mnemonic(word_count: MnemonicWordCount) -> Result<Mnemonic> {
    let mnemonic = match word_count {
        MnemonicWordCount::Twelve => Mnemonic::generate_in_with(&mut OsRng, Language::English, 128),
        MnemonicWordCount::TwentyFour => Mnemonic::generate_in_with(&mut OsRng, Language::English, 256),
    }
    .context("Failed to generate mnemonic")?;
    Ok(mnemonic)
}

/// Recover a master key from an existing mnemonic phrase.
/// The mnemonic is the ONLY backup mechanism — no server-side recovery.
pub fn recover_key_from_mnemonic(mnemonic: &Mnemonic, passphrase: &str) -> Result<DerivedKey> {
    let seed = mnemonic.to_seed(passphrase);
    let mut key = [0u8; DERIVED_KEY_LEN];
    key.copy_from_slice(&seed[..DERIVED_KEY_LEN]);
    Ok(DerivedKey(key))
}

/// Parse a mnemonic from a space-separated word string.
pub fn parse_mnemonic(phrase: &str) -> Result<Mnemonic> {
    let mnemonic = Mnemonic::parse_normalized(phrase.trim())
        .context("Invalid mnemonic phrase")?;
    Ok(mnemonic)
}

/// Validate a mnemonic phrase without generating a key.
pub fn validate_mnemonic(phrase: &str) -> bool {
    Mnemonic::parse_normalized(phrase.trim()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_12_word_mnemonic() {
        let mnemonic = generate_mnemonic(MnemonicWordCount::Twelve).unwrap();
        assert_eq!(mnemonic.word_count(), 12);
    }

    #[test]
    fn test_generate_24_word_mnemonic() {
        let mnemonic = generate_mnemonic(MnemonicWordCount::TwentyFour).unwrap();
        assert_eq!(mnemonic.word_count(), 24);
    }

    #[test]
    fn test_roundtrip_recovery() {
        let mnemonic = generate_mnemonic(MnemonicWordCount::Twelve).unwrap();
        let passphrase = "vaultkeeper-passphrase";
        let key1 = recover_key_from_mnemonic(&mnemonic, passphrase).unwrap();
        let key2 = recover_key_from_mnemonic(&mnemonic, passphrase).unwrap();
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_different_passphrase_different_key() {
        let mnemonic = generate_mnemonic(MnemonicWordCount::Twelve).unwrap();
        let key1 = recover_key_from_mnemonic(&mnemonic, "passphrase1").unwrap();
        let key2 = recover_key_from_mnemonic(&mnemonic, "passphrase2").unwrap();
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_parse_valid_mnemonic() {
        let mnemonic = generate_mnemonic(MnemonicWordCount::Twelve).unwrap();
        let phrase = mnemonic.to_string();
        assert!(parse_mnemonic(&phrase).is_ok());
    }

    #[test]
    fn test_parse_invalid_mnemonic() {
        assert!(!validate_mnemonic("invalid words here"));
    }

    #[test]
    fn test_deterministic_from_phrase() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        // This is a well-known test vector
        let result = validate_mnemonic(phrase);
        assert!(result);
    }
}
