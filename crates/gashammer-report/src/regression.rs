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

//! Regression detection — compare a run against a baseline and produce a
//! release gate verdict.
//!
//! Supports single-baseline comparison and multi-baseline statistical testing
//! (Welch's t-test) when multiple baseline runs are available.
//!
//! Ref: RFC-0009 §4.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Regression Delta
// ---------------------------------------------------------------------------

/// Comparison of a single metric between baseline and current run.
///
/// Ref: RFC-0009 §4.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionDelta {
    pub metric_name: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub delta_absolute: f64,
    pub delta_pct: f64,
    pub threshold_pct: f64,
    pub verdict: RegressionVerdict,
}

/// Verdict for a single metric regression check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegressionVerdict {
    /// Within threshold — acceptable.
    Pass,
    /// Better than baseline beyond threshold.
    Improvement,
    /// Worse than baseline beyond threshold.
    Regression,
    /// Insufficient data or high variance.
    Inconclusive,
}

// ---------------------------------------------------------------------------
// Release Gate
// ---------------------------------------------------------------------------

/// The overall release gate output.
///
/// Ref: RFC-0009 §4.4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGate {
    pub gate_verdict: GateVerdict,
    pub regressions: Vec<RegressionDelta>,
    pub improvements: Vec<RegressionDelta>,
    pub passes: Vec<RegressionDelta>,
    pub pass_count: u32,
    pub fail_count: u32,
    pub improvement_count: u32,
    pub inconclusive_count: u32,
}

/// Overall gate verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GateVerdict {
    Pass,
    Fail,
    Inconclusive,
}

// ---------------------------------------------------------------------------
// Threshold Configuration
// ---------------------------------------------------------------------------

/// Threshold for a single metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThreshold {
    pub metric_name: String,
    /// Maximum allowed regression as a percentage (e.g., 10.0 = 10%).
    pub max_regression_pct: f64,
    /// If true, higher values are worse (e.g., latency).
    /// If false, lower values are worse (e.g., throughput).
    pub higher_is_worse: bool,
    /// Optional absolute maximum value. Fail if current exceeds this.
    pub max_absolute: Option<f64>,
    /// Optional absolute minimum value. Fail if current is below this.
    pub min_absolute: Option<f64>,
}

/// Configuration for regression detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionConfig {
    pub thresholds: Vec<MetricThreshold>,
}

// ---------------------------------------------------------------------------
// Baseline Data
// ---------------------------------------------------------------------------

/// Metric values from a single baseline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineMetrics {
    /// Map of metric_name → value.
    pub values: std::collections::HashMap<String, f64>,
}

/// Current run metric values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentMetrics {
    /// Map of metric_name → value.
    pub values: std::collections::HashMap<String, f64>,
}

// ---------------------------------------------------------------------------
// Single-baseline comparison
// ---------------------------------------------------------------------------

/// Compare current metrics against a single baseline using configured thresholds.
pub fn compare_single_baseline(
    baseline: &BaselineMetrics,
    current: &CurrentMetrics,
    config: &RegressionConfig,
) -> ReleaseGate {
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    let mut passes = Vec::new();
    let mut fail_count: u32 = 0;
    let mut pass_count: u32 = 0;
    let mut improvement_count: u32 = 0;
    let mut inconclusive_count: u32 = 0;

    for threshold in &config.thresholds {
        let baseline_val = baseline.values.get(&threshold.metric_name);
        let current_val = current.values.get(&threshold.metric_name);

        let delta = match (baseline_val, current_val) {
            (Some(&bv), Some(&cv)) => compute_delta(&threshold.metric_name, bv, cv, threshold),
            _ => RegressionDelta {
                metric_name: threshold.metric_name.clone(),
                baseline_value: baseline_val.copied().unwrap_or(0.0),
                current_value: current_val.copied().unwrap_or(0.0),
                delta_absolute: 0.0,
                delta_pct: 0.0,
                threshold_pct: threshold.max_regression_pct,
                verdict: RegressionVerdict::Inconclusive,
            },
        };

        match &delta.verdict {
            RegressionVerdict::Regression => {
                fail_count += 1;
                regressions.push(delta);
            }
            RegressionVerdict::Improvement => {
                improvement_count += 1;
                improvements.push(delta);
            }
            RegressionVerdict::Pass => {
                pass_count += 1;
                passes.push(delta);
            }
            RegressionVerdict::Inconclusive => {
                inconclusive_count += 1;
                passes.push(delta);
            }
        }
    }

    let gate_verdict = if fail_count > 0 {
        GateVerdict::Fail
    } else if inconclusive_count > 0 && pass_count == 0 && improvement_count == 0 {
        GateVerdict::Inconclusive
    } else {
        GateVerdict::Pass
    };

    ReleaseGate {
        gate_verdict,
        regressions,
        improvements,
        passes,
        pass_count,
        fail_count,
        improvement_count,
        inconclusive_count,
    }
}

