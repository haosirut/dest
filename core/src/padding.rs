//! Fixed-block padding: pads chunks to exactly 4 MiB with random data.
//! Prevents size-based traffic analysis.

use crate::types::CHUNK_SIZE;
use rand::RngCore;

/// Pad data to exactly CHUNK_SIZE bytes using random padding.
/// If data exceeds CHUNK_SIZE, it is truncated (caller should chunk first).
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

/// Store the original data length in a 4-byte big-endian header.
/// The padded chunk format: [4-byte original_len] [random padding] [actual data]
/// Original data starts at offset: CHUNK_SIZE - original_len.
pub fn pad_with_length_header(data: &[u8]) -> Vec<u8> {
    let original_len = data.len() as u32;
    let padded = pad_chunk(data);

    // Write original length at the beginning of the padded chunk
    let mut result = padded;
    result[0..4].copy_from_slice(&original_len.to_be_bytes());

    result
}

/// Extract original data from a padded chunk with length header.
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
        let data = vec![0u8; CHUNK_SIZE - 4];
        let padded = pad_with_length_header(&data);
        assert_eq!(padded.len(), CHUNK_SIZE);
        let unpadded = unpad_with_length_header(&padded).unwrap();
        assert_eq!(unpadded.len(), CHUNK_SIZE - 4);
    }

    #[test]
    fn test_unpad_invalid_data() {
        let data = vec![0u8; 2];
        let result = unpad_with_length_header(&data);
        assert!(result.is_none());
    }

    #[test]
    fn test_random_padding_varies() {
        let data = vec![0u8; 10];
        let padded1 = pad_chunk(&data);
        let padded2 = pad_chunk(&data);
        // The padding portion should differ
        assert_ne!(padded1[10..], padded2[10..]);
    }
}
