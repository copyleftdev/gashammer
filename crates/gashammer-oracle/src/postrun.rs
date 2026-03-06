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

//! Post-run checks — state diff, ordering, cross-observer verification.
//!
//! These checks run after the workload completes and verify properties
//! that can only be assessed with full run data.
//!
//! Ref: RFC-0007 §3.2–3.4.

use serde::{Deserialize, Serialize};

use crate::invariant::{CheckType, Evidence, Invariant, Verdict};

// ---------------------------------------------------------------------------
// Check summary & aggregated verdict
// ---------------------------------------------------------------------------

/// Summary of a single check across all observations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub check_name: String,
    pub total_checked: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    /// Capped at 100 to prevent unbounded growth.
    pub violations: Vec<Verdict>,
}

/// Maximum violations stored per check.
const MAX_VIOLATIONS_PER_CHECK: usize = 100;

impl CheckSummary {
    pub fn new(check_name: impl Into<String>) -> Self {
        Self {
            check_name: check_name.into(),
            total_checked: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            violations: Vec::new(),
        }
    }

    pub fn record(&mut self, verdict: Verdict) {
        self.total_checked += 1;
        if verdict.passed {
            self.passed += 1;
        } else {
            self.failed += 1;
            if self.violations.len() < MAX_VIOLATIONS_PER_CHECK {
                self.violations.push(verdict);
            }
        }
    }

    pub fn record_skip(&mut self) {
        self.skipped += 1;
    }
}

/// Aggregated correctness verdict for an entire run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectnessVerdict {
    pub overall_pass: bool,
    pub summaries: Vec<CheckSummary>,
}

impl CorrectnessVerdict {
    /// Aggregate check summaries into a final verdict.
    pub fn from_summaries(summaries: Vec<CheckSummary>) -> Self {
        let overall_pass = summaries.iter().all(|s| s.failed == 0);
        Self {
            overall_pass,
            summaries,
        }
    }
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

/// Deterministic sampling based on hash of tx_hash.
/// Returns true if the item should be checked.
pub fn should_sample(tx_hash: &str, sample_rate: f64) -> bool {
    if sample_rate >= 1.0 {
        return true;
    }
    if sample_rate <= 0.0 {
        return false;
    }
    // Simple deterministic hash: sum of bytes mod 10000
    let hash_val: u64 = tx_hash.bytes().map(|b| b as u64).sum();
    let threshold = (sample_rate * 10000.0) as u64;
    (hash_val % 10000) < threshold
}

// ---------------------------------------------------------------------------
// Post-run data types
// ---------------------------------------------------------------------------

/// A submitted transaction record for ordering checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub tx_hash: String,
    pub account: String,
    pub nonce: u64,
    pub block_number: u64,
    pub tx_index: u64,
    pub timestamp: u64,
}

/// Storage slot observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSlot {
    pub contract: String,
    pub slot: String,
    pub expected_value: String,
    pub actual_value: String,
}

/// Cross-observer receipt comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptComparison {
    pub tx_hash: String,
    pub observer_a: String,
    pub observer_b: String,
    pub status_match: bool,
    pub gas_used_match: bool,
    pub block_match: bool,
}

/// Block record for timestamp ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub block_number: u64,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Post-run invariants
// ---------------------------------------------------------------------------

/// Verify known storage slots contain expected values after the run.
pub struct StorageIntegrityCheck {
    pub slots: Vec<StorageSlot>,
}

impl StorageIntegrityCheck {
    pub fn new(slots: Vec<StorageSlot>) -> Self {
        Self { slots }
    }
}

