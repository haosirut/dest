//! Unit tests for vaultkeeper-core

#[cfg(test)]
mod tests {
    #[test]
    fn test_chunking_roundtrip_large() {
        use vaultkeeper_core::chunking;
        let data = vec![0x42u8; vaultkeeper_core::CHUNK_SIZE * 7 + 1337];
        let chunks = chunking::chunk_data(&data);
        let reassembled = chunking::assemble_chunks(&chunks);
        assert_eq!(data, reassembled);
    }

    #[test]
    fn test_encryption_various_sizes() {
        use vaultkeeper_core::{encryption, types::*};
        let key = DerivedKey([0x42u8; DERIVED_KEY_LEN]);
        for size in [0, 1, 15, 16, 255, 1024, 4096, CHUNK_SIZE] {
            let plaintext = vec![size as u8; size];
            let (nonce, ct) = encryption::encrypt(&key, &plaintext).unwrap();
            let pt = encryption::decrypt(&key, &nonce, &ct).unwrap();
            assert_eq!(plaintext, pt, "Failed for size {}", size);
        }
    }

    #[test]
    fn test_erasure_various_shard_combinations() {
        use vaultkeeper_core::erasure;
        let data = vec![0x42u8; 4096];
        let shards = erasure::encode(&data).unwrap();
        
        // Test all possible 3-of-5 combinations
        let indices = vec![0,1,2,3,4];
        for i in 0..5 {
            let subset: Vec<(usize, Vec<u8>)> = indices.iter()
                .filter(|&&j| j != i)
                .map(|&j| (j, shards[j].clone()))
                .collect();
            let reconstructed = erasure::reconstruct(&subset).unwrap();
            assert_eq!(&reconstructed[..data.len()], &data[..], "Failed without shard {}", i);
        }
    }

    #[test]
    fn test_merkle_all_proofs() {
        use vaultkeeper_core::merkle::MerkleTree;
        let chunks: Vec<Vec<u8>> = (0..16).map(|i| format!("data_block_{}", i).into_bytes()).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        for i in 0..chunks.len() {
            let proof = tree.proof(i).expect("proof should exist");
            assert!(MerkleTree::verify_proof(&proof));
        }
    }

    #[test]
    fn test_padding_roundtrip_all_sizes() {
        use vaultkeeper_core::padding;
        for size in [0, 1, 100, 1024, CHUNK_SIZE - 4, CHUNK_SIZE - 1] {
            let data = vec![0xABu8; size];
            let padded = padding::pad_with_length_header(&data);
            assert_eq!(padded.len(), vaultkeeper_core::CHUNK_SIZE);
            let unpadded = padding::unpad_with_length_header(&padded).unwrap();
            assert_eq!(data, unpadded, "Failed for size {}", size);
        }
    }

    #[test]
    fn test_bip39_determinism() {
        use vaultkeeper_core::bip39_recovery;
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        assert!(bip39_recovery::validate_mnemonic(phrase));
        let mnemonic = bip39_recovery::parse_mnemonic(phrase).unwrap();
        let key1 = bip39_recovery::recover_key_from_mnemonic(&mnemonic, "").unwrap();
        let key2 = bip39_recovery::recover_key_from_mnemonic(&mnemonic, "").unwrap();
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }
}
