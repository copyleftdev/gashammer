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

//! L1 contract readers for Nitro's SequencerInbox and RollupCore.
//!
//! Monitors batch posting activity and assertion chain progress on L1.
//! See RFC-0001 §6–7.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, RootProvider};
use alloy::rpc::types::{BlockNumberOrTag, Filter, Log};
use alloy::sol;
use alloy::sol_types::SolEvent;
use alloy::transports::http::Http;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::NitroError;
use crate::headers;

// ---------------------------------------------------------------------------
// Solidity ABI definitions (alloy sol! macro)
// ---------------------------------------------------------------------------

sol! {
    /// SequencerInbox event emitted when a batch is posted to L1.
    /// See RFC-0001 §6.2.
    #[derive(Debug)]
    event SequencerBatchDelivered(
        uint256 indexed batchSequenceNumber,
        bytes32 indexed beforeAcc,
        bytes32 indexed afterAcc,
        bytes32 delayedAcc,
        uint256 afterDelayedMessagesRead,
        uint64 timeBoundsMin,
        uint64 timeBoundsMax,
        uint8 dataLocation
    );

    /// SequencerInbox function to get batch count.
    #[derive(Debug)]
    function batchCount() external view returns (uint256);

    /// RollupCore events for assertion lifecycle.
    /// See RFC-0001 §7.2.
    #[derive(Debug)]
    event AssertionCreated(bytes32 indexed assertionHash);

    #[derive(Debug)]
    event AssertionConfirmed(bytes32 indexed assertionHash);

    /// RollupCore function to get latest confirmed assertion.
    #[derive(Debug)]
    function latestConfirmed() external view returns (bytes32);
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for L1 contract monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Config {
    /// L1 RPC endpoint URL.
    pub rpc_url: String,
    /// SequencerInbox contract address on L1.
    pub sequencer_inbox_address: String,
    /// RollupCore contract address on L1.
    pub rollup_core_address: String,
    /// Polling interval in milliseconds.
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// Number of blocks to look back when scanning for events.
    #[serde(default = "default_lookback_blocks")]
    pub lookback_blocks: u64,
}

fn default_poll_interval_ms() -> u64 {
    15_000
}

fn default_lookback_blocks() -> u64 {
    100
}

impl L1Config {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }

    /// Parse the sequencer inbox address.
    pub fn sequencer_inbox(&self) -> Result<Address, NitroError> {
        self.sequencer_inbox_address
            .parse()
            .map_err(|e| NitroError::Config(format!("invalid SequencerInbox address: {e}")))
    }

    /// Parse the rollup core address.
    pub fn rollup_core(&self) -> Result<Address, NitroError> {
        self.rollup_core_address
            .parse()
            .map_err(|e| NitroError::Config(format!("invalid RollupCore address: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Parsed event types
// ---------------------------------------------------------------------------

/// A decoded SequencerBatchDelivered event from L1.
#[derive(Debug, Clone)]
pub struct BatchDeliveredEvent {
    /// Batch sequence number.
    pub batch_sequence_number: U256,
    /// Accumulator before this batch.
    pub before_acc: B256,
    /// Accumulator after this batch.
    pub after_acc: B256,
    /// Delayed message accumulator.
    pub delayed_acc: B256,
    /// Count of delayed messages read after this batch.
    pub after_delayed_messages_read: U256,
    /// Time bounds minimum (L1 block timestamp).
    pub time_bounds_min: u64,
    /// Time bounds maximum (L1 block timestamp).
    pub time_bounds_max: u64,
    /// Data location: 0 = calldata, 1 = blob, etc.
    pub data_location: u8,
    /// L1 block number where this event was emitted.
    pub l1_block_number: Option<u64>,
    /// L1 transaction hash.
    pub l1_tx_hash: Option<B256>,
}

/// A decoded assertion event from RollupCore.
#[derive(Debug, Clone)]
pub struct AssertionEvent {
    /// Assertion hash.
    pub assertion_hash: B256,
    /// Whether this is a creation or confirmation event.
    pub kind: AssertionEventKind,
    /// L1 block number where this event was emitted.
    pub l1_block_number: Option<u64>,
    /// L1 transaction hash.
    pub l1_tx_hash: Option<B256>,
}

/// Kind of assertion event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionEventKind {
    Created,
    Confirmed,
}

// ---------------------------------------------------------------------------
// L1 contract reader
// ---------------------------------------------------------------------------

/// Reads Nitro L1 contracts (SequencerInbox + RollupCore) for batch and
/// assertion data.
pub struct L1ContractReader {
    provider: RootProvider,
    config: L1Config,
    sequencer_inbox: Address,
    rollup_core: Address,
}

impl L1ContractReader {
    /// Create a new L1 contract reader.
    pub fn new(config: L1Config) -> Result<Self, NitroError> {
        let sequencer_inbox = config.sequencer_inbox()?;
        let rollup_core = config.rollup_core()?;

        let url: url::Url = config
            .rpc_url
            .parse()
            .map_err(|e| NitroError::Config(format!("invalid L1 RPC URL: {e}")))?;

        let client = headers::http_client()?;
        let http = Http::with_client(client, url);
        let rpc_client = alloy::rpc::client::RpcClient::new(http, false);
        let provider = RootProvider::new(rpc_client);

        Ok(Self {
            provider,
            config,
            sequencer_inbox,
            rollup_core,
        })
    }

    /// Get the current batch count from SequencerInbox.
    pub async fn batch_count(&self) -> Result<U256, NitroError> {
        let call = batchCountCall {};
        let result = self
            .provider
            .call(
                alloy::rpc::types::TransactionRequest::default()
                    .to(self.sequencer_inbox)
                    .input(
                        alloy::primitives::Bytes::from(
                            <batchCountCall as alloy::sol_types::SolCall>::abi_encode(&call),
                        )
                        .into(),
                    ),
            )
            .block(BlockNumberOrTag::Latest.into())
            .await
            .map_err(|e| NitroError::L1Contract(format!("batchCount call failed: {e}")))?;

        let decoded =
            <batchCountCall as alloy::sol_types::SolCall>::abi_decode_returns(&result, true)
                .map_err(|e| {
                    NitroError::InvalidResponse(format!("batchCount decode failed: {e}"))
                })?;

        Ok(decoded._0)
    }

    /// Get the latest confirmed assertion hash from RollupCore.
    pub async fn latest_confirmed(&self) -> Result<B256, NitroError> {
        let call = latestConfirmedCall {};
        let result = self
            .provider
            .call(
                alloy::rpc::types::TransactionRequest::default()
                    .to(self.rollup_core)
                    .input(
                        alloy::primitives::Bytes::from(
                            <latestConfirmedCall as alloy::sol_types::SolCall>::abi_encode(&call),
                        )
                        .into(),
                    ),
            )
            .block(BlockNumberOrTag::Latest.into())
            .await
            .map_err(|e| NitroError::L1Contract(format!("latestConfirmed call failed: {e}")))?;

        let decoded =
            <latestConfirmedCall as alloy::sol_types::SolCall>::abi_decode_returns(&result, true)
                .map_err(|e| {
                NitroError::InvalidResponse(format!("latestConfirmed decode failed: {e}"))
            })?;

        Ok(decoded._0)
    }

    /// Fetch recent SequencerBatchDelivered events.
    pub async fn get_batch_events(
        &self,
        from_block: BlockNumberOrTag,
        to_block: BlockNumberOrTag,
    ) -> Result<Vec<BatchDeliveredEvent>, NitroError> {
        let filter = Filter::new()
            .address(self.sequencer_inbox)
            .event_signature(SequencerBatchDelivered::SIGNATURE_HASH)
            .from_block(from_block)
            .to_block(to_block);

        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .map_err(|e| NitroError::L1Contract(format!("get batch events failed: {e}")))?;

        let mut events = Vec::with_capacity(logs.len());
        for log in logs {
            match decode_batch_delivered(&log) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to decode SequencerBatchDelivered event");
                }
            }
        }

        Ok(events)
    }

    /// Fetch recent assertion events (created and confirmed).
    pub async fn get_assertion_events(
        &self,
        from_block: BlockNumberOrTag,
        to_block: BlockNumberOrTag,
    ) -> Result<Vec<AssertionEvent>, NitroError> {
        // Query for both event types
        let created_filter = Filter::new()
            .address(self.rollup_core)
            .event_signature(AssertionCreated::SIGNATURE_HASH)
            .from_block(from_block)
            .to_block(to_block);

        let confirmed_filter = Filter::new()
            .address(self.rollup_core)
            .event_signature(AssertionConfirmed::SIGNATURE_HASH)
            .from_block(from_block)
            .to_block(to_block);

        let (created_logs, confirmed_logs) = tokio::try_join!(
            self.provider.get_logs(&created_filter),
            self.provider.get_logs(&confirmed_filter),
        )
        .map_err(|e| NitroError::L1Contract(format!("get assertion events failed: {e}")))?;

        let mut events = Vec::with_capacity(created_logs.len() + confirmed_logs.len());

        for log in created_logs {
            match decode_assertion_created(&log) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to decode AssertionCreated event");
                }
            }
        }

        for log in confirmed_logs {
            match decode_assertion_confirmed(&log) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to decode AssertionConfirmed event");
                }
            }
        }

        // Sort by block number for consistent ordering.
        events.sort_by_key(|e| e.l1_block_number.unwrap_or(0));

        Ok(events)
    }

    /// Returns a reference to the config.
    pub fn config(&self) -> &L1Config {
        &self.config
    }

    /// Returns the sequencer inbox address.
    pub fn sequencer_inbox_address(&self) -> Address {
        self.sequencer_inbox
    }

    /// Returns the rollup core address.
    pub fn rollup_core_address(&self) -> Address {
        self.rollup_core
    }
}

