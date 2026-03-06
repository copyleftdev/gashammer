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
// GasHammer is a registered trademark of Don Johnson.

//! Test contract bytecode and deployment helpers.
//!
//! Embeds pre-compiled Solidity bytecode as const byte arrays so that
//! no Solidity toolchain is needed at test time. Source lives in
//! `contracts/test/`. Rebuild with `cargo xtask build-contracts`.
//!
//! Ref: RFC-0013 §10.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Contract descriptors
// ---------------------------------------------------------------------------

/// Describes a test contract with its name, bytecode, and ABI selectors.
#[derive(Debug, Clone)]
pub struct ContractDescriptor {
    /// Contract name.
    pub name: &'static str,
    /// Creation bytecode (init code).
    pub bytecode: &'static [u8],
    /// Function selectors: name → 4-byte selector.
    pub selectors: &'static [(&'static str, [u8; 4])],
}

/// Addresses of deployed test contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestContracts {
    pub counter: [u8; 20],
    pub erc20: [u8; 20],
    pub gas_burner: [u8; 20],
    pub storage_writer: [u8; 20],
    pub reverter: [u8; 20],
    pub event_emitter: [u8; 20],
    pub contention_target: [u8; 20],
}

impl TestContracts {
    /// Create a TestContracts with placeholder zero addresses.
    pub fn empty() -> Self {
        Self {
            counter: [0u8; 20],
            erc20: [0u8; 20],
            gas_burner: [0u8; 20],
            storage_writer: [0u8; 20],
            reverter: [0u8; 20],
            event_emitter: [0u8; 20],
            contention_target: [0u8; 20],
        }
    }

    /// Returns all contracts as (name, address) pairs.
    pub fn all(&self) -> Vec<(&str, [u8; 20])> {
        vec![
            ("Counter", self.counter),
            ("GasHammerERC20", self.erc20),
            ("GasBurner", self.gas_burner),
            ("StorageWriter", self.storage_writer),
            ("Reverter", self.reverter),
            ("EventEmitter", self.event_emitter),
            ("ContentionTarget", self.contention_target),
        ]
    }

    /// Check if all contracts have been deployed (non-zero addresses).
    pub fn all_deployed(&self) -> bool {
        self.all().iter().all(|(_, addr)| addr != &[0u8; 20])
    }

    /// Get a contract address by name.
    pub fn address_of(&self, name: &str) -> Option<[u8; 20]> {
        self.all()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, addr)| addr)
    }
}

// ---------------------------------------------------------------------------
// Function selectors (keccak256 first 4 bytes)
// ---------------------------------------------------------------------------

/// Counter function selectors.
pub const COUNTER_SELECTORS: &[(&str, [u8; 4])] = &[
    ("increment()", [0xd0, 0x9d, 0xe0, 0x8a]),
    ("decrement()", [0x2b, 0xae, 0xc5, 0x60]),
    ("reset()", [0xd8, 0x26, 0xf8, 0x8f]),
    ("count()", [0x06, 0x66, 0x1a, 0xbd]),
    // incrementBy(uint256)
    ("incrementBy(uint256)", [0x30, 0x96, 0x26, 0x33]),
];

/// GasHammerERC20 function selectors.
pub const ERC20_SELECTORS: &[(&str, [u8; 4])] = &[
    // transfer(address,uint256)
    ("transfer(address,uint256)", [0xa9, 0x05, 0x9c, 0xbb]),
    // approve(address,uint256)
    ("approve(address,uint256)", [0x09, 0x5e, 0xa7, 0xb3]),
    // transferFrom(address,address,uint256)
    (
        "transferFrom(address,address,uint256)",
        [0x23, 0xb8, 0x72, 0xdd],
    ),
    // mint(address,uint256)
    ("mint(address,uint256)", [0x40, 0xc1, 0x0f, 0x19]),
    // balanceOf(address)
    ("balanceOf(address)", [0x70, 0xa0, 0x82, 0x31]),
];

/// GasBurner function selectors.
pub const GAS_BURNER_SELECTORS: &[(&str, [u8; 4])] = &[
    ("burn(uint256)", [0x89, 0xaf, 0xcb, 0x44]),
    ("lastBurned()", [0x5e, 0x03, 0x96, 0x89]),
];

/// StorageWriter function selectors.
pub const STORAGE_WRITER_SELECTORS: &[(&str, [u8; 4])] = &[
    ("writeSlots(uint256)", [0x42, 0xd2, 0x1e, 0xf7]),
    ("readSlot(uint256)", [0x23, 0xe7, 0xa0, 0xc4]),
    ("writeCount()", [0x84, 0xb3, 0x5a, 0xc9]),
];

/// Reverter function selectors.
pub const REVERTER_SELECTORS: &[(&str, [u8; 4])] = &[
    ("revertIf(bool)", [0x06, 0xfa, 0xec, 0xb0]),
    ("alwaysRevert()", [0x39, 0x2e, 0x53, 0xcd]),
    ("revertEmpty()", [0xd1, 0x5d, 0x2e, 0x6e]),
    ("callCount()", [0x87, 0xc3, 0xcd, 0x3e]),
];

