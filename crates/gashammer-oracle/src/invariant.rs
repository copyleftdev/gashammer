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

//! Invariant framework — traits, verdicts, and evidence for correctness checks.
//!
//! Every measurable correctness property is expressed as an `Invariant`.
//! Invariants produce `Verdict`s with `Evidence` on violation.
//!
//! Ref: RFC-0007 §2–3.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Check types
// ---------------------------------------------------------------------------

/// When an invariant check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckType {
    /// Runs during the test on each event.
    Live,
    /// Runs after the test completes.
    PostRun,
    /// Runs both live and post-run.
    Both,
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// Evidence collected when an invariant is violated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Human-readable description of the violation.
    pub description: String,
    /// What was expected (stringified).
    pub expected: String,
    /// What was actually observed (stringified).
    pub actual: String,
    /// Transaction hash involved (if applicable).
    pub tx_hash: Option<String>,
    /// Block number involved (if applicable).
    pub block_number: Option<u64>,
    /// Arbitrary raw data for debugging.
    pub raw_data: Option<String>,
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

/// Result of an invariant check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// Name of the invariant that produced this verdict.
    pub check_name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Evidence of violation (present when `passed == false`).
    pub evidence: Option<Evidence>,
}

impl Verdict {
    /// Create a passing verdict.
    pub fn pass(check_name: impl Into<String>) -> Self {
        Self {
            check_name: check_name.into(),
            passed: true,
            evidence: None,
        }
    }

    /// Create a failing verdict with evidence.
    pub fn fail(check_name: impl Into<String>, evidence: Evidence) -> Self {
        Self {
            check_name: check_name.into(),
            passed: false,
            evidence: Some(evidence),
        }
    }
}

// ---------------------------------------------------------------------------
// Observation — input data for invariant checks
// ---------------------------------------------------------------------------

/// Observation data passed to live invariant checks.
///
/// This is a simplified view of a telemetry event that the oracle needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxObservation {
    pub tx_hash: String,
    pub template_name: String,
    pub gas_limit: u64,
    pub gas_used: Option<u64>,
    pub succeeded: Option<bool>,
    pub nonce: u64,
    pub account: String,
    pub block_number: Option<u64>,
}

/// Summary data for post-run checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Account address → (initial_balance, final_balance, total_gas_spent).
    pub account_balances: Vec<AccountBalanceSummary>,
    /// Account address → (initial_nonce, final_nonce, successful_tx_count).
    pub account_nonces: Vec<AccountNonceSummary>,
    /// All feed sequence numbers observed.
    pub feed_sequences: Vec<u64>,
}

/// Balance summary for a single account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalanceSummary {
    pub account: String,
    pub initial_balance: u128,
    pub final_balance: u128,
    pub total_gas_spent: u128,
    pub total_value_sent: u128,
    pub total_value_received: u128,
}

/// Nonce summary for a single account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountNonceSummary {
    pub account: String,
    pub initial_nonce: u64,
    pub final_nonce: u64,
    pub successful_tx_count: u64,
}

// ---------------------------------------------------------------------------
// Invariant trait
// ---------------------------------------------------------------------------

/// Trait for correctness invariants.
///
/// Each invariant has a name, a check type, and methods for live and post-run
/// evaluation. Implement the relevant method(s) based on `check_type()`.
pub trait Invariant: Send + Sync {
    /// Name of this invariant.
    fn name(&self) -> &str;

    /// When this invariant runs.
    fn check_type(&self) -> CheckType;

    /// Check a single transaction observation (live mode).
    /// Default: returns pass.
    fn check_event(&self, _obs: &TxObservation) -> Verdict {
        Verdict::pass(self.name())
    }

    /// Check the final run summary (post-run mode).
    /// Default: returns pass.
    fn check_final(&self, _summary: &RunSummary) -> Vec<Verdict> {
        vec![Verdict::pass(self.name())]
    }
}

// ---------------------------------------------------------------------------
// Built-in live invariants
// ---------------------------------------------------------------------------

/// Receipt status check — flags unexpected reverts.
///
/// A tx is "unexpected revert" if it succeeded=false and the template
/// is not in the declared contention set.
pub struct ReceiptStatusCheck {
    /// Template names expected to have reverts (contention mode).
    pub expected_revert_templates: Vec<String>,
}

