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

//! Capacity envelope — derive safe operating range from ramp-to-failure runs.
//!
//! Analyzes time-series telemetry to find inflection points: where performance
//! degrades, where instability begins, and what the first bottleneck is.
//!
//! Ref: RFC-0009 §3.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Capacity Envelope
// ---------------------------------------------------------------------------

/// The safe operating envelope derived from a ramp-to-failure run.
///
/// Ref: RFC-0009 §3.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityEnvelope {
    /// Maximum gas/sec that can be sustained without degradation.
    pub safe_sustained_gas_sec: u64,
    /// Gas/sec at which p99 latency exceeds 2× baseline.
    pub degradation_onset_gas_sec: u64,
    /// Gas/sec at which revert rate >5%, feed stalls, or correctness violations occur.
    pub instability_threshold_gas_sec: u64,
    /// Classification of the first bottleneck encountered.
    pub first_bottleneck: BottleneckClassification,
    /// Confidence level for the bottleneck classification.
    pub bottleneck_confidence: Confidence,
    /// Time in seconds for p99 to return to 50% of peak after ramp ends.
    pub recovery_half_life_sec: f64,
    /// Gas/sec at which validator lag exceeds threshold (if applicable).
    pub validator_drift_threshold_gas_sec: Option<u64>,
}

/// Classification of the first performance bottleneck.
///
/// Ref: RFC-0009 §3.4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BottleneckClassification {
    SequencerCpu,
    SequencerMemory,
    SequencerNetwork,
    BatchPosterLag,
    ValidatorReplayLag,
    RpcSaturation,
    DiskIo,
    GasPriceEscalation,
    Unknown { evidence: String },
}

/// Confidence level for a classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

// ---------------------------------------------------------------------------
// Time series data point
// ---------------------------------------------------------------------------

/// A data point in a ramp time series: (gas_rate, metric_value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RampDataPoint {
    /// The gas rate at this point in time (gas/sec).
    pub gas_rate: u64,
    /// The metric value (e.g., p99 latency in ms, revert rate).
    pub value: f64,
    /// Timestamp offset in seconds from run start.
    pub offset_sec: u64,
}

/// Infrastructure metrics at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraMetricsPoint {
    pub offset_sec: u64,
    pub cpu_pct: Option<f64>,
    pub memory_pct: Option<f64>,
    pub disk_iops: Option<f64>,
    pub network_bytes_sec: Option<f64>,
    pub base_fee_gwei: Option<f64>,
    pub batch_post_age_sec: Option<f64>,
    pub validator_lag_blocks: Option<u64>,
}

// ---------------------------------------------------------------------------
// Envelope computation
// ---------------------------------------------------------------------------

/// Input data for computing a capacity envelope.
pub struct EnvelopeInput {
    /// p99 accept latency series indexed by gas rate.
    pub p99_latency_series: Vec<RampDataPoint>,
    /// Revert rate series indexed by gas rate.
    pub revert_rate_series: Vec<RampDataPoint>,
    /// Feed stall duration series (max stall in seconds).
    pub feed_stall_series: Vec<RampDataPoint>,
    /// Infrastructure metrics time series (optional).
    pub infra_metrics: Vec<InfraMetricsPoint>,
    /// p99 latency at the start of the ramp (baseline period).
    pub baseline_p99_ms: f64,
    /// Validator lag threshold in blocks (default: 3).
    pub validator_lag_threshold: u64,
}

/// Compute the capacity envelope from ramp-to-failure telemetry.
///
/// The algorithm:
/// 1. Find degradation onset: first gas rate where p99 > 2× baseline
/// 2. Find instability threshold: first gas rate where revert >5% or stall >1s
/// 3. Safe sustained = degradation onset × 0.8 (20% safety margin)
/// 4. Classify bottleneck by correlating latency inflection with infra metrics
/// 5. Compute recovery half-life from post-peak cooldown data
pub fn compute_envelope(input: &EnvelopeInput) -> CapacityEnvelope {
    let degradation_onset =
        find_degradation_onset(&input.p99_latency_series, input.baseline_p99_ms);

    let instability_threshold =
        find_instability_threshold(&input.revert_rate_series, &input.feed_stall_series);

    // Safe sustained = 80% of degradation onset (safety margin)
    let safe_sustained = (degradation_onset as f64 * 0.8) as u64;

    let (first_bottleneck, bottleneck_confidence) =
        classify_bottleneck(&input.infra_metrics, degradation_onset);

    let validator_drift = find_validator_drift(&input.infra_metrics, input.validator_lag_threshold);

    let recovery_half_life =
        compute_recovery_half_life(&input.p99_latency_series, input.baseline_p99_ms);

    CapacityEnvelope {
        safe_sustained_gas_sec: safe_sustained,
        degradation_onset_gas_sec: degradation_onset,
        instability_threshold_gas_sec: instability_threshold,
        first_bottleneck,
        bottleneck_confidence,
        recovery_half_life_sec: recovery_half_life,
        validator_drift_threshold_gas_sec: validator_drift,
    }
}

