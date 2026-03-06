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

//! Full lifecycle test framework — boot devnet, run scenario, verify results.
//!
//! Provides the `LifecycleTest` builder for composing end-to-end tests that
//! exercise the complete GasHammer pipeline: devnet → hive → edge → scenario
//! → telemetry → oracle → report.
//!
//! Ref: RFC-0013 §6.3.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::contracts::TestContracts;

// ---------------------------------------------------------------------------
// Lifecycle test configuration
// ---------------------------------------------------------------------------

/// Configuration for a full lifecycle test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Scenario YAML to execute.
    pub scenario_yaml: String,
    /// Number of edge instances to start.
    pub edge_count: u32,
    /// Maximum time to wait for the run to complete.
    pub timeout: Duration,
    /// Whether to deploy test contracts.
    pub deploy_contracts: bool,
    /// Whether to capture a diagnostics bundle on failure.
    pub capture_diagnostics: bool,
    /// Expected run outcome.
    pub expected_outcome: ExpectedOutcome,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            scenario_yaml: String::new(),
            edge_count: 1,
            timeout: Duration::from_secs(300),
            deploy_contracts: true,
            capture_diagnostics: true,
            expected_outcome: ExpectedOutcome::Pass,
        }
    }
}

/// Expected outcome of a lifecycle test run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// Run should complete with Pass verdict.
    Pass,
    /// Run should complete with Fail verdict.
    Fail,
    /// Run should complete (any outcome).
    AnyCompletion,
}

// ---------------------------------------------------------------------------
// Lifecycle test result
// ---------------------------------------------------------------------------

/// Result of a full lifecycle test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResult {
    /// Whether the test passed.
    pub passed: bool,
    /// Run outcome (pass/fail/error).
    pub run_outcome: String,
    /// Duration of the entire test (including setup).
    pub total_duration: Duration,
    /// Duration of the scenario execution only.
    pub run_duration: Duration,
    /// Deployed test contracts (if any).
    pub contracts: Option<TestContracts>,
    /// Verification results.
    pub verifications: Vec<Verification>,
    /// Diagnostics bundle path (if captured).
    pub diagnostics_path: Option<String>,
    /// Errors encountered.
    pub errors: Vec<String>,
}

/// A single verification check within a lifecycle test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Lifecycle test builder
// ---------------------------------------------------------------------------

/// A named verification function.
type VerifierFn = Box<dyn Fn(&LifecycleResult) -> bool + Send + Sync>;

/// Builder for composing lifecycle tests.
///
/// Usage:
/// ```ignore
/// let result = LifecycleTestBuilder::new()
///     .scenario_yaml(include_str!("../../scenarios/quick-smoke-test.yaml"))
///     .edge_count(1)
///     .timeout(Duration::from_secs(120))
///     .expect_pass()
///     .verify("report_has_provenance", |result| {
///         // check report DNA block
///     })
///     .run()
///     .await;
/// ```
pub struct LifecycleTestBuilder {
    config: LifecycleConfig,
    verifiers: Vec<(&'static str, VerifierFn)>,
}

impl LifecycleTestBuilder {
    pub fn new() -> Self {
        Self {
            config: LifecycleConfig::default(),
            verifiers: Vec::new(),
        }
    }

    pub fn scenario_yaml(mut self, yaml: &str) -> Self {
        self.config.scenario_yaml = yaml.into();
        self
    }

