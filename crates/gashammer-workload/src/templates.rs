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

//! Transaction templates — building blocks for gas-weighted workloads.
//!
//! Each template generates calldata for a specific transaction type with
//! a known gas profile. Templates use deterministic PRNG for reproducibility.
//!
//! Ref: RFC-0005 §2.

use alloy::primitives::{Address, Bytes, U256};
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

/// DNA magic bytes appended to calldata (RFC-0011 §5.1).
/// "GHMR" in ASCII = 0x47484D52.
pub const CALLDATA_MAGIC: [u8; 4] = [0x47, 0x48, 0x4D, 0x52];

/// Descriptor for a transaction template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateDescriptor {
    pub name: String,
    pub description: String,
    pub estimated_gas: u64,
    pub gas_variance: u64,
}

/// A built transaction from a template (ready for pipeline submission).
#[derive(Debug, Clone)]
pub struct TemplateOutput {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub estimated_gas: u64,
    pub template_name: String,
}

/// Trait for transaction template parameterizers.
///
/// Each implementation generates concrete transactions from a deterministic
/// PRNG, producing different calldata on each invocation while maintaining
/// reproducibility from the same seed.
pub trait Parameterizer: Send + Sync {
    /// Returns the template descriptor.
    fn descriptor(&self) -> TemplateDescriptor;

    /// Generate the next transaction output.
    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput;
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

/// Simple ETH transfer — 21,000 gas baseline.
pub struct SimpleTransfer;

impl Parameterizer for SimpleTransfer {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "simple-transfer".into(),
            description: "ETH value transfer (21,000 gas baseline)".into(),
            estimated_gas: 21_000,
            gas_variance: 0,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        let value_wei: u64 = rng.gen_range(1_000_000..10_000_000_000);
        TemplateOutput {
            to,
            value: U256::from(value_wei),
            data: Bytes::new(),
            gas_limit: 21_000,
            estimated_gas: 21_000,
            template_name: "simple-transfer".into(),
        }
    }
}

/// No-op self-transfer baseline — 21,000 gas.
pub struct Noop;

impl Parameterizer for Noop {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "noop".into(),
            description: "Self-transfer baseline (21,000 gas)".into(),
            estimated_gas: 21_000,
            gas_variance: 0,
        }
    }

    fn next(&self, _rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::new(),
            gas_limit: 21_000,
            estimated_gas: 21_000,
            template_name: "noop".into(),
        }
    }
}

/// ERC-20 transfer — ~65,000 gas.
pub struct Erc20Transfer;

impl Parameterizer for Erc20Transfer {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "erc20-transfer".into(),
            description: "ERC-20 token transfer (~65,000 gas)".into(),
            estimated_gas: 65_000,
            gas_variance: 5_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        // transfer(address,uint256) selector: 0xa9059cbb
        let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
        // recipient address (padded to 32 bytes)
        let recipient: [u8; 20] = rng.gen();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&recipient);
        // amount (random, padded to 32 bytes)
        let amount: u64 = rng.gen_range(1..1_000_000);
        let mut amount_bytes = [0u8; 32];
        amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());
        data.extend_from_slice(&amount_bytes);
        // DNA magic
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 70_000,
            estimated_gas: 65_000,
            template_name: "erc20-transfer".into(),
        }
    }
}

/// ERC-20 approve — ~46,000 gas.
pub struct Erc20Approve;

impl Parameterizer for Erc20Approve {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "erc20-approve".into(),
            description: "ERC-20 approve (~46,000 gas)".into(),
            estimated_gas: 46_000,
            gas_variance: 2_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        // approve(address,uint256) selector: 0x095ea7b3
        let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
        // spender
        let spender: [u8; 20] = rng.gen();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&spender);
        // amount (max uint256 for infinite approve)
        data.extend_from_slice(&[0xff; 32]);
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 50_000,
            estimated_gas: 46_000,
            template_name: "erc20-approve".into(),
        }
    }
}

/// Storage write — single slot, ~43,000 gas.
pub struct StorageWrite;

impl Parameterizer for StorageWrite {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "storage-write".into(),
            description: "Single storage slot write (~43,000 gas)".into(),
            estimated_gas: 43_000,
            gas_variance: 5_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        // Generic store(uint256,uint256) pattern
        let mut data = vec![0x6d, 0x4c, 0xe6, 0x3c]; // arbitrary selector
        let slot: u64 = rng.gen_range(0..1000);
        let mut slot_bytes = [0u8; 32];
        slot_bytes[24..].copy_from_slice(&slot.to_be_bytes());
        data.extend_from_slice(&slot_bytes);
        let value: [u8; 32] = rng.gen();
        data.extend_from_slice(&value);
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 50_000,
            estimated_gas: 43_000,
            template_name: "storage-write".into(),
        }
    }
}

