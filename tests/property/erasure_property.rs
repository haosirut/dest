#[cfg(test)]
mod tests {
    #[test]
    fn test_reconstruct_all_combinations() {
        for data_size in [0, 1, 100, 1024, 4096, 8192] {
            let data: Vec<u8> = (0..data_size).map(|i| (i % 256) as u8).collect();
            let shards = vaultkeeper_core::erasure::encode(&data).unwrap();
            assert_eq!(shards.len(), 5);
            // C(5,3) = 10 combinations
            for i in 0..5 {
                for j in (i+1)..5 {
                    for k in (j+1)..5 {
                        let subset = vec![(i, shards[i].clone()), (j, shards[j].clone()), (k, shards[k].clone())];
                        let result = vaultkeeper_core::erasure::reconstruct(&subset).unwrap();
                        assert_eq!(&result[..data_size], &data[..], "Failed for {} with [{},{},{}]", data_size, i, j, k);
                    }
                }
            }
        }
    }

    #[test]
    fn test_deterministic_encoding() {
        let data = vec![0x42u8; 1024];
        let s1 = vaultkeeper_core::erasure::encode(&data).unwrap();
        let s2 = vaultkeeper_core::erasure::encode(&data).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_unique_shards() {
        let s1 = vaultkeeper_core::erasure::encode(&vec![0u8; 1024]).unwrap();
        let s2 = vaultkeeper_core::erasure::encode(&vec![1u8; 1024]).unwrap();
        assert_ne!(s1[0], s2[0]);
    }
}