impl std::fmt::Debug for L1ContractReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("L1ContractReader")
            .field("rpc_url", &self.config.rpc_url)
            .field("sequencer_inbox", &self.sequencer_inbox)
            .field("rollup_core", &self.rollup_core)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Event decoding helpers
// ---------------------------------------------------------------------------

fn decode_batch_delivered(log: &Log) -> Result<BatchDeliveredEvent, NitroError> {
    use alloy::sol_types::SolEvent;

    let decoded = SequencerBatchDelivered::decode_log(log.as_ref(), true)
        .map_err(|e| NitroError::InvalidResponse(format!("batch event decode: {e}")))?;

    Ok(BatchDeliveredEvent {
        batch_sequence_number: decoded.batchSequenceNumber,
        before_acc: decoded.beforeAcc,
        after_acc: decoded.afterAcc,
        delayed_acc: decoded.delayedAcc,
        after_delayed_messages_read: decoded.afterDelayedMessagesRead,
        time_bounds_min: decoded.timeBoundsMin,
        time_bounds_max: decoded.timeBoundsMax,
        data_location: decoded.dataLocation,
        l1_block_number: log.block_number,
        l1_tx_hash: log.transaction_hash,
    })
}

fn decode_assertion_created(log: &Log) -> Result<AssertionEvent, NitroError> {
    use alloy::sol_types::SolEvent;

    let decoded = AssertionCreated::decode_log(log.as_ref(), true)
        .map_err(|e| NitroError::InvalidResponse(format!("assertion created decode: {e}")))?;

    Ok(AssertionEvent {
        assertion_hash: decoded.assertionHash,
        kind: AssertionEventKind::Created,
        l1_block_number: log.block_number,
        l1_tx_hash: log.transaction_hash,
    })
}