/// EventEmitter function selectors.
pub const EVENT_EMITTER_SELECTORS: &[(&str, [u8; 4])] = &[
    // emit_(bytes32,bytes)
    ("emit_(bytes32,bytes)", [0xef, 0x78, 0xd4, 0xfd]),
    ("ping()", [0x5c, 0x36, 0xb1, 0x86]),
    ("emitCount()", [0x5c, 0x15, 0xbd, 0xf2]),
];

/// ContentionTarget function selectors.
pub const CONTENTION_TARGET_SELECTORS: &[(&str, [u8; 4])] = &[
    ("contribute(uint256)", [0xc1, 0xcf, 0xb9, 0x9a]),
    ("swap(uint256)", [0x65, 0x04, 0x0b, 0x22]),
    ("value()", [0x3f, 0xa4, 0xf2, 0x45]),
];

// ---------------------------------------------------------------------------
// Compiled bytecodes (from contracts/test/build/*.bin)
// ---------------------------------------------------------------------------

/// Compiled bytecode for Counter.sol.
pub const COUNTER_BYTECODE: &[u8] = include_bytes!("../../../contracts/test/build/Counter.bin");

/// Compiled bytecode for GasHammerERC20.sol.
pub const ERC20_BYTECODE: &[u8] =
    include_bytes!("../../../contracts/test/build/GasHammerERC20.bin");

/// Compiled bytecode for GasBurner.sol.
pub const GAS_BURNER_BYTECODE: &[u8] =
    include_bytes!("../../../contracts/test/build/GasBurner.bin");

/// Compiled bytecode for StorageWriter.sol.
pub const STORAGE_WRITER_BYTECODE: &[u8] =
    include_bytes!("../../../contracts/test/build/StorageWriter.bin");

/// Compiled bytecode for Reverter.sol.
pub const REVERTER_BYTECODE: &[u8] = include_bytes!("../../../contracts/test/build/Reverter.bin");

/// Compiled bytecode for EventEmitter.sol.
pub const EVENT_EMITTER_BYTECODE: &[u8] =
    include_bytes!("../../../contracts/test/build/EventEmitter.bin");

/// Compiled bytecode for ContentionTarget.sol.
pub const CONTENTION_TARGET_BYTECODE: &[u8] =
    include_bytes!("../../../contracts/test/build/ContentionTarget.bin");

// ---------------------------------------------------------------------------
// Contract registry
// ---------------------------------------------------------------------------

/// Returns descriptors for all test contracts.
pub fn test_contract_descriptors() -> Vec<ContractDescriptor> {
    vec![
        ContractDescriptor {
            name: "Counter",
            bytecode: COUNTER_BYTECODE,
            selectors: COUNTER_SELECTORS,
        },
        ContractDescriptor {
            name: "GasHammerERC20",
            bytecode: ERC20_BYTECODE,
            selectors: ERC20_SELECTORS,
        },
        ContractDescriptor {
            name: "GasBurner",
            bytecode: GAS_BURNER_BYTECODE,
            selectors: GAS_BURNER_SELECTORS,
        },
        ContractDescriptor {
            name: "StorageWriter",
            bytecode: STORAGE_WRITER_BYTECODE,
            selectors: STORAGE_WRITER_SELECTORS,
        },
        ContractDescriptor {
            name: "Reverter",
            bytecode: REVERTER_BYTECODE,
            selectors: REVERTER_SELECTORS,
        },
        ContractDescriptor {
            name: "EventEmitter",
            bytecode: EVENT_EMITTER_BYTECODE,
            selectors: EVENT_EMITTER_SELECTORS,
        },
        ContractDescriptor {
            name: "ContentionTarget",
            bytecode: CONTENTION_TARGET_BYTECODE,
            selectors: CONTENTION_TARGET_SELECTORS,
        },
    ]
}

/// Returns a map of contract name → descriptor.
pub fn test_contract_map() -> HashMap<&'static str, ContractDescriptor> {
    test_contract_descriptors()
        .into_iter()
        .map(|d| (d.name, d))
        .collect()
}

/// Encode a function call: 4-byte selector + ABI-encoded arguments.
///
/// For simple uint256 arguments, pass the value as a 32-byte big-endian array.
pub fn encode_call(selector: [u8; 4], args: &[[u8; 32]]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + args.len() * 32);
    data.extend_from_slice(&selector);
    for arg in args {
        data.extend_from_slice(arg);
    }
    data
}

/// Encode a uint256 value as a 32-byte big-endian array.
pub fn encode_uint256(value: u64) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[24..32].copy_from_slice(&value.to_be_bytes());
    buf
}