impl ReceiptStatusCheck {
    pub fn new(expected_revert_templates: Vec<String>) -> Self {
        Self {
            expected_revert_templates,
        }
    }

    pub fn strict() -> Self {
        Self {
            expected_revert_templates: Vec::new(),
        }
    }
}

impl Invariant for ReceiptStatusCheck {
    fn name(&self) -> &str {
        "receipt_status"
    }

    fn check_type(&self) -> CheckType {
        CheckType::Live
    }

    fn check_event(&self, obs: &TxObservation) -> Verdict {
        if let Some(false) = obs.succeeded {
            if !self.expected_revert_templates.contains(&obs.template_name) {
                return Verdict::fail(
                    self.name(),
                    Evidence {
                        description: "unexpected revert".into(),
                        expected: "success or declared contention revert".into(),
                        actual: "reverted".into(),
                        tx_hash: Some(obs.tx_hash.clone()),
                        block_number: obs.block_number,
                        raw_data: None,
                    },
                );
            }
        }
        Verdict::pass(self.name())
    }
}

/// Gas used bounds check — gas_used within tolerance of template estimate.
pub struct GasUsedBoundsCheck {
    /// Tolerance as a fraction (0.5 = within 50% of estimate).
    pub tolerance: f64,
}

impl GasUsedBoundsCheck {
    pub fn new(tolerance: f64) -> Self {
        Self { tolerance }
    }

    pub fn default_tolerance() -> Self {
        Self { tolerance: 0.5 }
    }
}

impl Invariant for GasUsedBoundsCheck {
    fn name(&self) -> &str {
        "gas_used_bounds"
    }

    fn check_type(&self) -> CheckType {
        CheckType::Live
    }

    fn check_event(&self, obs: &TxObservation) -> Verdict {
        if let Some(gas_used) = obs.gas_used {
            let lower = (obs.gas_limit as f64 * (1.0 - self.tolerance)) as u64;
            // gas_used should not exceed gas_limit (EVM guarantee), but
            // we also check it's not absurdly low compared to estimate
            if gas_used < lower {
                return Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "gas_used {gas_used} below {}% of gas_limit {}",
                            ((1.0 - self.tolerance) * 100.0) as u32,
                            obs.gas_limit
                        ),
                        expected: format!(">= {lower}"),
                        actual: format!("{gas_used}"),
                        tx_hash: Some(obs.tx_hash.clone()),
                        block_number: obs.block_number,
                        raw_data: None,
                    },
                );
            }
        }
        Verdict::pass(self.name())
    }
}

/// Receipt completeness check — every submitted tx must have a receipt or timeout.
///
/// This is evaluated post-run by checking that all tx hashes have outcomes.
pub struct ReceiptCompletenessCheck;

impl Invariant for ReceiptCompletenessCheck {
    fn name(&self) -> &str {
        "receipt_completeness"
    }

    fn check_type(&self) -> CheckType {
        CheckType::Live
    }

    fn check_event(&self, obs: &TxObservation) -> Verdict {
        // In live mode, we flag txs that have no outcome yet.
        // This is a simplified check — the real check tracks pending txs.
        if obs.succeeded.is_none() && obs.gas_used.is_none() {
            return Verdict::fail(
                self.name(),
                Evidence {
                    description: "tx has no receipt or timeout outcome".into(),
                    expected: "receipt or explicit timeout".into(),
                    actual: "no outcome".into(),
                    tx_hash: Some(obs.tx_hash.clone()),
                    block_number: None,
                    raw_data: None,
                },
            );
        }
        Verdict::pass(self.name())
    }
}

/// Sequence monotonicity check — feed sequence numbers strictly increasing.
pub struct SequenceMonotonicityCheck;

impl Invariant for SequenceMonotonicityCheck {
    fn name(&self) -> &str {
        "sequence_monotonicity"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, summary: &RunSummary) -> Vec<Verdict> {
        if summary.feed_sequences.len() < 2 {
            return vec![Verdict::pass(self.name())];
        }
        let mut verdicts = Vec::new();
        for window in summary.feed_sequences.windows(2) {
            if window[1] <= window[0] {
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "feed sequence not monotonic: {} followed by {}",
                            window[0], window[1]
                        ),
                        expected: format!("> {}", window[0]),
                        actual: format!("{}", window[1]),
                        tx_hash: None,
                        block_number: None,
                        raw_data: None,
                    },
                ));
            }
        }
        if verdicts.is_empty() {
            verdicts.push(Verdict::pass(self.name()));
        }
        verdicts
    }
}

