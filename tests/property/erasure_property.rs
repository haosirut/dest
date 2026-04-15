//! Property-based tests for erasure coding

#[cfg(test)]
mod tests {
    use vaultkeeper_core::erasure;

    /// Property: Any 3 of 5 shards can reconstruct the original data
    #[test]
    fn test_property_reconstruct_from_any_3_shards() {
        for data_size in [0, 1, 100, 1024, 4096, 8192] {
            let data = (0..data_size).map(|i| (i % 256) as u8).collect::<Vec<_>>();
            let shards = erasure::encode(&data).unwrap();
            assert_eq!(shards.len(), 5);

            // Try all C(5,3) = 10 combinations
            for i in 0..5 {
                for j in (i+1)..5 {
                    for k in (j+1)..5 {
                        let subset = vec![
                            (i, shards[i].clone()),
                            (j, shards[j].clone()),
                            (k, shards[k].clone()),
                        ];
                        let reconstructed = erasure::reconstruct(&subset).unwrap();
                        assert_eq!(&reconstructed[..data_size], &data,
                            "Failed for size {} with shards [{}, {}, {}]", data_size, i, j, k);
                    }
                }
            }
        }
    }

    /// Property: Encoding is deterministic
    #[test]
    fn test_property_deterministic_encoding() {
        let data = vec![0x42u8; 1024];
        let shards1 = erasure::encode(&data).unwrap();
        let shards2 = erasure::encode(&data).unwrap();
        assert_eq!(shards1, shards2);
    }

    /// Property: Different data produces different shards
    #[test]
    fn test_property_unique_shards_for_different_data() {
        let data1 = vec![0u8; 1024];
        let data2 = vec![1u8; 1024];
        let shards1 = erasure::encode(&data1).unwrap();
        let shards2 = erasure::encode(&data2).unwrap();
        // At least first data shard should differ
        assert_ne!(shards1[0], shards2[0]);
    }
}
