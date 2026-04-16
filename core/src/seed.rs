use anyhow::Result;
use bip39::Language;

/// BIP39 mnemonic seed wrapper for key generation and recovery.
///
/// A BIP39 mnemonic (12 or 24 words) is the sole backup mechanism for
/// VaultKeeper keys. No server-side recovery is possible.
pub struct BIP39Seed {
    /// The mnemonic phrase as a space-separated string of English words.
    pub phrase: String,
    /// Number of words in the phrase (12 or 24).
    pub word_count: usize,
}

impl BIP39Seed {
    /// Generate a new random mnemonic with the specified word count.
    ///
    /// # Arguments
    /// * `words` - Must be 12 or 24
    ///
    /// # Errors
    /// Returns an error if the word count is not 12 or 24.
    pub fn generate(words: usize) -> Result<Self> {
        if words != 12 && words != 24 {
            anyhow::bail!("Word count must be 12 or 24, got {}", words);
        }
        let mnemonic = bip39::Mnemonic::generate_in(Language::English, words)
            .map_err(|e| anyhow::anyhow!("Failed to generate mnemonic: {}", e))?;
        Ok(Self {
            phrase: mnemonic.to_string(),
            word_count: mnemonic.word_count(),
        })
    }

    /// Parse an existing mnemonic phrase string.
    ///
    /// # Errors
    /// Returns an error if the phrase is not a valid BIP39 mnemonic.
    pub fn from_phrase(phrase: &str) -> Result<Self> {
        let mnemonic = bip39::Mnemonic::parse_normalized(phrase.trim())
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;
        Ok(Self {
            phrase: mnemonic.to_string(),
            word_count: mnemonic.word_count(),
        })
    }

    /// Validate a mnemonic phrase without creating a seed object.
    /// Returns `true` if the phrase is a valid BIP39 mnemonic.
    pub fn validate(phrase: &str) -> bool {
        bip39::Mnemonic::parse_normalized(phrase.trim()).is_ok()
    }

    /// Get the word count of this mnemonic.
    pub fn word_count(&self) -> usize {
        self.word_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_12_words() {
        let seed = BIP39Seed::generate(12).unwrap();
        assert_eq!(seed.word_count(), 12);
        assert_eq!(seed.phrase.split_whitespace().count(), 12);
    }

    #[test]
    fn test_generate_24_words() {
        let seed = BIP39Seed::generate(24).unwrap();
        assert_eq!(seed.word_count(), 24);
        assert_eq!(seed.phrase.split_whitespace().count(), 24);
    }

    #[test]
    fn test_invalid_word_count() {
        assert!(BIP39Seed::generate(11).is_err());
        assert!(BIP39Seed::generate(13).is_err());
        assert!(BIP39Seed::generate(0).is_err());
    }

    #[test]
    fn test_from_phrase_roundtrip() {
        let original = BIP39Seed::generate(12).unwrap();
        let restored = BIP39Seed::from_phrase(&original.phrase).unwrap();
        assert_eq!(original.phrase, restored.phrase);
    }

    #[test]
    fn test_from_phrase_24_roundtrip() {
        let original = BIP39Seed::generate(24).unwrap();
        let restored = BIP39Seed::from_phrase(&original.phrase).unwrap();
        assert_eq!(original.phrase, restored.phrase);
    }

    #[test]
    fn test_validate_valid() {
        let seed = BIP39Seed::generate(12).unwrap();
        assert!(BIP39Seed::validate(&seed.phrase));
    }

    #[test]
    fn test_validate_invalid() {
        assert!(!BIP39Seed::validate("not valid words at all"));
        assert!(!BIP39Seed::validate(""));
        assert!(!BIP39Seed::validate("one two three"));
    }

    #[test]
    fn test_known_vector() {
        assert!(BIP39Seed::validate("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"));
    }
}
