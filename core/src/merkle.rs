//! Merkle tree for data integrity verification and Proof-of-Storage.
//!
//! Uses blake3 as the hash function. The tree supports:
//! - Building from data chunks or pre-computed leaf hashes
//! - Generating inclusion proofs for any leaf
//! - Verifying proofs without access to the full tree
//! - Proof-of-Storage challenge generation for P2P auditing

use crate::types::{ChunkId, StorageChallenge, NONCE_LEN};
use blake3::{Hash, Hasher};
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

/// A Merkle tree built from data chunks.
///
/// Leaves are blake3 hashes of the input chunks. Internal nodes are
/// blake3(left_hash || right_hash). For odd-numbered layers, the last
/// node is duplicated (standard approach for balanced Merkle trees).
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
        let mut current = leaves.clone();

        while current.len() > 1 {
            let mut next = Vec::with_capacity((current.len() + 1) / 2);
            for i in (0..current.len()).step_by(2) {
                let left = current[i];
                let right = if i + 1 < current.len() {
                    current[i + 1]
                } else {
                    current[i] // Duplicate last node for odd counts
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

    /// Get the Merkle root hash as a 32-byte array.
    pub fn root_bytes(&self) -> [u8; 32] {
        *self.root.as_bytes()
    }

    /// Get the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Generate a Merkle proof for a leaf at the given index.
    /// Returns `None` if the index is out of range.
    ///
    /// The proof contains sibling hashes from the leaf level up to
    /// (but NOT including) the root. The root is stored separately
    /// in the proof for final comparison.
    pub fn proof(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaves.len() {
            return None;
        }

        // Iterate through layers, but stop before the root layer
        // (the root layer has only 1 element and no siblings)
        let upper_layers = if self.layers.len() > 1 {
            &self.layers[..self.layers.len() - 1]
        } else {
            // Single leaf tree: no proof path needed
            return Some(MerkleProof {
                leaf_index: index,
                leaf_hash: *self.leaves[index].as_bytes(),
                siblings: vec![],
                root: *self.root.as_bytes(),
            });
        };

        let mut siblings = Vec::new();
        let mut idx = index;

        for layer in upper_layers {
            let sibling_idx = if idx % 2 == 0 {
                idx + 1
            } else {
                idx - 1
            };
            let sibling = if sibling_idx < layer.len() {
                layer[sibling_idx]
            } else {
                layer[idx] // Duplicate for odd count
            };
            siblings.push(MerkleProofNode {
                hash: *sibling.as_bytes(),
                is_right: idx % 2 == 0,
            });
            idx /= 2;
        }

        Some(MerkleProof {
            leaf_index: index,
            leaf_hash: *self.leaves[index].as_bytes(),
            siblings,
            root: *self.root.as_bytes(),
        })
    }

    /// Verify a Merkle proof.
    ///
    /// Walks from the leaf to the root using the provided sibling hashes,
    /// then compares the computed root against the expected root.
    pub fn verify_proof(proof: &MerkleProof) -> bool {
        let mut current = proof.leaf_hash;

        for sibling in &proof.siblings {
            let mut hasher = Hasher::new();
            if sibling.is_right {
                hasher.update(&current);
                hasher.update(&sibling.hash);
            } else {
                hasher.update(&sibling.hash);
                hasher.update(&current);
            }
            let computed = hasher.finalize();
            current.copy_from_slice(computed.as_bytes());
        }

        current == proof.root
    }
}

/// A single node in a Merkle proof path.
/// Uses `[u8; 32]` for serialization compatibility (blake3::Hash
/// does not implement serde traits by default).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProofNode {
    /// The sibling hash at this level (32 bytes, blake3 output).
    pub hash: [u8; 32],
    /// Whether the sibling is on the right side of the current node.
    pub is_right: bool,
}

/// A complete Merkle proof for a single leaf.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProof {
    /// Index of the leaf this proof is for.
    pub leaf_index: usize,
    /// Hash of the leaf data (32 bytes).
    pub leaf_hash: [u8; 32],
    /// Sibling hashes from leaf to root.
    pub siblings: Vec<MerkleProofNode>,
    /// The expected root hash (32 bytes).
    pub root: [u8; 32],
}