fn decode_assertion_confirmed(log: &Log) -> Result<AssertionEvent, NitroError> {
    use alloy::sol_types::SolEvent;

    let decoded = AssertionConfirmed::decode_log(log.as_ref(), true)
        .map_err(|e| NitroError::InvalidResponse(format!("assertion confirmed decode: {e}")))?;

    Ok(AssertionEvent {
        assertion_hash: decoded.assertionHash,
        kind: AssertionEventKind::Confirmed,
        l1_block_number: log.block_number,
        l1_tx_hash: log.transaction_hash,
    })
}

// ---------------------------------------------------------------------------
// L1 monitor events
// ---------------------------------------------------------------------------

/// Events emitted by the L1 monitor poll loop.
#[derive(Debug, Clone)]
pub enum L1Event {
    /// New batch(es) detected on L1.
    BatchDelivered {
        events: Vec<BatchDeliveredEvent>,
        poll_block: u64,
    },
    /// Assertion event(s) detected on L1.
    AssertionUpdate {
        events: Vec<AssertionEvent>,
        poll_block: u64,
    },
    /// Poll error (non-fatal — monitor retries on next tick).
    PollError { error: String },
}

// ---------------------------------------------------------------------------
// L1 monitor metrics
// ---------------------------------------------------------------------------