/// Storage write heavy — 10 slots, ~200,000 gas.
pub struct StorageWriteHeavy;

impl Parameterizer for StorageWriteHeavy {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "storage-write-heavy".into(),
            description: "10 storage slot writes (~200,000 gas)".into(),
            estimated_gas: 200_000,
            gas_variance: 20_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        let mut data = vec![0xba, 0xdc, 0xaf, 0xe0]; // arbitrary selector
                                                     // 10 slot-value pairs
        for i in 0u64..10 {
            let slot = rng.gen_range(0u64..10_000) + i * 10_000;
            let mut slot_bytes = [0u8; 32];
            slot_bytes[24..].copy_from_slice(&slot.to_be_bytes());
            data.extend_from_slice(&slot_bytes);
            let value: [u8; 32] = rng.gen();
            data.extend_from_slice(&value);
        }
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 250_000,
            estimated_gas: 200_000,
            template_name: "storage-write-heavy".into(),
        }
    }
}

/// Compute-heavy — ~500,000 gas burn.
pub struct ComputeHeavy;

impl Parameterizer for ComputeHeavy {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "compute-heavy".into(),
            description: "Compute-heavy gas burn (~500,000 gas)".into(),
            estimated_gas: 500_000,
            gas_variance: 50_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        // burn(uint256) — a contract that loops to burn gas
        let mut data = vec![0x44, 0xdf, 0x8e, 0x70]; // burn selector
        let iterations: u64 = rng.gen_range(1000..5000);
        let mut iter_bytes = [0u8; 32];
        iter_bytes[24..].copy_from_slice(&iterations.to_be_bytes());
        data.extend_from_slice(&iter_bytes);
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 600_000,
            estimated_gas: 500_000,
            template_name: "compute-heavy".into(),
        }
    }
}

/// Calldata-heavy — large input data, ~100,000 gas.
pub struct CalldataHeavy;

impl Parameterizer for CalldataHeavy {
    fn descriptor(&self) -> TemplateDescriptor {
        TemplateDescriptor {
            name: "calldata-heavy".into(),
            description: "Large calldata payload (~100,000 gas)".into(),
            estimated_gas: 100_000,
            gas_variance: 10_000,
        }
    }

    fn next(&self, rng: &mut ChaCha20Rng, to: Address) -> TemplateOutput {
        let mut data = vec![0xde, 0xad, 0xbe, 0xef]; // arbitrary selector
                                                     // ~4KB of random data (each non-zero byte costs 16 gas)
        let payload_size: usize = rng.gen_range(3000..5000);
        let mut payload = vec![0u8; payload_size];
        rng.fill(&mut payload[..]);
        data.extend_from_slice(&payload);
        data.extend_from_slice(&CALLDATA_MAGIC);

        TemplateOutput {
            to,
            value: U256::ZERO,
            data: Bytes::from(data),
            gas_limit: 120_000,
            estimated_gas: 100_000,
            template_name: "calldata-heavy".into(),
        }
    }
}

/// Returns all built-in templates.
pub fn builtin_templates() -> Vec<Box<dyn Parameterizer>> {
    vec![
        Box::new(SimpleTransfer),
        Box::new(Noop),
        Box::new(Erc20Transfer),
        Box::new(Erc20Approve),
        Box::new(StorageWrite),
        Box::new(StorageWriteHeavy),
        Box::new(ComputeHeavy),
        Box::new(CalldataHeavy),
    ]
}

