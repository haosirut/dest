//! Data chunking: splits data into 4 MiB chunks for parallel storage, encryption, and reassembly.

use crate::types::ChunkId;

pub const CHUNK_SIZE: usize = crate::types::CHUNK_SIZE;

/// Split data into fixed-size 4 MiB chunks.
/// The last chunk may be smaller than CHUNK_SIZE.
/// Returns an empty vector for empty input.
pub fn chunk_data(data: &[u8]) -> Vec<Vec<u8>> {
    if data.is_empty() { return vec![]; }
    (0..data.len()).step_by(CHUNK_SIZE).map(|i| {
        let end = std::cmp::min(i + CHUNK_SIZE, data.len());
        data[i..end].to_vec()
    }).collect()
}

/// Reassemble chunks back into the original data.
/// Chunks are concatenated in order.
pub fn assemble_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    let mut data = Vec::with_capacity(total);
    for chunk in chunks { data.extend_from_slice(chunk); }
    data
}

/// Calculate the number of chunks needed for a given data size.
pub fn chunk_count(data_len: usize) -> usize {
    if data_len == 0 { 0 } else { (data_len + CHUNK_SIZE - 1) / CHUNK_SIZE }
}

/// Generate chunk IDs from data chunks using blake3 hashing.
/// Each chunk ID is a content-addressable identifier — identical data
/// always produces the same ChunkId.
pub fn generate_chunk_ids(chunks: &[Vec<u8>]) -> Vec<ChunkId> {
    chunks.iter().map(|c| ChunkId::new(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn test_exact_chunk() {
        let data = vec![0u8; CHUNK_SIZE];
        assert_eq!(chunk_data(&data).len(), 1);
    }

    #[test]
    fn test_multiple_chunks() {
        let data = vec![0u8; CHUNK_SIZE * 3 + 1024];
        assert_eq!(chunk_data(&data).len(), 4);
    }

    #[test]
    fn test_roundtrip() {
        let mut data = vec![0u8; 10 * 1024 * 1024];
        rand::thread_rng().fill_bytes(&mut data[..]);
        let chunks = chunk_data(&data);
        assert_eq!(assemble_chunks(&chunks), data);
    }

    #[test]
    fn test_empty() {
        assert!(chunk_data(&[]).is_empty());
    }

    #[test]
    fn test_chunk_ids_unique() {
        let c1 = chunk_data(&vec![0u8; CHUNK_SIZE]);
        let c2 = chunk_data(&vec![1u8; CHUNK_SIZE]);
        let ids1 = generate_chunk_ids(&c1);
        let ids2 = generate_chunk_ids(&c2);
        assert_ne!(ids1[0], ids2[0]);
    }

    #[test]
    fn test_chunk_ids_deterministic() {
        let c1 = chunk_data(&vec![0u8; CHUNK_SIZE]);
        let c2 = chunk_data(&vec![0u8; CHUNK_SIZE]);
        let ids1 = generate_chunk_ids(&c1);
        let ids2 = generate_chunk_ids(&c2);
        assert_eq!(ids1[0], ids2[0]);
    }

    #[test]
    fn test_chunk_count() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(CHUNK_SIZE), 1);
        assert_eq!(chunk_count(CHUNK_SIZE + 1), 2);
        assert_eq!(chunk_count(CHUNK_SIZE * 5), 5);
    }

    #[test]
    fn test_last_chunk_smaller() {
        let data = vec![0u8; CHUNK_SIZE * 2 + 500];
        let chunks = chunk_data(&data);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 500);
    }
}