/// Metrics for the L1 monitor.
#[derive(Debug, Default)]
pub struct L1MonitorMetrics {
    pub gashammer_l1_polls: AtomicU64,
    pub gashammer_l1_batches_seen: AtomicU64,
    pub gashammer_l1_assertions_seen: AtomicU64,
    pub gashammer_l1_poll_errors: AtomicU64,
}

impl L1MonitorMetrics {
    pub fn polls(&self) -> u64 {
        self.gashammer_l1_polls.load(Ordering::Relaxed)
    }

    pub fn batches_seen(&self) -> u64 {
        self.gashammer_l1_batches_seen.load(Ordering::Relaxed)
    }

    pub fn assertions_seen(&self) -> u64 {
        self.gashammer_l1_assertions_seen.load(Ordering::Relaxed)
    }

    pub fn poll_errors(&self) -> u64 {
        self.gashammer_l1_poll_errors.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// L1 monitor
// ---------------------------------------------------------------------------

/// Background L1 monitor that polls SequencerInbox and RollupCore contracts
/// at a configurable interval and emits events via a channel.
///
/// Ref: RFC-0001 §6–7.
pub struct L1Monitor {
    config: L1Config,
    metrics: Arc<L1MonitorMetrics>,
}

impl L1Monitor {
    /// Create a new L1 monitor.
    pub fn new(config: L1Config) -> Self {
        Self {
            config,
            metrics: Arc::new(L1MonitorMetrics::default()),
        }
    }

    /// Returns a reference to the monitor metrics.
    pub fn metrics(&self) -> &Arc<L1MonitorMetrics> {
        &self.metrics
    }

    /// Start the background polling task.
    ///
    /// Returns a `JoinHandle` (must be stored per project conventions) and
    /// a receiver for L1 events.
    pub fn start(&self, event_tx: mpsc::Sender<L1Event>) -> Result<JoinHandle<()>, NitroError> {
        let reader = L1ContractReader::new(self.config.clone())?;
        let poll_interval = self.config.poll_interval();
        let lookback = self.config.lookback_blocks;
        let metrics = Arc::clone(&self.metrics);

        let handle = tokio::spawn(async move {
            tracing::info!(
                poll_interval_ms = poll_interval.as_millis() as u64,
                lookback_blocks = lookback,
                "L1 monitor started"
            );

            let mut ticker = tokio::time::interval(poll_interval);
            let mut last_polled_block: u64 = 0;

            loop {
                ticker.tick().await;
                metrics.gashammer_l1_polls.fetch_add(1, Ordering::Relaxed);

                // Get current block number
                let current_block = match reader.provider.get_block_number().await {
                    Ok(n) => n,
                    Err(e) => {
                        metrics
                            .gashammer_l1_poll_errors
                            .fetch_add(1, Ordering::Relaxed);
                        let _ = event_tx
                            .send(L1Event::PollError {
                                error: format!("get_block_number failed: {e}"),
                            })
                            .await;
                        continue;
                    }
                };

                let from_block = if last_polled_block == 0 {
                    current_block.saturating_sub(lookback)
                } else {
                    last_polled_block + 1
                };

                if from_block > current_block {
                    continue;
                }

                let from = BlockNumberOrTag::Number(from_block);
                let to = BlockNumberOrTag::Number(current_block);

                // Fetch batch events
                match reader.get_batch_events(from, to).await {
                    Ok(events) if !events.is_empty() => {
                        let count = events.len() as u64;
                        metrics
                            .gashammer_l1_batches_seen
                            .fetch_add(count, Ordering::Relaxed);
                        tracing::info!(
                            batch_count = count,
                            from_block,
                            to_block = current_block,
                            "L1 batch events detected"
                        );
                        if event_tx
                            .send(L1Event::BatchDelivered {
                                events,
                                poll_block: current_block,
                            })
                            .await
                            .is_err()
                        {
                            tracing::info!("L1 event channel closed, monitor exiting");
                            break;
                        }
                    }
                    Ok(_) => {} // no events
                    Err(e) => {
                        metrics
                            .gashammer_l1_poll_errors
                            .fetch_add(1, Ordering::Relaxed);
                        let _ = event_tx
                            .send(L1Event::PollError {
                                error: format!("get_batch_events failed: {e}"),
                            })
                            .await;
                    }
                }

                // Fetch assertion events
                match reader.get_assertion_events(from, to).await {
                    Ok(events) if !events.is_empty() => {
                        let count = events.len() as u64;
                        metrics
                            .gashammer_l1_assertions_seen
                            .fetch_add(count, Ordering::Relaxed);
                        tracing::info!(
                            assertion_count = count,
                            from_block,
                            to_block = current_block,
                            "L1 assertion events detected"
                        );
                        if event_tx
                            .send(L1Event::AssertionUpdate {
                                events,
                                poll_block: current_block,
                            })
                            .await
                            .is_err()
                        {
                            tracing::info!("L1 event channel closed, monitor exiting");
                            break;
                        }
                    }
                    Ok(_) => {} // no events
                    Err(e) => {
                        metrics
                            .gashammer_l1_poll_errors
                            .fetch_add(1, Ordering::Relaxed);
                        let _ = event_tx
                            .send(L1Event::PollError {
                                error: format!("get_assertion_events failed: {e}"),
                            })
                            .await;
                    }
                }

                last_polled_block = current_block;
            }
        });

        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> L1Config {
        L1Config {
            rpc_url: "http://localhost:8545".into(),
            sequencer_inbox_address: "0x1111111111111111111111111111111111111111".into(),
            rollup_core_address: "0x2222222222222222222222222222222222222222".into(),
            poll_interval_ms: default_poll_interval_ms(),
            lookback_blocks: default_lookback_blocks(),
        }
    }

    #[test]
    fn test_l1_config_defaults() {
        let config = test_config();
        assert_eq!(config.poll_interval(), Duration::from_secs(15));
        assert_eq!(config.lookback_blocks, 100);
    }

    #[test]
    fn test_l1_config_serialization() {
        let config = test_config();
        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: L1Config = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.rpc_url, config.rpc_url);
        assert_eq!(
            parsed.sequencer_inbox_address,
            config.sequencer_inbox_address
        );
        assert_eq!(parsed.rollup_core_address, config.rollup_core_address);
    }

    #[test]
    fn test_l1_config_parse_addresses() {
        let config = test_config();
        let inbox = config.sequencer_inbox().expect("parse inbox");
        assert_eq!(
            format!("{inbox:?}"),
            "0x1111111111111111111111111111111111111111"
        );

        let rollup = config.rollup_core().expect("parse rollup");
        assert_eq!(
            format!("{rollup:?}"),
            "0x2222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn test_l1_config_invalid_address() {
        let config = L1Config {
            rpc_url: "http://localhost:8545".into(),
            sequencer_inbox_address: "not-an-address".into(),
            rollup_core_address: "0x2222222222222222222222222222222222222222".into(),
            poll_interval_ms: default_poll_interval_ms(),
            lookback_blocks: default_lookback_blocks(),
        };
        let err = config.sequencer_inbox().unwrap_err();
        assert!(err.to_string().contains("GH-E107"));
    }

    #[test]
    fn test_reader_construction() {
        let config = test_config();
        let reader = L1ContractReader::new(config).expect("construct");
        assert_eq!(
            reader.sequencer_inbox_address(),
            "0x1111111111111111111111111111111111111111"
                .parse::<Address>()
                .unwrap()
        );
    }

    #[test]
    fn test_reader_invalid_rpc_url() {
        let config = L1Config {
            rpc_url: "not a url".into(),
            sequencer_inbox_address: "0x1111111111111111111111111111111111111111".into(),
            rollup_core_address: "0x2222222222222222222222222222222222222222".into(),
            poll_interval_ms: default_poll_interval_ms(),
            lookback_blocks: default_lookback_blocks(),
        };
        let err = L1ContractReader::new(config).unwrap_err();
        assert!(err.to_string().contains("GH-E107"));
    }

    #[test]
    fn test_reader_debug() {
        let config = test_config();
        let reader = L1ContractReader::new(config).expect("construct");
        let debug = format!("{reader:?}");
        assert!(debug.contains("localhost:8545"));
        assert!(debug.contains("1111111111"));
    }

    #[test]
    fn test_assertion_event_kind_eq() {
        assert_eq!(AssertionEventKind::Created, AssertionEventKind::Created);
        assert_ne!(AssertionEventKind::Created, AssertionEventKind::Confirmed);
    }

    #[test]
    fn test_event_signature_hashes() {
        use alloy::sol_types::SolEvent;

        // Verify the event signatures are valid B256 (non-zero).
        let batch_sig = SequencerBatchDelivered::SIGNATURE_HASH;
        assert_ne!(batch_sig, B256::ZERO);

        let created_sig = AssertionCreated::SIGNATURE_HASH;
        assert_ne!(created_sig, B256::ZERO);

        let confirmed_sig = AssertionConfirmed::SIGNATURE_HASH;
        assert_ne!(confirmed_sig, B256::ZERO);

        // All three should be different.
        assert_ne!(batch_sig, created_sig);
        assert_ne!(batch_sig, confirmed_sig);
        assert_ne!(created_sig, confirmed_sig);
    }

    // -----------------------------------------------------------------------
    // L1Monitor tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_monitor_construction() {
        let config = test_config();
        let monitor = L1Monitor::new(config);
        assert_eq!(monitor.metrics().polls(), 0);
        assert_eq!(monitor.metrics().batches_seen(), 0);
        assert_eq!(monitor.metrics().assertions_seen(), 0);
        assert_eq!(monitor.metrics().poll_errors(), 0);
    }

    #[test]
    fn test_monitor_metrics_default() {
        let m = L1MonitorMetrics::default();
        assert_eq!(m.polls(), 0);
        assert_eq!(m.batches_seen(), 0);
        assert_eq!(m.assertions_seen(), 0);
        assert_eq!(m.poll_errors(), 0);
    }

    #[test]
    fn test_monitor_metrics_increment() {
        let m = L1MonitorMetrics::default();
        m.gashammer_l1_polls.fetch_add(3, Ordering::Relaxed);
        m.gashammer_l1_batches_seen.fetch_add(10, Ordering::Relaxed);
        m.gashammer_l1_assertions_seen
            .fetch_add(2, Ordering::Relaxed);
        m.gashammer_l1_poll_errors.fetch_add(1, Ordering::Relaxed);

        assert_eq!(m.polls(), 3);
        assert_eq!(m.batches_seen(), 10);
        assert_eq!(m.assertions_seen(), 2);
        assert_eq!(m.poll_errors(), 1);
    }

    #[test]
    fn test_l1_event_debug() {
        let ev = L1Event::PollError {
            error: "timeout".into(),
        };
        let dbg = format!("{ev:?}");
        assert!(dbg.contains("PollError"));
        assert!(dbg.contains("timeout"));
    }

    #[test]
    fn test_l1_event_batch_delivered() {
        let ev = L1Event::BatchDelivered {
            events: vec![BatchDeliveredEvent {
                batch_sequence_number: U256::from(42u64),
                before_acc: B256::ZERO,
                after_acc: B256::ZERO,
                delayed_acc: B256::ZERO,
                after_delayed_messages_read: U256::ZERO,
                time_bounds_min: 0,
                time_bounds_max: 0,
                data_location: 0,
                l1_block_number: Some(100),
                l1_tx_hash: Some(B256::ZERO),
            }],
            poll_block: 100,
        };
        if let L1Event::BatchDelivered { events, poll_block } = &ev {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].batch_sequence_number, U256::from(42u64));
            assert_eq!(*poll_block, 100);
        } else {
            panic!("expected BatchDelivered");
        }
    }

