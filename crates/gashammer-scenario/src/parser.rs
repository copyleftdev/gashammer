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

//! SDL parser — YAML schema definition and deserialization.
//!
//! Defines the `ScenarioDefinition` struct tree and parses scenario
//! YAML files into it via serde.
//!
//! Ref: RFC-0010 §2–3.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level scenario definition
// ---------------------------------------------------------------------------

/// A complete scenario definition as parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDefinition {
    /// SDL version (e.g., "1").
    pub version: String,
    /// Scenario metadata.
    pub scenario: ScenarioMeta,
    /// Target chain configuration.
    pub target: TargetConfig,
    /// Phase definitions (ordered).
    pub phases: Vec<PhaseDefinition>,
    /// Edge configuration.
    #[serde(default)]
    pub edges: EdgeConfig,
    /// Workload configuration.
    pub workload: WorkloadConfig,
    /// Fault injection schedule (optional).
    #[serde(default)]
    pub faults: Option<FaultConfig>,
    /// Correctness oracle configuration (optional).
    #[serde(default)]
    pub correctness: Option<CorrectnessConfig>,
    /// Regression detection configuration (optional).
    #[serde(default)]
    pub regression: Option<RegressionConfig>,
}

// ---------------------------------------------------------------------------
// Scenario metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Master PRNG seed for determinism.
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_seed() -> u64 {
    42
}

// ---------------------------------------------------------------------------
// Target chain
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    /// Chain type (e.g., "nitro").
    #[serde(default = "default_chain_type")]
    pub chain_type: String,
    /// Sequencer RPC URL.
    pub sequencer_url: String,
    /// Gateway RPC URL (optional, uses sequencer if absent).
    #[serde(default)]
    pub gateway_url: Option<String>,
    /// Feed relay WebSocket URL (optional).
    #[serde(default)]
    pub feed_url: Option<String>,
    /// L1 RPC URL (optional).
    #[serde(default)]
    pub l1_url: Option<String>,
    /// Chain ID (for tx signing).
    #[serde(default)]
    pub chain_id: Option<u64>,
    /// Environment label (e.g., "devnet", "testnet").
    #[serde(default = "default_environment")]
    pub environment: String,
}

fn default_chain_type() -> String {
    "nitro".into()
}

fn default_environment() -> String {
    "devnet".into()
}

// ---------------------------------------------------------------------------
// Phase definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDefinition {
    pub name: String,
    /// Duration in seconds.
    pub duration_sec: u64,
    /// Gas rate override for this phase (optional).
    #[serde(default)]
    pub gas_rate: Option<GasRateConfig>,
    /// Template weight overrides for this phase (optional).
    #[serde(default)]
    pub template_weights: Option<HashMap<String, f64>>,
}

/// Gas rate configuration (maps to GasRateMode).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GasRateConfig {
    Sustained {
        gas_per_sec: u64,
    },
    Ramped {
        start_gas_per_sec: u64,
        end_gas_per_sec: u64,
    },
    Bursty {
        baseline_gas_per_sec: u64,
        burst_peak_gas_per_sec: u64,
        burst_duration_sec: u64,
        burst_interval_sec: u64,
    },
}

// ---------------------------------------------------------------------------
// Edge configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Minimum number of edges required.
    #[serde(default = "default_min_edges")]
    pub min_edges: u32,
    /// Regional allocation: region_name → percentage (0–100).
    #[serde(default)]
    pub regions: HashMap<String, u32>,
    /// Account pool size.
    #[serde(default = "default_account_count")]
    pub account_count: u32,
}

fn default_min_edges() -> u32 {
    1
}

fn default_account_count() -> u32 {
    100
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            min_edges: default_min_edges(),
            regions: HashMap::new(),
            account_count: default_account_count(),
        }
    }
}

// ---------------------------------------------------------------------------
// Workload configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadConfig {
    /// Default gas rate.
    pub gas_rate: GasRateConfig,
    /// Template weights: name → weight.
    pub templates: HashMap<String, f64>,
}

// ---------------------------------------------------------------------------
// Fault configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultConfig {
    #[serde(default)]
    pub schedule: Vec<FaultScheduleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultScheduleEntry {
    pub offset_sec: u64,
    pub action: String, // "inject" or "clear"
    pub fault_name: String,
    #[serde(default)]
    pub fault_type: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub parameters: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Correctness configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectnessConfig {
    /// Which invariants to enable.
    #[serde(default)]
    pub invariants: Vec<String>,
    /// Expected revert rate (0.0–1.0) for contention modes.
    #[serde(default)]
    pub expected_revert_rate: Option<f64>,
    /// Cross-observer sample rate (0.0–1.0).
    #[serde(default)]
    pub cross_observer_sample_rate: Option<f64>,
}

// ---------------------------------------------------------------------------
// Regression configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionConfig {
    /// Path to baseline report for comparison.
    #[serde(default)]
    pub baseline_path: Option<String>,
    /// Maximum allowed p99 latency regression (percentage).
    #[serde(default)]
    pub max_p99_regression_pct: Option<f64>,
    /// Maximum allowed throughput regression (percentage).
    #[serde(default)]
    pub max_throughput_regression_pct: Option<f64>,
}

// ---------------------------------------------------------------------------
// Parse function
// ---------------------------------------------------------------------------