// ---------------------------------------------------------------------------
// Built-in post-run invariants
// ---------------------------------------------------------------------------

/// Balance conservation check — sum of balances + gas spent = initial funding.
pub struct BalanceConservationCheck;

impl Invariant for BalanceConservationCheck {
    fn name(&self) -> &str {
        "balance_conservation"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, summary: &RunSummary) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for acct in &summary.account_balances {
            let expected = acct.initial_balance + acct.total_value_received;
            let actual = acct.final_balance + acct.total_gas_spent + acct.total_value_sent;

            if expected != actual {
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!("balance not conserved for {}", acct.account),
                        expected: format!("{expected}"),
                        actual: format!("{actual}"),
                        tx_hash: None,
                        block_number: None,
                        raw_data: Some(serde_json::to_string(acct).unwrap_or_default()),
                    },
                ));
            }
        }
        if verdicts.is_empty() {
            verdicts.push(Verdict::pass(self.name()));
        }
        verdicts
    }
}

/// Nonce consistency check — final nonce = initial + successful tx count.
pub struct NonceConsistencyCheck;

impl Invariant for NonceConsistencyCheck {
    fn name(&self) -> &str {
        "nonce_consistency"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, summary: &RunSummary) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for acct in &summary.account_nonces {
            let expected = acct.initial_nonce + acct.successful_tx_count;
            if acct.final_nonce != expected {
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "nonce inconsistent for {}: expected {} got {}",
                            acct.account, expected, acct.final_nonce
                        ),
                        expected: format!("{expected}"),
                        actual: format!("{}", acct.final_nonce),
                        tx_hash: None,
                        block_number: None,
                        raw_data: Some(serde_json::to_string(acct).unwrap_or_default()),
                    },
                ));
            }
        }
        if verdicts.is_empty() {
            verdicts.push(Verdict::pass(self.name()));
        }
        verdicts
    }
}

// ---------------------------------------------------------------------------
// Oracle runner
// ---------------------------------------------------------------------------

/// Metrics for the oracle.
#[derive(Debug, Default)]
pub struct OracleMetrics {
    pub gashammer_oracle_checks_total: std::sync::atomic::AtomicU64,
    pub gashammer_oracle_violations_total: std::sync::atomic::AtomicU64,
}