    pub fn edge_count(mut self, count: u32) -> Self {
        self.config.edge_count = count;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    pub fn deploy_contracts(mut self, deploy: bool) -> Self {
        self.config.deploy_contracts = deploy;
        self
    }

    pub fn expect_pass(mut self) -> Self {
        self.config.expected_outcome = ExpectedOutcome::Pass;
        self
    }

    pub fn expect_fail(mut self) -> Self {
        self.config.expected_outcome = ExpectedOutcome::Fail;
        self
    }

    pub fn expect_any_completion(mut self) -> Self {
        self.config.expected_outcome = ExpectedOutcome::AnyCompletion;
        self
    }

    pub fn verify<F>(mut self, name: &'static str, check: F) -> Self
    where
        F: Fn(&LifecycleResult) -> bool + Send + Sync + 'static,
    {
        self.verifiers.push((name, Box::new(check)));
        self
    }

    /// Build the test configuration.
    ///
    /// Note: `run()` is not yet implemented — it requires the hive and edge
    /// runtime components from M2. This builder prepares the configuration
    /// and verifiers; actual execution will be wired up when the runtime
    /// components are complete.
    pub fn build(self) -> (LifecycleConfig, Vec<&'static str>) {
        let verifier_names: Vec<&'static str> = self.verifiers.iter().map(|(n, _)| *n).collect();
        (self.config, verifier_names)
    }
}

impl Default for LifecycleTestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in verification helpers
// ---------------------------------------------------------------------------

/// Standard verifications applied to every lifecycle test.
pub fn standard_verifications() -> Vec<(&'static str, String)> {
    vec![
        (
            "run_completed",
            "Run reached terminal state (Completed or Failed)".into(),
        ),
        (
            "report_generated",
            "Report was generated with non-empty content".into(),
        ),
        (
            "report_has_provenance",
            "Report contains DNA provenance block".into(),
        ),
        (
            "report_has_latency",
            "Report contains latency percentiles".into(),
        ),
        (
            "report_has_throughput",
            "Report contains throughput data".into(),
        ),
        (
            "oracle_checks_ran",
            "At least one oracle check executed".into(),
        ),
        (
            "telemetry_captured",
            "Telemetry events were captured".into(),
        ),
        ("no_edge_crashes", "No edges crashed during the run".into()),
    ]
}

/// Verifications specific to correctness-focused scenarios.
pub fn correctness_verifications() -> Vec<(&'static str, String)> {
    let mut v = standard_verifications();
    v.extend(vec![
        ("oracle_all_passed", "All oracle checks passed".into()),
        (
            "balance_conservation",
            "ETH balance conservation verified".into(),
        ),
        ("nonce_monotonicity", "Nonce monotonicity verified".into()),
    ]);
    v
}

/// Verifications specific to fault injection scenarios.
pub fn fault_verifications() -> Vec<(&'static str, String)> {
    let mut v = standard_verifications();
    v.extend(vec![
        (
            "faults_injected",
            "All scheduled faults were injected".into(),
        ),
        (
            "faults_cleared",
            "All faults were cleared at run end".into(),
        ),
        (
            "recovery_observed",
            "System recovered after fault clearance".into(),
        ),
    ]);
    v
}

// ---------------------------------------------------------------------------
// Diagnostics bundle
// ---------------------------------------------------------------------------

/// Contents of a diagnostics bundle captured on test failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsBundle {
    /// Test configuration.
    pub config: LifecycleConfig,
    /// Container logs (name → log text).
    pub container_logs: HashMap<String, String>,
    /// Hive logs.
    pub hive_log: String,
    /// Edge logs (edge_id → log text).
    pub edge_logs: HashMap<String, String>,
    /// Telemetry event count by type.
    pub event_counts: HashMap<String, u64>,
    /// Oracle verdicts.
    pub oracle_verdicts: Vec<String>,
    /// Report JSON (if generated).
    pub report_json: Option<String>,
    /// Timestamp of bundle capture.
    pub captured_at: String,
}

impl DiagnosticsBundle {
    /// Create an empty diagnostics bundle.
    pub fn empty(config: LifecycleConfig) -> Self {
        Self {
            config,
            container_logs: HashMap::new(),
            hive_log: String::new(),
            edge_logs: HashMap::new(),
            event_counts: HashMap::new(),
            oracle_verdicts: Vec::new(),
            report_json: None,
            captured_at: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_config_default() {
        let config = LifecycleConfig::default();
        assert_eq!(config.edge_count, 1);
        assert_eq!(config.timeout, Duration::from_secs(300));
        assert!(config.deploy_contracts);
        assert!(config.capture_diagnostics);
        assert_eq!(config.expected_outcome, ExpectedOutcome::Pass);
    }

    #[test]
    fn test_lifecycle_builder_basic() {
        let (config, verifiers) = LifecycleTestBuilder::new()
            .scenario_yaml("version: '1'\nscenario:\n  name: test")
            .edge_count(2)
            .timeout(Duration::from_secs(120))
            .expect_pass()
            .build();

        assert_eq!(config.edge_count, 2);
        assert_eq!(config.timeout, Duration::from_secs(120));
        assert!(config.scenario_yaml.contains("test"));
        assert!(verifiers.is_empty());
    }

    #[test]
    fn test_lifecycle_builder_with_verifiers() {
        let (_, verifiers) = LifecycleTestBuilder::new()
            .scenario_yaml("test")
            .verify("check_1", |_| true)
            .verify("check_2", |_| false)
            .build();

        assert_eq!(verifiers.len(), 2);
        assert_eq!(verifiers[0], "check_1");
        assert_eq!(verifiers[1], "check_2");
    }

    #[test]
    fn test_lifecycle_builder_expect_fail() {
        let (config, _) = LifecycleTestBuilder::new()
            .scenario_yaml("test")
            .expect_fail()
            .build();
        assert_eq!(config.expected_outcome, ExpectedOutcome::Fail);
    }

    #[test]
    fn test_lifecycle_builder_expect_any() {
        let (config, _) = LifecycleTestBuilder::new()
            .scenario_yaml("test")
            .expect_any_completion()
            .build();
        assert_eq!(config.expected_outcome, ExpectedOutcome::AnyCompletion);
    }

    #[test]
    fn test_lifecycle_result_structure() {
        let result = LifecycleResult {
            passed: true,
            run_outcome: "Pass".into(),
            total_duration: Duration::from_secs(60),
            run_duration: Duration::from_secs(30),
            contracts: Some(TestContracts::empty()),
            verifications: vec![Verification {
                name: "check_1".into(),
                passed: true,
                message: "ok".into(),
            }],
            diagnostics_path: None,
            errors: vec![],
        };
        assert!(result.passed);
        assert_eq!(result.verifications.len(), 1);
    }

    #[test]
    fn test_standard_verifications() {
        let v = standard_verifications();
        assert_eq!(v.len(), 8);
        assert!(v.iter().any(|(name, _)| *name == "run_completed"));
        assert!(v.iter().any(|(name, _)| *name == "report_has_provenance"));
    }

    #[test]
    fn test_correctness_verifications() {
        let v = correctness_verifications();
        assert!(v.len() > standard_verifications().len());
        assert!(v.iter().any(|(name, _)| *name == "balance_conservation"));
    }

    #[test]
    fn test_fault_verifications() {
        let v = fault_verifications();
        assert!(v.len() > standard_verifications().len());
        assert!(v.iter().any(|(name, _)| *name == "faults_injected"));
    }

    #[test]
    fn test_diagnostics_bundle_empty() {
        let bundle = DiagnosticsBundle::empty(LifecycleConfig::default());
        assert!(bundle.container_logs.is_empty());
        assert!(bundle.edge_logs.is_empty());
        assert!(bundle.report_json.is_none());
    }

    #[test]
    fn test_diagnostics_bundle_serialization() {
        let bundle = DiagnosticsBundle::empty(LifecycleConfig::default());
        let json = serde_json::to_string(&bundle).expect("serialize");
        let parsed: DiagnosticsBundle = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.container_logs.is_empty());
    }

    #[test]
    fn test_lifecycle_config_serialization() {
        let config = LifecycleConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: LifecycleConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.edge_count, 1);
    }

    #[test]
    fn test_lifecycle_result_with_errors() {
        let result = LifecycleResult {
            passed: false,
            run_outcome: "Error".into(),
            total_duration: Duration::from_secs(10),
            run_duration: Duration::from_secs(0),
            contracts: None,
            verifications: vec![],
            diagnostics_path: Some("/tmp/diag-001.json".into()),
            errors: vec!["devnet failed to start".into()],
        };
        assert!(!result.passed);
        assert!(!result.errors.is_empty());
        assert!(result.diagnostics_path.is_some());
    }
}
