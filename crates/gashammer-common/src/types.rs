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

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Generates a newtype wrapper around `Uuid` with standard trait impls.
macro_rules! uuid_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Creates an identifier from a raw `Uuid`.
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the inner `Uuid`.
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::from_str(s).map(Self)
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

uuid_newtype!(
    /// Unique identifier for a test run.
    RunId
);

uuid_newtype!(
    /// Unique identifier for an edge worker.
    EdgeId
);

uuid_newtype!(
    /// Unique identifier for a scenario definition.
    ScenarioId
);

uuid_newtype!(
    /// Unique identifier for a phase within a run.
    PhaseId
);

/// Generates a newtype wrapper around `u64` with standard trait impls.
macro_rules! u64_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Creates a new value.
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the inner `u64`.
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = std::num::ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                u64::from_str(s).map(Self)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

u64_newtype!(
    /// Target gas consumption rate in gas units per second.
    GasRate
);

u64_newtype!(
    /// Count of transactions.
    TxCount
);

#[cfg(test)]
mod tests {
    use super::*;

    use proptest::prelude::*;

    #[test]
    fn test_run_id_display_roundtrip() {
        let id = RunId::new();
        let s = id.to_string();
        let parsed: RunId = s.parse().expect("should parse back");
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_gas_rate_display_roundtrip() {
        let rate = GasRate::new(30_000_000);
        let s = rate.to_string();
        let parsed: GasRate = s.parse().expect("should parse back");
        assert_eq!(rate, parsed);
        assert_eq!(rate.get(), 30_000_000);
    }

    #[test]
    fn test_uuid_newtype_json_transparent() {
        let id = RunId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let parsed: RunId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, parsed);
        // JSON should be a plain UUID string, not an object
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
    }

    #[test]
    fn test_u64_newtype_json_transparent() {
        let count = TxCount::new(42);
        let json = serde_json::to_string(&count).expect("serialize");
        assert_eq!(json, "42");
        let parsed: TxCount = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(count, parsed);
    }

    #[test]
    fn test_gas_rate_ordering() {
        let low = GasRate::new(1_000_000);
        let high = GasRate::new(50_000_000);
        assert!(low < high);
    }

    #[test]
    fn test_uuid_from_into() {
        let uuid = Uuid::new_v4();
        let edge_id = EdgeId::from(uuid);
        let back: Uuid = edge_id.into();
        assert_eq!(uuid, back);
    }

    proptest! {
        #[test]
        fn prop_gas_rate_json_roundtrip(value: u64) {
            let rate = GasRate::new(value);
            let json = serde_json::to_string(&rate).unwrap();
            let parsed: GasRate = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(rate, parsed);
        }

        #[test]
        fn prop_tx_count_json_roundtrip(value: u64) {
            let count = TxCount::new(value);
            let json = serde_json::to_string(&count).unwrap();
            let parsed: TxCount = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(count, parsed);
        }

        #[test]
        fn prop_gas_rate_display_roundtrip(value: u64) {
            let rate = GasRate::new(value);
            let s = rate.to_string();
            let parsed: GasRate = s.parse().unwrap();
            prop_assert_eq!(rate, parsed);
        }

        #[test]
        fn prop_tx_count_display_roundtrip(value: u64) {
            let count = TxCount::new(value);
            let s = count.to_string();
            let parsed: TxCount = s.parse().unwrap();
            prop_assert_eq!(count, parsed);
        }

        #[test]
        fn prop_run_id_json_roundtrip(a: u128) {
            let id = RunId::from_uuid(Uuid::from_u128(a));
            let json = serde_json::to_string(&id).unwrap();
            let parsed: RunId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(id, parsed);
        }

        #[test]
        fn prop_edge_id_json_roundtrip(a: u128) {
            let id = EdgeId::from_uuid(Uuid::from_u128(a));
            let json = serde_json::to_string(&id).unwrap();
            let parsed: EdgeId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(id, parsed);
        }

        #[test]
        fn prop_scenario_id_json_roundtrip(a: u128) {
            let id = ScenarioId::from_uuid(Uuid::from_u128(a));
            let json = serde_json::to_string(&id).unwrap();
            let parsed: ScenarioId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(id, parsed);
        }

        #[test]
        fn prop_phase_id_json_roundtrip(a: u128) {
            let id = PhaseId::from_uuid(Uuid::from_u128(a));
            let json = serde_json::to_string(&id).unwrap();
            let parsed: PhaseId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(id, parsed);
        }
    }
}
