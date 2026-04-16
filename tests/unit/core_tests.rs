//! Cross-crate unit tests for vaultkeeper-core

#[cfg(test)]
mod tests {
    #[test]
    fn test_chunking_roundtrip_large() {
        let data = vec![0x42u8; vaultkeeper_core::CHUNK_SIZE * 7 + 1337];
        let chunks = vaultkeeper_core::chunking::chunk_data(&data);
        let reassembled = vaultkeeper_core::chunking::assemble_chunks(&chunks);
        assert_eq!(data, reassembled);
    }

    #[test]
    fn test_vault_key_roundtrip() {
        let key = vaultkeeper_core::VaultKey::generate().unwrap();
        let dk = key.derive_key("test");
        let (nonce, ct) = vaultkeeper_core::crypto::encrypt_data(&dk, b"secret data").unwrap();
        let pt = vaultkeeper_core::crypto::decrypt_data(&dk, &nonce, &ct).unwrap();
        assert_eq!(pt, b"secret data");
    }

    #[test]
    fn test_bip39_determinism() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let k1 = vaultkeeper_core::VaultKey::from_seed(phrase, "").unwrap();
        let k2 = vaultkeeper_core::VaultKey::from_seed(phrase, "").unwrap();
        assert_eq!(k1.public_id(), k2.public_id());
    }

    #[test]
    fn test_erasure_3_of_5() {
        let data = vec![0x42u8; 4096];
        let shards = vaultkeeper_core::erasure::encode(&data).unwrap();
        for i in 0..5 {
            let subset: Vec<(usize, Vec<u8>)> = (0..5).filter(|&j| j != i).map(|j| (j, shards[j].clone())).collect();
            let result = vaultkeeper_core::erasure::reconstruct(&subset).unwrap();
            assert_eq!(&result[..data.len()], &data[..], "Failed without shard {}", i);
        }
    }

    #[test]
    fn test_merkle_proofs() {
        let chunks: Vec<Vec<u8>> = (0..16).map(|i| format!("data_{}", i).into_bytes()).collect();
        let tree = vaultkeeper_core::merkle::MerkleTree::from_chunks(&chunks);
        for i in 0..16 {
            let proof = tree.proof(i).expect("proof should exist");
            assert!(vaultkeeper_core::merkle::MerkleTree::verify_proof(&proof));
        }
    }

    #[test]
    fn test_padding_roundtrip() {
        for size in [0, 1, 100, 1024, vaultkeeper_core::CHUNK_SIZE - 4] {
            let data = vec![0xABu8; size];
            let padded = vaultkeeper_core::padding::pad_with_length_header(&data);
            assert_eq!(padded.len(), vaultkeeper_core::CHUNK_SIZE);
            let unpadded = vaultkeeper_core::padding::unpad_with_length_header(&padded).unwrap();
            assert_eq!(data, unpadded, "Failed for size {}", size);
        }
    }

    #[test]
    fn test_derived_key_contexts() {
        let key = vaultkeeper_core::VaultKey::generate().unwrap();
        let d1 = key.derive_key("ctx1");
        let d2 = key.derive_key("ctx2");
        assert_ne!(d1.as_bytes(), d2.as_bytes());
    }
}