impl OracleMetrics {
    pub fn checks(&self) -> u64 {
        self.gashammer_oracle_checks_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn violations(&self) -> u64 {
        self.gashammer_oracle_violations_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The oracle runner evaluates invariants against observations.
pub struct OracleRunner {
    invariants: Vec<Box<dyn Invariant>>,
    metrics: OracleMetrics,
}

impl OracleRunner {
    /// Create a runner with the given invariants.
    pub fn new(invariants: Vec<Box<dyn Invariant>>) -> Self {
        Self {
            invariants,
            metrics: OracleMetrics::default(),
        }
    }

    /// Create a runner with all default invariants.
    pub fn default_invariants() -> Self {
        Self::new(vec![
            Box::new(ReceiptStatusCheck::strict()),
            Box::new(GasUsedBoundsCheck::default_tolerance()),
            Box::new(ReceiptCompletenessCheck),
            Box::new(SequenceMonotonicityCheck),
            Box::new(BalanceConservationCheck),
            Box::new(NonceConsistencyCheck),
        ])
    }

    /// Reference to oracle metrics.
    pub fn metrics(&self) -> &OracleMetrics {
        &self.metrics
    }

    /// Check a single observation against all live invariants.
    pub fn check_event(&self, obs: &TxObservation) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for inv in &self.invariants {
            match inv.check_type() {
                CheckType::Live | CheckType::Both => {
                    let verdict = inv.check_event(obs);
                    self.metrics
                        .gashammer_oracle_checks_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if !verdict.passed {
                        self.metrics
                            .gashammer_oracle_violations_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    verdicts.push(verdict);
                }
                CheckType::PostRun => {}
            }
        }
        verdicts
    }

    /// Run all post-run invariants against the summary.
    pub fn check_final(&self, summary: &RunSummary) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for inv in &self.invariants {
            match inv.check_type() {
                CheckType::PostRun | CheckType::Both => {
                    let results = inv.check_final(summary);
                    for verdict in results {
                        self.metrics
                            .gashammer_oracle_checks_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if !verdict.passed {
                            self.metrics
                                .gashammer_oracle_violations_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        verdicts.push(verdict);
                    }
                }
                CheckType::Live => {}
            }
        }
        verdicts
    }

    /// Names of all registered invariants.
    pub fn invariant_names(&self) -> Vec<&str> {
        self.invariants.iter().map(|i| i.name()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_observation() -> TxObservation {
        TxObservation {
            tx_hash: "0xabc".into(),
            template_name: "simple-transfer".into(),
            gas_limit: 21_000,
            gas_used: Some(21_000),
            succeeded: Some(true),
            nonce: 0,
            account: "0x123".into(),
            block_number: Some(100),
        }
    }

    fn reverted_observation() -> TxObservation {
        TxObservation {
            tx_hash: "0xdef".into(),
            template_name: "simple-transfer".into(),
            gas_limit: 21_000,
            gas_used: Some(21_000),
            succeeded: Some(false),
            nonce: 1,
            account: "0x123".into(),
            block_number: Some(101),
        }
    }

    fn passing_summary() -> RunSummary {
        RunSummary {
            account_balances: vec![AccountBalanceSummary {
                account: "0x123".into(),
                initial_balance: 1_000_000,
                final_balance: 900_000,
                total_gas_spent: 21_000,
                total_value_sent: 79_000,
                total_value_received: 0,
            }],
            account_nonces: vec![AccountNonceSummary {
                account: "0x123".into(),
                initial_nonce: 0,
                final_nonce: 5,
                successful_tx_count: 5,
            }],
            feed_sequences: vec![1, 2, 3, 4, 5],
        }
    }

    // ── ReceiptStatusCheck ──

    #[test]
    fn test_receipt_status_pass_on_success() {
        let check = ReceiptStatusCheck::strict();
        let verdict = check.check_event(&passing_observation());
        assert!(verdict.passed);
    }

    #[test]
    fn test_receipt_status_fail_on_unexpected_revert() {
        let check = ReceiptStatusCheck::strict();
        let verdict = check.check_event(&reverted_observation());
        assert!(!verdict.passed);
        assert!(verdict.evidence.is_some());
        assert!(verdict
            .evidence
            .unwrap()
            .description
            .contains("unexpected revert"));
    }

    #[test]
    fn test_receipt_status_pass_on_expected_revert() {
        let check = ReceiptStatusCheck::new(vec!["simple-transfer".into()]);
        let verdict = check.check_event(&reverted_observation());
        assert!(verdict.passed);
    }

    // ── GasUsedBoundsCheck ──

    #[test]
    fn test_gas_bounds_pass_normal() {
        let check = GasUsedBoundsCheck::default_tolerance();
        let verdict = check.check_event(&passing_observation());
        assert!(verdict.passed);
    }

    #[test]
    fn test_gas_bounds_fail_too_low() {
        let check = GasUsedBoundsCheck::new(0.5);
        let obs = TxObservation {
            gas_used: Some(5_000), // way below 50% of 21,000
            ..passing_observation()
        };
        let verdict = check.check_event(&obs);
        assert!(!verdict.passed);
    }

    #[test]
    fn test_gas_bounds_pass_when_no_gas_used() {
        let check = GasUsedBoundsCheck::default_tolerance();
        let obs = TxObservation {
            gas_used: None,
            ..passing_observation()
        };
        let verdict = check.check_event(&obs);
        assert!(verdict.passed);
    }

    // ── ReceiptCompletenessCheck ──

    #[test]
    fn test_completeness_pass_with_receipt() {
        let check = ReceiptCompletenessCheck;
        let verdict = check.check_event(&passing_observation());
        assert!(verdict.passed);
    }

    #[test]
    fn test_completeness_fail_no_outcome() {
        let check = ReceiptCompletenessCheck;
        let obs = TxObservation {
            succeeded: None,
            gas_used: None,
            ..passing_observation()
        };
        let verdict = check.check_event(&obs);
        assert!(!verdict.passed);
    }

    // ── SequenceMonotonicityCheck ──

    #[test]
    fn test_sequence_monotonic_pass() {
        let check = SequenceMonotonicityCheck;
        let summary = passing_summary();
        let verdicts = check.check_final(&summary);
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_sequence_monotonic_fail_on_gap() {
        let check = SequenceMonotonicityCheck;
        let summary = RunSummary {
            feed_sequences: vec![1, 2, 5, 4, 6],
            ..passing_summary()
        };
        let verdicts = check.check_final(&summary);
        let violations: Vec<_> = verdicts.iter().filter(|v| !v.passed).collect();
        assert_eq!(violations.len(), 1);
        assert!(violations[0]
            .evidence
            .as_ref()
            .unwrap()
            .description
            .contains("not monotonic"));
    }

    #[test]
    fn test_sequence_monotonic_pass_empty() {
        let check = SequenceMonotonicityCheck;
        let summary = RunSummary {
            feed_sequences: vec![],
            ..passing_summary()
        };
        let verdicts = check.check_final(&summary);
        assert!(verdicts.iter().all(|v| v.passed));
    }

    // ── BalanceConservationCheck ──

    #[test]
    fn test_balance_conservation_pass() {
        let check = BalanceConservationCheck;
        let verdicts = check.check_final(&passing_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_balance_conservation_fail() {
        let check = BalanceConservationCheck;
        let summary = RunSummary {
            account_balances: vec![AccountBalanceSummary {
                account: "0x123".into(),
                initial_balance: 1_000_000,
                final_balance: 999_999, // off by 1
                total_gas_spent: 0,
                total_value_sent: 0,
                total_value_received: 0,
            }],
            ..passing_summary()
        };
        let verdicts = check.check_final(&summary);
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── NonceConsistencyCheck ──

    #[test]
    fn test_nonce_consistency_pass() {
        let check = NonceConsistencyCheck;
        let verdicts = check.check_final(&passing_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_nonce_consistency_fail() {
        let check = NonceConsistencyCheck;
        let summary = RunSummary {
            account_nonces: vec![AccountNonceSummary {
                account: "0x123".into(),
                initial_nonce: 0,
                final_nonce: 4, // should be 5
                successful_tx_count: 5,
            }],
            ..passing_summary()
        };
        let verdicts = check.check_final(&summary);
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── OracleRunner ──

    #[test]
    fn test_oracle_runner_default_invariants() {
        let runner = OracleRunner::default_invariants();
        assert_eq!(runner.invariant_names().len(), 6);
    }

    #[test]
    fn test_oracle_runner_live_checks() {
        let runner = OracleRunner::default_invariants();
        let verdicts = runner.check_event(&passing_observation());
        // 3 live checks: receipt_status, gas_used_bounds, receipt_completeness
        assert_eq!(verdicts.len(), 3);
        assert!(verdicts.iter().all(|v| v.passed));
        assert_eq!(runner.metrics().checks(), 3);
        assert_eq!(runner.metrics().violations(), 0);
    }

    #[test]
    fn test_oracle_runner_live_checks_with_violation() {
        let runner = OracleRunner::default_invariants();
        let verdicts = runner.check_event(&reverted_observation());
        let violations: Vec<_> = verdicts.iter().filter(|v| !v.passed).collect();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].check_name, "receipt_status");
        assert_eq!(runner.metrics().violations(), 1);
    }

    #[test]
    fn test_oracle_runner_post_run_checks() {
        let runner = OracleRunner::default_invariants();
        let verdicts = runner.check_final(&passing_summary());
        // 3 post-run checks: sequence_monotonicity, balance_conservation, nonce_consistency
        assert_eq!(verdicts.len(), 3);
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_verdict_serialization() {
        let verdict = Verdict::fail(
            "test_check",
            Evidence {
                description: "test fail".into(),
                expected: "true".into(),
                actual: "false".into(),
                tx_hash: Some("0xabc".into()),
                block_number: Some(42),
                raw_data: None,
            },
        );
        let json = serde_json::to_string(&verdict).expect("serialize");
        let parsed: Verdict = serde_json::from_str(&json).expect("deserialize");
        assert!(!parsed.passed);
        assert_eq!(parsed.check_name, "test_check");
    }

    #[test]
    fn test_evidence_serialization() {
        let evidence = Evidence {
            description: "test".into(),
            expected: "a".into(),
            actual: "b".into(),
            tx_hash: None,
            block_number: None,
            raw_data: Some("{\"key\":\"val\"}".into()),
        };
        let json = serde_json::to_string(&evidence).expect("serialize");
        let parsed: Evidence = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.description, "test");
    }
}
