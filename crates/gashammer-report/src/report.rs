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

//! Standard run report — structured output from a completed load test run.
//!
//! Produces decision-grade reports answering "is this build safe to ship?"
//! Supports JSON (machine-readable) and Markdown (human-readable) formats.
//!
//! Ref: RFC-0009 §2, §6.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DNA Provenance
// ---------------------------------------------------------------------------

/// Report provenance metadata (DNA, RFC-0011 §6.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportProvenance {
    pub generator: String,
    pub version: String,
    pub build_sha: String,
    pub url: String,
    pub copyright: String,
    pub license: String,
    pub report_id: Uuid,
    pub generated_at: String,
}

impl Default for ReportProvenance {
    fn default() -> Self {
        Self {
            generator: "GasHammer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            build_sha: "unknown".into(),
            url: "https://github.com/copyleftdev/gashammer".into(),
            copyright: "Copyright (c) 2025-present Don Johnson".into(),
            license: "BSL-1.1".into(),
            report_id: Uuid::new_v4(),
            generated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Run Metadata
// ---------------------------------------------------------------------------

/// Metadata about the completed run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub run_id: Uuid,
    pub scenario_name: String,
    pub seed: u64,
    pub started_at: String,
    pub completed_at: String,
    pub duration_sec: u64,
    pub target_environment: String,
    pub sequencer_url: String,
    pub gashammer_version: String,
}

// ---------------------------------------------------------------------------
// Edge Topology
// ---------------------------------------------------------------------------

/// Summary of the edge topology used in the run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTopology {
    pub total_edges: u32,
    pub edges_by_region: HashMap<String, u32>,
    pub edges_healthy: u32,
    pub edges_lost: u32,
}

// ---------------------------------------------------------------------------
// Phase Timeline
// ---------------------------------------------------------------------------

/// Summary of a single phase within the run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseReport {
    pub name: String,
    pub index: u32,
    pub start_sec: u64,
    pub end_sec: u64,
    pub duration_sec: u64,
    pub target_gas_per_sec: u64,
    pub achieved_gas_per_sec: u64,
    pub tx_submitted: u64,
    pub tx_confirmed: u64,
    pub tx_failed: u64,
    pub fault_events: Vec<FaultEvent>,
}

/// A fault injection event within a phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    pub offset_sec: u64,
    pub action: String,
    pub fault_name: String,
}

// ---------------------------------------------------------------------------
// Latency Summary
// ---------------------------------------------------------------------------

/// Latency percentiles for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p999_ms: f64,
    pub max_ms: f64,
    pub min_ms: f64,
    pub mean_ms: f64,
    pub sample_count: u64,
}

impl LatencyPercentiles {
    /// Compute percentiles from a sorted list of latency samples (in ms).
    ///
    /// The input must already be sorted in ascending order.
    pub fn from_sorted_samples(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self {
                p50_ms: 0.0,
                p90_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                p999_ms: 0.0,
                max_ms: 0.0,
                min_ms: 0.0,
                mean_ms: 0.0,
                sample_count: 0,
            };
        }

        let n = samples.len();
        let sum: f64 = samples.iter().sum();

        Self {
            p50_ms: percentile_at(samples, 0.50),
            p90_ms: percentile_at(samples, 0.90),
            p95_ms: percentile_at(samples, 0.95),
            p99_ms: percentile_at(samples, 0.99),
            p999_ms: percentile_at(samples, 0.999),
            max_ms: samples[n - 1],
            min_ms: samples[0],
            mean_ms: sum / n as f64,
            sample_count: n as u64,
        }
    }
}

/// Nearest-rank percentile computation on sorted data.
fn percentile_at(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    let rank = (p * n as f64).ceil() as usize;
    let idx = rank.min(n).saturating_sub(1);
    sorted[idx]
}

/// Latency breakdown for different measurement points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySummary {
    /// Latency from submission to sequencer acknowledgement.
    pub accept_latency: LatencyPercentiles,
    /// Latency from submission to on-chain inclusion.
    pub include_latency: LatencyPercentiles,
    /// Per-region breakdown of accept latency.
    pub accept_by_region: HashMap<String, LatencyPercentiles>,
    /// Per-region breakdown of include latency.
    pub include_by_region: HashMap<String, LatencyPercentiles>,
}