/// Compute a regression delta for a single metric.
fn compute_delta(
    name: &str,
    baseline: f64,
    current: f64,
    threshold: &MetricThreshold,
) -> RegressionDelta {
    let delta_absolute = current - baseline;
    let delta_pct = if baseline.abs() > f64::EPSILON {
        (delta_absolute / baseline) * 100.0
    } else {
        0.0
    };

    // Check absolute bounds first
    if let Some(max) = threshold.max_absolute {
        if current > max {
            return RegressionDelta {
                metric_name: name.into(),
                baseline_value: baseline,
                current_value: current,
                delta_absolute,
                delta_pct,
                threshold_pct: threshold.max_regression_pct,
                verdict: RegressionVerdict::Regression,
            };
        }
    }
    if let Some(min) = threshold.min_absolute {
        if current < min {
            return RegressionDelta {
                metric_name: name.into(),
                baseline_value: baseline,
                current_value: current,
                delta_absolute,
                delta_pct,
                threshold_pct: threshold.max_regression_pct,
                verdict: RegressionVerdict::Regression,
            };
        }
    }

    // Check percentage regression
    let regression_pct = if threshold.higher_is_worse {
        delta_pct // positive = worse for latency-like metrics
    } else {
        -delta_pct // negative delta = worse for throughput-like metrics
    };

    let verdict = if regression_pct > threshold.max_regression_pct {
        RegressionVerdict::Regression
    } else if regression_pct < -threshold.max_regression_pct {
        RegressionVerdict::Improvement
    } else {
        RegressionVerdict::Pass
    };

    RegressionDelta {
        metric_name: name.into(),
        baseline_value: baseline,
        current_value: current,
        delta_absolute,
        delta_pct,
        threshold_pct: threshold.max_regression_pct,
        verdict,
    }
}

// ---------------------------------------------------------------------------
// Multi-baseline statistical comparison (Welch's t-test)
// ---------------------------------------------------------------------------

/// Multiple baseline samples for a single metric.
pub struct MultiBaselineSamples {
    pub metric_name: String,
    /// Values from multiple baseline runs.
    pub baseline_values: Vec<f64>,
    /// Value from the current run.
    pub current_value: f64,
    /// Threshold configuration.
    pub threshold: MetricThreshold,
}