/// Parse a YAML string into a ScenarioDefinition.
pub fn parse_yaml(yaml: &str) -> Result<ScenarioDefinition, String> {
    serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SCENARIO: &str = r#"
version: "1"
scenario:
  name: test-scenario
  description: A minimal test scenario
target:
  sequencer_url: "http://localhost:8547"
phases:
  - name: warmup
    duration_sec: 10
  - name: main
    duration_sec: 60
workload:
  gas_rate:
    mode: sustained
    gas_per_sec: 1000000
  templates:
    simple-transfer: 0.7
    noop: 0.3
"#;

    const FULL_SCENARIO: &str = r#"
version: "1"
scenario:
  name: full-scenario
  description: Full featured scenario
  tags: [load-test, regression]
  seed: 12345
target:
  chain_type: nitro
  sequencer_url: "http://localhost:8547"
  gateway_url: "http://localhost:8548"
  feed_url: "ws://localhost:9642"
  chain_id: 412346
  environment: devnet
phases:
  - name: warmup
    duration_sec: 10
    gas_rate:
      mode: ramped
      start_gas_per_sec: 100000
      end_gas_per_sec: 1000000
  - name: main
    duration_sec: 60
    gas_rate:
      mode: sustained
      gas_per_sec: 1000000
  - name: burst
    duration_sec: 30
    gas_rate:
      mode: bursty
      baseline_gas_per_sec: 500000
      burst_peak_gas_per_sec: 2000000
      burst_duration_sec: 5
      burst_interval_sec: 15
edges:
  min_edges: 2
  regions:
    us-east-1: 60
    eu-west-1: 40
  account_count: 500
workload:
  gas_rate:
    mode: sustained
    gas_per_sec: 1000000
  templates:
    simple-transfer: 0.5
    erc20-transfer: 0.3
    storage-write: 0.2
faults:
  schedule:
    - offset_sec: 30
      action: inject
      fault_name: latency
      fault_type: network_latency
      target: "127.0.0.1"
      parameters:
        delay_ms: "100"
    - offset_sec: 60
      action: clear
      fault_name: latency
correctness:
  invariants:
    - receipt_status
    - gas_used_bounds
    - balance_conservation
  expected_revert_rate: 0.05
  cross_observer_sample_rate: 0.1
regression:
  baseline_path: "./baselines/latest.json"
  max_p99_regression_pct: 10.0
  max_throughput_regression_pct: 5.0
"#;

    #[test]
    fn test_parse_minimal() {
        let def = parse_yaml(MINIMAL_SCENARIO).expect("should parse");
        assert_eq!(def.version, "1");
        assert_eq!(def.scenario.name, "test-scenario");
        assert_eq!(def.phases.len(), 2);
        assert_eq!(def.workload.templates.len(), 2);
        assert_eq!(def.scenario.seed, 42); // default
    }

    #[test]
    fn test_parse_full() {
        let def = parse_yaml(FULL_SCENARIO).expect("should parse");
        assert_eq!(def.scenario.name, "full-scenario");
        assert_eq!(def.scenario.seed, 12345);
        assert_eq!(def.scenario.tags.len(), 2);
        assert_eq!(def.phases.len(), 3);
        assert_eq!(def.target.chain_id, Some(412346));
        assert_eq!(def.edges.min_edges, 2);
        assert_eq!(def.edges.regions.len(), 2);
        assert_eq!(def.edges.account_count, 500);

        let faults = def.faults.as_ref().unwrap();
        assert_eq!(faults.schedule.len(), 2);

        let corr = def.correctness.as_ref().unwrap();
        assert_eq!(corr.invariants.len(), 3);

        let reg = def.regression.as_ref().unwrap();
        assert_eq!(reg.max_p99_regression_pct, Some(10.0));
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let result = parse_yaml("{{invalid yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_field() {
        let result = parse_yaml(
            r#"
version: "1"
scenario:
  name: test
target:
  sequencer_url: "http://localhost:8547"
phases: []
"#,
        );
        // Missing workload field
        assert!(result.is_err());
    }

    #[test]
    fn test_edge_config_defaults() {
        let def = parse_yaml(MINIMAL_SCENARIO).expect("should parse");
        assert_eq!(def.edges.min_edges, 1);
        assert_eq!(def.edges.account_count, 100);
        assert!(def.edges.regions.is_empty());
    }

    #[test]
    fn test_gas_rate_modes() {
        let def = parse_yaml(FULL_SCENARIO).expect("should parse");
        // Phase 0: ramped
        assert!(matches!(
            def.phases[0].gas_rate,
            Some(GasRateConfig::Ramped { .. })
        ));
        // Phase 1: sustained
        assert!(matches!(
            def.phases[1].gas_rate,
            Some(GasRateConfig::Sustained { .. })
        ));
        // Phase 2: bursty
        assert!(matches!(
            def.phases[2].gas_rate,
            Some(GasRateConfig::Bursty { .. })
        ));
    }

    #[test]
    fn test_round_trip_serialization() {
        let def = parse_yaml(FULL_SCENARIO).expect("should parse");
        let json = serde_json::to_string(&def).expect("serialize");
        let _parsed: ScenarioDefinition = serde_json::from_str(&json).expect("deserialize");
    }
}
