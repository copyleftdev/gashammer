// Copyright (c) 2025-present Don Johnson
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://github.com/copyleftdev/gashammer/blob/main/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.

//! Account pool — deterministic key derivation, partitioning, nonce tracking.
//!
//! Manages test accounts for workload generation. Keys are derived
//! deterministically from a master seed using the GasHammer DNA path
//! scheme: `keccak256(seed || 0x4748 || index)`.
//!
//! Ref: RFC-0005 §6, RFC-0011 §5.2.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use alloy::primitives::{keccak256, Address};
use alloy::signers::local::PrivateKeySigner;
use serde::{Deserialize, Serialize};

/// DNA marker bytes embedded in key derivation ("GH" = 0x4748).
/// Matches the HD path concept: m/44'/60'/0'/0'/0x4748/{index}.
const DERIVATION_MARKER: [u8; 2] = [0x47, 0x48];

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

/// A test account with a signing key and local nonce tracker.
pub struct Account {
    signer: PrivateKeySigner,
    nonce: AtomicU64,
}

impl Account {
    /// Create an account from a private key signer.
    pub fn new(signer: PrivateKeySigner) -> Self {
        Self {
            signer,
            nonce: AtomicU64::new(0),
        }
    }

    /// The Ethereum address of this account.
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Reference to the signer for transaction signing.
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Get and increment the local nonce (atomic).
    pub fn next_nonce(&self) -> u64 {
        self.nonce.fetch_add(1, Ordering::SeqCst)
    }

    /// Read the current nonce without incrementing.
    pub fn current_nonce(&self) -> u64 {
        self.nonce.load(Ordering::SeqCst)
    }

    /// Set the nonce to a specific value (for initialization or recovery).
    pub fn set_nonce(&self, nonce: u64) {
        self.nonce.store(nonce, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("address", &self.address())
            .field("nonce", &self.current_nonce())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

/// Derive a private key deterministically from a master seed and index.
///
/// Derivation: `keccak256(seed_bytes || 0x4748 || index_bytes)`.
/// The 0x4748 marker is the DNA provenance tag ("GH").
pub fn derive_key(master_seed: u64, index: u32) -> PrivateKeySigner {
    let mut preimage = Vec::with_capacity(14);
    preimage.extend_from_slice(&master_seed.to_be_bytes());
    preimage.extend_from_slice(&DERIVATION_MARKER);
    preimage.extend_from_slice(&index.to_be_bytes());

    let hash = keccak256(&preimage);

    // keccak256 output is 32 bytes, valid as a secp256k1 private key
    // (extremely unlikely to be zero or >= curve order)
    PrivateKeySigner::from_bytes(&hash).expect("derived key should be valid secp256k1 scalar")
}

/// Derive N accounts from a master seed starting at a given offset.
pub fn derive_accounts(master_seed: u64, offset: u32, count: u32) -> Vec<Account> {
    (offset..offset + count)
        .map(|i| Account::new(derive_key(master_seed, i)))
        .collect()
}

// ---------------------------------------------------------------------------
// Partitioning
// ---------------------------------------------------------------------------

/// Configuration for account pool partitioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionConfig {
    /// Master seed for key derivation.
    pub master_seed: u64,
    /// Total number of accounts in the pool.
    pub total_accounts: u32,
    /// Number of edges to partition across.
    pub edge_count: u32,
}

/// A partition assignment for a single edge.
#[derive(Debug, Clone)]
pub struct PartitionAssignment {
    pub edge_index: u32,
    pub offset: u32,
    pub count: u32,
}

/// Partition accounts across edges with disjoint, non-overlapping ranges.
///
/// Each edge gets `total_accounts / edge_count` accounts. Remainder
/// accounts are distributed one each to the first edges.
pub fn partition_accounts(config: &PartitionConfig) -> Vec<PartitionAssignment> {
    if config.edge_count == 0 {
        return Vec::new();
    }

    let base_count = config.total_accounts / config.edge_count;
    let remainder = config.total_accounts % config.edge_count;
    let mut assignments = Vec::with_capacity(config.edge_count as usize);
    let mut offset = 0u32;

    for i in 0..config.edge_count {
        let count = base_count + if i < remainder { 1 } else { 0 };
        assignments.push(PartitionAssignment {
            edge_index: i,
            offset,
            count,
        });
        offset += count;
    }

    assignments
}

/// Derive the edge-specific seed for deterministic workload generation.
///
/// `edge_seed = master_seed XOR (region_index << 32 | edge_index)`
pub fn edge_seed(master_seed: u64, region_index: u32, edge_index: u32) -> u64 {
    master_seed ^ ((region_index as u64) << 32 | edge_index as u64)
}

// ---------------------------------------------------------------------------
// Account pool
// ---------------------------------------------------------------------------

/// Validation result for pool sizing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSizeValidation {
    pub total_accounts: u32,
    pub min_required: u32,
    pub is_sufficient: bool,
}

/// Validate that the account pool is large enough for the target throughput.
///
/// `min_accounts >= max_tx_per_sec × avg_receipt_wait_sec`
pub fn validate_pool_size(
    total_accounts: u32,
    max_tx_per_sec: u64,
    avg_receipt_wait_sec: u64,
) -> PoolSizeValidation {
    let min_required = (max_tx_per_sec * avg_receipt_wait_sec) as u32;
    PoolSizeValidation {
        total_accounts,
        min_required,
        is_sufficient: total_accounts >= min_required,
    }
}

/// An account pool for a single edge with round-robin selection.
pub struct AccountPool {
    accounts: Vec<Arc<Account>>,
    cursor: AtomicU64,
}

impl AccountPool {
    /// Create a pool from a list of accounts.
    pub fn new(accounts: Vec<Account>) -> Self {
        let accounts = accounts.into_iter().map(Arc::new).collect();
        Self {
            accounts,
            cursor: AtomicU64::new(0),
        }
    }