impl Invariant for StorageIntegrityCheck {
    fn name(&self) -> &str {
        "storage_integrity"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, _summary: &crate::invariant::RunSummary) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for slot in &self.slots {
            if slot.expected_value != slot.actual_value {
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "storage slot {} on {} has wrong value",
                            slot.slot, slot.contract
                        ),
                        expected: slot.expected_value.clone(),
                        actual: slot.actual_value.clone(),
                        tx_hash: None,
                        block_number: None,
                        raw_data: Some(serde_json::to_string(slot).unwrap_or_default()),
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

/// Verify transactions from the same account are included in nonce order.
pub struct IntraAccountOrderingCheck {
    pub tx_records: Vec<TxRecord>,
}

impl IntraAccountOrderingCheck {
    pub fn new(tx_records: Vec<TxRecord>) -> Self {
        Self { tx_records }
    }
}

impl Invariant for IntraAccountOrderingCheck {
    fn name(&self) -> &str {
        "intra_account_ordering"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, _summary: &crate::invariant::RunSummary) -> Vec<Verdict> {
        use std::collections::HashMap;

        // Group by account
        let mut by_account: HashMap<&str, Vec<&TxRecord>> = HashMap::new();
        for tx in &self.tx_records {
            by_account.entry(&tx.account).or_default().push(tx);
        }

        let mut verdicts = Vec::new();
        for (account, mut txs) in by_account {
            // Sort by (block_number, tx_index) — inclusion order
            txs.sort_by_key(|t| (t.block_number, t.tx_index));

            for window in txs.windows(2) {
                if window[1].nonce < window[0].nonce {
                    verdicts.push(Verdict::fail(
                        self.name(),
                        Evidence {
                            description: format!(
                                "account {account}: nonce {} included before nonce {}",
                                window[1].nonce, window[0].nonce
                            ),
                            expected: format!(
                                "nonce {} before {}",
                                window[0].nonce, window[1].nonce
                            ),
                            actual: format!(
                                "nonce {} at block {}:{}, nonce {} at block {}:{}",
                                window[0].nonce,
                                window[0].block_number,
                                window[0].tx_index,
                                window[1].nonce,
                                window[1].block_number,
                                window[1].tx_index
                            ),
                            tx_hash: Some(window[1].tx_hash.clone()),
                            block_number: Some(window[1].block_number),
                            raw_data: None,
                        },
                    ));
                }
            }
        }

        if verdicts.is_empty() {
            verdicts.push(Verdict::pass(self.name()));
        }
        verdicts
    }
}

/// Verify block timestamps are non-decreasing.
pub struct TimestampOrderingCheck {
    pub blocks: Vec<BlockRecord>,
}

impl TimestampOrderingCheck {
    pub fn new(blocks: Vec<BlockRecord>) -> Self {
        Self { blocks }
    }
}

impl Invariant for TimestampOrderingCheck {
    fn name(&self) -> &str {
        "timestamp_ordering"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, _summary: &crate::invariant::RunSummary) -> Vec<Verdict> {
        let mut sorted = self.blocks.clone();
        sorted.sort_by_key(|b| b.block_number);

        let mut verdicts = Vec::new();
        for window in sorted.windows(2) {
            if window[1].timestamp < window[0].timestamp {
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "block {} timestamp {} < block {} timestamp {}",
                            window[1].block_number,
                            window[1].timestamp,
                            window[0].block_number,
                            window[0].timestamp
                        ),
                        expected: format!(">= {}", window[0].timestamp),
                        actual: format!("{}", window[1].timestamp),
                        tx_hash: None,
                        block_number: Some(window[1].block_number),
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

/// Cross-observer receipt comparison — same tx returns identical receipts from different RPCs.
pub struct CrossObserverReceiptCheck {
    pub comparisons: Vec<ReceiptComparison>,
}

impl CrossObserverReceiptCheck {
    pub fn new(comparisons: Vec<ReceiptComparison>) -> Self {
        Self { comparisons }
    }
}

impl Invariant for CrossObserverReceiptCheck {
    fn name(&self) -> &str {
        "cross_observer_receipt"
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, _summary: &crate::invariant::RunSummary) -> Vec<Verdict> {
        let mut verdicts = Vec::new();
        for cmp in &self.comparisons {
            if !cmp.status_match || !cmp.gas_used_match || !cmp.block_match {
                let mut mismatches = Vec::new();
                if !cmp.status_match {
                    mismatches.push("status");
                }
                if !cmp.gas_used_match {
                    mismatches.push("gas_used");
                }
                if !cmp.block_match {
                    mismatches.push("block");
                }
                verdicts.push(Verdict::fail(
                    self.name(),
                    Evidence {
                        description: format!(
                            "receipt mismatch for {} between {} and {}: {}",
                            cmp.tx_hash,
                            cmp.observer_a,
                            cmp.observer_b,
                            mismatches.join(", ")
                        ),
                        expected: "identical receipts".into(),
                        actual: format!("mismatched: {}", mismatches.join(", ")),
                        tx_hash: Some(cmp.tx_hash.clone()),
                        block_number: None,
                        raw_data: Some(serde_json::to_string(cmp).unwrap_or_default()),
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

/// User-defined contract invariant check (scenario YAML).
pub struct CustomContractInvariant {
    pub check_name: String,
    pub description: String,
    /// Pre-evaluated result (the actual contract call is done externally).
    pub result_pass: bool,
    pub evidence: Option<Evidence>,
}

impl CustomContractInvariant {
    pub fn new(
        check_name: impl Into<String>,
        description: impl Into<String>,
        result_pass: bool,
        evidence: Option<Evidence>,
    ) -> Self {
        Self {
            check_name: check_name.into(),
            description: description.into(),
            result_pass,
            evidence,
        }
    }
}

impl Invariant for CustomContractInvariant {
    fn name(&self) -> &str {
        &self.check_name
    }

    fn check_type(&self) -> CheckType {
        CheckType::PostRun
    }

    fn check_final(&self, _summary: &crate::invariant::RunSummary) -> Vec<Verdict> {
        if self.result_pass {
            vec![Verdict::pass(self.name())]
        } else {
            vec![Verdict::fail(
                self.name(),
                self.evidence.clone().unwrap_or(Evidence {
                    description: self.description.clone(),
                    expected: "pass".into(),
                    actual: "fail".into(),
                    tx_hash: None,
                    block_number: None,
                    raw_data: None,
                }),
            )]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invariant::RunSummary;

    fn empty_summary() -> RunSummary {
        RunSummary {
            account_balances: Vec::new(),
            account_nonces: Vec::new(),
            feed_sequences: Vec::new(),
        }
    }

    // ── CheckSummary ──

    #[test]
    fn test_check_summary_tracking() {
        let mut summary = CheckSummary::new("test");
        summary.record(Verdict::pass("test"));
        summary.record(Verdict::pass("test"));
        summary.record(Verdict::fail(
            "test",
            Evidence {
                description: "fail".into(),
                expected: "a".into(),
                actual: "b".into(),
                tx_hash: None,
                block_number: None,
                raw_data: None,
            },
        ));
        summary.record_skip();

        assert_eq!(summary.total_checked, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.violations.len(), 1);
    }

    #[test]
    fn test_check_summary_violation_cap() {
        let mut summary = CheckSummary::new("test");
        for _ in 0..200 {
            summary.record(Verdict::fail(
                "test",
                Evidence {
                    description: "fail".into(),
                    expected: "a".into(),
                    actual: "b".into(),
                    tx_hash: None,
                    block_number: None,
                    raw_data: None,
                },
            ));
        }
        assert_eq!(summary.failed, 200);
        assert_eq!(summary.violations.len(), MAX_VIOLATIONS_PER_CHECK);
    }

    // ── CorrectnessVerdict ──

    #[test]
    fn test_correctness_verdict_all_pass() {
        let mut s1 = CheckSummary::new("a");
        s1.record(Verdict::pass("a"));
        let mut s2 = CheckSummary::new("b");
        s2.record(Verdict::pass("b"));

        let verdict = CorrectnessVerdict::from_summaries(vec![s1, s2]);
        assert!(verdict.overall_pass);
    }

    #[test]
    fn test_correctness_verdict_one_fail() {
        let mut s1 = CheckSummary::new("a");
        s1.record(Verdict::pass("a"));
        let mut s2 = CheckSummary::new("b");
        s2.record(Verdict::fail(
            "b",
            Evidence {
                description: "fail".into(),
                expected: "".into(),
                actual: "".into(),
                tx_hash: None,
                block_number: None,
                raw_data: None,
            },
        ));

        let verdict = CorrectnessVerdict::from_summaries(vec![s1, s2]);
        assert!(!verdict.overall_pass);
    }

    // ── Sampling ──

    #[test]
    fn test_sampling_always() {
        assert!(should_sample("0xabc", 1.0));
        assert!(should_sample("0xdef", 1.0));
    }

    #[test]
    fn test_sampling_never() {
        assert!(!should_sample("0xabc", 0.0));
    }

    #[test]
    fn test_sampling_deterministic() {
        let result1 = should_sample("0xabc123", 0.5);
        let result2 = should_sample("0xabc123", 0.5);
        assert_eq!(result1, result2);
    }

    // ── StorageIntegrityCheck ──

    #[test]
    fn test_storage_integrity_pass() {
        let check = StorageIntegrityCheck::new(vec![StorageSlot {
            contract: "0x1".into(),
            slot: "0x0".into(),
            expected_value: "42".into(),
            actual_value: "42".into(),
        }]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_storage_integrity_fail() {
        let check = StorageIntegrityCheck::new(vec![StorageSlot {
            contract: "0x1".into(),
            slot: "0x0".into(),
            expected_value: "42".into(),
            actual_value: "99".into(),
        }]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── IntraAccountOrderingCheck ──

    #[test]
    fn test_ordering_pass() {
        let check = IntraAccountOrderingCheck::new(vec![
            TxRecord {
                tx_hash: "0xa".into(),
                account: "0x1".into(),
                nonce: 0,
                block_number: 100,
                tx_index: 0,
                timestamp: 1000,
            },
            TxRecord {
                tx_hash: "0xb".into(),
                account: "0x1".into(),
                nonce: 1,
                block_number: 100,
                tx_index: 1,
                timestamp: 1000,
            },
            TxRecord {
                tx_hash: "0xc".into(),
                account: "0x1".into(),
                nonce: 2,
                block_number: 101,
                tx_index: 0,
                timestamp: 1001,
            },
        ]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_ordering_fail_nonce_out_of_order() {
        let check = IntraAccountOrderingCheck::new(vec![
            TxRecord {
                tx_hash: "0xa".into(),
                account: "0x1".into(),
                nonce: 1, // higher nonce included first
                block_number: 100,
                tx_index: 0,
                timestamp: 1000,
            },
            TxRecord {
                tx_hash: "0xb".into(),
                account: "0x1".into(),
                nonce: 0, // lower nonce included after
                block_number: 100,
                tx_index: 1,
                timestamp: 1000,
            },
        ]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── TimestampOrderingCheck ──

    #[test]
    fn test_timestamp_pass() {
        let check = TimestampOrderingCheck::new(vec![
            BlockRecord {
                block_number: 100,
                timestamp: 1000,
            },
            BlockRecord {
                block_number: 101,
                timestamp: 1000,
            }, // same is ok
            BlockRecord {
                block_number: 102,
                timestamp: 1001,
            },
        ]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_timestamp_fail_decreasing() {
        let check = TimestampOrderingCheck::new(vec![
            BlockRecord {
                block_number: 100,
                timestamp: 1001,
            },
            BlockRecord {
                block_number: 101,
                timestamp: 1000,
            }, // decreased
        ]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── CrossObserverReceiptCheck ──

    #[test]
    fn test_cross_observer_pass() {
        let check = CrossObserverReceiptCheck::new(vec![ReceiptComparison {
            tx_hash: "0xabc".into(),
            observer_a: "sequencer".into(),
            observer_b: "gateway".into(),
            status_match: true,
            gas_used_match: true,
            block_match: true,
        }]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_cross_observer_fail_status_mismatch() {
        let check = CrossObserverReceiptCheck::new(vec![ReceiptComparison {
            tx_hash: "0xabc".into(),
            observer_a: "sequencer".into(),
            observer_b: "gateway".into(),
            status_match: false,
            gas_used_match: true,
            block_match: true,
        }]);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── CustomContractInvariant ──

    #[test]
    fn test_custom_invariant_pass() {
        let check =
            CustomContractInvariant::new("token_supply", "total supply unchanged", true, None);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().all(|v| v.passed));
    }

    #[test]
    fn test_custom_invariant_fail() {
        let check =
            CustomContractInvariant::new("token_supply", "total supply unchanged", false, None);
        let verdicts = check.check_final(&empty_summary());
        assert!(verdicts.iter().any(|v| !v.passed));
    }

    // ── Serialization ──

    #[test]
    fn test_correctness_verdict_serialization() {
        let mut s = CheckSummary::new("test");
        s.record(Verdict::pass("test"));
        let verdict = CorrectnessVerdict::from_summaries(vec![s]);
        let json = serde_json::to_string(&verdict).expect("serialize");
        let parsed: CorrectnessVerdict = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.overall_pass);
    }
}
