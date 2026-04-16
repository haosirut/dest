//! Reed-Solomon Erasure Coding: (3,5) scheme — any 3 of 5 shards can reconstruct data.
//! Used for fault tolerance in distributed P2P storage.
//!
//! Layout: 3 data shards + 2 parity shards = 5 total shards.
//! Data is padded to a multiple of shard_size before encoding.

use anyhow::Result;
use crate::types::{RS_DATA_SHARDS, RS_PARITY_SHARDS, RS_TOTAL_SHARDS, CHUNK_SIZE};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Encode data into data_shards + parity_shards using Reed-Solomon erasure coding.
///
/// Data is padded to a multiple of RS_DATA_SHARDS before splitting into shards.
/// Returns a vector of RS_TOTAL_SHARDS shards, where each shard is of equal length.
///
/// # Errors
/// Returns an error if shard parameters are invalid (should not happen with
/// the configured constants).
pub fn encode(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .map_err(|e| anyhow::anyhow!("Invalid shard params: {}", e))?;

    if data.is_empty() {
        // Empty data: produce RS_TOTAL_SHARDS zero-length shards
        return Ok(vec![vec![]; RS_TOTAL_SHARDS]);
    }

    let total_len = ((data.len() + RS_DATA_SHARDS - 1) / RS_DATA_SHARDS) * RS_DATA_SHARDS;
    let mut padded = data.to_vec();
    padded.resize(total_len, 0);
    let shard_size = total_len / RS_DATA_SHARDS;

    let mut shards: Vec<Vec<u8>> = padded.chunks_exact(shard_size).map(|c| c.to_vec()).collect();
    for _ in 0..RS_PARITY_SHARDS {
        shards.push(vec![0u8; shard_size]);
    }

    rs.encode(&mut shards)
        .map_err(|e| anyhow::anyhow!("Encoding failed: {}", e))?;

    Ok(shards)
}

/// Reconstruct original data from a subset of shards.
///
/// `shard_data` contains (shard_index, shard_bytes) pairs.
/// At least RS_DATA_SHARDS (3) shards are required for reconstruction.
///
/// # Errors
/// - Returns error if fewer than RS_DATA_SHARDS shards are provided
/// - Returns error if any shard index is out of range
/// - Returns error if reconstruction fails (corrupted shards)
pub fn reconstruct(shard_data: &[(usize, Vec<u8>)]) -> Result<Vec<u8>> {
    if shard_data.len() < RS_DATA_SHARDS {
        anyhow::bail!(
            "Need {} shards to reconstruct, got {}",
            RS_DATA_SHARDS,
            shard_data.len()
        );
    }

    if shard_data.is_empty() {
        anyhow::bail!("No shards provided");
    }

    let shard_size = shard_data[0].1.len();
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; RS_TOTAL_SHARDS];

    for &(idx, ref data) in shard_data {
        if idx >= RS_TOTAL_SHARDS {
            anyhow::bail!("Shard index {} out of range (max {})", idx, RS_TOTAL_SHARDS - 1);
        }
        shards[idx] = Some(data.clone());
    }

    let rs = ReedSolomon::new(RS_DATA_SHARDS, RS_PARITY_SHARDS)
        .map_err(|e| anyhow::anyhow!("Invalid params: {}", e))?;

    rs.reconstruct(&mut shards)
        .map_err(|e| anyhow::anyhow!("Reconstruction failed: {}", e))?;

    // Extract only data shards
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
    fn test_encode_shard_count() {
        assert_eq!(encode(&vec![0u8; 4096]).unwrap().len(), RS_TOTAL_SHARDS);
    }

    #[test]
    fn test_reconstruct_all_shards() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let sd: Vec<_> = shards.into_iter().enumerate().collect();
        let result = reconstruct(&sd).unwrap();
        assert_eq!(&result[..CHUNK_SIZE], &data[..]);
    }

    #[test]
    fn test_reconstruct_3_of_5() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let sd: Vec<_> = (0..RS_DATA_SHARDS).map(|i| (i, shards[i].clone())).collect();
        assert_eq!(&reconstruct(&sd).unwrap()[..CHUNK_SIZE], &data[..]);
    }

    #[test]
    fn test_reconstruct_missing_data_shard() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let sd = vec![(0, shards[0].clone()), (2, shards[2].clone()), (4, shards[4].clone())];
        assert_eq!(&reconstruct(&sd).unwrap()[..CHUNK_SIZE], &data[..]);
    }

    #[test]
    fn test_insufficient_fails() {
        let shards = encode(&vec![0u8; 1024]).unwrap();
        let sd: Vec<_> = (0..RS_DATA_SHARDS - 1).map(|i| (i, shards[i].clone())).collect();
        assert!(reconstruct(&sd).is_err());
    }

    #[test]
    fn test_empty_data() {
        assert_eq!(encode(&[]).unwrap().len(), RS_TOTAL_SHARDS);
    }

    #[test]
    fn test_large_data() {
        let data = vec![0xCCu8; CHUNK_SIZE * 3];
        let shards = encode(&data).unwrap();
        let sd: Vec<_> = shards.into_iter().enumerate().collect();
        let result = reconstruct(&sd).unwrap();
        assert_eq!(&result[..data.len()], &data[..]);
    }

    #[test]
    fn test_all_3_of_5_combinations() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        // Test all C(5,3) = 10 combinations
        for i in 0..5 {
            for j in (i+1)..5 {
                for k in (j+1)..5 {
                    let sd = vec![
                        (i, shards[i].clone()),
                        (j, shards[j].clone()),
                        (k, shards[k].clone()),
                    ];
                    let result = reconstruct(&sd).unwrap();
                    assert_eq!(&result[..CHUNK_SIZE], &data[..],
                        "Failed with shards {}, {}, {}", i, j, k);
                }
            }
        }
    }

    #[test]
    fn test_shard_sizes_equal() {
        let data = vec![0x42u8; CHUNK_SIZE];
        let shards = encode(&data).unwrap();
        let first_len = shards[0].len();
        for (i, shard) in shards.iter().enumerate() {
            assert_eq!(shard.len(), first_len, "Shard {} has different size", i);
        }
    }
}
