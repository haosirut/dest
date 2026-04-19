//! Reed-Solomon Erasure Coding: (3,5) scheme — any 3 of 5 shards can reconstruct data.
//! Used for fault tolerance in distributed storage.

use crate::types::{RS_DATA_SHARDS, RS_PARITY_SHARDS, RS_TOTAL_SHARDS};
#[cfg(test)]
use crate::types::CHUNK_SIZE;
use anyhow::{Context, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Encode data into data_shards + parity_shards using Reed-Solomon.
pub fn encode(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .context("Failed to create Reed-Solomon encoder (invalid shard counts)")?;

    if data.is_empty() {
        // Empty data: produce RS_TOTAL_SHARDS zero-length shards
        return Ok(vec![vec![]; RS_TOTAL_SHARDS]);
    }

    let total_len = ((data.len() + RS_DATA_SHARDS - 1) / RS_DATA_SHARDS) * RS_DATA_SHARDS;
    let mut padded = data.to_vec();
    padded.resize(total_len, 0);

    let shard_size = total_len / RS_DATA_SHARDS;

    let mut shards: Vec<Vec<u8>> = padded
        .chunks_exact(shard_size)
        .map(|c| c.to_vec())
        .collect();

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

    if shard_data.is_empty() {
        anyhow::bail!("No shards provided");
    }

    let shard_size = shard_data[0].1.len();

    // Build shard array: present shards keep their data, missing shards are None
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; RS_TOTAL_SHARDS];

    for &(idx, ref data) in shard_data {
        if idx >= RS_TOTAL_SHARDS {
            anyhow::bail!("Shard index {} out of range (max {})", idx, RS_TOTAL_SHARDS - 1);
        }
        shards[idx] = Some(data.clone());
    }

    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .context("Failed to create Reed-Solomon decoder")?;

    // reed-solomon-erasure v6: reconstruct takes a single &mut [Option<&mut [u8]>] argument
    // None = missing shard, Some(&mut [u8]) = present shard to use for recovery
    let mut shards_mut: Vec<Option<&mut [u8]>> = shards
        .iter_mut()
        .map(|opt| opt.as_mut().map(|v| v.as_mut_slice()))
        .collect();

    rs.reconstruct(&mut shards_mut)
        .context("Reed-Solomon reconstruction failed")?;

    // Extract only data shards (reconstruct filled in missing shards in-place)
    let mut result = Vec::with_capacity(RS_DATA_SHARDS * shard_size);
    for i in 0..RS_DATA_SHARDS {
        if let Some(ref shard) = shards[i] {
            result.extend_from_slice(shard);
        }
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