/// Perform Welch's t-test to determine if the current value is statistically
/// different from the baseline distribution.
///
/// Returns a RegressionDelta with Inconclusive if there are too few samples
/// (fewer than 3 baselines).
pub fn compare_multi_baseline(samples: &MultiBaselineSamples) -> RegressionDelta {
    let n = samples.baseline_values.len();
    if n < 3 {
        return RegressionDelta {
            metric_name: samples.metric_name.clone(),
            baseline_value: mean(&samples.baseline_values),
            current_value: samples.current_value,
            delta_absolute: 0.0,
            delta_pct: 0.0,
            threshold_pct: samples.threshold.max_regression_pct,
            verdict: RegressionVerdict::Inconclusive,
        };
    }

    let baseline_mean = mean(&samples.baseline_values);
    let baseline_var = variance(&samples.baseline_values, baseline_mean);
    let baseline_n = n as f64;

    let delta_absolute = samples.current_value - baseline_mean;
    let delta_pct = if baseline_mean.abs() > f64::EPSILON {
        (delta_absolute / baseline_mean) * 100.0
    } else {
        0.0
    };

    // Welch's t-statistic (one-sample variant: current is a single observation)
    // t = (current - baseline_mean) / sqrt(baseline_var / n)
    let std_err = (baseline_var / baseline_n).sqrt();
    let t_stat = if std_err > f64::EPSILON {
        delta_absolute / std_err
    } else {
        // Zero variance in baseline — any difference is "significant"
        if delta_absolute.abs() > f64::EPSILON {
            f64::INFINITY * delta_absolute.signum()
        } else {
            0.0
        }
    };

    // Degrees of freedom for one-sample test: n - 1
    let df = (n - 1) as f64;

    // Approximate two-tailed p-value using t-distribution
    // For simplicity, we use a threshold-based approach:
    // |t| > critical value at α=0.05 → statistically significant
    let critical_t = critical_t_value(df);
    let is_significant = t_stat.abs() > critical_t;

    let regression_pct = if samples.threshold.higher_is_worse {
        delta_pct
    } else {
        -delta_pct
    };

    let verdict = if !is_significant {
        // Not statistically significant — within noise
        RegressionVerdict::Pass
    } else if regression_pct > samples.threshold.max_regression_pct {
        RegressionVerdict::Regression
    } else if regression_pct < -samples.threshold.max_regression_pct {
        RegressionVerdict::Improvement
    } else {
        RegressionVerdict::Pass
    };

    RegressionDelta {
        metric_name: samples.metric_name.clone(),
        baseline_value: baseline_mean,
        current_value: samples.current_value,
        delta_absolute,
        delta_pct,
        threshold_pct: samples.threshold.max_regression_pct,
        verdict,
    }
}

/// Compute the mean of a slice.
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Compute the sample variance.
fn variance(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let sum_sq: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    sum_sq / (values.len() - 1) as f64
}