// ---------------------------------------------------------------------------
// Throughput Summary
// ---------------------------------------------------------------------------

/// A single data point in a time series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp_sec: u64,
    pub value: f64,
}

/// Throughput summary for the run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputSummary {
    /// Average gas/sec submitted across the run.
    pub avg_gas_submitted_per_sec: f64,
    /// Average gas/sec accepted across the run.
    pub avg_gas_accepted_per_sec: f64,
    /// Average gas/sec included on-chain across the run.
    pub avg_gas_included_per_sec: f64,
    /// Average tx/sec submitted.
    pub avg_tx_per_sec: f64,
    /// Total gas submitted.
    pub total_gas_submitted: u64,
    /// Total transactions submitted.
    pub total_tx_submitted: u64,
    /// Total transactions confirmed.
    pub total_tx_confirmed: u64,
    /// Total transactions failed.
    pub total_tx_failed: u64,
    /// Revert rate (0.0–1.0).
    pub revert_rate: f64,
    /// Revert rate by workload class.
    pub revert_rate_by_template: HashMap<String, f64>,
    /// Gas/sec time series (1-second buckets).
    pub gas_per_sec_series: Vec<TimeSeriesPoint>,
    /// Tx/sec time series (1-second buckets).
    pub tx_per_sec_series: Vec<TimeSeriesPoint>,
}

// ---------------------------------------------------------------------------
// Correctness Summary
// ---------------------------------------------------------------------------

/// Summary of correctness oracle results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectnessSummary {
    pub total_checks: u64,
    pub passed: u64,
    pub failed: u64,
    pub inconclusive: u64,
    pub checks: Vec<CheckResult>,
    pub verdict: String,
}

/// Result of a single correctness check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_name: String,
    pub passed: bool,
    pub violation_count: u64,
    pub sample_violations: Vec<String>,
}

// ---------------------------------------------------------------------------
// RunReport — the full standard report
// ---------------------------------------------------------------------------

/// A complete standard run report.
///
/// This is the primary output of every load test run. It answers the question
/// "is this build safe to ship?" with structured, decision-grade data.
///
/// Ref: RFC-0009 §2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    /// DNA provenance metadata.
    pub provenance: ReportProvenance,
    /// Run metadata.
    pub metadata: RunMetadata,
    /// Edge topology snapshot.
    pub edge_topology: EdgeTopology,
    /// Phase-by-phase timeline.
    pub phases: Vec<PhaseReport>,
    /// Latency summary with percentiles.
    pub latency: LatencySummary,
    /// Throughput summary with time series.
    pub throughput: ThroughputSummary,
    /// Correctness oracle results.
    pub correctness: CorrectnessSummary,
    /// Overall run verdict.
    pub verdict: RunVerdict,
}

/// Overall verdict for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunVerdict {
    /// All checks passed, performance within expectations.
    Pass,
    /// Correctness violations or significant performance degradation.
    Fail { reasons: Vec<String> },
    /// Insufficient data to determine outcome.
    Inconclusive { reasons: Vec<String> },
}

// ---------------------------------------------------------------------------
// Report Builder
// ---------------------------------------------------------------------------

/// Builder for constructing a RunReport from telemetry data.
pub struct RunReportBuilder {
    metadata: Option<RunMetadata>,
    edge_topology: Option<EdgeTopology>,
    phases: Vec<PhaseReport>,
    accept_samples: Vec<f64>,
    include_samples: Vec<f64>,
    accept_by_region: HashMap<String, Vec<f64>>,
    include_by_region: HashMap<String, Vec<f64>>,
    throughput: Option<ThroughputSummary>,
    correctness: Option<CorrectnessSummary>,
    provenance: ReportProvenance,
}

impl RunReportBuilder {
    pub fn new() -> Self {
        Self {
            metadata: None,
            edge_topology: None,
            phases: Vec::new(),
            accept_samples: Vec::new(),
            include_samples: Vec::new(),
            accept_by_region: HashMap::new(),
            include_by_region: HashMap::new(),
            throughput: None,
            correctness: None,
            provenance: ReportProvenance::default(),
        }
    }