/// Generate a Proof-of-Storage challenge for a given chunk.
///
/// Randomly selects `num_challenges` leaf indices and includes a nonce
/// and timestamp to prevent replay attacks.
///
/// The challenged node must respond with the leaf data at each requested
/// index, along with a Merkle proof path to the known root hash.
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

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    StorageChallenge {
        chunk_id: chunk_id.clone(),
        leaf_indices,
        nonce,
        timestamp,
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
    fn test_single_leaf_proof() {
        let chunks = vec![b"single".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let proof = tree.proof(0).unwrap();
        assert!(proof.siblings.is_empty(), "Single leaf should have no siblings");
        assert!(MerkleTree::verify_proof(&proof));
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
    fn test_proof_16_leaves() {
        let chunks: Vec<Vec<u8>> = (0..16).map(|i| format!("data_block_{}", i).into_bytes()).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        for i in 0..chunks.len() {
            let proof = tree.proof(i).expect("proof should exist");
            assert!(MerkleTree::verify_proof(&proof));
        }
    }

    #[test]
    fn test_tampered_leaf_fails() {
        let chunks = vec![b"original".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let mut proof = tree.proof(0).unwrap();
        proof.leaf_hash = *blake3::hash(b"tampered").as_bytes();
        assert!(!MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_tampered_sibling_fails() {
        let chunks = vec![b"a".to_vec(), b"b".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let mut proof = tree.proof(0).unwrap();
        if !proof.siblings.is_empty() {
            proof.siblings[0].hash = *blake3::hash(b"wrong").as_bytes();
        }
        assert!(!MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::from_chunks(&[]);
        let _root = tree.root();
        assert_eq!(tree.leaf_count(), 0);
        assert!(tree.proof(0).is_none());
    }

    #[test]
    fn test_from_precomputed_hashes() {
        let h1 = blake3::hash(b"first");
        let h2 = blake3::hash(b"second");
        let tree = MerkleTree::from_hashes(vec![h1, h2]);
        let proof = tree.proof(0).unwrap();
        assert!(MerkleTree::verify_proof(&proof));
    }

    #[test]
    fn test_leaf_count() {
        let chunks: Vec<Vec<u8>> = (0..5).map(|i| vec![i as u8]).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        assert_eq!(tree.leaf_count(), 5);
    }

    #[test]
    fn test_root_bytes() {
        let chunks = vec![b"test".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        let root = tree.root();
        let root_bytes = tree.root_bytes();
        assert_eq!(root_bytes, *root.as_bytes());
    }

    #[test]
    fn test_generate_challenge() {
        let chunk_id = ChunkId::new(b"test_chunk");
        let challenge = generate_challenge(&chunk_id, 100, 5);
        assert_eq!(challenge.leaf_indices.len(), 5);
        assert_eq!(challenge.chunk_id, chunk_id);
    }

    #[test]
    fn test_generate_challenge_unique_indices() {
        let chunk_id = ChunkId::new(b"test_chunk");
        let challenge = generate_challenge(&chunk_id, 100, 5);
        let unique: std::collections::HashSet<_> = challenge.leaf_indices.iter().collect();
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn test_proof_out_of_range() {
        let chunks = vec![b"only_one".to_vec()];
        let tree = MerkleTree::from_chunks(&chunks);
        assert!(tree.proof(1).is_none());
    }

    #[test]
    fn test_large_tree() {
        let chunks: Vec<Vec<u8>> = (0..256).map(|i| format!("chunk_{:04}", i).into_bytes()).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        assert_eq!(tree.leaf_count(), 256);
        // Spot check a few leaves
        for &i in &[0, 1, 127, 128, 255] {
            let proof = tree.proof(i).unwrap();
            assert!(MerkleTree::verify_proof(&proof), "Proof for leaf {} failed", i);
        }
    }

    #[test]
    fn test_proof_sibling_count() {
        // For 4 leaves, depth = 2, so 2 siblings per proof
        let chunks: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8]).collect();
        let tree = MerkleTree::from_chunks(&chunks);
        let proof = tree.proof(0).unwrap();
        assert_eq!(proof.siblings.len(), 2);

        // For 8 leaves, depth = 3, so 3 siblings per proof
        let chunks8: Vec<Vec<u8>> = (0..8).map(|i| vec![i as u8]).collect();
        let tree8 = MerkleTree::from_chunks(&chunks8);
        let proof8 = tree8.proof(0).unwrap();
        assert_eq!(proof8.siblings.len(), 3);
    }
}