    /// Create a pool by deriving accounts from a master seed.
    pub fn from_seed(master_seed: u64, offset: u32, count: u32) -> Self {
        let accounts = derive_accounts(master_seed, offset, count);
        Self::new(accounts)
    }

    /// Number of accounts in the pool.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Get the next account via round-robin rotation.
    pub fn next_account(&self) -> Arc<Account> {
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) as usize % self.accounts.len();
        Arc::clone(&self.accounts[idx])
    }

    /// Get an account by index.
    pub fn get(&self, index: usize) -> Option<Arc<Account>> {
        self.accounts.get(index).map(Arc::clone)
    }

    /// All accounts in the pool.
    pub fn accounts(&self) -> &[Arc<Account>] {
        &self.accounts
    }

    /// All addresses in the pool.
    pub fn addresses(&self) -> Vec<Address> {
        self.accounts.iter().map(|a| a.address()).collect()
    }
}

impl std::fmt::Debug for AccountPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountPool")
            .field("count", &self.accounts.len())
            .field("cursor", &self.cursor.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let key1 = derive_key(42, 0);
        let key2 = derive_key(42, 0);
        assert_eq!(key1.address(), key2.address());
    }

    #[test]
    fn test_derive_key_different_index() {
        let key0 = derive_key(42, 0);
        let key1 = derive_key(42, 1);
        assert_ne!(key0.address(), key1.address());
    }

    #[test]
    fn test_derive_key_different_seed() {
        let key_a = derive_key(42, 0);
        let key_b = derive_key(99, 0);
        assert_ne!(key_a.address(), key_b.address());
    }

    #[test]
    fn test_derive_accounts_count() {
        let accounts = derive_accounts(42, 0, 10);
        assert_eq!(accounts.len(), 10);
    }

    #[test]
    fn test_derive_accounts_unique_addresses() {
        let accounts = derive_accounts(42, 0, 100);
        let mut addresses: Vec<Address> = accounts.iter().map(|a| a.address()).collect();
        addresses.sort();
        addresses.dedup();
        assert_eq!(addresses.len(), 100);
    }

    #[test]
    fn test_derive_accounts_offset() {
        let all = derive_accounts(42, 0, 10);
        let offset = derive_accounts(42, 5, 5);

        // offset[0] should match all[5]
        assert_eq!(offset[0].address(), all[5].address());
        assert_eq!(offset[4].address(), all[9].address());
    }

    #[test]
    fn test_account_nonce_tracking() {
        let account = Account::new(derive_key(42, 0));
        assert_eq!(account.current_nonce(), 0);
        assert_eq!(account.next_nonce(), 0);
        assert_eq!(account.current_nonce(), 1);
        assert_eq!(account.next_nonce(), 1);
        assert_eq!(account.current_nonce(), 2);
    }

    #[test]
    fn test_account_set_nonce() {
        let account = Account::new(derive_key(42, 0));
        account.set_nonce(100);
        assert_eq!(account.current_nonce(), 100);
        assert_eq!(account.next_nonce(), 100);
        assert_eq!(account.current_nonce(), 101);
    }

    #[test]
    fn test_partition_even_split() {
        let config = PartitionConfig {
            master_seed: 42,
            total_accounts: 100,
            edge_count: 4,
        };
        let parts = partition_accounts(&config);
        assert_eq!(parts.len(), 4);
        for p in &parts {
            assert_eq!(p.count, 25);
        }
        // Verify disjoint ranges
        assert_eq!(parts[0].offset, 0);
        assert_eq!(parts[1].offset, 25);
        assert_eq!(parts[2].offset, 50);
        assert_eq!(parts[3].offset, 75);
    }

    #[test]
    fn test_partition_uneven_split() {
        let config = PartitionConfig {
            master_seed: 42,
            total_accounts: 10,
            edge_count: 3,
        };
        let parts = partition_accounts(&config);
        assert_eq!(parts.len(), 3);
        // 10 / 3 = 3 remainder 1
        assert_eq!(parts[0].count, 4); // gets extra
        assert_eq!(parts[1].count, 3);
        assert_eq!(parts[2].count, 3);

        // Total should equal total_accounts
        let total: u32 = parts.iter().map(|p| p.count).sum();
        assert_eq!(total, 10);
    }

    #[test]
    fn test_partition_disjoint_addresses() {
        let config = PartitionConfig {
            master_seed: 42,
            total_accounts: 20,
            edge_count: 4,
        };
        let parts = partition_accounts(&config);

        let mut all_addresses = Vec::new();
        for part in &parts {
            let accounts = derive_accounts(config.master_seed, part.offset, part.count);
            for acc in &accounts {
                all_addresses.push(acc.address());
            }
        }
        let count_before = all_addresses.len();
        all_addresses.sort();
        all_addresses.dedup();
        assert_eq!(
            all_addresses.len(),
            count_before,
            "partitions must not overlap"
        );
    }

    #[test]
    fn test_partition_zero_edges() {
        let config = PartitionConfig {
            master_seed: 42,
            total_accounts: 10,
            edge_count: 0,
        };
        let parts = partition_accounts(&config);
        assert!(parts.is_empty());
    }

    #[test]
    fn test_edge_seed_derivation() {
        let seed = edge_seed(42, 0, 0);
        assert_eq!(seed, 42); // XOR with 0

        let seed1 = edge_seed(42, 1, 0);
        let seed2 = edge_seed(42, 0, 1);
        assert_ne!(seed1, seed2);

        // Deterministic
        assert_eq!(edge_seed(42, 3, 7), edge_seed(42, 3, 7));
    }

    #[test]
    fn test_validate_pool_size_sufficient() {
        let result = validate_pool_size(1000, 100, 2);
        assert!(result.is_sufficient);
        assert_eq!(result.min_required, 200);
    }

    #[test]
    fn test_validate_pool_size_insufficient() {
        let result = validate_pool_size(10, 100, 2);
        assert!(!result.is_sufficient);
        assert_eq!(result.min_required, 200);
    }

    #[test]
    fn test_pool_round_robin() {
        let pool = AccountPool::from_seed(42, 0, 3);
        assert_eq!(pool.len(), 3);

        let addr0 = pool.next_account().address();
        let addr1 = pool.next_account().address();
        let addr2 = pool.next_account().address();
        // Wraps around
        let addr3 = pool.next_account().address();

        assert_eq!(addr0, addr3, "round-robin should wrap");
        assert_ne!(addr0, addr1);
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn test_pool_get() {
        let pool = AccountPool::from_seed(42, 0, 5);
        assert!(pool.get(0).is_some());
        assert!(pool.get(4).is_some());
        assert!(pool.get(5).is_none());
    }

    #[test]
    fn test_pool_addresses() {
        let pool = AccountPool::from_seed(42, 0, 5);
        let addrs = pool.addresses();
        assert_eq!(addrs.len(), 5);
    }

    #[test]
    fn test_pool_from_seed_matches_derive() {
        let pool = AccountPool::from_seed(42, 10, 5);
        let direct = derive_accounts(42, 10, 5);
        for (i, acc) in direct.iter().enumerate() {
            assert_eq!(pool.get(i).unwrap().address(), acc.address());
        }
    }
}
