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

//! Workload engine — gas-targeted rate control with temporal patterns.
//!
//! The engine drives transaction generation at a target gas/sec rate using
//! weighted template selection and a token-bucket rate limiter.
//!
//! Ref: RFC-0005 §3–4.

use std::sync::atomic::{AtomicU64, Ordering};

use alloy::primitives::Address;
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

use crate::templates::{Parameterizer, TemplateOutput};

// ---------------------------------------------------------------------------
// Gas rate modes
// ---------------------------------------------------------------------------

/// Target gas rate configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GasRateMode {
    /// Constant gas/sec for the duration of a phase.
    Sustained { gas_per_sec: u64 },
    /// Linear ramp from start to end over a duration.
    Ramped {
        start_gas_per_sec: u64,
        end_gas_per_sec: u64,
        duration_ms: u64,
    },
    /// Periodic spikes over a baseline.
    Bursty {
        baseline_gas_per_sec: u64,
        burst_peak_gas_per_sec: u64,
        burst_duration_ms: u64,
        burst_interval_ms: u64,
    },
    /// Piecewise-linear gas/sec schedule.
    Custom {
        /// Points as (offset_ms, gas_per_sec) — must be sorted by offset.
        schedule: Vec<(u64, u64)>,
    },
}

impl GasRateMode {
    /// Compute the target gas/sec at a given elapsed time.
    pub fn gas_per_sec_at(&self, elapsed_ms: u64) -> u64 {
        match self {
            GasRateMode::Sustained { gas_per_sec } => *gas_per_sec,
            GasRateMode::Ramped {
                start_gas_per_sec,
                end_gas_per_sec,
                duration_ms,
            } => {
                if *duration_ms == 0 {
                    return *end_gas_per_sec;
                }
                let progress = (elapsed_ms as f64 / *duration_ms as f64).min(1.0);
                let start = *start_gas_per_sec as f64;
                let end = *end_gas_per_sec as f64;
                (start + (end - start) * progress) as u64
            }
            GasRateMode::Bursty {
                baseline_gas_per_sec,
                burst_peak_gas_per_sec,
                burst_duration_ms,
                burst_interval_ms,
            } => {
                if *burst_interval_ms == 0 {
                    return *baseline_gas_per_sec;
                }
                let cycle_pos = elapsed_ms % burst_interval_ms;
                if cycle_pos < *burst_duration_ms {
                    *burst_peak_gas_per_sec
                } else {
                    *baseline_gas_per_sec
                }
            }
            GasRateMode::Custom { schedule } => {
                if schedule.is_empty() {
                    return 0;
                }
                if schedule.len() == 1 {
                    return schedule[0].1;
                }
                // Find the segment containing elapsed_ms
                if elapsed_ms <= schedule[0].0 {
                    return schedule[0].1;
                }
                if elapsed_ms >= schedule[schedule.len() - 1].0 {
                    return schedule[schedule.len() - 1].1;
                }
                for i in 0..schedule.len() - 1 {
                    let (t0, g0) = schedule[i];
                    let (t1, g1) = schedule[i + 1];
                    if elapsed_ms >= t0 && elapsed_ms < t1 {
                        let progress = (elapsed_ms - t0) as f64 / (t1 - t0) as f64;
                        return (g0 as f64 + (g1 as f64 - g0 as f64) * progress) as u64;
                    }
                }
                schedule[schedule.len() - 1].1
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

/// Rate limiter mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RateLimitMode {
    /// Limit by gas per second (primary mode).
    GasPerSecond,
    /// Limit by transactions per second.
    TxPerSecond { max_tx_per_sec: u64 },
    /// No rate limiting.
    Unlimited,
}

/// Token-bucket rate limiter for gas-based rate control.
///
/// Tick interval: 50ms (5 ticks per 250ms Nitro block).
pub struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64, // tokens per ms
}

/// Tick interval in milliseconds (50ms = 5 ticks per 250ms Nitro block).
pub const TICK_INTERVAL_MS: u64 = 50;

impl TokenBucket {
    /// Create a new token bucket for a given gas/sec target.
    pub fn new(gas_per_sec: u64) -> Self {
        let capacity = gas_per_sec as f64;
        Self {
            tokens: capacity, // start full
            capacity,
            refill_rate: gas_per_sec as f64 / 1000.0, // per ms
        }
    }

    /// Update the target gas/sec (for ramp/burst patterns).
    pub fn set_rate(&mut self, gas_per_sec: u64) {
        self.capacity = gas_per_sec as f64;
        self.refill_rate = gas_per_sec as f64 / 1000.0;
        if self.tokens > self.capacity {
            self.tokens = self.capacity;
        }
    }

    /// Refill tokens for the elapsed time.
    pub fn refill(&mut self, elapsed_ms: u64) {
        self.tokens = (self.tokens + self.refill_rate * elapsed_ms as f64).min(self.capacity);
    }

    /// Try to consume tokens for a transaction. Returns true if allowed.
    pub fn try_consume(&mut self, gas_cost: f64) -> bool {
        if self.tokens >= gas_cost {
            self.tokens -= gas_cost;
            true
        } else {
            false
        }
    }

    /// Current token level.
    pub fn tokens(&self) -> f64 {
        self.tokens
    }

    /// Current capacity.
    pub fn capacity(&self) -> f64 {
        self.capacity
    }
}

// ---------------------------------------------------------------------------
// Weighted template selection
// ---------------------------------------------------------------------------

/// A template with a selection weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedTemplate {
    pub template_name: String,
    pub weight: f64,
}

/// Selects a template index from weighted distribution using the given RNG.
pub fn select_template(weights: &[f64], rng: &mut ChaCha20Rng) -> usize {
    debug_assert!(!weights.is_empty());
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    let roll: f64 = rng.gen_range(0.0..total);
    let mut cumulative = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        cumulative += w;
        if roll < cumulative {
            return i;
        }
    }
    weights.len() - 1
}

