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

//! Transaction pipeline — build, sign, submit, and track transactions.
//!
//! Pipeline stages: Template Selector → Account Selector → Tx Builder →
//! Signer → Submitter → Event Log
//!
//! Ref: RFC-0004 §5.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use alloy::primitives::{Address, Bytes, TxHash, U256};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::error::EdgeError;

// ---------------------------------------------------------------------------
// Transaction template trait
// ---------------------------------------------------------------------------

/// A built transaction ready for signing and submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltTx {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub nonce: u64,
    pub chain_id: u64,
    /// Estimated gas this tx will consume.
    pub estimated_gas: u64,
    /// Template name that generated this tx.
    pub template_name: String,
}

// ---------------------------------------------------------------------------
// Pipeline events
// ---------------------------------------------------------------------------

/// Events emitted by the pipeline for telemetry.
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    TxSubmitted {
        tx_hash: TxHash,
        template_name: String,
        account: Address,
        nonce: u64,
        estimated_gas: u64,
        submitted_at: Instant,
    },
    TxFailed {
        template_name: String,
        account: Address,
        nonce: u64,
        error: String,
        failed_at: Instant,
    },
}

// ---------------------------------------------------------------------------
// Pipeline metrics
// ---------------------------------------------------------------------------

/// Atomic metrics for the transaction pipeline.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    pub gashammer_tx_submitted: AtomicU64,
    pub gashammer_tx_failed: AtomicU64,
    pub gashammer_tx_total_gas: AtomicU64,
}

impl PipelineMetrics {
    pub fn submitted(&self) -> u64 {
        self.gashammer_tx_submitted.load(Ordering::Relaxed)
    }

    pub fn failed(&self) -> u64 {
        self.gashammer_tx_failed.load(Ordering::Relaxed)
    }

    pub fn total_gas(&self) -> u64 {
        self.gashammer_tx_total_gas.load(Ordering::Relaxed)
    }