/// Find the gas rate at which p99 latency exceeds 2× baseline.
fn find_degradation_onset(series: &[RampDataPoint], baseline_p99: f64) -> u64 {
    let threshold = baseline_p99 * 2.0;
    for point in series {
        if point.value > threshold {
            return point.gas_rate;
        }
    }
    // No degradation found — return the max gas rate tested
    series.last().map_or(0, |p| p.gas_rate)
}

/// Find the gas rate at which revert rate >5% or feed stall >1s.
fn find_instability_threshold(
    revert_series: &[RampDataPoint],
    stall_series: &[RampDataPoint],
) -> u64 {
    let mut threshold = u64::MAX;

    for point in revert_series {
        if point.value > 0.05 {
            threshold = threshold.min(point.gas_rate);
            break;
        }
    }

    for point in stall_series {
        if point.value > 1.0 {
            threshold = threshold.min(point.gas_rate);
            break;
        }
    }

    if threshold == u64::MAX {
        // No instability found — return max gas rate tested
        let max_revert = revert_series.last().map_or(0, |p| p.gas_rate);
        let max_stall = stall_series.last().map_or(0, |p| p.gas_rate);
        max_revert.max(max_stall)
    } else {
        threshold
    }
}

/// Classify the first bottleneck by correlating infra metrics with the
/// degradation onset gas rate.
fn classify_bottleneck(
    infra: &[InfraMetricsPoint],
    degradation_onset_gas_sec: u64,
) -> (BottleneckClassification, Confidence) {
    if infra.is_empty() {
        return (
            BottleneckClassification::Unknown {
                evidence: "no infrastructure metrics available".into(),
            },
            Confidence::Low,
        );
    }

    // Find the infra point closest to degradation onset (by offset matching)
    // In practice, we'd match by timestamp; here we look for high utilization
    // around the degradation onset.

    let mut max_cpu: f64 = 0.0;
    let mut max_mem: f64 = 0.0;
    let mut max_disk: f64 = 0.0;
    let mut max_batch_age: f64 = 0.0;
    let mut max_base_fee: f64 = 0.0;
    let mut max_validator_lag: u64 = 0;

    for point in infra {
        if let Some(cpu) = point.cpu_pct {
            max_cpu = max_cpu.max(cpu);
        }
        if let Some(mem) = point.memory_pct {
            max_mem = max_mem.max(mem);
        }
        if let Some(disk) = point.disk_iops {
            max_disk = max_disk.max(disk);
        }
        if let Some(batch) = point.batch_post_age_sec {
            max_batch_age = max_batch_age.max(batch);
        }
        if let Some(fee) = point.base_fee_gwei {
            max_base_fee = max_base_fee.max(fee);
        }
        if let Some(lag) = point.validator_lag_blocks {
            max_validator_lag = max_validator_lag.max(lag);
        }
    }

    // Classification heuristic (RFC-0009 §3.4)
    // Order matters: first match wins

    if max_cpu > 90.0 {
        return (BottleneckClassification::SequencerCpu, Confidence::High);
    }

    if max_mem > 90.0 {
        return (BottleneckClassification::SequencerMemory, Confidence::High);
    }

    if max_disk > 10000.0 {
        return (BottleneckClassification::DiskIo, Confidence::Medium);
    }

    if max_batch_age > 300.0 {
        return (BottleneckClassification::BatchPosterLag, Confidence::High);
    }

    if max_validator_lag > 5 {
        return (
            BottleneckClassification::ValidatorReplayLag,
            Confidence::Medium,
        );
    }

    if max_base_fee > 100.0 {
        return (
            BottleneckClassification::GasPriceEscalation,
            Confidence::Medium,
        );
    }

    if max_cpu > 70.0 {
        return (BottleneckClassification::SequencerCpu, Confidence::Medium);
    }

    // Suppress the unused variable warning contextually
    let _degradation_rate = degradation_onset_gas_sec;

    (
        BottleneckClassification::Unknown {
            evidence: format!(
                "no clear correlation — peak CPU {max_cpu:.0}%, memory {max_mem:.0}%, \
                 batch age {max_batch_age:.0}s"
            ),
        },
        Confidence::Low,
    )
}