// ---------------------------------------------------------------------------
// Engine metrics
// ---------------------------------------------------------------------------

/// Atomic counters for the workload engine.
#[derive(Debug, Default)]
pub struct EngineMetrics {
    pub gashammer_tx_generated: AtomicU64,
    pub gashammer_gas_generated: AtomicU64,
    pub gashammer_rate_limited_count: AtomicU64,
}

impl EngineMetrics {
    pub fn tx_generated(&self) -> u64 {
        self.gashammer_tx_generated.load(Ordering::Relaxed)
    }

    pub fn gas_generated(&self) -> u64 {
        self.gashammer_gas_generated.load(Ordering::Relaxed)
    }

    pub fn rate_limited(&self) -> u64 {
        self.gashammer_rate_limited_count.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Workload engine
// ---------------------------------------------------------------------------

/// Configuration for the workload engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub gas_rate: GasRateMode,
    pub rate_limit_mode: RateLimitMode,
    pub templates: Vec<WeightedTemplate>,
    pub seed: u64,
}

/// Output from a single engine tick — zero or more transactions to submit.
pub struct TickOutput {
    pub transactions: Vec<TemplateOutput>,
}

/// The workload engine generates transactions at a controlled gas rate.
///
/// Call `tick()` every `TICK_INTERVAL_MS` milliseconds. The engine
/// produces transactions up to the token-bucket budget for that tick.
pub struct WorkloadEngine {
    config: EngineConfig,
    bucket: TokenBucket,
    rng: ChaCha20Rng,
    templates: Vec<Box<dyn Parameterizer>>,
    weights: Vec<f64>,
    metrics: EngineMetrics,
    started_ms: u64,
    last_tick_ms: u64,
}

impl WorkloadEngine {
    /// Create a new workload engine from config and template implementations.
    ///
    /// `templates` must be in the same order as `config.templates`.
    pub fn new(config: EngineConfig, templates: Vec<Box<dyn Parameterizer>>) -> Self {
        use rand::SeedableRng;

        assert_eq!(
            config.templates.len(),
            templates.len(),
            "config.templates and templates must have the same length"
        );

        let initial_rate = config.gas_rate.gas_per_sec_at(0);
        let bucket = match &config.rate_limit_mode {
            RateLimitMode::GasPerSecond => TokenBucket::new(initial_rate),
            RateLimitMode::TxPerSecond { max_tx_per_sec } => TokenBucket::new(*max_tx_per_sec),
            RateLimitMode::Unlimited => TokenBucket::new(u64::MAX / 2),
        };

        let weights: Vec<f64> = config.templates.iter().map(|t| t.weight).collect();
        let rng = ChaCha20Rng::seed_from_u64(config.seed);

        Self {
            config,
            bucket,
            rng,
            templates,
            weights,
            metrics: EngineMetrics::default(),
            started_ms: 0,
            last_tick_ms: 0,
        }
    }