    #[test]
    fn test_l1_event_assertion_update() {
        let ev = L1Event::AssertionUpdate {
            events: vec![AssertionEvent {
                kind: AssertionEventKind::Created,
                assertion_hash: B256::ZERO,
                l1_block_number: Some(200),
                l1_tx_hash: Some(B256::ZERO),
            }],
            poll_block: 200,
        };
        if let L1Event::AssertionUpdate { events, poll_block } = &ev {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].assertion_hash, B256::ZERO);
            assert_eq!(events[0].kind, AssertionEventKind::Created);
            assert_eq!(*poll_block, 200);
        } else {
            panic!("expected AssertionUpdate");
        }
    }

    #[test]
    fn test_l1_event_clone() {
        let ev = L1Event::PollError {
            error: "clone test".into(),
        };
        let ev2 = ev.clone();
        if let L1Event::PollError { error } = ev2 {
            assert_eq!(error, "clone test");
        } else {
            panic!("expected PollError");
        }
    }

    #[test]
    fn test_monitor_metrics_shared_across_arc() {
        let config = test_config();
        let monitor = L1Monitor::new(config);
        let metrics = Arc::clone(monitor.metrics());
        metrics.gashammer_l1_polls.fetch_add(5, Ordering::Relaxed);
        assert_eq!(monitor.metrics().polls(), 5);
    }
}
