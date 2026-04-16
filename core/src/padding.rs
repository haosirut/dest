//! Fixed-block padding: pads chunks to exactly 4 MiB with random data.
//!
//! This prevents size-based traffic analysis by ensuring all encrypted
//! chunks appear identical in size regardless of original data length.
//!
//! Format with length header:
//!   [4-byte original_len BE] [random padding] [actual data]
//!
//! The actual data is placed at the END of the chunk. The 4-byte header
//! at the start stores the original length, enabling lossless unpadding.

use crate::types::CHUNK_SIZE;
use rand::RngCore;

/// Maximum data size that can be stored with a length header.
/// CHUNK_SIZE - 4 bytes for the header.
pub const MAX_PADDED_DATA_LEN: usize = CHUNK_SIZE - 4;

/// Pad data to exactly CHUNK_SIZE bytes using cryptographically random padding.
///
/// If data exceeds CHUNK_SIZE, this function will panic (caller should
/// chunk the data first using the chunking module).
///
/// The first bytes of the result contain the original data; the rest
/// is random padding. Use `pad_with_length_header` / `unpad_with_length_header`
/// for lossless roundtrip.
pub fn pad_chunk(data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() <= CHUNK_SIZE,
        "Data size {} exceeds CHUNK_SIZE {}",
        data.len(),
        CHUNK_SIZE
    );
    let mut padded = data.to_vec();
    if padded.len() < CHUNK_SIZE {
        let padding_len = CHUNK_SIZE - padded.len();
        padded.reserve(padding_len);
        let mut rng = rand::thread_rng();
        let mut padding = vec![0u8; padding_len];
        rng.fill_bytes(&mut padding);
        padded.extend_from_slice(&padding);
    }
    padded
}

/// Pad data with a 4-byte length header for lossless roundtrip.
///
/// Format: [4-byte original_len BE] [random padding] [actual data]
/// The actual data starts at offset: CHUNK_SIZE - original_len.
///
/// Maximum data size: CHUNK_SIZE - 4 bytes.
pub fn pad_with_length_header(data: &[u8]) -> Vec<u8> {
    let original_len = data.len();
    assert!(
        original_len <= MAX_PADDED_DATA_LEN,
        "Data size {} exceeds maximum {} (CHUNK_SIZE - 4-byte header)",
        original_len,
        MAX_PADDED_DATA_LEN
    );

    // Build: [4-byte header] [random padding] [actual data]
    let mut result = vec![0u8; CHUNK_SIZE];

    // Write header
    result[0..4].copy_from_slice(&(original_len as u32).to_be_bytes());

    // Write actual data at the end
    let data_start = CHUNK_SIZE - original_len;
    result[data_start..].copy_from_slice(data);

    // Fill middle with random padding
    if data_start > 4 {
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut result[4..data_start]);
    }

    result
}

/// Extract original data from a padded chunk with length header.
///
/// Reads the 4-byte header to determine original length, then extracts
/// the last `original_len` bytes from the chunk.
///
/// Returns `None` if the input is too short (< 4 bytes) or if the
/// claimed original length is invalid.
pub fn unpad_with_length_header(padded: &[u8]) -> Option<Vec<u8>> {
    if padded.len() < 4 {
        return None;
    }
    let original_len = u32::from_be_bytes([padded[0], padded[1], padded[2], padded[3]]) as usize;
    if original_len > padded.len() - 4 {
        return None;
    }
    let start = padded.len() - original_len;
    Some(padded[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_exact_size() {
        let data = vec![0u8; CHUNK_SIZE];
        let padded = pad_chunk(&data);
        assert_eq!(padded.len(), CHUNK_SIZE);
    }

    #[test]
    fn test_pad_smaller_data() {
        let data = vec![0x42u8; 1024];
        let padded = pad_chunk(&data);
        assert_eq!(padded.len(), CHUNK_SIZE);
        assert_eq!(&padded[..1024], &data[..]);
    }

    #[test]
    fn test_pad_with_header_roundtrip() {
        let data = b"Hello, World! This is test data for padding.";
        let padded = pad_with_length_header(data);
        assert_eq!(padded.len(), CHUNK_SIZE);
        let unpadded = unpad_with_length_header(&padded).unwrap();
        assert_eq!(unpadded.as_slice(), data);
    }

    #[test]
    fn test_pad_header_full_chunk() {
        let data = vec![0u8; MAX_PADDED_DATA_LEN];
        let padded = pad_with_length_header(&data);
        assert_eq!(padded.len(), CHUNK_SIZE);
        let unpadded = unpad_with_length_header(&padded).unwrap();
        assert_eq!(unpadded.len(), MAX_PADDED_DATA_LEN);
    }

    #[test]
    fn test_unpad_invalid_data() {
        let data = vec![0u8; 2];
        let result = unpad_with_length_header(&data);
        assert!(result.is_none());
    }

    #[test]
    fn test_unpad_invalid_length() {
        let mut data = vec![0u8; 100];
        // Set length header to a value larger than the data
        data[0..4].copy_from_slice(&200u32.to_be_bytes());
        let result = unpad_with_length_header(&data);
        assert!(result.is_none());
    }

    #[test]
    fn test_random_padding_varies() {
        let data = vec![0u8; 10];
        let padded1 = pad_chunk(&data);
        let padded2 = pad_chunk(&data);
        // The padding portion should differ (statistically)
        assert_ne!(padded1[10..], padded2[10..]);
    }

    #[test]
    fn test_pad_with_header_roundtrip_various_sizes() {
        for size in [0, 1, 100, 1024, MAX_PADDED_DATA_LEN, MAX_PADDED_DATA_LEN - 100] {
            let data = vec![0xABu8; size];
            let padded = pad_with_length_header(&data);
            assert_eq!(padded.len(), CHUNK_SIZE);
            let unpadded = unpad_with_length_header(&padded).unwrap();
            assert_eq!(data, unpadded, "Failed for size {}", size);
        }
    }

    #[test]
    fn test_pad_empty_data() {
        let padded = pad_chunk(&[]);
        assert_eq!(padded.len(), CHUNK_SIZE);
    }

    #[test]
    fn test_pad_header_empty_data() {
        let padded = pad_with_length_header(&[]);
        assert_eq!(padded.len(), CHUNK_SIZE);
        // Header should read 0
        assert_eq!(&padded[0..4], &0u32.to_be_bytes());
        let unpadded = unpad_with_length_header(&padded).unwrap();
        assert!(unpadded.is_empty());
    }

    #[test]
    fn test_pad_header_preserves_data_integrity() {
        // Test with data that has specific patterns to verify byte-level correctness
        let data: Vec<u8> = (0..100).map(|i| (i * 7 + 3) as u8).collect();
        let padded = pad_with_length_header(&data);
        let unpadded = unpad_with_length_header(&padded).unwrap();
        assert_eq!(data, unpadded);
    }
}