    /// Returns a reference to engine metrics.
    pub fn metrics(&self) -> &EngineMetrics {
        &self.metrics
    }

    /// Returns the engine config.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Produce transactions for a single tick.
    ///
    /// `elapsed_ms` is the time since engine start.
    /// `target` is the default `to` address for generated transactions.
    pub fn tick(&mut self, elapsed_ms: u64, target: Address) -> TickOutput {
        // Update rate based on temporal pattern
        let current_rate = match &self.config.rate_limit_mode {
            RateLimitMode::GasPerSecond => self.config.gas_rate.gas_per_sec_at(elapsed_ms),
            RateLimitMode::TxPerSecond { max_tx_per_sec } => *max_tx_per_sec,
            RateLimitMode::Unlimited => u64::MAX / 2,
        };

        self.bucket.set_rate(current_rate);

        // Refill tokens for elapsed time since last tick
        let delta = if elapsed_ms > self.last_tick_ms {
            elapsed_ms - self.last_tick_ms
        } else {
            TICK_INTERVAL_MS
        };
        self.bucket.refill(delta);
        self.last_tick_ms = elapsed_ms;

        let mut transactions = Vec::new();

        // Generate transactions until bucket is exhausted
        loop {
            if self.templates.is_empty() {
                break;
            }

            let idx = select_template(&self.weights, &mut self.rng);
            let template = &self.templates[idx];
            let descriptor = template.descriptor();

            // Gas variance compensation: estimated_gas × (1 + variance/2)
            let variance_factor =
                1.0 + descriptor.gas_variance as f64 / (2.0 * descriptor.estimated_gas as f64);
            let token_cost = descriptor.estimated_gas as f64 * variance_factor;

            let consume_cost = match &self.config.rate_limit_mode {
                RateLimitMode::GasPerSecond => token_cost,
                RateLimitMode::TxPerSecond { .. } => 1.0,
                RateLimitMode::Unlimited => 0.0,
            };

            if !self.bucket.try_consume(consume_cost) {
                self.metrics
                    .gashammer_rate_limited_count
                    .fetch_add(1, Ordering::Relaxed);
                break;
            }

            let output = template.next(&mut self.rng, target);

            self.metrics
                .gashammer_tx_generated
                .fetch_add(1, Ordering::Relaxed);
            self.metrics
                .gashammer_gas_generated
                .fetch_add(output.estimated_gas, Ordering::Relaxed);

            transactions.push(output);

            // Safety: cap at 1000 txs per tick to avoid runaway generation
            if transactions.len() >= 1000 {
                break;
            }
        }

        TickOutput { transactions }
    }

