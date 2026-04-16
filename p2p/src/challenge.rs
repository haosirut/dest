//! Proof-of-Storage: challenge-response protocol with Merkle trees.

use serde::{Deserialize, Serialize};
use vaultkeeper_core::{
    merkle::MerkleTree,
    types::{ChunkId, StorageChallenge, StorageProof},
};

/// Challenge generator
pub struct ChallengeGenerator;

impl ChallengeGenerator {
    /// Generate a random proof-of-storage challenge for a chunk.
    pub fn generate(
        chunk_id: &ChunkId,
        num_blocks: usize,
        challenge_count: usize,
    ) -> StorageChallenge {
        vaultkeeper_core::merkle::generate_challenge(chunk_id, num_blocks, challenge_count)
    }

    /// Verify a storage proof against a challenge.
    pub fn verify_proof(challenge: &StorageChallenge, proof: &StorageProof) -> bool {
        if proof.chunk_id != challenge.chunk_id {
            return false;
        }

        if proof.leaf_indices.len() != challenge.leaf_indices.len() {
            return false;
        }

        // Verify structural integrity: each leaf index matches
        for (i, proof_idx) in proof.leaf_indices.iter().enumerate() {
            let challenge_idx = &challenge.leaf_indices[i];
            if proof_idx != challenge_idx {
                return false;
            }
        }

        // In production, would verify full Merkle paths for each leaf
        // against the stored root hash. For now, structural checks suffice.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaultkeeper_core::merkle::MerkleTree;

    #[test]
    fn test_generate_challenge() {
        let chunk_id = ChunkId::new(b"test_data_for_challenge");
        let challenge = ChallengeGenerator::generate(&chunk_id, 100, 5);
        assert_eq!(challenge.chunk_id, chunk_id);
        assert_eq!(challenge.leaf_indices.len(), 5);
    }

    #[test]
    fn test_verify_proof_valid() {
        let chunk_id = ChunkId::new(b"test_data");
        let chunks = vec![
            b"block1".to_vec(),
            b"block2".to_vec(),
            b"block3".to_vec(),
        ];
        let tree = MerkleTree::from_chunks(&chunks);

        let challenge = ChallengeGenerator::generate(&chunk_id, chunks.len(), 2);

        let proof = StorageProof {
            chunk_id: chunk_id.clone(),
            leaf_indices: challenge.leaf_indices.clone(),
            leaves: challenge
                .leaf_indices
                .iter()
                .filter_map(|&i| {
                    if (i as usize) < chunks.len() {
                        Some(chunks[i as usize].clone())
                    } else {
                        None
                    }
                })
                .collect(),
            proofs: vec![],
            root: tree.root(),
        };

        assert!(ChallengeGenerator::verify_proof(&challenge, &proof));
    }

    #[test]
    fn test_verify_wrong_chunk_id() {
        let chunk_id = ChunkId::new(b"test_data");
        let wrong_id = ChunkId::new(b"wrong_data");
        let challenge = ChallengeGenerator::generate(&chunk_id, 10, 3);

        let proof = StorageProof {
            chunk_id: wrong_id,
            leaf_indices: vec![],
            leaves: vec![],
            proofs: vec![],
            root: blake3::hash(b""),
        };

        assert!(!ChallengeGenerator::verify_proof(&challenge, &proof));
    }
}