    pub fn metadata(mut self, metadata: RunMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn edge_topology(mut self, topology: EdgeTopology) -> Self {
        self.edge_topology = Some(topology);
        self
    }

    pub fn add_phase(mut self, phase: PhaseReport) -> Self {
        self.phases.push(phase);
        self
    }

    pub fn accept_latency_samples(mut self, samples: Vec<f64>) -> Self {
        self.accept_samples = samples;
        self
    }

    pub fn include_latency_samples(mut self, samples: Vec<f64>) -> Self {
        self.include_samples = samples;
        self
    }

    pub fn add_regional_accept_samples(mut self, region: String, samples: Vec<f64>) -> Self {
        self.accept_by_region.insert(region, samples);
        self
    }

    pub fn add_regional_include_samples(mut self, region: String, samples: Vec<f64>) -> Self {
        self.include_by_region.insert(region, samples);
        self
    }

    pub fn throughput(mut self, throughput: ThroughputSummary) -> Self {
        self.throughput = Some(throughput);
        self
    }

    pub fn correctness(mut self, correctness: CorrectnessSummary) -> Self {
        self.correctness = Some(correctness);
        self
    }

    pub fn provenance(mut self, provenance: ReportProvenance) -> Self {
        self.provenance = provenance;
        self
    }

    /// Build the final RunReport.
    ///
    /// Returns `Err` if required fields are missing.
    pub fn build(mut self) -> Result<RunReport, String> {
        let metadata = self.metadata.ok_or("metadata is required")?;
        let edge_topology = self.edge_topology.ok_or("edge_topology is required")?;
        let throughput = self.throughput.ok_or("throughput is required")?;
        let correctness = self.correctness.ok_or("correctness is required")?;

        // Sort and compute latency percentiles
        self.accept_samples
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.include_samples
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let accept_latency = LatencyPercentiles::from_sorted_samples(&self.accept_samples);
        let include_latency = LatencyPercentiles::from_sorted_samples(&self.include_samples);

        let accept_by_region: HashMap<String, LatencyPercentiles> = self
            .accept_by_region
            .iter_mut()
            .map(|(region, samples)| {
                samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (
                    region.clone(),
                    LatencyPercentiles::from_sorted_samples(samples),
                )
            })
            .collect();

        let include_by_region: HashMap<String, LatencyPercentiles> = self
            .include_by_region
            .iter_mut()
            .map(|(region, samples)| {
                samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (
                    region.clone(),
                    LatencyPercentiles::from_sorted_samples(samples),
                )
            })
            .collect();

        let latency = LatencySummary {
            accept_latency,
            include_latency,
            accept_by_region,
            include_by_region,
        };

        // Determine verdict
        let verdict = compute_verdict(&correctness, &throughput);

        Ok(RunReport {
            provenance: self.provenance,
            metadata,
            edge_topology,
            phases: self.phases,
            latency,
            throughput,
            correctness,
            verdict,
        })
    }
}

impl Default for RunReportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute overall run verdict from correctness and throughput data.
fn compute_verdict(correctness: &CorrectnessSummary, throughput: &ThroughputSummary) -> RunVerdict {
    let mut reasons = Vec::new();

    if correctness.failed > 0 {
        reasons.push(format!(
            "{} correctness check(s) failed",
            correctness.failed
        ));
    }

    if throughput.revert_rate > 0.10 {
        reasons.push(format!(
            "revert rate {:.1}% exceeds 10% threshold",
            throughput.revert_rate * 100.0
        ));
    }

    if correctness.total_checks == 0 {
        return RunVerdict::Inconclusive {
            reasons: vec!["no correctness checks were configured".into()],
        };
    }

    if reasons.is_empty() {
        RunVerdict::Pass
    } else {
        RunVerdict::Fail { reasons }
    }
}

// ---------------------------------------------------------------------------
// JSON rendering
// ---------------------------------------------------------------------------

/// Render the report as a JSON string.
pub fn render_json(report: &RunReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| format!("JSON serialization error: {e}"))
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

/// Render the report as a Markdown string.
pub fn render_markdown(report: &RunReport) -> String {
    let mut md = String::with_capacity(4096);

    // Title
    md.push_str(&format!(
        "# GasHammer Run Report: {}\n\n",
        report.metadata.scenario_name
    ));

    // Run Metadata
    md.push_str("## Run Metadata\n\n");
    md.push_str("| Field | Value |\n");
    md.push_str("|-------|-------|\n");
    md.push_str(&format!("| Run ID | `{}` |\n", report.metadata.run_id));
    md.push_str(&format!(
        "| Scenario | {} |\n",
        report.metadata.scenario_name
    ));
    md.push_str(&format!("| Seed | {} |\n", report.metadata.seed));
    md.push_str(&format!("| Started | {} |\n", report.metadata.started_at));
    md.push_str(&format!(
        "| Completed | {} |\n",
        report.metadata.completed_at
    ));
    md.push_str(&format!(
        "| Duration | {}s |\n",
        report.metadata.duration_sec
    ));
    md.push_str(&format!(
        "| Environment | {} |\n",
        report.metadata.target_environment
    ));
    md.push_str(&format!(
        "| GasHammer | {} |\n",
        report.metadata.gashammer_version
    ));
    md.push('\n');

    // Edge Topology
    md.push_str("## Edge Topology\n\n");
    md.push_str(&format!(
        "- **Total edges:** {}\n",
        report.edge_topology.total_edges
    ));
    md.push_str(&format!(
        "- **Healthy:** {}\n",
        report.edge_topology.edges_healthy
    ));
    md.push_str(&format!(
        "- **Lost:** {}\n",
        report.edge_topology.edges_lost
    ));
    if !report.edge_topology.edges_by_region.is_empty() {
        md.push_str("- **By region:**\n");
        let mut regions: Vec<_> = report.edge_topology.edges_by_region.iter().collect();
        regions.sort_by_key(|(k, _)| (*k).clone());
        for (region, count) in regions {
            md.push_str(&format!("  - {region}: {count}\n"));
        }
    }
    md.push('\n');

    // Phase Timeline
    md.push_str("## Phase Timeline\n\n");
    md.push_str(
        "| Phase | Duration | Target Gas/s | Achieved Gas/s | Submitted | Confirmed | Failed |\n",
    );
    md.push_str(
        "|-------|----------|-------------|----------------|-----------|-----------|--------|\n",
    );
    for phase in &report.phases {
        md.push_str(&format!(
            "| {} | {}s | {} | {} | {} | {} | {} |\n",
            phase.name,
            phase.duration_sec,
            format_gas(phase.target_gas_per_sec),
            format_gas(phase.achieved_gas_per_sec),
            phase.tx_submitted,
            phase.tx_confirmed,
            phase.tx_failed,
        ));
        for fault in &phase.fault_events {
            md.push_str(&format!(
                "| ↳ *fault @{}s: {} {}* | | | | | | |\n",
                fault.offset_sec, fault.action, fault.fault_name
            ));
        }
    }
    md.push('\n');

    // Throughput Summary
    md.push_str("## Throughput Summary\n\n");
    md.push_str(&format!(
        "- **Gas submitted/s:** {}\n",
        format_gas_f64(report.throughput.avg_gas_submitted_per_sec)
    ));
    md.push_str(&format!(
        "- **Gas accepted/s:** {}\n",
        format_gas_f64(report.throughput.avg_gas_accepted_per_sec)
    ));
    md.push_str(&format!(
        "- **Gas included/s:** {}\n",
        format_gas_f64(report.throughput.avg_gas_included_per_sec)
    ));
    md.push_str(&format!(
        "- **Tx/s:** {:.1}\n",
        report.throughput.avg_tx_per_sec
    ));
    md.push_str(&format!(
        "- **Total tx submitted:** {}\n",
        report.throughput.total_tx_submitted
    ));
    md.push_str(&format!(
        "- **Total tx confirmed:** {}\n",
        report.throughput.total_tx_confirmed
    ));
    md.push_str(&format!(
        "- **Revert rate:** {:.2}%\n",
        report.throughput.revert_rate * 100.0
    ));
    if !report.throughput.revert_rate_by_template.is_empty() {
        md.push_str("- **Revert rate by template:**\n");
        let mut templates: Vec<_> = report.throughput.revert_rate_by_template.iter().collect();
        templates.sort_by_key(|(k, _)| (*k).clone());
        for (template, rate) in templates {
            md.push_str(&format!("  - {template}: {:.2}%\n", rate * 100.0));
        }
    }
    md.push('\n');

    // Latency Summary
    md.push_str("## Latency Summary\n\n");
    md.push_str("### Accept Latency\n\n");
    render_latency_table(&mut md, &report.latency.accept_latency);
    md.push('\n');

    md.push_str("### Include Latency\n\n");
    render_latency_table(&mut md, &report.latency.include_latency);
    md.push('\n');

    if !report.latency.accept_by_region.is_empty() {
        md.push_str("### Accept Latency by Region\n\n");
        let mut regions: Vec<_> = report.latency.accept_by_region.iter().collect();
        regions.sort_by_key(|(k, _)| (*k).clone());
        for (region, pct) in regions {
            md.push_str(&format!("**{region}:**\n\n"));
            render_latency_table(&mut md, pct);
            md.push('\n');
        }
    }

    // Correctness Summary
    md.push_str("## Correctness Summary\n\n");
    md.push_str(&format!("- **Verdict:** {}\n", report.correctness.verdict));
    md.push_str(&format!(
        "- **Checks:** {} total, {} passed, {} failed, {} inconclusive\n",
        report.correctness.total_checks,
        report.correctness.passed,
        report.correctness.failed,
        report.correctness.inconclusive,
    ));
    if !report.correctness.checks.is_empty() {
        md.push('\n');
        md.push_str("| Check | Status | Violations |\n");
        md.push_str("|-------|--------|------------|\n");
        for check in &report.correctness.checks {
            let status = if check.passed { "PASS" } else { "FAIL" };
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                check.check_name, status, check.violation_count
            ));
        }
    }
    md.push('\n');

    // Overall Verdict
    md.push_str("## Verdict\n\n");
    match &report.verdict {
        RunVerdict::Pass => md.push_str("**PASS** — All checks passed.\n"),
        RunVerdict::Fail { reasons } => {
            md.push_str("**FAIL**\n\n");
            for reason in reasons {
                md.push_str(&format!("- {reason}\n"));
            }
        }
        RunVerdict::Inconclusive { reasons } => {
            md.push_str("**INCONCLUSIVE**\n\n");
            for reason in reasons {
                md.push_str(&format!("- {reason}\n"));
            }
        }
    }
    md.push('\n');

    // Footer (DNA)
    md.push_str("---\n\n");
    md.push_str(&format!(
        "*Generated by GasHammer v{} — Adversarial load testing for rollup infrastructure.*\n",
        report.provenance.version
    ));

    md
}