    /// Reset the engine state (for re-use with a new phase).
    pub fn reset(&mut self, seed: u64) {
        use rand::SeedableRng;
        self.rng = ChaCha20Rng::seed_from_u64(seed);
        self.started_ms = 0;
        self.last_tick_ms = 0;
        let initial_rate = self.config.gas_rate.gas_per_sec_at(0);
        self.bucket = TokenBucket::new(initial_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::{template_by_name, SimpleTransfer};

    fn simple_config(gas_per_sec: u64, seed: u64) -> EngineConfig {
        EngineConfig {
            gas_rate: GasRateMode::Sustained { gas_per_sec },
            rate_limit_mode: RateLimitMode::GasPerSecond,
            templates: vec![WeightedTemplate {
                template_name: "simple-transfer".into(),
                weight: 1.0,
            }],
            seed,
        }
    }

    fn make_engine(gas_per_sec: u64, seed: u64) -> WorkloadEngine {
        let config = simple_config(gas_per_sec, seed);
        let templates: Vec<Box<dyn Parameterizer>> = vec![Box::new(SimpleTransfer)];
        WorkloadEngine::new(config, templates)
    }

    #[test]
    fn test_sustained_rate_constant() {
        let mode = GasRateMode::Sustained {
            gas_per_sec: 1_000_000,
        };
        assert_eq!(mode.gas_per_sec_at(0), 1_000_000);
        assert_eq!(mode.gas_per_sec_at(5000), 1_000_000);
        assert_eq!(mode.gas_per_sec_at(999_999), 1_000_000);
    }

    #[test]
    fn test_ramped_rate() {
        let mode = GasRateMode::Ramped {
            start_gas_per_sec: 100_000,
            end_gas_per_sec: 200_000,
            duration_ms: 10_000,
        };
        assert_eq!(mode.gas_per_sec_at(0), 100_000);
        assert_eq!(mode.gas_per_sec_at(5000), 150_000);
        assert_eq!(mode.gas_per_sec_at(10_000), 200_000);
        // Past duration stays at end
        assert_eq!(mode.gas_per_sec_at(20_000), 200_000);
    }

    #[test]
    fn test_bursty_rate() {
        let mode = GasRateMode::Bursty {
            baseline_gas_per_sec: 50_000,
            burst_peak_gas_per_sec: 200_000,
            burst_duration_ms: 1000,
            burst_interval_ms: 5000,
        };
        // Within burst window
        assert_eq!(mode.gas_per_sec_at(0), 200_000);
        assert_eq!(mode.gas_per_sec_at(500), 200_000);
        assert_eq!(mode.gas_per_sec_at(999), 200_000);
        // After burst window
        assert_eq!(mode.gas_per_sec_at(1000), 50_000);
        assert_eq!(mode.gas_per_sec_at(3000), 50_000);
        // Next cycle
        assert_eq!(mode.gas_per_sec_at(5000), 200_000);
        assert_eq!(mode.gas_per_sec_at(6000), 50_000);
    }

    #[test]
    fn test_custom_rate() {
        let mode = GasRateMode::Custom {
            schedule: vec![(0, 10_000), (1000, 50_000), (2000, 50_000), (3000, 10_000)],
        };
        assert_eq!(mode.gas_per_sec_at(0), 10_000);
        assert_eq!(mode.gas_per_sec_at(500), 30_000);
        assert_eq!(mode.gas_per_sec_at(1000), 50_000);
        assert_eq!(mode.gas_per_sec_at(1500), 50_000);
        assert_eq!(mode.gas_per_sec_at(2500), 30_000);
        assert_eq!(mode.gas_per_sec_at(3000), 10_000);
        // Past end
        assert_eq!(mode.gas_per_sec_at(5000), 10_000);
    }

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(100_000); // 100k gas/sec
        assert_eq!(bucket.capacity(), 100_000.0);

        // Consume some tokens
        assert!(bucket.try_consume(50_000.0));
        assert_eq!(bucket.tokens(), 50_000.0);

        // Can't consume more than available
        assert!(!bucket.try_consume(60_000.0));
        assert_eq!(bucket.tokens(), 50_000.0);

        // Refill
        bucket.refill(500); // 500ms = half capacity
        assert!((bucket.tokens() - 100_000.0).abs() < 1.0);
    }

    #[test]
    fn test_token_bucket_refill_caps_at_capacity() {
        let mut bucket = TokenBucket::new(100_000);
        bucket.refill(10_000); // way more than needed
        assert!((bucket.tokens() - 100_000.0).abs() < 1.0);
    }

    #[test]
    fn test_token_bucket_set_rate() {
        let mut bucket = TokenBucket::new(100_000);
        bucket.set_rate(200_000);
        assert_eq!(bucket.capacity(), 200_000.0);

        // Tokens should stay at 100k (not exceed old value)
        assert!((bucket.tokens() - 100_000.0).abs() < 1.0);
    }

    #[test]
    fn test_select_template_single() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let weights = vec![1.0];
        for _ in 0..100 {
            assert_eq!(select_template(&weights, &mut rng), 0);
        }
    }

    #[test]
    fn test_select_template_distribution() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let weights = vec![0.5, 0.5];
        let mut counts = [0u64; 2];
        for _ in 0..10_000 {
            let idx = select_template(&weights, &mut rng);
            counts[idx] += 1;
        }
        // Each should be roughly 5000 ± 500
        assert!(counts[0] > 4000 && counts[0] < 6000);
        assert!(counts[1] > 4000 && counts[1] < 6000);
    }

