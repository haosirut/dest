//! Merkle tree for data integrity verification and Proof-of-Storage.

use crate::types::*;
use blake3::{Hash, Hasher};
use rand::Rng;

/// A Merkle tree built from data chunks.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    leaves: Vec<Hash>,
    layers: Vec<Vec<Hash>>,
    root: Hash,
}

impl MerkleTree {
    /// Build a Merkle tree from data chunks.
    pub fn from_chunks(chunks: &[Vec<u8>]) -> Self {
        let leaves: Vec<Hash> = chunks.iter().map(|chunk| blake3::hash(chunk)).collect();
        Self::from_hashes(leaves)
    }

    /// Build a Merkle tree from pre-computed leaf hashes.
    pub fn from_hashes(leaves: Vec<Hash>) -> Self {
        if leaves.is_empty() {
            return Self {
                leaves: vec![],
                layers: vec![],
                root: blake3::hash(b""),
            };
        }

        let mut layers = vec![leaves.clone()];
        let mut current = leaves;

        while current.len() > 1 {
            let mut next = Vec::with_capacity((current.len() + 1) / 2);
            for i in (0..current.len()).step_by(2) {
                let left = current[i];
                let right = if i + 1 < current.len() {
                    current[i + 1]
                } else {
                    current[i]
                };
                let mut hasher = Hasher::new();
                hasher.update(left.as_bytes());
                hasher.update(right.as_bytes());
                next.push(hasher.finalize());
            }
            layers.push(next.clone());
            current = next;
        }

        let root = current[0];
        Self { leaves, layers, root }
    }

    /// Get the Merkle root hash.
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Generate a Merkle proof for a leaf at the given index.
    /// Returns a list of sibling hashes from leaf to root.
    pub fn proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() {
            return None;
        }

        let mut siblings = Vec::new();
        let mut idx = index;

        for layer in &self.layers {
            let sibling_idx = if idx % 2 == 0 {
                idx + 1
            } else {
                idx - 1
            };
            let sibling = if sibling_idx < layer.len() {
                layer[sibling_idx]
            } else {
                layer[idx]
            };
            siblings.push(MerkleProofNode {
                hash: sibling,
                is_right: idx % 2 == 0,
            });
            idx /= 2;
        }

        Some(MerkleProof {
            leaf_index: index,
            leaf_hash: self.leaves[index],
            siblings,
            root: self.root,
        })
    }

    /// Verify a Merkle proof.
    pub fn verify_proof(proof: &MerkleProof) -> bool {
        let mut current = proof.leaf_hash;

        for sibling in &proof.siblings {
            let mut hasher = Hasher::new();
            if sibling.is_right {
                hasher.update(current.as_bytes());
                hasher.update(sibling.hash.as_bytes());
            } else {
                hasher.update(sibling.hash.as_bytes());
                hasher.update(current.as_bytes());
            }
            current = hasher.finalize();
        }

        current == proof.root
    }
}

/// A single node in a Merkle proof path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProofNode {
    pub hash: Hash,
    pub is_right: bool,
}

/// A complete Merkle proof for a single leaf.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProof {
    pub leaf_index: usize,
    pub leaf_hash: Hash,
    pub siblings: Vec<MerkleProofNode>,
    pub root: Hash,
}

/// Generate a Proof-of-Storage challenge with random leaf indices.
pub fn generate_challenge(chunk_id: &ChunkId, num_leaves: usize, num_challenges: usize) -> StorageChallenge {
    let mut rng = rand::thread_rng();
    let mut leaf_indices = Vec::with_capacity(num_challenges);

    if num_leaves == 0 {
        leaf_indices.push(0);
    } else {
        use rand::seq::SliceRandom;
        let indices: Vec<u64> = (0..num_leaves as u64).collect();
        for &idx in indices.choose_multiple(&mut rng, num_challenges) {
            leaf_indices.push(idx);
        }
    }

    let mut nonce = [0u8; NONCE_LEN];
    rng.fill(&mut nonce[..]);

    StorageChallenge {
        chunk_id: chunk_id.clone(),
        leaf_indices,
        nonce,
        timestamp: chrono::Utc::now().timestamp() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_leaf_tree() {
        let chunks = vec![b"single".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let root = tree.root();
        assert_eq!(root, blake3::hash(b"single"));
    }

    #[test]
    fn test_even_leaf_count() {
        let chunks = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let proof = tree.proof(0).unwrap();
        assert!(MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_odd_leaf_count() {
        let chunks = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let proof = tree.proof(2).unwrap();
        assert!(MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_proof_all_leaves() {
        let chunks: Vec<Vec<u8>> = (0..8).map(|i| format!("chunk_{}", i).into_bytes()).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        for i in 0..chunks.len() {
            let proof = tree.proof(i).unwrap();
            assert!(MerkleTree::verify_proof(&proof), "Proof for leaf {} failed", i);
        }
    }

    #[test]
    fn test_tampered_leaf_fails() {
        let chunks = vec![b"original".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let mut proof = tree.proof(0).unwrap();
        proof.leaf_hash = blake3::hash(b"tampered");
        assert!(!MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::from_chunks(&[]);
        let _root = tree.root();
    }

    #[test]
    fn test_generate_challenge() {
        let chunk_id = ChunkId::new(b"test_chunk");
        let challenge = generate_challenge(&chunk_id, 100, 5);
        assert_eq!(challenge.leaf_indices.len(), 5);
        assert_eq!(challenge.chunk_id, chunk_id);
    }
}
