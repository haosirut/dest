//! Reed-Solomon Erasure Coding: (3,5) scheme — any 3 of 5 shards can reconstruct data.
//! Used for fault tolerance in distributed storage.

use crate::types::*;
use anyhow::{Context, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Encode data into data_shards + parity_shards using Reed-Solomon.
pub fn encode(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .context("Failed to create Reed-Solomon encoder (invalid shard counts)")?;

    let total_len = ((data.len() + RS_DATA_SHARDS - 1) / RS_DATA_SHARDS) * RS_DATA_SHARDS;
    let padded_data = {
        let mut padded = data.to_vec();
        padded.resize(total_len, 0);
        padded
    };

    let shard_size = total_len / RS_DATA_SHARDS;
    let mut shards: Vec<Vec<u8>> = padded_data
        .chunks_exact(shard_size)
        .map(|c| c.to_vec())
        .collect();

    // Pad shards to equal length
    for shard in &mut shards {
        shard.resize(shard_size, 0);
    }

    // Add parity shards (filled with zeros initially)
    for _ in 0..RS_PARITY_SHARDS {
        shards.push(vec![0u8; shard_size]);
    }

    rs.encode(&mut shards)
        .context("Reed-Solomon encoding failed")?;

    Ok(shards)
}

/// Reconstruct original data from a subset of shards.
/// `shard_data` contains (shard_index, shard_bytes) pairs.
/// At least RS_DATA_SHARDS shards are required.
pub fn reconstruct(shard_data: &[(usize, Vec<u8>)]) -> Result<Vec<u8>> {
    if shard_data.len() < RS_DATA_SHARDS {
        anyhow::bail!(
            "Need at least {} shards to reconstruct, got {}",
            RS_DATA_SHARDS,
            shard_data.len()
        );
    }

    let shard_size = shard_data[0].1.len();
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; RS_TOTAL_SHARDS];
    let mut present_indices = Vec::new();

    for &(idx, ref data) in shard_data {
        if idx >= RS_TOTAL_SHARDS {
            anyhow::bail!("Shard index {} out of range (max {})", idx, RS_TOTAL_SHARDS - 1);
        }
        shards[idx] = Some(data.clone());
        present_indices.push(idx);
    }

    let mut shard_vecs: Vec<Vec<u8>> = shards
        .into_iter()
        .enumerate()
        .map(|(i, opt)| {
            opt.unwrap_or_else(|| {
                vec![0u8; shard_size]
            })
        })
        .collect();

    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .context("Failed to create Reed-Solomon decoder")?;

    rs.reconstruct(&mut shard_vecs, &present_indices)
        .context("Reed-Solomon reconstruction failed")?;

    // Extract only data shards
    let mut result = Vec::with_capacity(RS_DATA_SHARDS * shard_size);
    for i in 0..RS_DATA_SHARDS {
        result.extend_from_slice(&shard_vecs[i]);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_produces_correct_shard_count() {
        let data = vec![0xABu8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        assert_eq!(shards.len(), RS_TOTAL_SHARDS);
    }

    #[test]
    fn test_reconstruct_from_all_shards() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let shard_data: Vec<(usize, Vec<u8>)> = shards.into_iter().enumerate().collect();
        let reconstructed = reconstruct(&shard_data).unwrap();
        // Trim padding
        let trimmed = &reconstructed[..CHUNK_SIZE];
        assert_eq!(data, trimmed);
    }

    #[test]
    fn test_reconstruct_from_minimal_shards() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        // Use only first 3 data shards
        let shard_data: Vec<(usize, Vec<u8>)> = shards
            .iter()
            .enumerate()
            .take(RS_DATA_SHARDS)
            .map(|(i, s)| (i, s.clone()))
            .collect();
        let reconstructed = reconstruct(&shard_data).unwrap();
        let trimmed = &reconstructed[..CHUNK_SIZE];
        assert_eq!(data, trimmed);
    }

    #[test]
    fn test_reconstruct_missing_data_shards() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        // Use data shard 0, 2 and parity shard 3, 4
        let shard_data = vec![
            (0, shards[0].clone()),
            (2, shards[2].clone()),
            (3, shards[3].clone()),
        ];
        let reconstructed = reconstruct(&shard_data).unwrap();
        let trimmed = &reconstructed[..CHUNK_SIZE];
        assert_eq!(data, trimmed);
    }

    #[test]
    fn test_insufficient_shards_fails() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let shard_data: Vec<(usize, Vec<u8>)> = shards
            .iter()
            .enumerate()
            .take(RS_DATA_SHARDS - 1)
            .map(|(i, s)| (i, s.clone()))
            .collect();
        assert!(reconstruct(&shard_data).is_err());
    }

    #[test]
    fn test_empty_data() {
        let shards = encode(&[]).unwrap();
        assert_eq!(shards.len(), RS_TOTAL_SHARDS);
    }

    #[test]
    fn test_large_data() {
        let data = vec![0xCCu8; CHUNK_SIZE * 3];
        let shards = encode(&data).unwrap();
        let shard_data: Vec<(usize, Vec<u8>)> = shards.into_iter().enumerate().collect();
        let reconstructed = reconstruct(&shard_data).unwrap();
        let trimmed = &reconstructed[..CHUNK_SIZE * 3];
        assert_eq!(data, trimmed);
    }
}