    #[test]
    fn test_select_template_weighted() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        // 90% / 10% split
        let weights = vec![0.9, 0.1];
        let mut counts = [0u64; 2];
        for _ in 0..10_000 {
            let idx = select_template(&weights, &mut rng);
            counts[idx] += 1;
        }
        assert!(counts[0] > 8000);
        assert!(counts[1] < 2000);
    }

    #[test]
    fn test_engine_tick_produces_transactions() {
        let mut engine = make_engine(1_000_000, 42);
        // First tick at t=0, should produce some txs (bucket starts full at 1M)
        let output = engine.tick(0, Address::ZERO);
        assert!(!output.transactions.is_empty());

        // All should be simple-transfer
        for tx in &output.transactions {
            assert_eq!(tx.template_name, "simple-transfer");
        }
    }

    #[test]
    fn test_engine_rate_limits() {
        // Very low rate: 21,000 gas/sec = 1 tx per second
        let mut engine = make_engine(21_000, 42);

        // First tick at t=0 — bucket starts full
        let output = engine.tick(0, Address::ZERO);
        assert_eq!(output.transactions.len(), 1);

        // Immediately tick again — no time elapsed, bucket empty
        let output = engine.tick(0, Address::ZERO);
        assert!(output.transactions.is_empty());
        assert!(engine.metrics().rate_limited() > 0);
    }

    #[test]
    fn test_engine_determinism() {
        let mut engine1 = make_engine(1_000_000, 42);
        let mut engine2 = make_engine(1_000_000, 42);

        let out1 = engine1.tick(0, Address::ZERO);
        let out2 = engine2.tick(0, Address::ZERO);

        assert_eq!(out1.transactions.len(), out2.transactions.len());
        for (t1, t2) in out1.transactions.iter().zip(out2.transactions.iter()) {
            assert_eq!(t1.data, t2.data);
            assert_eq!(t1.value, t2.value);
            assert_eq!(t1.gas_limit, t2.gas_limit);
        }
    }

    #[test]
    fn test_engine_different_seed_different_output() {
        let mut engine1 = make_engine(1_000_000, 42);
        let mut engine2 = make_engine(1_000_000, 99);

        let out1 = engine1.tick(0, Address::ZERO);
        let out2 = engine2.tick(0, Address::ZERO);

        // Same template, so same gas_limit, but different values
        assert_eq!(out1.transactions.len(), out2.transactions.len());
        let differ = out1
            .transactions
            .iter()
            .zip(out2.transactions.iter())
            .any(|(t1, t2)| t1.value != t2.value);
        assert!(differ, "different seeds should produce different values");
    }

    #[test]
    fn test_engine_unlimited_mode() {
        let config = EngineConfig {
            gas_rate: GasRateMode::Sustained {
                gas_per_sec: 1_000_000,
            },
            rate_limit_mode: RateLimitMode::Unlimited,
            templates: vec![WeightedTemplate {
                template_name: "simple-transfer".into(),
                weight: 1.0,
            }],
            seed: 42,
        };
        let templates: Vec<Box<dyn Parameterizer>> = vec![Box::new(SimpleTransfer)];
        let mut engine = WorkloadEngine::new(config, templates);

        let output = engine.tick(0, Address::ZERO);
        // With unlimited, should hit the 1000 per-tick safety cap
        assert_eq!(output.transactions.len(), 1000);
    }

    #[test]
    fn test_engine_tx_per_second_mode() {
        let config = EngineConfig {
            gas_rate: GasRateMode::Sustained {
                gas_per_sec: 1_000_000,
            },
            rate_limit_mode: RateLimitMode::TxPerSecond { max_tx_per_sec: 5 },
            templates: vec![WeightedTemplate {
                template_name: "simple-transfer".into(),
                weight: 1.0,
            }],
            seed: 42,
        };
        let templates: Vec<Box<dyn Parameterizer>> = vec![Box::new(SimpleTransfer)];
        let mut engine = WorkloadEngine::new(config, templates);

        // Bucket has capacity 5. First tick should drain it.
        let output = engine.tick(0, Address::ZERO);
        assert_eq!(output.transactions.len(), 5);
    }

    #[test]
    fn test_engine_multi_template_mix() {
        let config = EngineConfig {
            gas_rate: GasRateMode::Sustained {
                gas_per_sec: 10_000_000,
            },
            rate_limit_mode: RateLimitMode::GasPerSecond,
            templates: vec![
                WeightedTemplate {
                    template_name: "simple-transfer".into(),
                    weight: 0.5,
                },
                WeightedTemplate {
                    template_name: "noop".into(),
                    weight: 0.5,
                },
            ],
            seed: 42,
        };
        let templates: Vec<Box<dyn Parameterizer>> =
            vec![Box::new(SimpleTransfer), Box::new(crate::templates::Noop)];
        let mut engine = WorkloadEngine::new(config, templates);

        let output = engine.tick(0, Address::ZERO);
        let mut has_transfer = false;
        let mut has_noop = false;
        for tx in &output.transactions {
            match tx.template_name.as_str() {
                "simple-transfer" => has_transfer = true,
                "noop" => has_noop = true,
                _ => panic!("unexpected template: {}", tx.template_name),
            }
        }
        assert!(has_transfer, "should have simple-transfer txs");
        assert!(has_noop, "should have noop txs");
    }

    #[test]
    fn test_engine_metrics_track_generation() {
        let mut engine = make_engine(1_000_000, 42);
        let output = engine.tick(0, Address::ZERO);

        assert_eq!(
            engine.metrics().tx_generated(),
            output.transactions.len() as u64
        );
        assert!(engine.metrics().gas_generated() > 0);
    }

    #[test]
    fn test_engine_reset() {
        let mut engine = make_engine(1_000_000, 42);
        let out1 = engine.tick(0, Address::ZERO);

        engine.reset(42);
        let out2 = engine.tick(0, Address::ZERO);

        // After reset with same seed, should produce identical output
        assert_eq!(out1.transactions.len(), out2.transactions.len());
        for (t1, t2) in out1.transactions.iter().zip(out2.transactions.iter()) {
            assert_eq!(t1.data, t2.data);
            assert_eq!(t1.value, t2.value);
        }
    }

    #[test]
    fn test_engine_ramp_increases_throughput() {
        let config = EngineConfig {
            gas_rate: GasRateMode::Ramped {
                start_gas_per_sec: 21_000,
                end_gas_per_sec: 2_100_000,
                duration_ms: 10_000,
            },
            rate_limit_mode: RateLimitMode::GasPerSecond,
            templates: vec![WeightedTemplate {
                template_name: "simple-transfer".into(),
                weight: 1.0,
            }],
            seed: 42,
        };
        let templates: Vec<Box<dyn Parameterizer>> = vec![Box::new(SimpleTransfer)];
        let mut engine = WorkloadEngine::new(config, templates);

        // Early tick — low rate
        let early = engine.tick(0, Address::ZERO);
        // Late tick — high rate (refill with time jump)
        let late = engine.tick(10_000, Address::ZERO);

        assert!(
            late.transactions.len() >= early.transactions.len(),
            "ramp should produce more txs later: early={}, late={}",
            early.transactions.len(),
            late.transactions.len()
        );
    }

    #[test]
    fn test_gas_rate_mode_serialization() {
        let modes = vec![
            GasRateMode::Sustained {
                gas_per_sec: 1_000_000,
            },
            GasRateMode::Ramped {
                start_gas_per_sec: 100_000,
                end_gas_per_sec: 200_000,
                duration_ms: 10_000,
            },
            GasRateMode::Bursty {
                baseline_gas_per_sec: 50_000,
                burst_peak_gas_per_sec: 200_000,
                burst_duration_ms: 1000,
                burst_interval_ms: 5000,
            },
            GasRateMode::Custom {
                schedule: vec![(0, 10_000), (1000, 50_000)],
            },
        ];

        for mode in &modes {
            let json = serde_json::to_string(mode).expect("serialize");
            let _parsed: GasRateMode = serde_json::from_str(&json).expect("deserialize");
        }
    }

    #[test]
    fn test_engine_config_serialization() {
        let config = simple_config(1_000_000, 42);
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: EngineConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.seed, 42);
    }

    #[test]
    fn test_template_by_name_integration() {
        let names = ["simple-transfer", "noop", "erc20-transfer", "compute-heavy"];
        for name in &names {
            assert!(
                template_by_name(name).is_some(),
                "template_by_name should find '{name}'"
            );
        }
    }
}
