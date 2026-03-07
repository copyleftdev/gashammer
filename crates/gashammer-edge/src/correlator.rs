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

//! Feed correlator — match submitted transactions to feed messages.
//!
//! Correlates transactions submitted by the pipeline with their appearance
//! in the sequencer feed to measure acceptance latency.
//!
//! Ref: RFC-0004 §6.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Address, TxHash};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::Instant;

// ---------------------------------------------------------------------------
// Submit record
// ---------------------------------------------------------------------------

/// Record of a submitted transaction awaiting feed correlation.
#[derive(Debug, Clone)]
pub struct TxSubmitRecord {
    pub tx_hash: TxHash,
    pub submit_time: Instant,
    pub gas_limit: u64,
    pub template_name: String,
    pub account: Address,
}

// ---------------------------------------------------------------------------
// Correlation events
// ---------------------------------------------------------------------------

/// Events emitted by the correlator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrelationEvent {
    /// Transaction appeared in the feed.
    TxAccepted {
        tx_hash_hex: String,
        acceptance_latency_ms: u64,
        gas_limit: u64,
        template_name: String,
    },
    /// Transaction timed out — never appeared in feed.
    TxTimeout {
        tx_hash_hex: String,
        waited_ms: u64,
        gas_limit: u64,
        template_name: String,
    },
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Atomic metrics for feed correlation.
#[derive(Debug, Default)]
pub struct CorrelatorMetrics {
    pub gashammer_feed_tx_correlated: AtomicU64,
    pub gashammer_correlation_misses: AtomicU64,
    pub gashammer_correlation_latency_sum_ms: AtomicU64,
}

impl CorrelatorMetrics {
    pub fn correlated(&self) -> u64 {
        self.gashammer_feed_tx_correlated.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.gashammer_correlation_misses.load(Ordering::Relaxed)
    }

    pub fn avg_latency_ms(&self) -> u64 {
        let count = self.correlated();
        if count == 0 {
            return 0;
        }
        self.gashammer_correlation_latency_sum_ms
            .load(Ordering::Relaxed)
            / count
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the feed correlator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatorConfig {
    /// Timeout for unmatched txs (default: 60s).
    #[serde(default = "default_eviction_timeout_ms")]
    pub eviction_timeout_ms: u64,
    /// How often to sweep for timed-out txs (default: 5s).
    #[serde(default = "default_sweep_interval_ms")]
    pub sweep_interval_ms: u64,
}

fn default_eviction_timeout_ms() -> u64 {
    60_000
}

fn default_sweep_interval_ms() -> u64 {
    5_000
}

impl Default for CorrelatorConfig {
    fn default() -> Self {
        Self {
            eviction_timeout_ms: default_eviction_timeout_ms(),
            sweep_interval_ms: default_sweep_interval_ms(),
        }
    }
}

// ---------------------------------------------------------------------------
// FeedCorrelator
// ---------------------------------------------------------------------------

/// Correlates submitted transactions with their appearance in the feed.
pub struct FeedCorrelator {
    pending: Arc<RwLock<HashMap<TxHash, TxSubmitRecord>>>,
    config: CorrelatorConfig,
    metrics: Arc<CorrelatorMetrics>,
}

impl FeedCorrelator {
    /// Create a new correlator.
    pub fn new(config: CorrelatorConfig) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(CorrelatorMetrics::default()),
        }
    }

    /// Returns a reference to correlator metrics.
    pub fn metrics(&self) -> &Arc<CorrelatorMetrics> {
        &self.metrics
    }

    /// Record a submitted transaction for correlation tracking.
    pub async fn record_submission(&self, record: TxSubmitRecord) {
        let mut pending = self.pending.write().await;
        pending.insert(record.tx_hash, record);
    }

    /// Attempt to correlate a tx hash from the feed with a pending submission.
    /// Returns a `CorrelationEvent::TxAccepted` if matched.
    pub async fn correlate(&self, tx_hash: &TxHash) -> Option<CorrelationEvent> {
        let mut pending = self.pending.write().await;
        if let Some(record) = pending.remove(tx_hash) {
            let latency = record.submit_time.elapsed();
            let latency_ms = latency.as_millis() as u64;

            self.metrics
                .gashammer_feed_tx_correlated
                .fetch_add(1, Ordering::Relaxed);
            self.metrics
                .gashammer_correlation_latency_sum_ms
                .fetch_add(latency_ms, Ordering::Relaxed);

            tracing::debug!(
                tx_hash = %tx_hash,
                latency_ms,
                "tx correlated with feed"
            );

            Some(CorrelationEvent::TxAccepted {
                tx_hash_hex: format!("{tx_hash:?}"),
                acceptance_latency_ms: latency_ms,
                gas_limit: record.gas_limit,
                template_name: record.template_name,
            })
        } else {
            None
        }
    }

    /// Batch-correlate multiple tx hashes from a feed message.
    pub async fn correlate_batch(&self, tx_hashes: &[TxHash]) -> Vec<CorrelationEvent> {
        let mut events = Vec::new();
        for hash in tx_hashes {
            if let Some(event) = self.correlate(hash).await {
                events.push(event);
            }
        }
        events
    }