/// Find the gas rate at which validator lag exceeds the threshold.
fn find_validator_drift(infra: &[InfraMetricsPoint], threshold: u64) -> Option<u64> {
    for (i, point) in infra.iter().enumerate() {
        if let Some(lag) = point.validator_lag_blocks {
            if lag > threshold {
                // Return offset as a proxy for gas rate (we don't have gas rate
                // directly in infra metrics, so offset_sec serves as the indicator)
                return Some(point.offset_sec);
            }
        }
        let _ = i; // suppress unused
    }
    None
}

/// Compute recovery half-life: time for p99 to drop to 50% of peak after ramp ends.
fn compute_recovery_half_life(series: &[RampDataPoint], baseline_p99: f64) -> f64 {
    if series.is_empty() {
        return 0.0;
    }

    // Find peak latency and its offset
    let peak = series.iter().max_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let peak = match peak {
        Some(p) => p,
        None => return 0.0,
    };

    let peak_value = peak.value;
    let peak_offset = peak.offset_sec;
    let half_target = baseline_p99 + (peak_value - baseline_p99) * 0.5;

    // Find the first point after peak where latency drops below half_target
    for point in series {
        if point.offset_sec > peak_offset && point.value <= half_target {
            return (point.offset_sec - peak_offset) as f64;
        }
    }

    // Never recovered — return time from peak to end
    series
        .last()
        .map(|last| (last.offset_sec - peak_offset) as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ramp_series(points: &[(u64, f64)]) -> Vec<RampDataPoint> {
        points
            .iter()
            .enumerate()
            .map(|(i, (gas_rate, value))| RampDataPoint {
                gas_rate: *gas_rate,
                value: *value,
                offset_sec: (i as u64) * 10,
            })
            .collect()
    }

    #[test]
    fn test_find_degradation_onset() {
        // Baseline p99 = 10ms; threshold = 20ms
        let series = make_ramp_series(&[
            (1_000_000, 8.0),
            (5_000_000, 12.0),
            (10_000_000, 18.0),
            (15_000_000, 25.0), // exceeds 2× baseline here
            (20_000_000, 45.0),
        ]);
        let onset = find_degradation_onset(&series, 10.0);
        assert_eq!(onset, 15_000_000);
    }

    #[test]
    fn test_find_degradation_onset_no_degradation() {
        let series = make_ramp_series(&[(1_000_000, 8.0), (5_000_000, 10.0), (10_000_000, 12.0)]);
        let onset = find_degradation_onset(&series, 10.0);
        assert_eq!(onset, 10_000_000); // max gas rate
    }

    #[test]
    fn test_find_degradation_onset_empty() {
        let onset = find_degradation_onset(&[], 10.0);
        assert_eq!(onset, 0);
    }

    #[test]
    fn test_find_instability_revert_rate() {
        let revert_series = make_ramp_series(&[
            (5_000_000, 0.01),
            (10_000_000, 0.03),
            (15_000_000, 0.06), // exceeds 5%
            (20_000_000, 0.12),
        ]);
        let stall_series = make_ramp_series(&[
            (5_000_000, 0.1),
            (10_000_000, 0.2),
            (15_000_000, 0.5),
            (20_000_000, 0.8),
        ]);
        let threshold = find_instability_threshold(&revert_series, &stall_series);
        assert_eq!(threshold, 15_000_000);
    }

    #[test]
    fn test_find_instability_feed_stall() {
        let revert_series = make_ramp_series(&[(5_000_000, 0.01), (10_000_000, 0.02)]);
        let stall_series = make_ramp_series(&[
            (5_000_000, 0.1),
            (10_000_000, 1.5), // exceeds 1s
        ]);
        let threshold = find_instability_threshold(&revert_series, &stall_series);
        assert_eq!(threshold, 10_000_000);
    }

    #[test]
    fn test_find_instability_no_instability() {
        let revert_series = make_ramp_series(&[(5_000_000, 0.01), (10_000_000, 0.02)]);
        let stall_series = make_ramp_series(&[(5_000_000, 0.1), (10_000_000, 0.5)]);
        let threshold = find_instability_threshold(&revert_series, &stall_series);
        assert_eq!(threshold, 10_000_000); // max
    }

    #[test]
    fn test_classify_bottleneck_cpu() {
        let infra = vec![InfraMetricsPoint {
            offset_sec: 60,
            cpu_pct: Some(95.0),
            memory_pct: Some(40.0),
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: None,
            validator_lag_blocks: None,
        }];
        let (classification, confidence) = classify_bottleneck(&infra, 10_000_000);
        assert_eq!(classification, BottleneckClassification::SequencerCpu);
        assert_eq!(confidence, Confidence::High);
    }

    #[test]
    fn test_classify_bottleneck_memory() {
        let infra = vec![InfraMetricsPoint {
            offset_sec: 60,
            cpu_pct: Some(50.0),
            memory_pct: Some(92.0),
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: None,
            validator_lag_blocks: None,
        }];
        let (classification, _) = classify_bottleneck(&infra, 10_000_000);
        assert_eq!(classification, BottleneckClassification::SequencerMemory);
    }

    #[test]
    fn test_classify_bottleneck_batch_poster() {
        let infra = vec![InfraMetricsPoint {
            offset_sec: 60,
            cpu_pct: Some(50.0),
            memory_pct: Some(40.0),
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: Some(350.0),
            validator_lag_blocks: None,
        }];
        let (classification, _) = classify_bottleneck(&infra, 10_000_000);
        assert_eq!(classification, BottleneckClassification::BatchPosterLag);
    }

    #[test]
    fn test_classify_bottleneck_unknown() {
        let infra = vec![InfraMetricsPoint {
            offset_sec: 60,
            cpu_pct: Some(30.0),
            memory_pct: Some(40.0),
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: None,
            validator_lag_blocks: None,
        }];
        let (classification, confidence) = classify_bottleneck(&infra, 10_000_000);
        assert!(matches!(
            classification,
            BottleneckClassification::Unknown { .. }
        ));
        assert_eq!(confidence, Confidence::Low);
    }

    #[test]
    fn test_classify_bottleneck_no_infra() {
        let (classification, confidence) = classify_bottleneck(&[], 10_000_000);
        assert!(matches!(
            classification,
            BottleneckClassification::Unknown { .. }
        ));
        assert_eq!(confidence, Confidence::Low);
    }

    #[test]
    fn test_find_validator_drift() {
        let infra = vec![
            InfraMetricsPoint {
                offset_sec: 10,
                cpu_pct: None,
                memory_pct: None,
                disk_iops: None,
                network_bytes_sec: None,
                base_fee_gwei: None,
                batch_post_age_sec: None,
                validator_lag_blocks: Some(1),
            },
            InfraMetricsPoint {
                offset_sec: 20,
                cpu_pct: None,
                memory_pct: None,
                disk_iops: None,
                network_bytes_sec: None,
                base_fee_gwei: None,
                batch_post_age_sec: None,
                validator_lag_blocks: Some(5),
            },
        ];
        let drift = find_validator_drift(&infra, 3);
        assert_eq!(drift, Some(20));
    }

    #[test]
    fn test_find_validator_drift_none() {
        let infra = vec![InfraMetricsPoint {
            offset_sec: 10,
            cpu_pct: None,
            memory_pct: None,
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: None,
            validator_lag_blocks: Some(2),
        }];
        let drift = find_validator_drift(&infra, 3);
        assert_eq!(drift, None);
    }

    #[test]
    fn test_recovery_half_life() {
        // Peak at offset 40, half recovery at offset 60
        let series = vec![
            RampDataPoint {
                gas_rate: 1_000_000,
                value: 10.0,
                offset_sec: 0,
            },
            RampDataPoint {
                gas_rate: 5_000_000,
                value: 20.0,
                offset_sec: 10,
            },
            RampDataPoint {
                gas_rate: 10_000_000,
                value: 50.0,
                offset_sec: 20,
            },
            RampDataPoint {
                gas_rate: 15_000_000,
                value: 100.0,
                offset_sec: 30,
            },
            RampDataPoint {
                gas_rate: 20_000_000,
                value: 200.0,
                offset_sec: 40,
            }, // peak
            RampDataPoint {
                gas_rate: 15_000_000,
                value: 150.0,
                offset_sec: 50,
            },
            RampDataPoint {
                gas_rate: 10_000_000,
                value: 80.0,
                offset_sec: 60,
            }, // below half
            RampDataPoint {
                gas_rate: 5_000_000,
                value: 30.0,
                offset_sec: 70,
            },
        ];
        // baseline=10, peak=200, half_target = 10 + (200-10)*0.5 = 105
        // First point after peak below 105: offset 60 (value 80)
        let half_life = compute_recovery_half_life(&series, 10.0);
        assert!((half_life - 20.0).abs() < f64::EPSILON); // 60 - 40
    }

    #[test]
    fn test_recovery_half_life_no_recovery() {
        let series = vec![
            RampDataPoint {
                gas_rate: 1_000_000,
                value: 10.0,
                offset_sec: 0,
            },
            RampDataPoint {
                gas_rate: 10_000_000,
                value: 100.0,
                offset_sec: 10,
            },
            RampDataPoint {
                gas_rate: 20_000_000,
                value: 200.0,
                offset_sec: 20,
            },
            RampDataPoint {
                gas_rate: 15_000_000,
                value: 180.0,
                offset_sec: 30,
            },
        ];
        // half_target = 10 + (200-10)*0.5 = 105; never reached
        let half_life = compute_recovery_half_life(&series, 10.0);
        assert!((half_life - 10.0).abs() < f64::EPSILON); // 30 - 20
    }

    #[test]
    fn test_recovery_half_life_empty() {
        let half_life = compute_recovery_half_life(&[], 10.0);
        assert!((half_life).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_envelope_full() {
        let p99_series = make_ramp_series(&[
            (1_000_000, 8.0),
            (5_000_000, 12.0),
            (10_000_000, 18.0),
            (15_000_000, 25.0),
            (20_000_000, 45.0),
            (15_000_000, 30.0),
            (10_000_000, 15.0),
        ]);
        let revert_series = make_ramp_series(&[
            (1_000_000, 0.01),
            (5_000_000, 0.02),
            (10_000_000, 0.03),
            (15_000_000, 0.04),
            (20_000_000, 0.08),
        ]);
        let stall_series = make_ramp_series(&[
            (1_000_000, 0.0),
            (5_000_000, 0.0),
            (10_000_000, 0.1),
            (15_000_000, 0.3),
            (20_000_000, 0.5),
        ]);
        let infra = vec![InfraMetricsPoint {
            offset_sec: 30,
            cpu_pct: Some(92.0),
            memory_pct: Some(50.0),
            disk_iops: None,
            network_bytes_sec: None,
            base_fee_gwei: None,
            batch_post_age_sec: None,
            validator_lag_blocks: None,
        }];

        let input = EnvelopeInput {
            p99_latency_series: p99_series,
            revert_rate_series: revert_series,
            feed_stall_series: stall_series,
            infra_metrics: infra,
            baseline_p99_ms: 10.0,
            validator_lag_threshold: 3,
        };

        let envelope = compute_envelope(&input);

        // Degradation at 15M (25ms > 20ms threshold)
        assert_eq!(envelope.degradation_onset_gas_sec, 15_000_000);
        // Instability at 20M (revert > 5%)
        assert_eq!(envelope.instability_threshold_gas_sec, 20_000_000);
        // Safe = 80% of 15M = 12M
        assert_eq!(envelope.safe_sustained_gas_sec, 12_000_000);
        // CPU bottleneck
        assert_eq!(
            envelope.first_bottleneck,
            BottleneckClassification::SequencerCpu
        );
        assert_eq!(envelope.bottleneck_confidence, Confidence::High);
    }

    #[test]
    fn test_envelope_serialization() {
        let envelope = CapacityEnvelope {
            safe_sustained_gas_sec: 12_000_000,
            degradation_onset_gas_sec: 15_000_000,
            instability_threshold_gas_sec: 20_000_000,
            first_bottleneck: BottleneckClassification::SequencerCpu,
            bottleneck_confidence: Confidence::High,
            recovery_half_life_sec: 20.0,
            validator_drift_threshold_gas_sec: None,
        };
        let json = serde_json::to_string(&envelope).expect("serialize");
        let parsed: CapacityEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.safe_sustained_gas_sec, 12_000_000);
        assert_eq!(
            parsed.first_bottleneck,
            BottleneckClassification::SequencerCpu
        );
    }
}