/// Encode an address as a 32-byte ABI-encoded value (left-padded).
pub fn encode_address(addr: [u8; 20]) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[12..32].copy_from_slice(&addr);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_descriptors_count() {
        let descriptors = test_contract_descriptors();
        assert_eq!(descriptors.len(), 7);
    }

    #[test]
    fn test_contract_descriptors_names() {
        let descriptors = test_contract_descriptors();
        let names: Vec<&str> = descriptors.iter().map(|d| d.name).collect();
        assert!(names.contains(&"Counter"));
        assert!(names.contains(&"GasHammerERC20"));
        assert!(names.contains(&"GasBurner"));
        assert!(names.contains(&"StorageWriter"));
        assert!(names.contains(&"Reverter"));
        assert!(names.contains(&"EventEmitter"));
        assert!(names.contains(&"ContentionTarget"));
    }

    #[test]
    fn test_contract_map_lookup() {
        let map = super::test_contract_map();
        assert_eq!(map.len(), 7);
        assert!(map.contains_key("Counter"));
        assert!(map.contains_key("GasHammerERC20"));
    }

    #[test]
    fn test_contract_has_bytecode() {
        for desc in test_contract_descriptors() {
            assert!(
                !desc.bytecode.is_empty(),
                "{} has empty bytecode",
                desc.name
            );
        }
    }

    #[test]
    fn test_contract_has_selectors() {
        for desc in test_contract_descriptors() {
            assert!(!desc.selectors.is_empty(), "{} has no selectors", desc.name);
        }
    }

    #[test]
    fn test_test_contracts_empty() {
        let tc = TestContracts::empty();
        assert!(!tc.all_deployed());
        assert_eq!(tc.all().len(), 7);
    }

    #[test]
    fn test_test_contracts_all_deployed() {
        let tc = TestContracts {
            counter: [1u8; 20],
            erc20: [2u8; 20],
            gas_burner: [3u8; 20],
            storage_writer: [4u8; 20],
            reverter: [5u8; 20],
            event_emitter: [6u8; 20],
            contention_target: [7u8; 20],
        };
        assert!(tc.all_deployed());
    }

    #[test]
    fn test_test_contracts_address_of() {
        let tc = TestContracts {
            counter: [0xAA; 20],
            erc20: [0xBB; 20],
            gas_burner: [0u8; 20],
            storage_writer: [0u8; 20],
            reverter: [0u8; 20],
            event_emitter: [0u8; 20],
            contention_target: [0u8; 20],
        };
        assert_eq!(tc.address_of("Counter"), Some([0xAA; 20]));
        assert_eq!(tc.address_of("GasHammerERC20"), Some([0xBB; 20]));
        assert_eq!(tc.address_of("NonExistent"), None);
    }

    #[test]
    fn test_test_contracts_serialization() {
        let tc = TestContracts::empty();
        let json = serde_json::to_string(&tc).expect("serialize");
        let parsed: TestContracts = serde_json::from_str(&json).expect("deserialize");
        assert!(!parsed.all_deployed());
    }

    #[test]
    fn test_encode_call_no_args() {
        let selector = [0x06, 0x66, 0x1a, 0xbd]; // count()
        let data = encode_call(selector, &[]);
        assert_eq!(data.len(), 4);
        assert_eq!(data, selector);
    }

    #[test]
    fn test_encode_call_with_uint256() {
        let selector = [0xd0, 0x9d, 0xe0, 0x8a]; // increment()
        let arg = encode_uint256(42);
        let data = encode_call(selector, &[arg]);
        assert_eq!(data.len(), 36); // 4 + 32
        assert_eq!(&data[0..4], &selector);
        assert_eq!(data[35], 42);
    }

    #[test]
    fn test_encode_uint256() {
        let encoded = encode_uint256(256);
        assert_eq!(encoded[30], 1);
        assert_eq!(encoded[31], 0);
        // First 24 bytes should be zero
        assert!(encoded[..24].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_encode_address() {
        let addr = [0xAB; 20];
        let encoded = encode_address(addr);
        assert!(encoded[..12].iter().all(|&b| b == 0));
        assert_eq!(&encoded[12..], &addr);
    }

    #[test]
    fn test_encode_call_with_address_and_uint() {
        // transfer(address,uint256)
        let selector = [0xa9, 0x05, 0x9c, 0xbb];
        let to = encode_address([0xDE; 20]);
        let amount = encode_uint256(1000);
        let data = encode_call(selector, &[to, amount]);
        assert_eq!(data.len(), 68); // 4 + 32 + 32
    }

    #[test]
    fn test_counter_selectors() {
        assert_eq!(COUNTER_SELECTORS.len(), 5);
        assert_eq!(COUNTER_SELECTORS[0].0, "increment()");
    }

    #[test]
    fn test_erc20_selectors() {
        assert_eq!(ERC20_SELECTORS.len(), 5);
    }

    #[test]
    fn test_all_selectors_are_4_bytes() {
        let all_selectors: Vec<&[(&str, [u8; 4])]> = vec![
            COUNTER_SELECTORS,
            ERC20_SELECTORS,
            GAS_BURNER_SELECTORS,
            STORAGE_WRITER_SELECTORS,
            REVERTER_SELECTORS,
            EVENT_EMITTER_SELECTORS,
            CONTENTION_TARGET_SELECTORS,
        ];
        for selectors in all_selectors {
            for (name, sel) in selectors {
                assert_eq!(sel.len(), 4, "{name} selector is not 4 bytes");
            }
        }
    }
}