    fn record_submit(&self, gas: u64) {
        self.gashammer_tx_submitted.fetch_add(1, Ordering::Relaxed);
        self.gashammer_tx_total_gas
            .fetch_add(gas, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.gashammer_tx_failed.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Pipeline configuration
// ---------------------------------------------------------------------------

/// Configuration for the transaction pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Number of concurrent tx generator tasks (default: 4).
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Channel buffer size for built txs (default: 1024).
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Chain ID for transactions.
    pub chain_id: u64,
}

fn default_concurrency() -> usize {
    4
}

fn default_buffer_size() -> usize {
    1024
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            buffer_size: default_buffer_size(),
            chain_id: 412346, // Nitro dev default
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction pipeline
// ---------------------------------------------------------------------------

/// The transaction pipeline manages submission of built transactions.
///
/// Receives `BuiltTx` via a channel, submits them via the RPC provider,
/// and emits `PipelineEvent`s for telemetry.
pub struct TxPipeline {
    config: PipelineConfig,
    metrics: Arc<PipelineMetrics>,
    tx_sender: mpsc::Sender<BuiltTx>,
    tx_receiver: Option<mpsc::Receiver<BuiltTx>>,
    event_sender: mpsc::Sender<PipelineEvent>,
}

impl TxPipeline {
    /// Create a new pipeline. Returns the pipeline and a receiver for events.
    pub fn new(config: PipelineConfig, event_sender: mpsc::Sender<PipelineEvent>) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel(config.buffer_size);
        Self {
            config,
            metrics: Arc::new(PipelineMetrics::default()),
            tx_sender,
            tx_receiver: Some(tx_receiver),
            event_sender,
        }
    }

    /// Returns a sender for submitting built transactions to the pipeline.
    pub fn tx_sender(&self) -> mpsc::Sender<BuiltTx> {
        self.tx_sender.clone()
    }

    /// Returns a reference to pipeline metrics.
    pub fn metrics(&self) -> &Arc<PipelineMetrics> {
        &self.metrics
    }

    /// Start the pipeline workers. Each worker pulls from the tx channel,
    /// submits to the RPC provider, and emits events.
    ///
    /// Returns JoinHandles for all worker tasks.
    pub fn start(&mut self, rpc_url: String) -> Result<Vec<JoinHandle<()>>, EdgeError> {
        let receiver = self
            .tx_receiver
            .take()
            .ok_or_else(|| EdgeError::TxPipeline("pipeline already started".into()))?;

        // Fan-out: wrap receiver in Arc<Mutex> so workers can share it
        let shared_rx = Arc::new(tokio::sync::Mutex::new(receiver));
        let mut handles = Vec::with_capacity(self.config.concurrency);

        for worker_id in 0..self.config.concurrency {
            let rx = Arc::clone(&shared_rx);
            let metrics = Arc::clone(&self.metrics);
            let events = self.event_sender.clone();
            let url = rpc_url.clone();

            let handle = tokio::spawn(async move {
                tracing::info!(worker_id, "pipeline worker started");

                loop {
                    let built_tx = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };

                    let built_tx = match built_tx {
                        Some(tx) => tx,
                        None => {
                            tracing::info!(worker_id, "pipeline channel closed, worker exiting");
                            break;
                        }
                    };

                    let submit_time = Instant::now();
                    let account = built_tx.to; // For logging; real impl would use the sender
                    let nonce = built_tx.nonce;
                    let template_name = built_tx.template_name.clone();
                    let estimated_gas = built_tx.estimated_gas;

                    // Submit the transaction via RPC.
                    // In the real implementation, this would:
                    // 1. RLP-encode the transaction
                    // 2. Sign with the account's private key
                    // 3. Call eth_sendRawTransaction
                    //
                    // For now, we simulate the submission pattern.
                    let result = Self::submit_tx(&url, &built_tx).await;

                    match result {
                        Ok(tx_hash) => {
                            metrics.record_submit(estimated_gas);
                            let event = PipelineEvent::TxSubmitted {
                                tx_hash,
                                template_name,
                                account,
                                nonce,
                                estimated_gas,
                                submitted_at: submit_time,
                            };
                            if events.send(event).await.is_err() {
                                tracing::warn!(
                                    worker_id,
                                    "event channel closed, dropping TxSubmitted"
                                );
                            }
                        }
                        Err(e) => {
                            metrics.record_failure();
                            let event = PipelineEvent::TxFailed {
                                template_name,
                                account,
                                nonce,
                                error: e.to_string(),
                                failed_at: Instant::now(),
                            };
                            if events.send(event).await.is_err() {
                                tracing::warn!(
                                    worker_id,
                                    "event channel closed, dropping TxFailed"
                                );
                            }
                            // No retry — nonce safety (RFC-0004 §5)
                            tracing::warn!(
                                worker_id,
                                error = %e,
                                "tx submission failed, not retrying"
                            );
                        }
                    }
                }
            });

            handles.push(handle);
        }

        Ok(handles)
    }

    /// Submit a transaction to the RPC endpoint via `eth_sendTransaction`.
    ///
    /// Uses the provider's built-in signer (EthereumWallet) for signing.
    /// Falls back to deterministic hash if no real RPC is available.
    async fn submit_tx(rpc_url: &str, built_tx: &BuiltTx) -> Result<TxHash, EdgeError> {
        use alloy::network::TransactionBuilder;
        use alloy::providers::{Provider, ProviderBuilder};
        use alloy::rpc::types::TransactionRequest;

        let url: url::Url = rpc_url
            .parse()
            .map_err(|e| EdgeError::TxPipeline(format!("invalid RPC URL: {e}")))?;

        let provider = ProviderBuilder::new().on_http(url);

        let tx = TransactionRequest::default()
            .with_to(built_tx.to)
            .with_value(built_tx.value)
            .with_input(built_tx.data.clone())
            .with_gas_limit(built_tx.gas_limit)
            .with_nonce(built_tx.nonce)
            .with_chain_id(built_tx.chain_id);

        let pending = provider
            .send_transaction(tx)
            .await
            .map_err(|e| EdgeError::TxPipeline(format!("send_transaction failed: {e}")))?;

        Ok(*pending.tx_hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_built_tx(nonce: u64) -> BuiltTx {
        BuiltTx {
            to: Address::ZERO,
            value: U256::from(1_000_000_000_000_000_000u64), // 1 ETH
            data: Bytes::new(),
            gas_limit: 21_000,
            nonce,
            chain_id: 412346,
            estimated_gas: 21_000,
            template_name: "simple_transfer".into(),
        }
    }

    #[tokio::test]
    async fn test_pipeline_processes_transactions() {
        // With real RPC submission, txs to unreachable endpoint produce TxFailed events
        let (event_tx, mut event_rx) = mpsc::channel(100);
        let mut pipeline = TxPipeline::new(PipelineConfig::default(), event_tx);
        let sender = pipeline.tx_sender();

        let handles = pipeline
            .start("http://127.0.0.1:19999".into())
            .expect("start should succeed");

        for i in 0..3 {
            sender
                .send(test_built_tx(i))
                .await
                .expect("send should work");
        }

        drop(sender);
        drop(pipeline);

        for h in handles {
            h.await.expect("worker should complete");
        }

        let mut events = vec![];
        while let Ok(evt) = event_rx.try_recv() {
            events.push(evt);
        }

        // All 3 should produce events (TxFailed since no RPC server)
        assert_eq!(events.len(), 3);
        for evt in &events {
            assert!(matches!(evt, PipelineEvent::TxFailed { .. }));
        }
    }

    #[tokio::test]
    async fn test_pipeline_metrics() {
        // With unreachable RPC, submissions fail and metrics track failures
        let (event_tx, _event_rx) = mpsc::channel(100);
        let mut pipeline = TxPipeline::new(PipelineConfig::default(), event_tx);
        let sender = pipeline.tx_sender();
        let metrics = Arc::clone(pipeline.metrics());

        let handles = pipeline
            .start("http://127.0.0.1:19999".into())
            .expect("start");

        sender.send(test_built_tx(0)).await.unwrap();
        sender.send(test_built_tx(1)).await.unwrap();

        drop(sender);
        drop(pipeline);

        for h in handles {
            h.await.unwrap();
        }

        // Txs fail because no RPC server is running
        assert_eq!(metrics.failed(), 2);
        assert_eq!(metrics.submitted(), 0);
    }

    #[tokio::test]
    async fn test_pipeline_cannot_start_twice() {
        let (event_tx, _event_rx) = mpsc::channel(100);
        let mut pipeline = TxPipeline::new(PipelineConfig::default(), event_tx);

        let _handles = pipeline.start("http://localhost:8547".into()).unwrap();
        let result = pipeline.start("http://localhost:8547".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_pipeline_config_defaults() {
        let config = PipelineConfig::default();
        assert_eq!(config.concurrency, 4);
        assert_eq!(config.buffer_size, 1024);
        assert_eq!(config.chain_id, 412346);
    }

    #[test]
    fn test_built_tx_serialization() {
        let tx = test_built_tx(42);
        let json = serde_json::to_string(&tx).expect("serialize");
        let parsed: BuiltTx = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tx.nonce, parsed.nonce);
        assert_eq!(tx.chain_id, parsed.chain_id);
        assert_eq!(tx.template_name, parsed.template_name);
    }

    #[tokio::test]
    async fn test_submit_tx_fails_with_bad_url() {
        let tx = test_built_tx(0);
        let result = TxPipeline::submit_tx("http://127.0.0.1:19999", &tx).await;
        assert!(result.is_err(), "should fail with unreachable RPC");
    }

    #[test]
    fn test_pipeline_event_variants() {
        let submitted = PipelineEvent::TxSubmitted {
            tx_hash: TxHash::ZERO,
            template_name: "test".into(),
            account: Address::ZERO,
            nonce: 0,
            estimated_gas: 21_000,
            submitted_at: Instant::now(),
        };
        assert!(matches!(submitted, PipelineEvent::TxSubmitted { .. }));

        let failed = PipelineEvent::TxFailed {
            template_name: "test".into(),
            account: Address::ZERO,
            nonce: 0,
            error: "nonce too low".into(),
            failed_at: Instant::now(),
        };
        assert!(matches!(failed, PipelineEvent::TxFailed { .. }));
    }
}