    /// Number of pending (unmatched) transactions.
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }

    /// Start the eviction sweep task that removes timed-out transactions.
    /// Returns a JoinHandle that must be stored.
    pub fn start_eviction_sweep(
        &self,
        event_sender: tokio::sync::mpsc::Sender<CorrelationEvent>,
    ) -> JoinHandle<()> {
        let pending = Arc::clone(&self.pending);
        let metrics = Arc::clone(&self.metrics);
        let timeout = Duration::from_millis(self.config.eviction_timeout_ms);
        let interval = Duration::from_millis(self.config.sweep_interval_ms);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let mut pending = pending.write().await;
                let now = Instant::now();
                let mut evicted = Vec::new();

                pending.retain(|hash, record| {
                    let elapsed = now.duration_since(record.submit_time);
                    if elapsed >= timeout {
                        evicted.push(CorrelationEvent::TxTimeout {
                            tx_hash_hex: format!("{hash:?}"),
                            waited_ms: elapsed.as_millis() as u64,
                            gas_limit: record.gas_limit,
                            template_name: record.template_name.clone(),
                        });
                        false
                    } else {
                        true
                    }
                });

                for event in evicted {
                    metrics
                        .gashammer_correlation_misses
                        .fetch_add(1, Ordering::Relaxed);
                    if event_sender.send(event).await.is_err() {
                        tracing::warn!("correlation event channel closed");
                        return;
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_submit_record(nonce: u64) -> TxSubmitRecord {
        // Create a unique hash for each nonce
        use alloy::primitives::keccak256;
        let hash_bytes = keccak256(nonce.to_le_bytes());
        TxSubmitRecord {
            tx_hash: TxHash::from(hash_bytes),
            submit_time: Instant::now(),
            gas_limit: 21_000,
            template_name: "test_transfer".into(),
            account: Address::ZERO,
        }
    }

    #[tokio::test]
    async fn test_record_and_correlate() {
        let correlator = FeedCorrelator::new(CorrelatorConfig::default());
        let record = test_submit_record(0);
        let tx_hash = record.tx_hash;

        correlator.record_submission(record).await;
        assert_eq!(correlator.pending_count().await, 1);

        let event = correlator.correlate(&tx_hash).await;
        assert!(event.is_some());
        assert!(matches!(event, Some(CorrelationEvent::TxAccepted { .. })));
        assert_eq!(correlator.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_correlate_unknown_hash_returns_none() {
        let correlator = FeedCorrelator::new(CorrelatorConfig::default());
        let event = correlator.correlate(&TxHash::ZERO).await;
        assert!(event.is_none());
    }

    #[tokio::test]
    async fn test_double_correlate_returns_none() {
        let correlator = FeedCorrelator::new(CorrelatorConfig::default());
        let record = test_submit_record(0);
        let tx_hash = record.tx_hash;

        correlator.record_submission(record).await;

        let first = correlator.correlate(&tx_hash).await;
        assert!(first.is_some());

        let second = correlator.correlate(&tx_hash).await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn test_batch_correlate() {
        let correlator = FeedCorrelator::new(CorrelatorConfig::default());

        let r0 = test_submit_record(0);
        let r1 = test_submit_record(1);
        let h0 = r0.tx_hash;
        let h1 = r1.tx_hash;

        correlator.record_submission(r0).await;
        correlator.record_submission(r1).await;

        let events = correlator.correlate_batch(&[h0, h1, TxHash::ZERO]).await;
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_metrics_track_correlations() {
        let correlator = FeedCorrelator::new(CorrelatorConfig::default());
        let record = test_submit_record(0);
        let tx_hash = record.tx_hash;

        correlator.record_submission(record).await;
        correlator.correlate(&tx_hash).await;

        assert_eq!(correlator.metrics().correlated(), 1);
        assert_eq!(correlator.metrics().misses(), 0);
    }

    #[tokio::test]
    async fn test_eviction_sweep_removes_timed_out_txs() {
        let config = CorrelatorConfig {
            eviction_timeout_ms: 50,
            sweep_interval_ms: 20,
        };
        let correlator = FeedCorrelator::new(config);

        let record = test_submit_record(0);
        correlator.record_submission(record).await;
        assert_eq!(correlator.pending_count().await, 1);

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(10);
        let handle = correlator.start_eviction_sweep(event_tx);

        // Wait for eviction
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(correlator.pending_count().await, 0);
        assert_eq!(correlator.metrics().misses(), 1);

        // Should have received a TxTimeout event
        let event = event_rx.try_recv().expect("should have timeout event");
        assert!(matches!(event, CorrelationEvent::TxTimeout { .. }));

        handle.abort();
    }

    #[tokio::test]
    async fn test_correlate_before_eviction() {
        let config = CorrelatorConfig {
            eviction_timeout_ms: 200,
            sweep_interval_ms: 50,
        };
        let correlator = FeedCorrelator::new(config);

        let record = test_submit_record(0);
        let tx_hash = record.tx_hash;
        correlator.record_submission(record).await;

        let (event_tx, _event_rx) = tokio::sync::mpsc::channel(10);
        let handle = correlator.start_eviction_sweep(event_tx);

        // Correlate immediately before eviction
        let event = correlator.correlate(&tx_hash).await;
        assert!(event.is_some());
        assert_eq!(correlator.pending_count().await, 0);

        // No eviction should happen
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(correlator.metrics().misses(), 0);

        handle.abort();
    }

    #[test]
    fn test_correlator_config_defaults() {
        let config = CorrelatorConfig::default();
        assert_eq!(config.eviction_timeout_ms, 60_000);
        assert_eq!(config.sweep_interval_ms, 5_000);
    }

    #[test]
    fn test_correlation_event_serialization() {
        let events = vec![
            CorrelationEvent::TxAccepted {
                tx_hash_hex: "0x1234".into(),
                acceptance_latency_ms: 42,
                gas_limit: 21_000,
                template_name: "transfer".into(),
            },
            CorrelationEvent::TxTimeout {
                tx_hash_hex: "0x5678".into(),
                waited_ms: 60_000,
                gas_limit: 21_000,
                template_name: "transfer".into(),
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).expect("serialize");
            let parsed: CorrelationEvent = serde_json::from_str(&json).expect("deserialize");
            // Just check it round-trips without panic
            let _ = serde_json::to_string(&parsed).unwrap();
        }
    }
}
