//! Data chunking: splits data into 4 MiB chunks for storage and reassembly.

use crate::types::*;

/// Split data into fixed-size 4 MiB chunks.
pub fn chunk_data(data: &[u8]) -> Vec<Vec<u8>> {
    if data.is_empty() {
        return vec![];
    }
    let mut chunks = Vec::new();
    for i in (0..data.len()).step_by(CHUNK_SIZE) {
        let end = std::cmp::min(i + CHUNK_SIZE, data.len());
        chunks.push(data[i..end].to_vec());
    }
    chunks
}

/// Reassemble chunks back into the original data.
pub fn assemble_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total_size: usize = chunks.iter().map(|c| c.len()).sum();
    let mut data = Vec::with_capacity(total_size);
    for chunk in chunks {
        data.extend_from_slice(chunk);
    }
    data
}

/// Calculate the number of chunks needed for a given data size.
pub fn chunk_count(data_len: usize) -> usize {
    if data_len == 0 {
        0
    } else {
        (data_len + CHUNK_SIZE - 1) / CHUNK_SIZE
    }
}

/// Generate chunk IDs from data chunks.
pub fn generate_chunk_ids(chunks: &[Vec<u8>]) -> Vec<ChunkId> {
    chunks.iter().map(|c| ChunkId::new(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn test_chunk_exact_size() {
        let data = vec![0u8; CHUNK_SIZE];
        let chunks = chunk_data(&data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
    }

    #[test]
    fn test_chunk_multiple() {
        let data = vec![0u8; CHUNK_SIZE * 3 + 1024];
        let chunks = chunk_data(&data);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[3].len(), 1024);
    }

    #[test]
    fn test_roundtrip_chunk_assemble() {
        let mut data = vec![0u8; 10 * 1024 * 1024];
        rand::thread_rng().fill(&mut data[..]);
        let chunks = chunk_data(&data);
        let reassembled = assemble_chunks(&chunks);
        assert_eq!(data, reassembled);
    }

    #[test]
    fn test_chunk_empty_data() {
        let chunks = chunk_data(&[]);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_ids_unique() {
        let data1 = vec![0u8; CHUNK_SIZE];
        let data2 = vec![1u8; CHUNK_SIZE];
        let chunks = chunk_data(&data1);
        let chunks2 = chunk_data(&data2);
        let ids1 = generate_chunk_ids(&chunks);
        let ids2 = generate_chunk_ids(&chunks2);
        assert_ne!(ids1[0], ids2[0]);
    }

    #[test]
    fn test_chunk_count() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(CHUNK_SIZE), 1);
        assert_eq!(chunk_count(CHUNK_SIZE + 1), 2);
        assert_eq!(chunk_count(CHUNK_SIZE * 5), 5);
    }
}