/// Look up a template by name.
pub fn template_by_name(name: &str) -> Option<Box<dyn Parameterizer>> {
    match name {
        "simple-transfer" => Some(Box::new(SimpleTransfer)),
        "noop" => Some(Box::new(Noop)),
        "erc20-transfer" => Some(Box::new(Erc20Transfer)),
        "erc20-approve" => Some(Box::new(Erc20Approve)),
        "storage-write" => Some(Box::new(StorageWrite)),
        "storage-write-heavy" => Some(Box::new(StorageWriteHeavy)),
        "compute-heavy" => Some(Box::new(ComputeHeavy)),
        "calldata-heavy" => Some(Box::new(CalldataHeavy)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn test_rng() -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(42)
    }

    fn test_address() -> Address {
        Address::ZERO
    }

    #[test]
    fn test_simple_transfer() {
        let tmpl = SimpleTransfer;
        let desc = tmpl.descriptor();
        assert_eq!(desc.name, "simple-transfer");
        assert_eq!(desc.estimated_gas, 21_000);

        let output = tmpl.next(&mut test_rng(), test_address());
        assert_eq!(output.gas_limit, 21_000);
        assert!(output.data.is_empty());
        assert!(output.value > U256::ZERO);
    }

    #[test]
    fn test_noop() {
        let tmpl = Noop;
        let output = tmpl.next(&mut test_rng(), test_address());
        assert_eq!(output.value, U256::ZERO);
        assert!(output.data.is_empty());
        assert_eq!(output.estimated_gas, 21_000);
    }

    #[test]
    fn test_erc20_transfer_calldata() {
        let tmpl = Erc20Transfer;
        let output = tmpl.next(&mut test_rng(), test_address());
        // Function selector
        assert_eq!(&output.data[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        // DNA magic at the end
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
        assert_eq!(output.estimated_gas, 65_000);
    }

    #[test]
    fn test_erc20_approve_calldata() {
        let tmpl = Erc20Approve;
        let output = tmpl.next(&mut test_rng(), test_address());
        assert_eq!(&output.data[..4], &[0x09, 0x5e, 0xa7, 0xb3]);
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
    }

    #[test]
    fn test_storage_write() {
        let tmpl = StorageWrite;
        let desc = tmpl.descriptor();
        assert_eq!(desc.estimated_gas, 43_000);

        let output = tmpl.next(&mut test_rng(), test_address());
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
    }

    #[test]
    fn test_storage_write_heavy() {
        let tmpl = StorageWriteHeavy;
        let desc = tmpl.descriptor();
        assert_eq!(desc.estimated_gas, 200_000);

        let output = tmpl.next(&mut test_rng(), test_address());
        // Should have selector + 10 * (32 + 32) bytes + magic
        assert!(output.data.len() > 640);
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
    }

    #[test]
    fn test_compute_heavy() {
        let tmpl = ComputeHeavy;
        let desc = tmpl.descriptor();
        assert_eq!(desc.estimated_gas, 500_000);

        let output = tmpl.next(&mut test_rng(), test_address());
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
    }

    #[test]
    fn test_calldata_heavy() {
        let tmpl = CalldataHeavy;
        let desc = tmpl.descriptor();
        assert_eq!(desc.estimated_gas, 100_000);

        let output = tmpl.next(&mut test_rng(), test_address());
        // Should have large payload
        assert!(output.data.len() > 3000);
        let len = output.data.len();
        assert_eq!(&output.data[len - 4..], &CALLDATA_MAGIC);
    }

    #[test]
    fn test_deterministic_output() {
        let tmpl = Erc20Transfer;
        let addr = test_address();

        let output1 = tmpl.next(&mut ChaCha20Rng::seed_from_u64(42), addr);
        let output2 = tmpl.next(&mut ChaCha20Rng::seed_from_u64(42), addr);

        // Same seed → same output
        assert_eq!(output1.data, output2.data);
    }

    #[test]
    fn test_different_seed_different_output() {
        let tmpl = Erc20Transfer;
        let addr = test_address();

        let output1 = tmpl.next(&mut ChaCha20Rng::seed_from_u64(42), addr);
        let output2 = tmpl.next(&mut ChaCha20Rng::seed_from_u64(99), addr);

        assert_ne!(output1.data, output2.data);
    }

    #[test]
    fn test_builtin_templates_count() {
        let templates = builtin_templates();
        assert_eq!(templates.len(), 8);
    }

    #[test]
    fn test_template_by_name() {
        assert!(template_by_name("simple-transfer").is_some());
        assert!(template_by_name("erc20-transfer").is_some());
        assert!(template_by_name("compute-heavy").is_some());
        assert!(template_by_name("unknown").is_none());
    }

    #[test]
    fn test_all_templates_produce_valid_output() {
        let mut rng = test_rng();
        let addr = test_address();

        for tmpl in builtin_templates() {
            let desc = tmpl.descriptor();
            let output = tmpl.next(&mut rng, addr);
            assert_eq!(output.template_name, desc.name);
            assert!(output.gas_limit >= output.estimated_gas);
        }
    }

    #[test]
    fn test_calldata_magic_constant() {
        assert_eq!(CALLDATA_MAGIC, [0x47, 0x48, 0x4D, 0x52]);
        // "GHMR" in ASCII
        assert_eq!(&CALLDATA_MAGIC, b"GHMR");
    }

    #[test]
    fn test_template_descriptor_serialization() {
        let desc = SimpleTransfer.descriptor();
        let json = serde_json::to_string(&desc).expect("serialize");
        let parsed: TemplateDescriptor = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(desc.name, parsed.name);
        assert_eq!(desc.estimated_gas, parsed.estimated_gas);
    }
}