fn render_latency_table(md: &mut String, pct: &LatencyPercentiles) {
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!("| p50 | {:.2}ms |\n", pct.p50_ms));
    md.push_str(&format!("| p90 | {:.2}ms |\n", pct.p90_ms));
    md.push_str(&format!("| p95 | {:.2}ms |\n", pct.p95_ms));
    md.push_str(&format!("| p99 | {:.2}ms |\n", pct.p99_ms));
    md.push_str(&format!("| p99.9 | {:.2}ms |\n", pct.p999_ms));
    md.push_str(&format!("| max | {:.2}ms |\n", pct.max_ms));
    md.push_str(&format!("| min | {:.2}ms |\n", pct.min_ms));
    md.push_str(&format!("| mean | {:.2}ms |\n", pct.mean_ms));
    md.push_str(&format!("| samples | {} |\n", pct.sample_count));
}

/// Format gas value with K/M suffix for readability.
fn format_gas(gas: u64) -> String {
    if gas >= 1_000_000 {
        format!("{:.1}M", gas as f64 / 1_000_000.0)
    } else if gas >= 1_000 {
        format!("{:.1}K", gas as f64 / 1_000.0)
    } else {
        gas.to_string()
    }
}

/// Format gas value (f64) with K/M suffix.
fn format_gas_f64(gas: f64) -> String {
    if gas >= 1_000_000.0 {
        format!("{:.1}M", gas / 1_000_000.0)
    } else if gas >= 1_000.0 {
        format!("{:.1}K", gas / 1_000.0)
    } else {
        format!("{:.0}", gas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn sample_metadata() -> RunMetadata {
        RunMetadata {
            run_id: Uuid::nil(),
            scenario_name: "test-scenario".into(),
            seed: 42,
            started_at: "2025-01-01T00:00:00Z".into(),
            completed_at: "2025-01-01T00:05:00Z".into(),
            duration_sec: 300,
            target_environment: "devnet".into(),
            sequencer_url: "http://localhost:8547".into(),
            gashammer_version: "0.1.0".into(),
        }
    }

    fn sample_topology() -> EdgeTopology {
        let mut regions = HashMap::new();
        regions.insert("us-east-1".into(), 2);
        regions.insert("eu-west-1".into(), 1);
        EdgeTopology {
            total_edges: 3,
            edges_by_region: regions,
            edges_healthy: 3,
            edges_lost: 0,
        }
    }

    fn sample_phase() -> PhaseReport {
        PhaseReport {
            name: "steady-state".into(),
            index: 0,
            start_sec: 0,
            end_sec: 300,
            duration_sec: 300,
            target_gas_per_sec: 5_000_000,
            achieved_gas_per_sec: 4_800_000,
            tx_submitted: 10000,
            tx_confirmed: 9950,
            tx_failed: 50,
            fault_events: vec![],
        }
    }

    fn sample_throughput() -> ThroughputSummary {
        ThroughputSummary {
            avg_gas_submitted_per_sec: 4_900_000.0,
            avg_gas_accepted_per_sec: 4_800_000.0,
            avg_gas_included_per_sec: 4_750_000.0,
            avg_tx_per_sec: 33.3,
            total_gas_submitted: 1_470_000_000,
            total_tx_submitted: 10000,
            total_tx_confirmed: 9950,
            total_tx_failed: 50,
            revert_rate: 0.005,
            revert_rate_by_template: {
                let mut m = HashMap::new();
                m.insert("simple-transfer".into(), 0.003);
                m.insert("storage-write".into(), 0.01);
                m
            },
            gas_per_sec_series: vec![
                TimeSeriesPoint {
                    timestamp_sec: 0,
                    value: 1_000_000.0,
                },
                TimeSeriesPoint {
                    timestamp_sec: 60,
                    value: 5_000_000.0,
                },
            ],
            tx_per_sec_series: vec![
                TimeSeriesPoint {
                    timestamp_sec: 0,
                    value: 10.0,
                },
                TimeSeriesPoint {
                    timestamp_sec: 60,
                    value: 35.0,
                },
            ],
        }
    }

    fn sample_correctness() -> CorrectnessSummary {
        CorrectnessSummary {
            total_checks: 4,
            passed: 4,
            failed: 0,
            inconclusive: 0,
            checks: vec![
                CheckResult {
                    check_name: "receipt_status".into(),
                    passed: true,
                    violation_count: 0,
                    sample_violations: vec![],
                },
                CheckResult {
                    check_name: "gas_used_bounds".into(),
                    passed: true,
                    violation_count: 0,
                    sample_violations: vec![],
                },
                CheckResult {
                    check_name: "balance_conservation".into(),
                    passed: true,
                    violation_count: 0,
                    sample_violations: vec![],
                },
                CheckResult {
                    check_name: "nonce_consistency".into(),
                    passed: true,
                    violation_count: 0,
                    sample_violations: vec![],
                },
            ],
            verdict: "PASS".into(),
        }
    }

    fn build_sample_report() -> RunReport {
        let accept_samples: Vec<f64> = (1..=1000).map(|i| i as f64 * 0.1).collect();
        let include_samples: Vec<f64> = (1..=1000).map(|i| i as f64 * 0.5).collect();

        RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .add_phase(sample_phase())
            .accept_latency_samples(accept_samples)
            .include_latency_samples(include_samples)
            .throughput(sample_throughput())
            .correctness(sample_correctness())
            .build()
            .expect("build should succeed")
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_percentile_at_basic() {
        let data: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert!((percentile_at(&data, 0.50) - 50.0).abs() < f64::EPSILON);
        assert!((percentile_at(&data, 0.99) - 99.0).abs() < f64::EPSILON);
        assert!((percentile_at(&data, 1.0) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_at_empty() {
        let data: Vec<f64> = vec![];
        assert!((percentile_at(&data, 0.50)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_at_single_element() {
        let data = vec![42.0];
        assert!((percentile_at(&data, 0.50) - 42.0).abs() < f64::EPSILON);
        assert!((percentile_at(&data, 0.99) - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_latency_percentiles_from_sorted() {
        let samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let pct = LatencyPercentiles::from_sorted_samples(&samples);
        assert!((pct.p50_ms - 50.0).abs() < f64::EPSILON);
        assert!((pct.p99_ms - 99.0).abs() < f64::EPSILON);
        assert!((pct.min_ms - 1.0).abs() < f64::EPSILON);
        assert!((pct.max_ms - 100.0).abs() < f64::EPSILON);
        assert!((pct.mean_ms - 50.5).abs() < f64::EPSILON);
        assert_eq!(pct.sample_count, 100);
    }

    #[test]
    fn test_latency_percentiles_empty() {
        let pct = LatencyPercentiles::from_sorted_samples(&[]);
        assert_eq!(pct.sample_count, 0);
        assert!((pct.p50_ms).abs() < f64::EPSILON);
    }

    #[test]
    fn test_builder_missing_metadata() {
        let result = RunReportBuilder::new()
            .edge_topology(sample_topology())
            .throughput(sample_throughput())
            .correctness(sample_correctness())
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("metadata"));
    }

    #[test]
    fn test_builder_missing_topology() {
        let result = RunReportBuilder::new()
            .metadata(sample_metadata())
            .throughput(sample_throughput())
            .correctness(sample_correctness())
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("edge_topology"));
    }

    #[test]
    fn test_builder_missing_throughput() {
        let result = RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .correctness(sample_correctness())
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("throughput"));
    }

    #[test]
    fn test_builder_missing_correctness() {
        let result = RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .throughput(sample_throughput())
            .build();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("correctness"));
    }

    #[test]
    fn test_builder_success() {
        let report = build_sample_report();
        assert_eq!(report.metadata.scenario_name, "test-scenario");
        assert_eq!(report.metadata.seed, 42);
        assert_eq!(report.edge_topology.total_edges, 3);
        assert_eq!(report.phases.len(), 1);
        assert!(matches!(report.verdict, RunVerdict::Pass));
    }

    #[test]
    fn test_verdict_pass() {
        let correctness = sample_correctness();
        let throughput = sample_throughput();
        let verdict = compute_verdict(&correctness, &throughput);
        assert!(matches!(verdict, RunVerdict::Pass));
    }

    #[test]
    fn test_verdict_fail_correctness() {
        let mut correctness = sample_correctness();
        correctness.failed = 2;
        let throughput = sample_throughput();
        let verdict = compute_verdict(&correctness, &throughput);
        match verdict {
            RunVerdict::Fail { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("correctness")));
            }
            _ => panic!("expected Fail verdict"),
        }
    }

    #[test]
    fn test_verdict_fail_high_revert_rate() {
        let correctness = sample_correctness();
        let mut throughput = sample_throughput();
        throughput.revert_rate = 0.15;
        let verdict = compute_verdict(&correctness, &throughput);
        match verdict {
            RunVerdict::Fail { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("revert rate")));
            }
            _ => panic!("expected Fail verdict"),
        }
    }

    #[test]
    fn test_verdict_inconclusive_no_checks() {
        let mut correctness = sample_correctness();
        correctness.total_checks = 0;
        correctness.failed = 0;
        let throughput = sample_throughput();
        let verdict = compute_verdict(&correctness, &throughput);
        assert!(matches!(verdict, RunVerdict::Inconclusive { .. }));
    }

    #[test]
    fn test_render_json() {
        let report = build_sample_report();
        let json = render_json(&report).expect("json rendering should succeed");
        assert!(json.contains("test-scenario"));
        assert!(json.contains("provenance"));
        assert!(json.contains("GasHammer"));
        assert!(json.contains("BSL-1.1"));
    }

    #[test]
    fn test_render_json_deserializes_back() {
        let report = build_sample_report();
        let json = render_json(&report).expect("serialize");
        let parsed: RunReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.metadata.scenario_name, "test-scenario");
        assert_eq!(parsed.phases.len(), 1);
    }

    #[test]
    fn test_render_markdown_contains_sections() {
        let report = build_sample_report();
        let md = render_markdown(&report);

        assert!(md.contains("# GasHammer Run Report: test-scenario"));
        assert!(md.contains("## Run Metadata"));
        assert!(md.contains("## Edge Topology"));
        assert!(md.contains("## Phase Timeline"));
        assert!(md.contains("## Throughput Summary"));
        assert!(md.contains("## Latency Summary"));
        assert!(md.contains("## Correctness Summary"));
        assert!(md.contains("## Verdict"));
        assert!(md.contains("**PASS**"));
    }

    #[test]
    fn test_render_markdown_contains_footer() {
        let report = build_sample_report();
        let md = render_markdown(&report);
        assert!(md.contains("Generated by GasHammer"));
        assert!(md.contains("Adversarial load testing for rollup infrastructure"));
    }

    #[test]
    fn test_render_markdown_contains_latency_percentiles() {
        let report = build_sample_report();
        let md = render_markdown(&report);
        assert!(md.contains("| p50 |"));
        assert!(md.contains("| p90 |"));
        assert!(md.contains("| p95 |"));
        assert!(md.contains("| p99 |"));
        assert!(md.contains("| p99.9 |"));
    }

    #[test]
    fn test_render_markdown_contains_regions() {
        let report = build_sample_report();
        let md = render_markdown(&report);
        assert!(md.contains("us-east-1"));
        assert!(md.contains("eu-west-1"));
    }

    #[test]
    fn test_render_markdown_with_faults() {
        let mut phase = sample_phase();
        phase.fault_events.push(FaultEvent {
            offset_sec: 60,
            action: "inject".into(),
            fault_name: "network-latency".into(),
        });

        let accept_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let include_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();

        let report = RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .add_phase(phase)
            .accept_latency_samples(accept_samples)
            .include_latency_samples(include_samples)
            .throughput(sample_throughput())
            .correctness(sample_correctness())
            .build()
            .expect("build");

        let md = render_markdown(&report);
        assert!(md.contains("fault @60s"));
        assert!(md.contains("network-latency"));
    }

    #[test]
    fn test_render_markdown_fail_verdict() {
        let mut correctness = sample_correctness();
        correctness.failed = 1;
        correctness.checks[0].passed = false;
        correctness.checks[0].violation_count = 5;

        let accept_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let include_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();

        let report = RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .add_phase(sample_phase())
            .accept_latency_samples(accept_samples)
            .include_latency_samples(include_samples)
            .throughput(sample_throughput())
            .correctness(correctness)
            .build()
            .expect("build");

        let md = render_markdown(&report);
        assert!(md.contains("**FAIL**"));
    }

    #[test]
    fn test_format_gas() {
        assert_eq!(format_gas(500), "500");
        assert_eq!(format_gas(5000), "5.0K");
        assert_eq!(format_gas(5_000_000), "5.0M");
        assert_eq!(format_gas(12_500_000), "12.5M");
    }

    #[test]
    fn test_format_gas_f64() {
        assert_eq!(format_gas_f64(500.0), "500");
        assert_eq!(format_gas_f64(5000.0), "5.0K");
        assert_eq!(format_gas_f64(5_000_000.0), "5.0M");
    }

    #[test]
    fn test_report_provenance_defaults() {
        let prov = ReportProvenance::default();
        assert_eq!(prov.generator, "GasHammer");
        assert_eq!(prov.license, "BSL-1.1");
        assert!(prov.url.contains("gashammer"));
    }

    #[test]
    fn test_regional_latency_samples() {
        let accept_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let include_samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let us_samples: Vec<f64> = (1..=50).map(|i| i as f64).collect();
        let eu_samples: Vec<f64> = (20..=80).map(|i| i as f64).collect();

        let report = RunReportBuilder::new()
            .metadata(sample_metadata())
            .edge_topology(sample_topology())
            .add_phase(sample_phase())
            .accept_latency_samples(accept_samples)
            .include_latency_samples(include_samples)
            .add_regional_accept_samples("us-east-1".into(), us_samples)
            .add_regional_accept_samples("eu-west-1".into(), eu_samples)
            .throughput(sample_throughput())
            .correctness(sample_correctness())
            .build()
            .expect("build");

        assert_eq!(report.latency.accept_by_region.len(), 2);
        assert!(report.latency.accept_by_region.contains_key("us-east-1"));
        assert!(report.latency.accept_by_region.contains_key("eu-west-1"));
    }
}