/// Approximate critical t-value for two-tailed test at α=0.05.
/// Uses a lookup table for small df, and normal approximation for large df.
fn critical_t_value(df: f64) -> f64 {
    // Two-tailed α=0.05 critical values
    let table: &[(f64, f64)] = &[
        (1.0, 12.706),
        (2.0, 4.303),
        (3.0, 3.182),
        (4.0, 2.776),
        (5.0, 2.571),
        (6.0, 2.447),
        (7.0, 2.365),
        (8.0, 2.306),
        (9.0, 2.262),
        (10.0, 2.228),
        (15.0, 2.131),
        (20.0, 2.086),
        (25.0, 2.060),
        (30.0, 2.042),
        (60.0, 2.000),
        (120.0, 1.980),
    ];

    if df >= 120.0 {
        return 1.960; // normal approximation
    }

    // Find the closest entry
    for window in table.windows(2) {
        if df >= window[0].0 && df <= window[1].0 {
            // Linear interpolation
            let frac = (df - window[0].0) / (window[1].0 - window[0].0);
            return window[0].1 + frac * (window[1].1 - window[0].1);
        }
    }

    // Default to most conservative small-sample value
    table[0].1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(thresholds: Vec<MetricThreshold>) -> RegressionConfig {
        RegressionConfig { thresholds }
    }

    fn latency_threshold(name: &str, max_pct: f64) -> MetricThreshold {
        MetricThreshold {
            metric_name: name.into(),
            max_regression_pct: max_pct,
            higher_is_worse: true,
            max_absolute: None,
            min_absolute: None,
        }
    }

    fn throughput_threshold(name: &str, max_pct: f64) -> MetricThreshold {
        MetricThreshold {
            metric_name: name.into(),
            max_regression_pct: max_pct,
            higher_is_worse: false,
            max_absolute: None,
            min_absolute: None,
        }
    }

    #[test]
    fn test_compute_delta_pass() {
        let t = latency_threshold("p99_ms", 10.0);
        let delta = compute_delta("p99_ms", 100.0, 105.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Pass);
        assert!((delta.delta_pct - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_delta_regression() {
        let t = latency_threshold("p99_ms", 10.0);
        let delta = compute_delta("p99_ms", 100.0, 115.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Regression);
        assert!((delta.delta_pct - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_delta_improvement() {
        let t = latency_threshold("p99_ms", 10.0);
        let delta = compute_delta("p99_ms", 100.0, 80.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Improvement);
    }

    #[test]
    fn test_compute_delta_throughput_regression() {
        // Throughput: lower is worse
        let t = throughput_threshold("gas_per_sec", 5.0);
        let delta = compute_delta("gas_per_sec", 1_000_000.0, 900_000.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Regression);
    }

    #[test]
    fn test_compute_delta_throughput_improvement() {
        let t = throughput_threshold("gas_per_sec", 5.0);
        let delta = compute_delta("gas_per_sec", 1_000_000.0, 1_200_000.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Improvement);
    }

    #[test]
    fn test_compute_delta_absolute_max_exceeded() {
        let t = MetricThreshold {
            metric_name: "revert_rate".into(),
            max_regression_pct: 100.0,
            higher_is_worse: true,
            max_absolute: Some(0.05),
            min_absolute: None,
        };
        let delta = compute_delta("revert_rate", 0.03, 0.08, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Regression);
    }

    #[test]
    fn test_compute_delta_absolute_min_violated() {
        let t = MetricThreshold {
            metric_name: "gas_per_sec".into(),
            max_regression_pct: 100.0,
            higher_is_worse: false,
            max_absolute: None,
            min_absolute: Some(500_000.0),
        };
        let delta = compute_delta("gas_per_sec", 1_000_000.0, 400_000.0, &t);
        assert_eq!(delta.verdict, RegressionVerdict::Regression);
    }

    #[test]
    fn test_compute_delta_zero_baseline() {
        let t = latency_threshold("p99_ms", 10.0);
        let delta = compute_delta("p99_ms", 0.0, 5.0, &t);
        // Can't compute percentage from zero baseline
        assert!((delta.delta_pct).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compare_single_baseline_pass() {
        let baseline = BaselineMetrics {
            values: HashMap::from([("p99_ms".into(), 50.0), ("gas_per_sec".into(), 5_000_000.0)]),
        };
        let current = CurrentMetrics {
            values: HashMap::from([("p99_ms".into(), 52.0), ("gas_per_sec".into(), 4_900_000.0)]),
        };
        let config = make_config(vec![
            latency_threshold("p99_ms", 10.0),
            throughput_threshold("gas_per_sec", 5.0),
        ]);

        let gate = compare_single_baseline(&baseline, &current, &config);
        assert_eq!(gate.gate_verdict, GateVerdict::Pass);
        assert_eq!(gate.fail_count, 0);
    }

    #[test]
    fn test_compare_single_baseline_fail() {
        let baseline = BaselineMetrics {
            values: HashMap::from([("p99_ms".into(), 50.0)]),
        };
        let current = CurrentMetrics {
            values: HashMap::from([("p99_ms".into(), 70.0)]),
        };
        let config = make_config(vec![latency_threshold("p99_ms", 10.0)]);

        let gate = compare_single_baseline(&baseline, &current, &config);
        assert_eq!(gate.gate_verdict, GateVerdict::Fail);
        assert_eq!(gate.fail_count, 1);
        assert_eq!(gate.regressions.len(), 1);
        assert_eq!(gate.regressions[0].metric_name, "p99_ms");
    }

    #[test]
    fn test_compare_single_baseline_missing_metric() {
        let baseline = BaselineMetrics {
            values: HashMap::from([("p99_ms".into(), 50.0)]),
        };
        let current = CurrentMetrics {
            values: HashMap::new(), // missing p99_ms
        };
        let config = make_config(vec![latency_threshold("p99_ms", 10.0)]);

        let gate = compare_single_baseline(&baseline, &current, &config);
        assert_eq!(gate.inconclusive_count, 1);
    }

    #[test]
    fn test_compare_single_baseline_mixed() {
        let baseline = BaselineMetrics {
            values: HashMap::from([("p99_ms".into(), 50.0), ("gas_per_sec".into(), 5_000_000.0)]),
        };
        let current = CurrentMetrics {
            values: HashMap::from([
                ("p99_ms".into(), 70.0),             // regression
                ("gas_per_sec".into(), 6_000_000.0), // improvement
            ]),
        };
        let config = make_config(vec![
            latency_threshold("p99_ms", 10.0),
            throughput_threshold("gas_per_sec", 5.0),
        ]);

        let gate = compare_single_baseline(&baseline, &current, &config);
        assert_eq!(gate.gate_verdict, GateVerdict::Fail);
        assert_eq!(gate.fail_count, 1);
        assert_eq!(gate.improvement_count, 1);
    }

    #[test]
    fn test_release_gate_serialization() {
        let gate = ReleaseGate {
            gate_verdict: GateVerdict::Fail,
            regressions: vec![RegressionDelta {
                metric_name: "p99_ms".into(),
                baseline_value: 50.0,
                current_value: 70.0,
                delta_absolute: 20.0,
                delta_pct: 40.0,
                threshold_pct: 10.0,
                verdict: RegressionVerdict::Regression,
            }],
            improvements: vec![],
            passes: vec![],
            pass_count: 0,
            fail_count: 1,
            improvement_count: 0,
            inconclusive_count: 0,
        };
        let json = serde_json::to_string(&gate).expect("serialize");
        let parsed: ReleaseGate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.gate_verdict, GateVerdict::Fail);
        assert_eq!(parsed.regressions.len(), 1);
    }

    #[test]
    fn test_mean() {
        assert!((mean(&[1.0, 2.0, 3.0, 4.0, 5.0]) - 3.0).abs() < f64::EPSILON);
        assert!((mean(&[])).abs() < f64::EPSILON);
    }

    #[test]
    fn test_variance() {
        let values = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let m = mean(&values);
        let var = variance(&values, m);
        // Expected sample variance ≈ 4.571
        assert!((var - 4.571).abs() < 0.01);
    }

    #[test]
    fn test_critical_t_value() {
        // Known values
        assert!((critical_t_value(1.0) - 12.706).abs() < 0.01);
        assert!((critical_t_value(10.0) - 2.228).abs() < 0.01);
        assert!((critical_t_value(200.0) - 1.960).abs() < 0.01);
    }

    #[test]
    fn test_multi_baseline_too_few_samples() {
        let samples = MultiBaselineSamples {
            metric_name: "p99_ms".into(),
            baseline_values: vec![50.0, 52.0],
            current_value: 70.0,
            threshold: latency_threshold("p99_ms", 10.0),
        };
        let delta = compare_multi_baseline(&samples);
        assert_eq!(delta.verdict, RegressionVerdict::Inconclusive);
    }

    #[test]
    fn test_multi_baseline_significant_regression() {
        let samples = MultiBaselineSamples {
            metric_name: "p99_ms".into(),
            baseline_values: vec![50.0, 51.0, 49.0, 50.5, 50.2],
            current_value: 70.0, // way above baseline
            threshold: latency_threshold("p99_ms", 10.0),
        };
        let delta = compare_multi_baseline(&samples);
        assert_eq!(delta.verdict, RegressionVerdict::Regression);
    }

    #[test]
    fn test_multi_baseline_within_noise() {
        let samples = MultiBaselineSamples {
            metric_name: "p99_ms".into(),
            baseline_values: vec![50.0, 55.0, 45.0, 52.0, 48.0],
            current_value: 53.0, // within baseline variance
            threshold: latency_threshold("p99_ms", 10.0),
        };
        let delta = compare_multi_baseline(&samples);
        assert_eq!(delta.verdict, RegressionVerdict::Pass);
    }

    #[test]
    fn test_multi_baseline_improvement() {
        let samples = MultiBaselineSamples {
            metric_name: "p99_ms".into(),
            baseline_values: vec![50.0, 51.0, 49.0, 50.5, 50.2],
            current_value: 30.0, // much better
            threshold: latency_threshold("p99_ms", 10.0),
        };
        let delta = compare_multi_baseline(&samples);
        assert_eq!(delta.verdict, RegressionVerdict::Improvement);
    }
}
